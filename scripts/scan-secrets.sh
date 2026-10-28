#!/usr/bin/env bash
# Scans text on stdin for credential-shaped values. The exact values in
# scripts/synthetic-tokens.txt are allowed, and each of them must contain "SYNTHETIC" so the
# allowlist can never launder a real key. Any other match fails; the report names the pattern
# and the line number, never the value.
#
# Byte-wise (LC_ALL=C, grep -a) so a key next to non-UTF-8 bytes or inside a binary blob is
# still seen. Known limit: a value split across lines or base64-encoded is not detected.
set -euo pipefail
export LC_ALL=C
here="$(cd "$(dirname "$0")" && pwd)"
allowlist="$here/synthetic-tokens.txt"

patterns=(
  'anthropic:sk-ant-[A-Za-z0-9_-]{24,}'
  'google-api:AIza[0-9A-Za-z_-]{35}'
  'google-oauth:GOCSPX-[A-Za-z0-9_-]{28}'
  'openai-project:sk-proj-[A-Za-z0-9_-]{24,}'
  'openai-service:sk-svcacct-[A-Za-z0-9_-]{24,}'
  'openai-legacy:sk-[A-Za-z0-9]{48}'
  'aws-access-key:AKIA[0-9A-Z]{16}'
  'github-token:gh[opusr]_[A-Za-z0-9]{36}'
  'github-pat:github_pat_[A-Za-z0-9_]{82}'
  'slack-token:xox[abp]-[A-Za-z0-9-]{20,}'
  'private-key:-----BEGIN [A-Z ]*PRIVATE KEY-----'
)

while IFS= read -r entry; do
  [ -z "$entry" ] && continue
  case "$entry" in \#*) continue ;; esac
  case "$entry" in *SYNTHETIC*) ;; *)
    echo "synthetic-tokens.txt has an entry without SYNTHETIC: refusing to trust the allowlist" >&2
    exit 2 ;;
  esac
done < "$allowlist"

input="$(mktemp)"
trap 'rm -f "$input"' EXIT
cat > "$input"

status=0
for spec in "${patterns[@]}"; do
  name="${spec%%:*}"
  pattern="${spec#*:}"
  matches="$(grep -naoE -- "$pattern" "$input" || true)"
  [ -z "$matches" ] && continue
  while IFS= read -r hit; do
    line="${hit%%:*}"
    value="${hit#*:}"
    grep -qxF -- "$value" "$allowlist" && continue
    echo "credential-shaped value ($name) at input line $line" >&2
    status=1
  done <<< "$matches"
done
exit $status
