#!/usr/bin/env bash
# The Python pack's disconnected acceptance (SPEC.md "The Python pack and its bundle"): build
# this machine's bundle (the one network step), then, with the network disabled where the
# platform allows it (`unshare -rn` on Linux) and the uv and pip caches pointed at empty
# directories, `rascal tools setup --bundle` installs into a clean directory, the installed
# pack answers `echo_region` through `rascal tools call`, a tampered copy of the bundle is
# refused, and the pytest bundle test checks the build's output.
#
#   scripts/bundle-acceptance.sh <rascal binary> [scratch dir]
#
# Needs uv and a Python 3.12 uv can find (`uv python install 3.12` on a connected machine).
# Without uv the step is skipped with a line saying so, except in CI, where it fails.
set -euo pipefail
cd "$(dirname "$0")/.."
rascal="${1:?the rascal binary}"
rascal="$(cd "$(dirname "$rascal")" && pwd)/$(basename "$rascal")"
scratch="${2:-$(mktemp -d "${TMPDIR:-/tmp}/rascal-bundle.XXXXXX")}"
if ! command -v uv >/dev/null 2>&1; then
  if [ -n "${CI:-}" ]; then echo "bundle: uv is not installed in CI" >&2; exit 1; fi
  echo "bundle: skipped (uv is not installed)"
  exit 0
fi
case "$(uname -s)/$(uname -m)" in
  Darwin/arm64) platform=macos-arm64 ;;
  Linux/x86_64) platform=linux-x86_64 ;;
  *)
    if [ -n "${CI:-}" ]; then echo "bundle: no bundle platform for $(uname -s)/$(uname -m) in CI" >&2; exit 1; fi
    echo "bundle: skipped (no bundle platform for $(uname -s)/$(uname -m))"
    exit 0 ;;
esac
python="$(tr -d '[:space:]' < tools/python/.python-version)"
UV_PYTHON_DOWNLOADS=never uv python find "$python" >/dev/null || { echo "bundle: no Python $python for uv; uv python install $python first" >&2; exit 1; }

echo "bundle: building for $platform"
scripts/build-bundle.sh "$platform" "$scratch/bundle" | tail -1

# From here on, no network: an empty namespace where the kernel grants one, and always
# --offline plus empty caches inside setup itself.
disconnected=()
if command -v unshare >/dev/null 2>&1 && unshare -rn true 2>/dev/null; then
  disconnected=(unshare -rn)
  echo "bundle: the network is disabled (unshare -rn)"
elif [ -n "${CI:-}" ] && [ "$platform" = linux-x86_64 ]; then
  echo "bundle: unshare -rn is refused on this Linux runner; the disconnected step needs it" >&2
  exit 1
else
  echo "bundle: no network namespace here (macOS); the proof is uv --offline --no-index --no-cache and the launcher never calling uv"
fi
export UV_CACHE_DIR="$scratch/uv-cache" PIP_CACHE_DIR="$scratch/pip-cache" PIP_NO_INDEX=1 UV_PYTHON_DOWNLOADS=never
mkdir -p "$UV_CACHE_DIR" "$PIP_CACHE_DIR"

echo "bundle: setup into a clean directory"
${disconnected[@]+"${disconnected[@]}"} "$rascal" tools setup --bundle "$scratch/bundle" --into "$scratch/pack" | tee "$scratch/setup.out"
command_line="$(grep '^\[packs.rascal-tools\]: command' "$scratch/setup.out" | sed 's/^\[packs.rascal-tools\]: //')"
printf 'config_version = 1\n[packs.rascal-tools]\n%s\n' "$command_line" > "$scratch/rascal.toml"

echo "bundle: the installed pack answers echo_region"
${disconnected[@]+"${disconnected[@]}"} "$rascal" tools call rascal-tools echo_region --config "$scratch/rascal.toml" \
  --workspace "$scratch/ws" --image fixtures/images/floor.png --bbox 0.25,0.5,0.75,1 --json > "$scratch/call.json"
python3 -I -c '
import json, sys
text = open(sys.argv[1]).read()
envelope = json.JSONDecoder().raw_decode(text[text.index("{"):])[0]
assert envelope["status"] == "ok", envelope
print("echo_region:", envelope["summary"])
' "$scratch/call.json"

echo "bundle: a tampered copy is refused"
cp -R "$scratch/bundle" "$scratch/tampered"
first_wheel="$(ls "$scratch/tampered/wheelhouse" | head -1)"
printf 'x' >> "$scratch/tampered/wheelhouse/$first_wheel"
if ${disconnected[@]+"${disconnected[@]}"} "$rascal" tools setup --bundle "$scratch/tampered" --into "$scratch/pack-2" 2> "$scratch/tampered.err"; then
  echo "bundle: the tampered bundle installed" >&2; exit 1
fi
grep -q "$first_wheel" "$scratch/tampered.err" || { cat "$scratch/tampered.err" >&2; exit 1; }
[ ! -e "$scratch/pack-2" ] || { echo "bundle: the tampered install left files" >&2; exit 1; }
echo "refused: $(head -c 160 "$scratch/tampered.err")"

echo "bundle: the pytest bundle test on the build's output"
if [ -x tools/python/.venv/bin/python ]; then
  RASCAL_BUNDLE="$scratch/bundle" uv run --project tools/python --frozen --offline --no-sync pytest tools/python/tests/test_bundle.py -q
else
  echo "bundle: tools/python/.venv is missing; the pytest bundle test did not run"
  [ -z "${CI:-}" ] || exit 1
fi

echo "bundle: the Rust install test against this bundle"
RASCAL_BUNDLE="$scratch/bundle" RASCAL_BUNDLE_REQUIRED=1 cargo test --quiet -p rascal-mask --test tools_setup -- a_built_bundle
echo "bundle ok: $platform"
