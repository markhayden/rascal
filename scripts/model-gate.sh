#!/usr/bin/env bash
# The Linux x86_64 model gate (PLAN.md M14, "Model-required runs"): a model's real tests on
# linux/amd64, offline. It builds a linux/amd64 image with uv, Python 3.12 and the pack's
# environment; installs the model with `rascal-tools get` into a mounted models directory (the
# one step with the network); then runs the model's tests, `RASCAL_TOOLS_REQUIRE_MODELS=<model>
# pytest -m model_<model>`, in a second container with `--network none`. A missing prerequisite,
# a skipped test or a model no test is marked for fails it. On an arm64 host the image runs
# under emulation: slow, but the same machine code a Linux x86_64 user runs.
#
#   scripts/model-gate.sh <model> [models dir]
set -euo pipefail
cd "$(dirname "$0")/.."
model="${1:?the catalogue id of the model to gate}"
command -v docker >/dev/null 2>&1 || { echo "model-gate: docker is not installed" >&2; exit 1; }
docker info >/dev/null 2>&1 || { echo "model-gate: the docker daemon is not running" >&2; exit 1; }
models="${2:-$(mktemp -d "${TMPDIR:-/tmp}/rascal-model-gate.XXXXXX")}"
mkdir -p "$models"
models="$(cd "$models" && pwd)"
uv_version="$(sed -n 's/^ *version: "\(0\.[0-9.]*\)"$/\1/p' .github/workflows/bundles.yml | head -1)"
python="$(tr -d '[:space:]' < tools/python/.python-version)"
context="$(mktemp -d "${TMPDIR:-/tmp}/rascal-model-gate-context.XXXXXX")"
trap 'rm -rf "$context"' EXIT
mkdir -p "$context/src/tools" "$context/src/fixtures"
rsync -a --exclude .venv --exclude __pycache__ --exclude .pytest_cache --exclude .ruff_cache \
  tools/python "$context/src/tools/"
rsync -a fixtures/images fixtures/parity "$context/src/fixtures/"
cat > "$context/Dockerfile" <<DOCKER
FROM --platform=linux/amd64 python:${python}-slim
RUN pip install --no-cache-dir "uv==${uv_version}"
COPY src /src
WORKDIR /src/tools/python
RUN uv sync --frozen
DOCKER
image="rascal-model-gate:$(git rev-parse --short HEAD)"
echo "model-gate: building $image (linux/amd64, Python $python, uv $uv_version)"
docker build --platform linux/amd64 -q -t "$image" "$context" >/dev/null

echo "model-gate: installing $model into $models (network on)"
docker run --rm --platform linux/amd64 -v "$models:/models" "$image" \
  uv run --frozen rascal-tools get "$model" --models /models

echo "model-gate: $model's tests with the network off"
docker run --rm --platform linux/amd64 --network none -v "$models:/models:ro" \
  -e RASCAL_TOOLS_MODELS=/models -e RASCAL_TOOLS_REQUIRE_MODELS="$model" -e UV_OFFLINE=1 \
  "$image" uv run --frozen --offline pytest tests -m "model_$model" -p no:cacheprovider
echo "model-gate ok: $model on linux/amd64"
