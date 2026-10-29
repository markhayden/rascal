# ADR 0019: the pack protocol

Decided 2026-10-29 (M3 task 3.3). Status: accepted.

## Context

A pack is an MCP server rascal starts and calls (SPEC.md "The MCP server and client"). Three
things the spec left to the implementation: how a pack's tool carries rascal's typed ports and
parameters when the manifest shape is the copied one (closed, with no room for them); how a
finder's variable number of instances fits an envelope whose `artifacts` are declared ports;
and what a replayed run does about pack calls it cannot make.

## Decision

- **Manifests ride in the tool's `_meta`.** A listed tool carries `_meta.manifest` in the
  copied shape (`schemas/copied/manifest-v1.json`, so a pack written for the other harness in
  the family lists unchanged) and `_meta.rascal`, the annotation block
  (`schemas/rascal/pack-annotation-v1.json`: `inputs`, `outputs`, `params`, `constants`).
  rascal builds the native `Manifest` from the two, runs the registry's checks on it and
  refuses bounds over its ceilings (`rascal_core::pack::check_bounds`). Both are closed types
  and, in a checkout, validated against their schemas too.
- **Many ports.** An output `Port` may be `many`: its artifacts are `<port>_0`, `<port>_1`, …
  and required means at least one. The derived output schema names them as
  `patternProperties`. A pack tool's regions land on its many `Location` port and its images on
  its many `Image` port, in order; a single port of the type takes exactly one. Input ports are
  never many in v1; `select` (M4) will bind a many output by prefix.
- **Replay never calls.** At startup the listing is journaled (`pack manifest:`), and each
  successful call journals its request identity (`sha256` over pack, tool, arguments and input
  identities), envelope and outputs (`pack stored:`). Under `--replay`, a pack is constructed
  from the recorded listing without a process, and a call is answered from the matching stored
  call (same identity, pack name and revision) or fails naming the request. The recording
  carries no pixels: outputs are loaded from the recorded run by identity.
- **The escalation is rascal's.** The process is spawned by rascal (not by the SDK's
  child-process transport, which kills on drop), so stdin close, SIGTERM (`nix`) and SIGKILL
  happen in the spec's order with the spec's waits, and the child is waited for exactly once.
  Every call carries the SDK's per-request timeout; a failure cancels an epoch token every
  in-flight call on the pack selects on, so they all return before the shutdown.

## Alternatives

- **Rascal's whole manifest in `_meta`, the copied one ignored.** One document instead of two,
  but a pack would then be rascal-only; the copied shape is what the family shares.
- **Numbered output ports declared up front (`plane_0` … `plane_7`).** Fixes the maximum in the
  manifest and clutters every listing; `many` says the same thing once.
- **Replay falling back to a live call.** Convenient, but it would make a replayed run depend
  on an executable the spec says may be absent, and silently call a model under a flag that
  promises not to.

## Consequences

- `schemas/rascal/manifest-v1.json` gains `many`; `config-v1.json` gains `packs`;
  `pack-annotation-v1.json` and `pack-context-v1.json` are new boundary types.
- `nix` (signal only) and `tokio-util` join the dependency list.
- The Python pack (M5) lists its tools with both `_meta` blocks and reads `_harness`; the fake
  pack in the testkit is the reference.
