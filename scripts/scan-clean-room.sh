#!/usr/bin/env bash
# The clean-room scan. Reads the forbidden-word list named by RASCAL_FORBIDDEN_WORDS (one word or
# phrase per line, matched case-insensitively as fixed strings, byte-wise so binary files are
# seen too) and scans every tracked file, or the files given as arguments. A hit fails with the
# file and line number; the word itself is never printed, so a log can be shared.
#
# With the variable unset the scan passes and says so: CI has no list. The list must live
# outside the repository; a list inside the tree is refused so it can never be committed.
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
case "$(cd "$(dirname "$list")" && pwd -P)/" in
  "$top"/*) echo "clean-room scan: the word list must live outside the repository" >&2; exit 2 ;;
esac

words="$(mktemp)"
trap 'rm -f "$words"' EXIT
grep -v -e '^[[:space:]]*$' -e '^#' "$list" > "$words" || true
if [ ! -s "$words" ]; then
  echo "clean-room scan: word list is empty" >&2
  exit 2
fi

hits=0
scan () {
  local file="$1"
  [ -f "$file" ] || return 0
  local lines
  lines="$(grep -n -a -i -F -f "$words" -- "$file" | cut -d: -f1 | tr '\n' ' ' || true)"
  if [ -n "$lines" ]; then
    echo "forbidden word in $file at line(s) $lines" >&2
    hits=$((hits + 1))
  fi
}
if [ "$#" -gt 0 ]; then
  for file in "$@"; do scan "$file"; done
else
  while IFS= read -r -d '' file; do scan "$file"; done < <(git ls-files -z)
fi
if [ "$hits" -gt 0 ]; then
  echo "clean-room scan: $hits file(s) contain a forbidden word" >&2
  exit 1
fi
echo "clean-room scan ok"
