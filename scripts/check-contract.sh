#!/usr/bin/env bash
# Warns when crates.io has a newer table-manners than the one pinned in Cargo.toml (ADR 0020).
# Never fails: moving the pin is a deliberate commit. Needs the network, so it runs in CI, not in
# `just check`; offline it says so and exits 0. CONTRACT_LATEST overrides the lookup (self-test).
set -euo pipefail
cd "$(dirname "$0")/.."
pinned="$(sed -n 's/^table-manners = "=\([0-9.]*\)"$/\1/p' Cargo.toml)"
if [ -z "$pinned" ]; then
  echo "check-contract: no exact table-manners pin in Cargo.toml" >&2
  exit 1
fi
latest="${CONTRACT_LATEST:-}"
if [ -z "$latest" ]; then
  latest="$(curl -fsS --max-time 15 -A 'rascal-ci (https://github.com/markhayden/rascal)' \
    https://crates.io/api/v1/crates/table-manners 2>/dev/null \
    | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["crate"]["max_stable_version"] or "")' 2>/dev/null || true)"
fi
if [ -z "$latest" ]; then
  echo "check-contract: crates.io not reachable; pinned table-manners $pinned, not compared"
  exit 0
fi
newest="$(printf '%s\n%s\n' "$pinned" "$latest" | sort -V | tail -1)"
if [ "$newest" != "$pinned" ]; then
  echo "::warning title=table-manners::the contract has $latest; rascal pins $pinned. Moving the pin is a deliberate commit (ADR 0020)."
  echo "check-contract: newer contract $latest available (pinned $pinned)"
else
  echo "check-contract: table-manners $pinned is the newest"
fi
