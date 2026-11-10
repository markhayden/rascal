#!/usr/bin/env bash
# The Linux x86_64 model gate (PLAN.md M14, "Model-required runs"): a model's real tests on
# linux/amd64, offline. It builds a linux/amd64 image with uv, Python 3.12 and the pack's
# environment; installs the model with `rascal-tools get` into a mounted models directory (the
# one step with the network); then runs the model's tests, `RASCAL_TOOLS_REQUIRE_MODELS=<model>
# pytest -m model_<model>`, in a second container with `--network none`. A missing prerequisite,
# a skipped test or a model no test is marked for fails it. On an arm64 host the image runs
# under emulation: slow, but the same machine code a Linux x86_64 user runs. The larger models
# need more than Docker Desktop's default 8 GB VM under emulation (BiRefNet reached about 7.5 GB
# resident and was killed, exit 137); it warns below 16 GB, and all four passed at 32 GB.
#
# With --heal it also builds this checkout's rascal in the image (the pinned toolchain) and runs
# `checked_retry_real_model` there with the model, offline: a scripted checker fails the first
# try with a place, and the test fails unless the retry's `segment` ran on a crop through the
# shipped adapter (PLAN.md 14.10 (3/3)). `RASCAL_TEST_NO_THRESHOLD=1` for a model without one.
#
#   scripts/model-gate.sh [--heal] <model> [models dir]
set -euo pipefail
cd "$(dirname "$0")/.."
heal=0
if [ "${1:-}" = "--heal" ]; then heal=1; shift; fi
model="${1:?the catalogue id of the model to gate}"
command -v docker >/dev/null 2>&1 || { echo "model-gate: docker is not installed" >&2; exit 1; }
docker info >/dev/null 2>&1 || { echo "model-gate: the docker daemon is not running" >&2; exit 1; }
memory=$(docker info --format '{{.MemTotal}}' 2>/dev/null || echo 0)
if [ "$memory" -lt 17179869184 ]; then
  echo "model-gate: warning: docker's VM has $((memory / 1073741824)) GB; the larger models need 16 GB or more under emulation" >&2
fi
models="${2:-$(mktemp -d "${TMPDIR:-/tmp}/rascal-model-gate.XXXXXX")}"
mkdir -p "$models"
models="$(cd "$models" && pwd)"
uv_version="$(sed -n 's/^ *version: "\(0\.[0-9.]*\)"$/\1/p' .github/workflows/bundles.yml | head -1)"
python="$(tr -d '[:space:]' < tools/python/.python-version)"
context="$(mktemp -d "${TMPDIR:-/tmp}/rascal-model-gate-context.XXXXXX")"
trap 'rm -rf "$context"' EXIT
mkdir -p "$context/src/tools"
rsync -a --exclude .venv --exclude __pycache__ --exclude .pytest_cache --exclude .ruff_cache \
  tools/python "$context/src/tools/"
rsync -a fixtures "$context/src/"
toolchain="$(sed -n 's/^channel *= *"\(.*\)"$/\1/p' rust-toolchain.toml)"
if [ "$heal" = 1 ]; then
  rsync -a --exclude target Cargo.toml Cargo.lock rust-toolchain.toml crates schemas "$context/src/"
  [ -d ../table-manners ] && grep -q 'path = "../table-manners"' Cargo.toml \
    && rsync -a --exclude target ../table-manners "$context/"
fi
cat > "$context/Dockerfile" <<DOCKER
FROM --platform=linux/amd64 python:${python}-slim
RUN pip install --no-cache-dir "uv==${uv_version}"
$( [ "$heal" = 1 ] && printf '%s\n' \
  "RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates build-essential && rm -rf /var/lib/apt/lists/*" \
  "RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain ${toolchain}" \
  "ENV PATH=/root/.cargo/bin:\$PATH" )
COPY . /ctx
RUN mv /ctx/src /src && { [ -d /ctx/table-manners ] && mv /ctx/table-manners /table-manners || true; }
WORKDIR /src/tools/python
RUN uv sync --frozen
$( [ "$heal" = 1 ] && printf '%s\n' \
  "WORKDIR /src" \
  "RUN cargo test --locked -p rascal-mask --features rascal-mask/testkit --test it --no-run" \
  "WORKDIR /src/tools/python" )
DOCKER
image="rascal-model-gate:$(git rev-parse --short HEAD)"
echo "model-gate: building $image (linux/amd64, Python $python, uv $uv_version)"
docker build --platform linux/amd64 -q -t "$image" -f "$context/Dockerfile" "$context" >/dev/null

echo "model-gate: installing $model into $models (network on)"
docker run --rm --platform linux/amd64 -v "$models:/models" "$image" \
  uv run --frozen rascal-tools get "$model" --models /models

echo "model-gate: $model's tests with the network off"
docker run --rm --platform linux/amd64 --network none -v "$models:/models:ro" \
  -e RASCAL_TOOLS_MODELS=/models -e RASCAL_TOOLS_REQUIRE_MODELS="$model" -e UV_OFFLINE=1 \
  "$image" uv run --frozen --offline pytest tests -m "model_$model" -p no:cacheprovider
if [ "$heal" = 1 ]; then
  echo "model-gate: $model's checked retry through rascal with the network off"
  # The run holds the installation, which writes under the models directory: mounted writable.
  docker run --rm --platform linux/amd64 --network none -v "$models:/models" \
    -e RASCAL_TOOLS_MODELS=/models -e RASCAL_TEST_MODEL="$model" -e RASCAL_TEST_PACK=/src/tools/python \
    -e RASCAL_TEST_NO_THRESHOLD="${RASCAL_TEST_NO_THRESHOLD:-}" -e UV_OFFLINE=1 -w /src "$image" \
    cargo test --locked --offline -p rascal-mask --features rascal-mask/testkit --test it \
      checker::checked_retry_real_model -- --ignored --exact
fi
echo "model-gate ok: $model on linux/amd64"
