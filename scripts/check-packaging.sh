#!/usr/bin/env bash
# Every asset a crate embeds with include_str! or include_bytes! must sit inside that crate and
# be in the file list `cargo package` ships, or the published crate cannot build. Checks every
# workspace member; CRATE_DIRS (one directory per line) overrides the list for the self-test.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -z "${CRATE_DIRS:-}" ]; then
  CRATE_DIRS="$(cargo metadata --format-version 1 --no-deps \
    | python3 -I -c 'import json, os, sys; m = json.load(sys.stdin); print("\n".join(os.path.dirname(p["manifest_path"]) for p in m["packages"]))')"
fi
status=0
while IFS= read -r crate; do
  [ -n "$crate" ] || continue
  if ! listing="$(cd "$crate" && cargo package --list --allow-dirty 2>&1)"; then
    echo "$crate: cargo package --list failed: $listing" >&2
    status=1
    continue
  fi
  python3 -I - "$crate" "$listing" <<'PY' || status=1
import pathlib, re, sys

crate = pathlib.Path(sys.argv[1]).resolve()
shipped = set(sys.argv[2].split())
include = re.compile(r'include_(?:str|bytes)!\s*\(\s*"([^"]+)"')
problems = 0
assets = set()
for source in sorted(crate.glob("src/**/*.rs")):
    for relative in include.findall(source.read_text()):
        target = (source.parent / relative).resolve()
        try:
            inside = target.relative_to(crate)
        except ValueError:
            print(f"{source}: include of {relative} reaches outside the crate")
            problems += 1
            continue
        if str(inside) not in shipped:
            print(f"{source}: {inside} is embedded but not packaged")
            problems += 1
            continue
        assets.add(str(inside))
if assets:
    print(f"{crate.name}: embeds " + ", ".join(sorted(assets)))
else:
    print(f"{crate.name}: embeds nothing")
sys.exit(1 if problems else 0)
PY
done <<< "$CRATE_DIRS"
[ "$status" -eq 0 ] && echo "packaging ok"
exit "$status"
