#!/usr/bin/env bash
# Builds the Python pack's release bundle for one platform (SPEC.md "The Python pack and its
# bundle"): a directory `rascal tools setup --bundle <dir>` installs from with no network.
#
#   scripts/build-bundle.sh <macos-arm64|linux-x86_64> <out-dir> [--archive <file.tar.gz>]
#
# The bundle is the provisioning project itself: `pyproject.toml` pinning the pack wheel by
# relative path for that platform only, its `uv.lock` resolved against the wheelhouse and
# nothing else, `wheel/` (the pack, from `uv build`), `wheelhouse/` (every locked dependency as
# a wheel for the platform and the pack's Python, downloaded with pip under a uv-managed
# interpreter), `MODEL.md`, `artifacts.toml`, `NOTICE`, `LICENSE`, the model artifacts under
# `models/` when the pack's model directory holds them, and `bundle.toml` naming the pack, its
# version, the Python version, the platform, whether the models are in, and the SHA-256 of
# every file. Building needs uv and the network; it is the one step of the pack's life that
# does.
set -euo pipefail
cd "$(dirname "$0")/.."
platform="${1:?platform: macos-arm64 or linux-x86_64}"
out="${2:?the bundle directory to create}"
archive=""
if [ "${3:-}" = "--archive" ]; then archive="${4:?the .tar.gz to write}"; fi
case "$platform" in
  macos-arm64)
    tags=(--platform macosx_11_0_arm64 --platform macosx_12_0_arm64 --platform macosx_13_0_arm64 --platform macosx_14_0_arm64)
    environment="sys_platform == 'darwin' and platform_machine == 'arm64'" ;;
  linux-x86_64)
    tags=(--platform manylinux2014_x86_64 --platform manylinux_2_17_x86_64 --platform manylinux_2_28_x86_64 --platform manylinux_2_34_x86_64)
    environment="sys_platform == 'linux' and platform_machine == 'x86_64'" ;;
  *) echo "build-bundle: unknown platform $platform (macos-arm64 or linux-x86_64)" >&2; exit 64 ;;
esac
command -v uv >/dev/null 2>&1 || { echo "build-bundle: uv is not installed" >&2; exit 64; }
python="$(tr -d '[:space:]' < tools/python/.python-version)"
if [ -e "$out" ] && [ -n "$(ls -A "$out")" ]; then
  echo "build-bundle: $out exists and is not empty" >&2
  exit 64
fi
mkdir -p "$out/wheel" "$out/wheelhouse"
out="$(cd "$out" && pwd)"

