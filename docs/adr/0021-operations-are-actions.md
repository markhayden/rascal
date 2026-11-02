# 0021: Operations are actions

Date: 2026-11-01. Status: accepted (2026-11-01). Supersedes in part ADR 0015 (its op surface;
its transport stands). Amends ADR 0020; ADR 0022 gains its MCP tools.

## Context

table-manners v0.4 (CONTRACT 4.7 to 4.9, 5.1, 5.2) lets a host test one step at a time: every
built-in op becomes an action a host can call with typed ports, and the artifacts a call makes
travel between calls, servers, workspaces and machines by reference. Until M10 rascal served its
ops twice: as MCP tools with rascal's own envelope and workspace ids in their arguments (ADR
0015), and through the recipes as the contract's tasks. A host had to learn rascal's ids, its
envelope and its `workspace_*` tools, a path could name a mask, and nothing told a host which
artifact types it may hold or how one reaches another store.

## Decision

- **Every op is an action.** The description lists each built-in op (all but the pack's
  `segment`) as an operation with `kind`, a `group` from `vocabulary.group` (`Op::group`) and
  `ports` typed by the contract's names; the recipes stay tasks. `artifact_types` names what a
  host may hold (`location`, `rascal:Mask`, `rascal:BandProfile`, `rascal:Series`,
  `rascal:Plan`), whether each travels (`portable`) and whether it has a preview.
- **Artifacts by reference and identity.** An artifact crosses as a reference (type, schema
  version, tool, identity, frame identity, `on`, resource), never a workspace id or a path. It
  resolves by identity in the session's store, `serve`'s workspace, so a restarted server and a
  moved workspace resolve the same references, and rascal never searches folders.
- **The binding refusal.** Before any run, every reference is checked (type, version,
  resolution) and the inputs' frames together (`Op::check_frames`); every problem goes in one
  `-32602` `binding` refusal, the call's `client_run_id` released. `estimate` binds by the same
  function, so it refuses the same way.
- **Runs commit into the store.** An operation's call is a run: its op runs on the store, its
  outputs commit there and bind at once; a task runs in its own run directory, records the Mask
  it makes as the step completes, and publishes it into the store (with its image and frame
  chain) before `run.json` is written. Both list `artifacts.in` and `artifacts.out`.
- **Previews.** A Mask and a location out carry a preview, an image out drawn for people that
  never stands for the artifact; `rascal://artifact/{identity}` reads any held artifact with its
  preview, drawn once into rascal's cache.
- **Export and import** wrap the mask bundle (ADR 0022) and the contract's location JSON as
  reserved controls; `rascal call` runs any action from an invocation's arguments, and
  `rascal call --bundle` replays a host's invocation bundle checked whole.
- **The op tools and the `workspace_*` tools are removed**, with no shim: `serve` lists the
  description's actions and the reserved tools only.
- **The path rule.** `serve` reads a caller's path only inside its `--allow-dir` directories,
  closed by default; MCP roots are not used.
- **Pack operations are deferred.** A pack's tools are not served as operations; the segmenter
  is reached through `remove_prompt`.

## Consequences

A host needs no knowledge of rascal's ids, envelope or workspace layout: it holds references,
binds them, reads them back and moves them as files. The envelope stays rascal's record of a
step, inside an operation's `body.envelope`. Tests that drove the op tools moved to the
operations (`operations.rs`, `operations_path.rs`); the contract's fixtures at v0.4 come from
rascal's own test (`table_manners_fixtures.rs`). The op CLI (`rascal fan --workspace …`) is
unchanged: ids remain a workspace's own, on the command line. Reversing this means restoring
ADR 0015's op surface on v0.3 (PLAN.md M10 rollback, rule 3 or 4).
