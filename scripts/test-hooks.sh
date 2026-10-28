#!/usr/bin/env bash
# Proves the credential scanner, the image guard, the clean-room step and the two hooks accept
# what they must and refuse what they must, end to end in a scratch repository. Run by
# `just check`. Credential-shaped values are assembled at run time so this file never contains
# a full match. Every refusal is asserted by the refusing guard's own message, so a refusal for
# the wrong reason is a failure.
set -euo pipefail
cd "$(dirname "$0")/.."
tmp="$(mktemp)"; tree="$(mktemp)"; work="$(mktemp -d)"; outside="$(mktemp -d)"
trap 'rm -rf "$work" "$outside"; rm -f "$tmp" "$tree"' EXIT
pass=0; fail=0
ok ()  { pass=$((pass + 1)); }
bad () { echo "FAIL: $1" >&2; fail=$((fail + 1)); }
rep () { printf "$1%.0s" $(seq 1 "$2"); }
scan_ok ()  { if scripts/scan-secrets.sh < "$1" 2>/dev/null; then ok; else bad "scanner should accept: $2"; fi; }
scan_bad () {
  local out
  if out="$(scripts/scan-secrets.sh < "$1" 2>&1)"; then bad "scanner should refuse: $2"; return; fi
  if grep -q "credential-shaped value ($3)" <<< "$out" && ! grep -qF -- "$4" <<< "$out"; then ok
  else bad "scanner refused '$2' without naming $3, or printed the value: $out"; fi
}

# --- the scanner on the tree itself ----------------------------------------------------------
scan_ok scripts/scan-secrets.sh "the scanner's own source"
scan_ok scripts/hooks/pre-commit "the hook's own source"
scan_ok scripts/synthetic-tokens.txt "the synthetic token list"
scan_ok fixtures/synthetic-secrets/example.env "a fixture using only synthetic values"
git ls-files -z | while IFS= read -r -d '' f; do [ -f "$f" ] && cat "$f" || true; done > "$tree"
scan_ok "$tree" "the whole tracked tree"

# --- the scanner on shaped values: refused, named by pattern, value never printed -------------
v="sk-ant-$(rep r 30)";        printf 'KEY=%s\n' "$v" > "$tmp";          scan_bad "$tmp" "an Anthropic-shaped key" anthropic "$v"
v="AKIA$(rep Q 16)";           printf '%s\n' "$v" > "$tmp";              scan_bad "$tmp" "an AWS-shaped key id" aws-access-key "$v"
v="ghp_$(rep a 36)";           printf 'token = "%s"\n' "$v" > "$tmp";    scan_bad "$tmp" "a GitHub-shaped token" github-token "$v"
v="github_pat_$(rep b 82)";    printf '%s\n' "$v" > "$tmp";              scan_bad "$tmp" "a GitHub fine-grained token" github-pat "$v"
v="AIza$(rep z 35)";           printf 'KEY=%s\n' "$v" > "$tmp";          scan_bad "$tmp" "a Google-shaped key" google-api "$v"
v="GOCSPX-$(rep g 28)";        printf 'secret=%s\n' "$v" > "$tmp";       scan_bad "$tmp" "a Google OAuth secret" google-oauth "$v"
v="sk-proj-$(rep p 30)";       printf 'KEY=%s\n' "$v" > "$tmp";          scan_bad "$tmp" "an OpenAI project key" openai-project "$v"
v="sk-svcacct-$(rep s 30)";    printf 'KEY=%s\n' "$v" > "$tmp";          scan_bad "$tmp" "an OpenAI service key" openai-service "$v"
v="sk-$(rep k 48)";            printf 'KEY=%s\n' "$v" > "$tmp";          scan_bad "$tmp" "a legacy OpenAI key" openai-legacy "$v"
v="xoxb-$(rep 1 24)";          printf 'KEY=%s\n' "$v" > "$tmp";          scan_bad "$tmp" "a Slack bot token" slack-token "$v"
v="xoxp-$(rep 2 24)";          printf 'KEY=%s\n' "$v" > "$tmp";          scan_bad "$tmp" "a Slack user token" slack-token "$v"
v="$(printf -- '-----BEGIN %s KEY-----' 'RSA PRIVATE')"; printf '%s\nabc\n' "$v" > "$tmp"; scan_bad "$tmp" "a private key header" private-key "$v"
v="sk-ant-$(rep r 30)";        printf 'KEY=\x93%s\x94\n' "$v" > "$tmp";  scan_bad "$tmp" "a key in CP1252 quotes" anthropic "$v"
{ printf '\x00\x01\x02\xff\xfe\x89PNG\x00'; printf '%s' "$v"; printf '\x00\xff\x00\x01'; } > "$tmp"
scan_bad "$tmp" "a key inside binary bytes" anthropic "$v"
printf 'KEY=%s-suffix\n' "$(head -1 fixtures/synthetic-secrets/example.env | cut -d= -f2)" > "$tmp"
v="$(grep -m1 '^sk-ant' scripts/synthetic-tokens.txt)"; printf 'KEY=%sX\n' "$v" > "$tmp"
scan_bad "$tmp" "a synthetic value with a character appended" anthropic "${v}X"
printf 'nothing to see here\n' > "$tmp";                                 scan_ok "$tmp" "a clean change"

