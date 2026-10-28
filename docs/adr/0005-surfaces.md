# 0005: Surfaces in v1: a library, a CLI and an MCP server

Date: 2026-10-28. Status: accepted.

## Context

Three consumers exist from day one: a Rust pipeline that links the library, a shell or CI that
runs commands, and an agent that calls tools over MCP. Node, Swift and WASM consumers are
plausible later. Every surface must behave identically, and none may hold state the others
cannot see.

## Decision

v1 ships the library (`rascal-core`, `rascal-ops`), the CLI (`rascal`) and the MCP server
(`rascal serve`). Files are the state: every op reads and writes files, and each surface is a
thin wrapper over the same functions on the same files. The server takes ordinary arguments
(workspace ids and parameters), never an injected object, so a client needs nothing but the
manifests. Node (`@markhayden/rascal` over napi), Swift (uniffi) and WASM are later milestones,
each a thin crate over the core.

## Alternatives

- **A long-running daemon with in-memory state.** Faster for an agent's tight loop, but every
  other surface would have to talk to it, and a crash would lose work that files keep.
- **CLI only.** Leaves agents to shell out and parse text; the manifests exist precisely so
  they need not.

## Consequences

- A run is reproducible from its directory alone, which therefore holds every input it used.
- Concurrency is a workspace concern (generations, staging, an atomic commit), not a surface
  concern.
