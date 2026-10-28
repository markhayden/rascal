#!/usr/bin/env bash
# Publish every workspace crate to crates.io in dependency order (core, testkit, ops, mcp, mask).
# The owner runs this from a clean, tagged checkout; CI and everyone else run it with --dry-run,
# which packages and verifies without uploading.
#
#   scripts/publish.sh --dry-run
#   scripts/publish.sh            # needs `cargo login` and a tag v<version> on HEAD
set -euo pipefail
cd "$(dirname "$0")/.."
dry_run=""
case "${1:-}" in
  --dry-run) dry_run="--dry-run" ;;
  "") ;;
  *) echo "usage: scripts/publish.sh [--dry-run]" >&2; exit 64 ;;
esac
version="$(cargo metadata --format-version 1 --no-deps \
  | python3 -I -c 'import json, sys; m = json.load(sys.stdin); print(next(p["version"] for p in m["packages"] if p["name"] == "rascal-mask"))')"
if [ -z "$dry_run" ]; then
  [ -z "$(git status --porcelain)" ] || { echo "working tree is not clean" >&2; exit 1; }
  git tag --points-at HEAD | grep -qx "v$version" || { echo "HEAD is not tagged v$version" >&2; exit 1; }
fi
scripts/check-packaging.sh
cargo publish --workspace --locked ${dry_run:+--allow-dirty} $dry_run
echo "published${dry_run:+ (dry run)} rascal $version"
