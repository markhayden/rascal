#!/usr/bin/env bash
# The clean-room scan. Reads the forbidden-word list named by RASCAL_FORBIDDEN_WORDS (one word or
# phrase per line; blank lines and # comments ignored; CR and surrounding whitespace stripped)
# and matches it case-insensitively as fixed strings, byte-wise (LC_ALL=C: ASCII case folding;
# a non-ASCII word matches exactly), so binary files are seen too. Three modes:
#
#   scripts/scan-clean-room.sh                 every tracked file's contents and every tracked path
#   scripts/scan-clean-room.sh --staged        every staged blob (the index, not the worktree) and
#                                              every staged path; what the pre-commit hook runs
#   scripts/scan-clean-room.sh <file>...       the given files' contents (a missing file is an error)
#
# A hit fails with the file and line numbers. The matched word is never printed: a path that
# itself contains a word is reported by its position, not its name, so a log can be shared.
# With the variable unset the scan passes and says so: CI has no list. The list must live outside
# the repository (symlinks resolved); a list inside the tree is refused so it can never be committed.
set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

list="${RASCAL_FORBIDDEN_WORDS:-}"
if [ -z "$list" ]; then
  echo "clean-room scan: skipped (RASCAL_FORBIDDEN_WORDS is unset)"
  exit 0
fi
if [ ! -r "$list" ]; then
  echo "clean-room scan: word list $list is not readable" >&2
  exit 2
fi
top="$(git rev-parse --show-toplevel)"
real="$(python3 -I -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$list")"
case "$real/" in
  "$top"/*) echo "clean-room scan: the word list must live outside the repository" >&2; exit 2 ;;
esac

words="$(mktemp)"
blob="$(mktemp)"
trap 'rm -f "$words" "$blob"' EXIT
tr -d '\r' < "$list" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//' | grep -v -e '^$' -e '^#' > "$words" || true
if [ ! -s "$words" ]; then
  echo "clean-room scan: word list is empty" >&2
  exit 2
fi
if awk 'length($0) < 3 { bad = 1 } END { exit bad }' "$words"; then :; else
  echo "clean-room scan: word list has an entry shorter than three characters" >&2
  exit 2
fi

hits=0
# scan_content <file-to-read> <label> <position>: report by label unless the label itself matches.
scan_content () {
  local lines
  lines="$(grep -n -a -i -F -f "$words" -- "$1" | cut -d: -f1 | tr '\n' ' ' || true)"
  [ -n "$lines" ] || return 0
  if printf '%s' "$2" | grep -q -a -i -F -f "$words"; then
    echo "forbidden word in the contents of entry $3 (its path also matches) at line(s) $lines" >&2
  else
    echo "forbidden word in $2 at line(s) $lines" >&2
  fi
  hits=$((hits + 1))
}
# scan_path <path> <position>
scan_path () {
  if printf '%s' "$1" | grep -q -a -i -F -f "$words"; then
    echo "forbidden word in a path (entry $2 of the listing)" >&2
    hits=$((hits + 1))
  fi
}

n=0
if [ "$#" -eq 0 ]; then
  while IFS= read -r -d '' file; do
    n=$((n + 1))
    scan_path "$file" "$n"
    [ -f "$file" ] && scan_content "$file" "$file" "$n"
  done < <(git ls-files -z)
elif [ "$1" = "--staged" ]; then
  while IFS= read -r -d '' file; do
    n=$((n + 1))
    scan_path "$file" "$n"
    git show ":$file" > "$blob"
    scan_content "$blob" "$file" "$n"
  done < <(git diff --cached --name-only -z --diff-filter=ACMR)
else
  for file in "$@"; do
    n=$((n + 1))
    if [ ! -f "$file" ]; then
      echo "clean-room scan: $file is not a file" >&2
      exit 2
    fi
    scan_content "$file" "$file" "$n"
  done
fi
if [ "$hits" -gt 0 ]; then
  echo "clean-room scan: $hits hit(s)" >&2
  exit 1
fi
echo "clean-room scan ok"
