#!/usr/bin/env bash
# Tiny's release assets in a directory, fetched and verified with the pack's own installer
# (`rascal-tools get tiny`, the network step): what `scripts/bundle-acceptance.sh --models`
# installs from with the network off. Prints the directory.
#
#   scripts/tiny-assets.sh <dir>
set -euo pipefail
cd "$(dirname "$0")/.."
out="${1:?the directory to fill with Tiny's release assets}"
models="$(mktemp -d "${TMPDIR:-/tmp}/rascal-tiny.XXXXXX")"
trap 'rm -rf "$models"' EXIT
uv run --frozen --project tools/python rascal-tools get tiny --models "$models" >&2
identity="$(tr -d '[:space:]' < "$models/tiny/current")"
mkdir -p "$out"
for file in "$models/tiny/$identity"/*; do
  [ "$(basename "$file")" = install.json ] || cp "$file" "$out/"
done
echo "$out"
