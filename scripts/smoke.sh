#!/usr/bin/env bash
# The offline smoke against any built `rascal` binary: it names itself with the workspace
# version, and it answers no arguments with a usage error (exit 64) and no output on stdout.
# Grows with the binary: each milestone adds the commands it ships.
set -euo pipefail
cd "$(dirname "$0")/.."
binary="${1:?usage: scripts/smoke.sh <binary>}"
[ -x "$binary" ] || { echo "smoke: $binary is not executable" >&2; exit 1; }
version="$(cargo metadata --format-version 1 --no-deps \
  | python3 -I -c 'import json, sys; m = json.load(sys.stdin); print(next(p["version"] for p in m["packages"] if p["name"] == "rascal-mask"))')"

got="$("$binary" --version)"
if [ "$got" != "rascal $version" ]; then
  echo "smoke: --version printed '$got', expected 'rascal $version'" >&2
  exit 1
fi
set +e
out="$("$binary" 2>/dev/null)"; code=$?
set -e
if [ "$code" -ne 64 ]; then
  echo "smoke: no arguments exited $code, expected exit 64" >&2
  exit 1
fi
if [ -n "$out" ]; then
  echo "smoke: no arguments wrote to stdout" >&2
  exit 1
fi
echo "smoke ok: rascal $version"
