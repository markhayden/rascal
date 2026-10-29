#!/usr/bin/env bash
# Docs coverage, both ways. Every op in the catalogue, every rendering profile, every run-page
# panel, every measure and every recipe output field is named in docs/; and every name under the documented
# tables exists in the registry, so a stale name fails too. Runs the debug binary; `cargo build` first if needed.
# DOCS_DIR and RASCAL_BIN override the locations for the self-test.
set -euo pipefail
cd "$(dirname "$0")/.."
docs="${DOCS_DIR:-docs}"
bin="${RASCAL_BIN:-target/debug/rascal}"
[ -x "$bin" ] || cargo build -q -p rascal-mask
status=0

# --- ops: the catalogue against docs/tools.md's table -----------------------------------------
ops_json="$("$bin" ops list --json)"
catalogue="$(python3 -I -c 'import json,sys; print("\n".join(sorted(m["name"] for m in json.load(sys.stdin))))' <<< "$ops_json")"
documented="$(grep -E '^\| `[a-z_]+` \| (find|refine|prep|out|inspect) \|' "$docs/tools.md" | sed -E 's/^\| `([a-z_]+)`.*/\1/' | sort -u)"
for op in $catalogue; do
  grep -qx "$op" <<< "$documented" || { echo "docs: op '$op' is in the catalogue but not in $docs/tools.md's table" >&2; status=1; }
done
for name in $documented; do
  grep -qx "$name" <<< "$catalogue" || { echo "docs: '$name' is in $docs/tools.md's table but not in the catalogue" >&2; status=1; }
done

# --- rendering profiles: render's choices against docs/masks.md's table -----------------------------
profiles="$(python3 -I -c 'import json,sys
ms=json.load(sys.stdin); r=next(m for m in ms if m["name"]=="render"); p=next(p for p in r["params"] if p["name"]=="for")
print("\n".join(sorted(p["choices"])))' <<< "$ops_json")"
documented_profiles="$(grep -E '^\| `[a-z]+` \| `?(mask_input|image_input|inspection)`? \|' "$docs/masks.md" | sed -E 's/^\| `([a-z]+)`.*/\1/' | sort -u)"
for p in $profiles; do
  grep -qx "$p" <<< "$documented_profiles" || { echo "docs: rendering profile '$p' is not in $docs/masks.md's table" >&2; status=1; }
done
for p in $documented_profiles; do
  grep -qx "$p" <<< "$profiles" || { echo "docs: '$p' is in $docs/masks.md's profile table but render has no such profile" >&2; status=1; }
done

# --- panels: the registry against docs/diagnostics.md's table -----------------------------------
panels="$("$bin" dash --panels | sort -u)"
documented_panels="$(grep -E '^\| `[a-z_]+` \| .* \| [0-9]+\.[0-9]+ \|$' "$docs/diagnostics.md" | sed -E 's/^\| `([a-z_]+)`.*/\1/' | sort -u)"
for p in $panels; do
  grep -qx "$p" <<< "$documented_panels" || { echo "docs: panel '$p' is registered but not in $docs/diagnostics.md's table" >&2; status=1; }
done
for p in $documented_panels; do
  grep -qx "$p" <<< "$panels" || { echo "docs: '$p' is in $docs/diagnostics.md's panel table but no such panel is registered" >&2; status=1; }
done

# --- measures: the registry against docs/measures.md's table ---------------------------------
measures="$("$bin" measure --list | sort -u)"
documented_measures="$(grep -E '^\| `[a-z_]+` \| .* \| .* \| .* \|$' "$docs/measures.md" | sed -E 's/^\| `([a-z_]+)`.*/\1/' | sort -u)"
for m in $measures; do
  grep -qx "$m" <<< "$documented_measures" || { echo "docs: measure '$m' is registered but not in $docs/measures.md's table" >&2; status=1; }
done
for m in $documented_measures; do
  grep -qx "$m" <<< "$measures" || { echo "docs: '$m' is in $docs/measures.md's table but no such measure is registered" >&2; status=1; }
done

# --- recipe outputs and config keys: the schemas against the pages -----------------------------------
for key in mask image plan render; do
  grep -q "\`$key\`\|^$key = \|\b$key = \"" "$docs/recipes.md" || { echo "docs: recipe output '$key' is not named in $docs/recipes.md" >&2; status=1; }
done
for key in workspace corpus; do
  grep -q "\[$key\]\|\`$key\`" "$docs/recipes.md" "$docs/workspace.md" "$docs/lab.md" 2>/dev/null || { echo "docs: config section '$key' is not named in docs/" >&2; status=1; }
done

[ "$status" -eq 0 ] && echo "docs ok: $(wc -w <<< "$catalogue" | tr -d ' ') ops, $(wc -w <<< "$profiles" | tr -d ' ') profiles, $(wc -w <<< "$panels" | tr -d ' ') panels, $(wc -w <<< "$measures" | tr -d ' ') measures"
exit "$status"
