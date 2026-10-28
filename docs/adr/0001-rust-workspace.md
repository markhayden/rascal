# 0001: A Rust library, CLI and MCP server in one Cargo workspace

Date: 2026-10-28. Status: accepted.

## Context

rascal is the find-and-mask step of an image pipeline. It must run as a library inside other
Rust tools, as a command-line program in scripts and CI, and as an MCP server an agent drives.
It processes pixels (distance transforms, morphology, a Poisson solve) at panorama sizes, so
the core must be fast and deterministic, and the binary must install with no system libraries
on macOS arm64 and Linux x86_64. An evaluation harness in the same family already uses Rust
2024 with a thin binary over a core library, and the two should feel alike to work on.

## Decision

Rust, stable, edition 2024, pinned by `rust-toolchain.toml`. One Cargo workspace of five
crates: `rascal-core` (the contracts and records), `rascal-ops` (the pixels), `rascal-mcp` (the
client and server), `rascal-testkit` (fakes and builders) and `rascal-mask` (the `rascal`
binary). Every surface is a thin wrapper over the same functions on the same files. The crate
graph is a checked file (`scripts/crate-graph.txt`), so an edge cannot appear by accident.

## Alternatives

- **Python.** Reuses the ML ecosystem directly, but a mask tool that every pipeline calls must
  not drag a Python runtime behind it; Python stays where the models are, in the tool pack
  (ADR 0012).
- **Go.** Effortless binaries, but thin imaging and no path to a WASM or Swift binding.
- **TypeScript.** The agent ecosystem is TypeScript-first, but there is no small static binary
  and no fast pixel code without native bindings.

## Consequences

- Early iteration is slower than a scripting language; plain structs, enums and boxed trait
  objects keep the code simple enough to pay it back.
- Node, Swift and WASM bindings are later milestones, each a thin crate over the core, and
  nothing in the core assumes a runtime it was not given.
- `just check` runs Clippy with warnings denied on a pinned toolchain, so toolchain bumps are
  deliberate commits.