echo "build-bundle: the pack wheel"
uv build --wheel --project tools/python --out-dir "$out/wheel" --quiet
wheel="$(basename "$(ls "$out"/wheel/*.whl)")"
pack_version="$(echo "$wheel" | cut -d- -f2)"

echo "build-bundle: the wheelhouse for $platform, Python $python"
uv export --project tools/python --format requirements-txt --no-dev --no-emit-project --no-hashes --frozen --quiet > "$out/requirements.txt"
uv run --no-project --python "$python" --with pip python -m pip download --quiet \
  -r "$out/requirements.txt" --dest "$out/wheelhouse" --only-binary=:all: --no-deps \
  --python-version "$python" "${tags[@]}"
rm "$out/requirements.txt"

echo "build-bundle: the provisioning project and its lock"
cat > "$out/pyproject.toml" <<TOML
# The provisioning project of a rascal-tools bundle (scripts/build-bundle.sh): the pack wheel
# by relative path, every dependency from the wheelhouse beside it, this platform only.
[project]
name = "rascal-tools-install"
version = "$pack_version"
description = "Installs the rascal-tools pack from its bundle: rascal tools setup --bundle <this directory>"
requires-python = "==$python.*"
dependencies = ["rascal-tools==$pack_version"]

[tool.uv]
package = false
environments = ["$environment"]

[tool.uv.sources]
rascal-tools = { path = "wheel/$wheel" }
TOML
(cd "$out" && uv lock --no-index --find-links wheelhouse --python "$python" --quiet)
cp tools/python/MODEL.md tools/python/src/rascal_tools/artifacts.toml tools/python/NOTICE LICENSE "$out/"

# The model artifacts (MODEL.md), from the pack's model directory when every one is there
# with its digest; otherwise the bundle serves echo_region only and says so, or fails when
# BUNDLE_MODELS=required.
models_dir="${RASCAL_TOOLS_MODELS:-$HOME/.cache/rascal-tools}"
models=false
if uv run --no-project --python "$python" python -I - "$models_dir" tools/python/src/rascal_tools/artifacts.toml <<'PY'
import hashlib, sys, tomllib
from pathlib import Path
models_dir, manifest = Path(sys.argv[1]), Path(sys.argv[2])
for entry in tomllib.loads(manifest.read_text())["artifact"]:
    path = models_dir / entry["name"]
    if not path.is_file():
        print(f"build-bundle: {path} is absent", file=sys.stderr); sys.exit(1)
    if hashlib.sha256(path.read_bytes()).hexdigest() != entry["sha256"]:
        print(f"build-bundle: {path} does not match artifacts.toml", file=sys.stderr); sys.exit(1)
PY
then
  echo "build-bundle: the models from $models_dir"
  mkdir -p "$out/models"
  while IFS= read -r name; do cp "$models_dir/$name" "$out/models/"; done < <(grep '^name = ' tools/python/src/rascal_tools/artifacts.toml | sed 's/^name = "\(.*\)"$/\1/')
  models=true
elif [ "${BUNDLE_MODELS:-}" = required ]; then
  echo "build-bundle: BUNDLE_MODELS=required but the models are not all in $models_dir (rascal-tools fetch)" >&2
  exit 1
else
  echo "build-bundle: no models in $models_dir; this bundle serves echo_region only (rascal-tools fetch, then build again)"
fi

echo "build-bundle: bundle.toml"
uv run --no-project --python "$python" python -I - "$out" "$platform" "$pack_version" "$python" "wheel/$wheel" "$models" <<'PY'
import hashlib, os, sys
from pathlib import Path
out, platform, pack_version, python, wheel, models = sys.argv[1:7]
root = Path(out)
files = sorted(
    p.relative_to(root).as_posix()
    for p in root.rglob("*")
    if p.is_file() and p.name != "bundle.toml"
)
lines = [
    "# The bundle manifest (scripts/build-bundle.sh): what rascal tools setup verifies before",
    "# it installs. Every file below is listed with its SHA-256; nothing else is installed.",
    "bundle_version = 1",
    'pack = "rascal-tools"',
    f'pack_version = "{pack_version}"',
    f'python = "{python}"',
    f'platform = "{platform}"',
    f'wheel = "{wheel}"',
    f"models = {models}",
    "",
    "[files]",
]
for rel in files:
    digest = hashlib.sha256((root / rel).read_bytes()).hexdigest()
    lines.append(f'"{rel}" = "sha256:{digest}"')
(root / "bundle.toml").write_text("\n".join(lines) + "\n")
print(f"build-bundle: {len(files)} files listed")
PY
echo "build-bundle: $out ($platform, rascal-tools $pack_version, Python $python)"
if [ -n "$archive" ]; then
  # The archive holds the bundle as one top-level directory named after the platform.
  staged="$(mktemp -d "${TMPDIR:-/tmp}/rascal-bundle-archive.XXXXXX")"
  ln -s "$out" "$staged/rascal-tools-bundle-$platform"
  tar -czf "$archive" -C "$staged" -h "rascal-tools-bundle-$platform"
  rm -rf "$staged"
  echo "build-bundle: $archive ($(du -h "$archive" | cut -f1)), sha256:$(shasum -a 256 "$archive" | cut -d' ' -f1)"
fi
