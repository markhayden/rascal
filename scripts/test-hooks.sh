#!/usr/bin/env bash
# Proves the credential scanner and the pre-commit hook accept what they must and refuse what
# they must, end to end in a scratch repository. Run by `just check`. Credential-shaped values
# are assembled at run time so this file never contains a full match.
set -euo pipefail
cd "$(dirname "$0")/.."
pass=0; fail=0
ok ()  { pass=$((pass + 1)); }
bad () { echo "FAIL: $1" >&2; fail=$((fail + 1)); }
rep () { printf "$1%.0s" $(seq 1 "$2"); }
scan_ok ()  { if scripts/scan-secrets.sh < "$1" 2>/dev/null; then ok; else bad "scanner should accept: $2"; fi; }
scan_bad () { if scripts/scan-secrets.sh < "$1" 2>/dev/null; then bad "scanner should refuse: $2"; else ok; fi; }

# --- the scanner on the tree itself ----------------------------------------------------------
scan_ok scripts/scan-secrets.sh "the scanner's own source"
scan_ok scripts/hooks/pre-commit "the hook's own source"
scan_ok scripts/synthetic-tokens.txt "the synthetic token list"
scan_ok fixtures/synthetic-secrets/example.env "a fixture using only synthetic values"
tree="$(mktemp)"
git ls-files -z | while IFS= read -r -d '' f; do [ -f "$f" ] && cat "$f" || true; done > "$tree"
scan_ok "$tree" "the whole tracked tree"

# --- the scanner on shaped values -----------------------------------------------------------
tmp="$(mktemp)"
printf 'KEY=sk-ant-%s\n' "$(rep r 30)" > "$tmp";              scan_bad "$tmp" "an Anthropic-shaped key"
printf 'AKIA%s\n' "$(rep Q 16)" > "$tmp";                     scan_bad "$tmp" "an AWS-shaped key id"
printf 'token = "ghp_%s"\n' "$(rep a 36)" > "$tmp";           scan_bad "$tmp" "a GitHub-shaped token"
printf 'KEY=AIza%s\n' "$(rep z 35)" > "$tmp";                 scan_bad "$tmp" "a Google-shaped key"
printf 'KEY=sk-proj-%s\n' "$(rep p 30)" > "$tmp";             scan_bad "$tmp" "an OpenAI-shaped key"
printf 'KEY=xoxb-%s\n' "$(rep 1 24)" > "$tmp";                scan_bad "$tmp" "a Slack-shaped token"
printf 'KEY=\x93sk-ant-%s\x94\n' "$(rep r 30)" > "$tmp";      scan_bad "$tmp" "a key in CP1252 quotes"
{ head -c 64 /dev/urandom; printf 'sk-ant-%s' "$(rep r 30)"; head -c 64 /dev/urandom; } > "$tmp"
scan_bad "$tmp" "a key inside binary bytes"
printf 'nothing to see here\n' > "$tmp";                      scan_ok "$tmp" "a clean change"

# --- the allowlist invariant -----------------------------------------------------------------
fake="$(mktemp -d)"; cp scripts/scan-secrets.sh "$fake/"
printf 'sk-ant-%s\n' "$(rep r 30)" > "$fake/synthetic-tokens.txt"
if printf 'x\n' | "$fake/scan-secrets.sh" 2>/dev/null; then bad "an allowlist entry without SYNTHETIC should be refused"; else ok; fi
rm -rf "$fake"

# --- the hook in a scratch repository --------------------------------------------------------
work="$(mktemp -d)"
words="$(mktemp)"
trap 'rm -rf "$work"; rm -f "$tmp" "$tree" "$words"' EXIT
git -C "$work" init -q
git -C "$work" config user.email test@example.invalid
git -C "$work" config user.name test
mkdir -p "$work/scripts/hooks"
cp scripts/scan-secrets.sh scripts/synthetic-tokens.txt scripts/scan-clean-room.sh "$work/scripts/"
cp scripts/hooks/pre-commit "$work/scripts/hooks/"
git -C "$work" config core.hooksPath scripts/hooks
export RASCAL_HOOK_SKIP_CARGO=1
printf 'zq-planted-forbidden-zq\n' > "$words"
export RASCAL_FORBIDDEN_WORDS="$words"

commit_ok ()  { if git -C "$work" commit -q -m "$1" >/dev/null 2>&1; then ok; else bad "hook should accept: $1"; fi; }
# After a refusal the planted file is removed too, so a later `git add <dir>` cannot pick it up.
commit_bad () {
  if git -C "$work" commit -q -m "$1" >/dev/null 2>&1; then bad "hook should refuse: $1"; else ok; fi
  git -C "$work" reset -q --hard; git -C "$work" clean -fdq
}
# A tiny valid PNG (1x1), written from bytes, for the image guard.
png () { mkdir -p "$(dirname "$1")"; printf '\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89\0\0\0\nIDATx\x9cc\0\x01\0\0\x05\0\x01\r\n\x2d\xb4\0\0\0\0IEND\xaeB`\x82' > "$1"; }

git -C "$work" add -A; commit_ok "the scaffold"
echo hello > "$work/a.txt"; git -C "$work" add a.txt; commit_ok "an ordinary addition"
git -C "$work" rm -q a.txt; commit_ok "a deletion-only commit"
printf 'sk-ant-%s\n' "$(rep r 30)" > "$work/notes.txt"; git -C "$work" add notes.txt; commit_bad "a shaped key in an ordinary file"
mkdir -p "$work/fixtures/synthetic-secrets"
printf 'sk-ant-%s\n' "$(rep r 30)" > "$work/fixtures/synthetic-secrets/x.env"; git -C "$work" add fixtures; commit_bad "a shaped key inside the synthetic fixtures directory"
cp fixtures/synthetic-secrets/example.env "$work/synthetic.env"; git -C "$work" add synthetic.env; commit_ok "the synthetic values"
{ head -c 64 /dev/urandom; printf 'AKIA%s' "$(rep Q 16)"; head -c 64 /dev/urandom; } > "$work/blob.bin"; git -C "$work" add blob.bin; commit_bad "a key inside a binary blob"
png "$work/docs/photo.jpg"; git -C "$work" add docs; commit_bad "an image file outside fixtures/"
png "$work/photo.PNG"; git -C "$work" add photo.PNG; commit_bad "an image file at the root, upper-case extension"
png "$work/fixtures/images/golden.png"; git -C "$work" add fixtures; commit_ok "an image file under fixtures/"
png "$work/.github/assets/readme.png"; git -C "$work" add .github; commit_ok "an image file under .github/assets/"
printf 'the ZQ-PLANTED-FORBIDDEN-ZQ word\n' > "$work/dirty.md"; git -C "$work" add dirty.md; commit_bad "a forbidden word in a staged file"
unset RASCAL_FORBIDDEN_WORDS
printf 'the ZQ-PLANTED-FORBIDDEN-ZQ word\n' > "$work/dirty.md"; git -C "$work" add dirty.md; commit_ok "no word list configured: the clean-room step is skipped"
unset RASCAL_HOOK_SKIP_CARGO

echo "hook tests: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
