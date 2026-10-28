#!/usr/bin/env bash
# Proves the repository checks fail when they should. Run by `just check`.
set -euo pipefail
cd "$(dirname "$0")/.."
pass=0; fail=0
ok ()  { pass=$((pass + 1)); }
bad () { echo "FAIL: $1" >&2; fail=$((fail + 1)); }

# --- crate graph ------------------------------------------------------------------------------
empty="$(mktemp)"; : > "$empty"
if CRATE_GRAPH_FILE="$empty" scripts/check-crate-graph.sh >/dev/null 2>&1; then
  bad "crate-graph check accepted an empty graph"; else ok; fi
one="$(mktemp)"; grep -v 'rascal-mask -> rascal-ops' scripts/crate-graph.txt > "$one"
out="$(CRATE_GRAPH_FILE="$one" scripts/check-crate-graph.sh 2>&1 || true)"
if grep -q 'unexpected prod edge: rascal-mask -> rascal-ops' <<< "$out"; then ok
else bad "crate-graph check did not name the missing edge: $out"; fi
rm -f "$empty" "$one"

# --- packaging --------------------------------------------------------------------------------
probe="$(mktemp -d)"; mkdir -p "$probe/crate/src" "$probe/crate/assets"
printf 'x = 1\n' > "$probe/outside.toml"; printf 'y = 2\n' > "$probe/crate/assets/a.toml"
printf '[package]\nname = "probe"\nversion = "0.0.0"\nedition = "2024"\nexclude = ["assets/"]\n' > "$probe/crate/Cargo.toml"
printf 'pub const A: &str = include_str!("../../outside.toml");\n' > "$probe/crate/src/lib.rs"
out="$(CRATE_DIRS="$probe/crate" scripts/check-packaging.sh 2>&1 || true)"
if grep -q 'reaches outside the crate' <<< "$out"; then ok
else bad "packaging check accepted an include outside the crate: $out"; fi
printf 'pub const A: &str = include_str!("../assets/a.toml");\n' > "$probe/crate/src/lib.rs"
out="$(CRATE_DIRS="$probe/crate" scripts/check-packaging.sh 2>&1 || true)"
if grep -q 'embedded but not packaged' <<< "$out"; then ok
else bad "packaging check accepted an excluded asset: $out"; fi
printf '[package]\nname = "probe"\nversion = "0.0.0"\nedition = "2024"\n' > "$probe/crate/Cargo.toml"
if CRATE_DIRS="$probe/crate" scripts/check-packaging.sh >/dev/null 2>&1; then ok
else bad "packaging check rejected a packaged asset"; fi
rm -rf "$probe"

# --- clean room -------------------------------------------------------------------------------
# A planted word in a file fails naming the file; a clean file passes; no list means skipped;
# a list inside the repository is refused. The planted word is nonsense so this file is clean.
planted="zq-planted-forbidden-zq"
scratch="$(mktemp -d)"
printf '# comment\n\n%s\n' "$planted" > "$scratch/words.txt"
printf 'line one\nthe %s is here\n' "$(tr a-z A-Z <<< "$planted")" > "$scratch/dirty.md"
printf 'nothing to see\n' > "$scratch/clean.md"
out="$(RASCAL_FORBIDDEN_WORDS="$scratch/words.txt" scripts/scan-clean-room.sh "$scratch/dirty.md" "$scratch/clean.md" 2>&1 || true)"
if grep -q "forbidden word in $scratch/dirty.md at line(s) 2" <<< "$out" && ! grep -q "$planted" <<< "$out"; then ok
else bad "clean-room scan did not name the dirty file (and only its line): $out"; fi
if RASCAL_FORBIDDEN_WORDS="$scratch/words.txt" scripts/scan-clean-room.sh "$scratch/clean.md" >/dev/null 2>&1; then ok
else bad "clean-room scan rejected a clean file"; fi
if RASCAL_FORBIDDEN_WORDS="$scratch/words.txt" scripts/scan-clean-room.sh "$scratch/dirty.md" >/dev/null 2>&1; then
  bad "clean-room scan accepted a planted word"; else ok; fi
out="$(env -u RASCAL_FORBIDDEN_WORDS scripts/scan-clean-room.sh "$scratch/dirty.md" 2>&1)" \
  && grep -q 'skipped' <<< "$out" && ok || bad "clean-room scan without a list should skip: $out"
inside="$(mktemp "$(pwd)/.words-XXXXXX")"; cp "$scratch/words.txt" "$inside"
if RASCAL_FORBIDDEN_WORDS="$inside" scripts/scan-clean-room.sh "$scratch/clean.md" >/dev/null 2>&1; then
  bad "clean-room scan accepted a word list inside the repository"; else ok; fi
rm -f "$inside"; rm -rf "$scratch"

# --- smoke ------------------------------------------------------------------------------------
# A binary with the wrong version fails; one that exits 2 on no arguments fails.
probe="$(mktemp -d)"
cat > "$probe/rascal" <<'STUB'
#!/usr/bin/env bash
case "${1:-}" in
  --version) echo "rascal ${STUB_VERSION-0.1.0}" ;;
  "") echo "Usage: rascal <COMMAND>" >&2; exit "${STUB_EXIT-64}" ;;
esac
STUB
chmod +x "$probe/rascal"
if scripts/smoke.sh "$probe/rascal" >/dev/null 2>&1; then ok; else bad "smoke rejected a conforming stub"; fi
out="$(STUB_VERSION=9.9.9 scripts/smoke.sh "$probe/rascal" 2>&1 || true)"
if grep -q "expected 'rascal 0.1.0'" <<< "$out"; then ok; else bad "smoke accepted a wrong version: $out"; fi
out="$(STUB_EXIT=2 scripts/smoke.sh "$probe/rascal" 2>&1 || true)"
if grep -q 'expected exit 64' <<< "$out"; then ok; else bad "smoke accepted exit 2 on no arguments: $out"; fi
rm -rf "$probe"

echo "check tests: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
