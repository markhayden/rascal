#!/usr/bin/env bash
# The Python pack's gates: ruff check, ruff format --check and pytest, run offline in the
# pack's synced environment (tools/python/.venv). Without `uv` or without the environment the
# gates are skipped with a line saying so, so a Rust-only machine still passes `just check`;
# CI syncs the environment first and never skips.
set -euo pipefail
cd "$(dirname "$0")/../tools/python"
if ! command -v uv >/dev/null 2>&1; then
  echo "python: skipped (uv is not installed)"
  exit 0
fi
if [ ! -x .venv/bin/python ]; then
  if [ -n "${CI:-}" ]; then
    echo "python: tools/python/.venv is missing in CI; run uv sync --frozen first" >&2
    exit 1
  fi
  echo "python: skipped (tools/python/.venv is missing; run: cd tools/python && uv sync --frozen)"
  exit 0
fi
uv run --frozen --offline --no-sync ruff check .
uv run --frozen --offline --no-sync ruff format --check .
uv run --frozen --offline --no-sync pytest
echo "python ok"