# --- the allowlist invariants ----------------------------------------------------------------
fake="$(mktemp -d)"; cp scripts/scan-secrets.sh "$fake/"
printf 'sk-ant-%s\n' "$(rep r 30)" > "$fake/synthetic-tokens.txt"
if printf 'x\n' | "$fake/scan-secrets.sh" 2>/dev/null; then bad "an allowlist entry without SYNTHETIC should be refused"; else ok; fi
rm -f "$fake/synthetic-tokens.txt"
if printf 'x\n' | "$fake/scan-secrets.sh" 2>/dev/null; then bad "a missing allowlist should stop the scanner"; else ok; fi
rm -rf "$fake"

# --- the hooks in a scratch repository -------------------------------------------------------
git -C "$work" init -q
git -C "$work" config user.email test@example.invalid
git -C "$work" config user.name test
git -C "$work" config commit.gpgsign false
mkdir -p "$work/scripts/hooks"
cp scripts/scan-secrets.sh scripts/synthetic-tokens.txt scripts/scan-clean-room.sh "$work/scripts/"
cp scripts/hooks/pre-commit scripts/hooks/commit-msg "$work/scripts/hooks/"
git -C "$work" config core.hooksPath scripts/hooks
export RASCAL_HOOK_SKIP_CARGO=1
planted="zq-planted-forbidden-zq"
printf '%s\n' "$planted" > "$outside/words.txt"
export RASCAL_FORBIDDEN_WORDS="$outside/words.txt"

commit_ok () { if git -C "$work" commit -q -m "$1" >/dev/null 2>&1; then ok; else bad "hook should accept: $1"; fi; }
# commit_bad <description> <the refusing guard's message>: refused, and for the stated reason.
# Afterwards the index and the planted files are removed so no later case inherits them.
commit_bad () {
  local out
  if out="$(git -C "$work" commit -q -m "$1" 2>&1)"; then bad "hook should refuse: $1"
  elif grep -q "$2" <<< "$out"; then ok
  else bad "hook refused '$1' but not with '$2': $out"; fi
  git -C "$work" reset -q --hard; git -C "$work" clean -fdq
}
# A tiny valid PNG (1x1), written from bytes, for the image guard.
png () { mkdir -p "$(dirname "$1")"; printf '\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89\0\0\0\nIDATx\x9cc\0\x01\0\0\x05\0\x01\r\n\x2d\xb4\0\0\0\0IEND\xaeB`\x82' > "$1"; }

git -C "$work" add -A; commit_ok "the scaffold"
echo hello > "$work/a.txt"; git -C "$work" add a.txt; commit_ok "an ordinary addition"
git -C "$work" rm -q a.txt; commit_ok "a deletion-only commit"

