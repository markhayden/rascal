#!/usr/bin/env bash
# Compares the workspace's crate-to-crate edges (all features) with the allowed edges in
# scripts/crate-graph.txt (CRATE_GRAPH_FILE overrides it for the self-test). Exits 1 and names
# every edge that is not allowed.
set -euo pipefail
cd "$(dirname "$0")/.."
graph_file="${CRATE_GRAPH_FILE:-scripts/crate-graph.txt}"
metadata="$(mktemp)"
trap 'rm -f "$metadata"' EXIT
cargo metadata --format-version 1 --all-features --no-deps > "$metadata"
python3 -I - "$graph_file" "$metadata" <<'PY'
import json, pathlib, sys

graph_file, metadata_file = sys.argv[1], sys.argv[2]
metadata = json.loads(pathlib.Path(metadata_file).read_text())
members = {package["name"] for package in metadata["packages"]}

actual = set()
for package in metadata["packages"]:
    for dependency in package["dependencies"]:
        if dependency["name"] in members:
            kind = "dev" if dependency["kind"] == "dev" else "prod"
            actual.add((kind, package["name"], dependency["name"]))

allowed = set()
for line in pathlib.Path(graph_file).read_text().splitlines():
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    kind, edge = line.split(" ", 1)
    source, target = (part.strip() for part in edge.split("->"))
    allowed.add((kind, source, target))

unexpected = sorted(actual - allowed)
for kind, source, target in unexpected:
    print(f"unexpected {kind} edge: {source} -> {target}")
sys.exit(1 if unexpected else 0)
PY
echo "crate graph ok"
