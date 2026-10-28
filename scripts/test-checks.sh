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
# In a scratch repository with the scan copied in (the scan's repository is the one it lives
# in), with the word list in a second scratch directory outside it. The planted word is
# nonsense so this file is clean.
planted="zq-planted-forbidden-zq"
outside="$(mktemp -d)"; repo="$(mktemp -d)"
cleanup_clean_room () { rm -rf "$outside" "$repo"; }
trap cleanup_clean_room EXIT
mkdir -p "$repo/scripts"; cp scripts/scan-clean-room.sh "$repo/scripts/"
git -C "$repo" init -q; git -C "$repo" config user.email t@example.invalid; git -C "$repo" config user.name t
scan="$repo/scripts/scan-clean-room.sh"
printf '# comment\n\n%s\n' "$planted" > "$outside/words.txt"
printf 'line one\nthe %s is here\n' "$(tr a-z A-Z <<< "$planted")" > "$repo/dirty.md"
printf 'nothing to see\n' > "$repo/clean.md"
export RASCAL_FORBIDDEN_WORDS="$outside/words.txt"

out="$("$scan" "$repo/dirty.md" "$repo/clean.md" 2>&1 || true)"
if grep -q "forbidden word in $repo/dirty.md at line(s) 2" <<< "$out" && ! grep -qi "$planted" <<< "$out"; then ok
else bad "clean-room scan did not name the dirty file by line without the word: $out"; fi
if "$scan" "$repo/clean.md" >/dev/null 2>&1; then ok; else bad "clean-room scan rejected a clean file"; fi
if "$scan" "$repo/dirty.md" >/dev/null 2>&1; then bad "clean-room scan accepted a planted word"; else ok; fi
out="$(env -u RASCAL_FORBIDDEN_WORDS "$scan" "$repo/dirty.md" 2>&1)" \
  && grep -q 'skipped' <<< "$out" && ok || bad "clean-room scan without a list should skip: $out"
if "$scan" "$repo/missing.md" >/dev/null 2>&1; then bad "clean-room scan accepted a missing file argument"; else ok; fi

# A list with CRLF endings, or a trailing space, must still work; a too-short entry is refused.
printf '%s\r\n' "$planted" > "$outside/crlf.txt"
if RASCAL_FORBIDDEN_WORDS="$outside/crlf.txt" "$scan" "$repo/dirty.md" >/dev/null 2>&1; then
  bad "clean-room scan accepted a planted word because the list had CRLF endings"; else ok; fi
printf '  %s  \n' "$planted" > "$outside/spaces.txt"
printf 'ends with %s\n' "$planted" > "$repo/eol.md"
if RASCAL_FORBIDDEN_WORDS="$outside/spaces.txt" "$scan" "$repo/eol.md" >/dev/null 2>&1; then
  bad "clean-room scan missed a word at end of line because the list entry had spaces"; else ok; fi
printf 'ab\n' > "$outside/short.txt"
out="$(RASCAL_FORBIDDEN_WORDS="$outside/short.txt" "$scan" "$repo/clean.md" 2>&1 || true)"
if grep -q 'shorter than three' <<< "$out"; then ok; else bad "clean-room scan accepted a two-character entry: $out"; fi

# A list inside the repository, directly or through a symlink, is refused.
cp "$outside/words.txt" "$repo/words.txt"
if RASCAL_FORBIDDEN_WORDS="$repo/words.txt" "$scan" "$repo/clean.md" >/dev/null 2>&1; then
  bad "clean-room scan accepted a word list inside the repository"; else ok; fi
ln -s "$repo/words.txt" "$outside/link.txt"
if RASCAL_FORBIDDEN_WORDS="$outside/link.txt" "$scan" "$repo/clean.md" >/dev/null 2>&1; then
  bad "clean-room scan accepted a symlink to a word list inside the repository"; else ok; fi
rm -f "$repo/words.txt" "$outside/link.txt"

# Tracked mode: a committed dirty file fails naming it; an untracked dirty file is not seen;
# a tracked path containing the word fails without printing the word.
git -C "$repo" add clean.md scripts; git -C "$repo" commit -q -m init
if (cd "$repo" && "$scan") >/dev/null 2>&1; then ok; else bad "clean-room tracked mode rejected a clean repository with untracked dirty files"; fi
git -C "$repo" add dirty.md; git -C "$repo" commit -q -m dirty
out="$( (cd "$repo" && "$scan") 2>&1 || true)"
if grep -q 'forbidden word in dirty.md at line(s) 2' <<< "$out"; then ok; else bad "clean-room tracked mode did not name the committed dirty file: $out"; fi
git -C "$repo" rm -q dirty.md; git -C "$repo" commit -q -m clean
printf 'clean\n' > "$repo/notes-$planted.md"; git -C "$repo" add "notes-$planted.md"; git -C "$repo" commit -q -m named
out="$( (cd "$repo" && "$scan") 2>&1 || true)"
if grep -q 'forbidden word in a path (entry' <<< "$out" && ! grep -qi "$planted" <<< "$out"; then ok
else bad "clean-room tracked mode did not report a forbidden path without printing it: $out"; fi
printf 'the %s\n' "$planted" > "$repo/notes-$planted.md"; git -C "$repo" commit -q -am both
out="$( (cd "$repo" && "$scan") 2>&1 || true)"
if grep -q 'its path also matches' <<< "$out" && ! grep -qi "$planted" <<< "$out"; then ok
else bad "clean-room scan printed a path that contains the word: $out"; fi
git -C "$repo" rm -q "notes-$planted.md"; git -C "$repo" commit -q -m removed

# Staged mode scans the index, not the worktree.
printf 'the %s\n' "$planted" > "$repo/a.md"; git -C "$repo" add a.md; printf 'clean\n' > "$repo/a.md"
if (cd "$repo" && "$scan" --staged) >/dev/null 2>&1; then bad "clean-room staged mode missed a staged word hidden by a clean worktree"; else ok; fi
git -C "$repo" reset -q
printf 'clean\n' > "$repo/b.md"; git -C "$repo" add b.md; printf 'the %s\n' "$planted" > "$repo/b.md"
if (cd "$repo" && "$scan" --staged) >/dev/null 2>&1; then ok; else bad "clean-room staged mode rejected a clean index because the worktree was dirty"; fi
unset RASCAL_FORBIDDEN_WORDS
cleanup_clean_room; trap - EXIT

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
