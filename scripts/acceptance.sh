#!/usr/bin/env bash
# Release acceptance (SPEC.md "Testing strategy", G10): what a user gets from the packaged crates.
#
# 1. Package every crate as crates.io would serve it and install `rascal-mask` from the packages
#    into a clean directory, the sibling crates supplied from their packages (not the checkout).
# 2. Run the six shipped recipes by name on the fixtures with no configuration file; each must
#    produce its mask and its renders.
# 3. Start `serve` and drive it with the independent client through the seam ops; read the same
#    workspace back with the CLI.
# 4. The docs name every op, profile, panel and measure in the registry and nothing else.
#
# Needs the toolchain and a warm registry cache (no network is used when the cache holds every
# dependency; CI's cache does). `ACCEPTANCE_KEEP=1` keeps the scratch directory.
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/rascal-acceptance.XXXXXX")"
cleanup() { [ -n "${ACCEPTANCE_KEEP:-}" ] || rm -rf "$scratch"; }
trap cleanup EXIT
step() { printf '\n== %s\n' "$*"; }

version="$(cargo metadata --format-version 1 --no-deps \
  | python3 -I -c 'import json, sys; m = json.load(sys.stdin); print(next(p["version"] for p in m["packages"] if p["name"] == "rascal-mask"))')"

step "package the crates"
cargo package --workspace --no-verify --allow-dirty --quiet
mkdir -p "$scratch/crates"
for crate in rascal-core rascal-testkit rascal-ops rascal-mcp rascal-mask; do
  tar -xzf "target/package/$crate-$version.crate" -C "$scratch/crates"
done

step "install rascal-mask from the packages"
patch=()
for crate in rascal-core rascal-testkit rascal-ops rascal-mcp; do
  patch+=(--config "patch.crates-io.$crate.path=\"$scratch/crates/$crate-$version\"")
done
cargo install --quiet --path "$scratch/crates/rascal-mask-$version" --root "$scratch/install" --locked "${patch[@]}"
rascal="$scratch/install/bin/rascal"
[ "$("$rascal" --version)" = "rascal $version" ]
scripts/smoke.sh "$rascal"

step "the six recipes by name, no configuration file"
images="$root/fixtures/images"
locations="$root/fixtures/locations"
runs="$scratch/runs"
expect_outputs() { # <run dir> <render file> [journal dir: the workspace when --workspace was given]
  local journal="${3:-$1}"
  [ -s "$journal/journal.jsonl" ] || { echo "no journal in $journal" >&2; exit 1; }
  [ -s "$1/$2" ] || { echo "no $2 in $1" >&2; exit 1; }
  [ -s "$1/frames.json" ] && [ -s "$1/locations.json" ] || { echo "no tables in $1" >&2; exit 1; }
}
mask_of() { grep '^mask: ' | cut -d' ' -f2; }
"$rascal" run rect "$images/floor.png" --bbox 0.2,0.2,0.6,0.6 --out "$runs/rect" | mask_of >/dev/null
expect_outputs "$runs/rect" rect-grey.png
"$rascal" run seam "$images/step-pano.png" --out "$runs/seam" --workspace "$runs/ws" | tee "$scratch/seam.out" | mask_of >/dev/null
expect_outputs "$runs/seam" seam-grey.png "$runs/ws"
"$rascal" run remove "$images/chair-blob.png" --location "$locations/chairs.json" --out "$runs/remove" | mask_of >/dev/null
expect_outputs "$runs/remove" remove-grey.png
"$rascal" run repair "$images/scratch-smooth.png" --stroke "$locations/scratch-stroke.json" --out "$runs/repair" | mask_of >/dev/null
expect_outputs "$runs/repair" repair-grey.png
"$rascal" run repair-area "$images/chair-blob.png" --bbox 0.6,0.7,0.8,0.9 --out "$runs/repair-area" | mask_of >/dev/null
expect_outputs "$runs/repair-area" repair-area-grey.png
seam_mask="$(mask_of < "$scratch/seam.out")"
"$rascal" run protect "$images/step-pano.png" --mask "$seam_mask" --workspace "$runs/ws" --out "$runs/protect" | mask_of >/dev/null
expect_outputs "$runs/protect" protect-grey.png "$runs/ws"
echo "six recipes ran"

step "serve, driven by the independent client, and the CLI reading the same workspace"
cargo run --quiet -p rascal-testkit --bin independent-walk -- "$rascal" "$runs/served" "$images/step-pano.png" | tee "$scratch/walk.out"
served_mask="$(mask_of < "$scratch/walk.out")"
"$rascal" workspace list "$runs/served" --type Mask --json | python3 -I -c "
import json, sys
rows = json.load(sys.stdin)
assert any(r['id'] == '$served_mask' for r in rows), rows
print('the CLI sees the served mask')"
"$rascal" inspect "$runs/served/artifacts/$("$rascal" workspace list "$runs/served" --type Mask --json | python3 -I -c "
import json, sys
rows = json.load(sys.stdin)
print(next(r['identity'] for r in rows if r['id'] == '$served_mask').split(':')[1] + '.mask')")" --json >/dev/null

step "docs coverage"
scripts/check-docs.sh

echo
echo "acceptance ok: rascal $version"