# credentials
printf 'sk-ant-%s\n' "$(rep r 30)" > "$work/notes.txt"; git -C "$work" add notes.txt
commit_bad "a shaped key in an ordinary file" "notes.txt: credential-shaped value"
mkdir -p "$work/fixtures/synthetic-secrets"
printf 'sk-ant-%s\n' "$(rep r 30)" > "$work/fixtures/synthetic-secrets/x.env"; git -C "$work" add fixtures
commit_bad "a shaped key inside the synthetic fixtures directory" "credential-shaped value"
cp fixtures/synthetic-secrets/example.env "$work/synthetic.env"; git -C "$work" add synthetic.env; commit_ok "the synthetic values"
{ printf '\x00\x01\xff\xfe'; printf 'AKIA%s' "$(rep Q 16)"; printf '\x00\xff'; } > "$work/blob.bin"; git -C "$work" add blob.bin
commit_bad "a key inside a binary blob" "blob.bin: credential-shaped value"
printf 'AKIA%s\n' "$(rep Q 16)" > "$work/staged.txt"; git -C "$work" add staged.txt; printf 'clean\n' > "$work/staged.txt"
commit_bad "a shaped key staged but removed from the worktree" "staged.txt: credential-shaped value"

# images
png "$work/docs/photo.jpg"; git -C "$work" add docs; commit_bad "an image file outside fixtures/" "image file outside"
png "$work/photo.PNG"; git -C "$work" add photo.PNG; commit_bad "an image at the root, upper-case extension" "image file outside"
png "$work/docs/a/b/photo.HEIC"; git -C "$work" add docs; commit_bad "a nested HEIC" "image file outside"
png "$work/fixtures-old/x.png"; git -C "$work" add fixtures-old; commit_bad "an image under a directory that only starts with fixtures" "image file outside"
png "$work/src/fixtures/x.png"; git -C "$work" add src; commit_bad "an image under a nested fixtures/ directory" "image file outside"
png "$work/docs/my photo.jpg"; git -C "$work" add docs; commit_bad "an image with a space in its name" "image file outside"
png "$work/fixtures/images/golden.png"; git -C "$work" add fixtures; commit_ok "an image file under fixtures/"
png "$work/.github/assets/readme.png"; git -C "$work" add .github; commit_ok "an image file under .github/assets/"
mkdir -p "$work/docs"; git -C "$work" mv fixtures/images/golden.png docs/golden.png; commit_bad "an image moved out of fixtures/" "image file outside"
git -C "$work" rm -q fixtures/images/golden.png; commit_ok "deleting an image"

# clean room: staged blobs, staged paths and the commit message
printf 'the ZQ-PLANTED-FORBIDDEN-ZQ word\n' > "$work/dirty.md"; git -C "$work" add dirty.md
commit_bad "a forbidden word in a staged file" "forbidden word in dirty.md"
printf 'the %s word\n' "$planted" > "$work/hidden.md"; git -C "$work" add hidden.md; printf 'clean\n' > "$work/hidden.md"
commit_bad "a forbidden word staged but removed from the worktree" "forbidden word in hidden.md"
printf 'clean\n' > "$work/about-$planted.md"; git -C "$work" add "about-$planted.md"
commit_bad "a forbidden word in a staged file name" "forbidden word in a path"
printf 'clean\n' > "$work/msg.md"; git -C "$work" add msg.md
commit_bad "a forbidden word in the commit message: $planted" "forbidden word in"
printf 'clean\n' > "$work/msg.md"; git -C "$work" add msg.md; commit_ok "a clean file and message"
unset RASCAL_FORBIDDEN_WORDS
printf 'the ZQ-PLANTED-FORBIDDEN-ZQ word\n' > "$work/dirty.md"; git -C "$work" add dirty.md
commit_ok "no word list configured: the clean-room step is skipped"
unset RASCAL_HOOK_SKIP_CARGO

echo "hook tests: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
