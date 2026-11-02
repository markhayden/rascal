# 0020: rascal conforms to table-manners, the shared contract of the waffle tools

Status: accepted 2026-10-30 (owner decision 13).

## Context

The owner is building several tools meant to sit in one agentic pipeline: squint checks an image
against what was asked, rascal finds, masks and composites regions, and more follow, with one
shared playground over all of them. Until now the two tools shared only copied schemas (ADR 0009),
and those copies had gone stale unnoticed. An agent chaining them had to rewrite image references
and locations by hand between calls, and squint had no MCP server at all.

On 2026-10-30 the owner created a neutral repository, `table-manners`
(https://github.com/markhayden/table-manners, tagged v0.1.0, crate `table-manners` 0.1.0 on
crates.io), holding the contract's text, its JSON Schemas, a Rust crate of serde types and the
embedded schemas with no logic, examples and pixel-digest test vectors. The contract has a core
every tool follows (the result header, the description document, long runs, the MCP server
conventions, exit codes) and an images profile (the image reference, the location, images in and
out). `rascal serve` was its reference for the server conventions, and rascal's pixel digest is its
digest recipe byte for byte; rascal reproduces all twelve vectors.

## Decision

rascal conforms to table-manners v0.1, core and images profile, as milestone M8 (PLAN.md). The
crate `table-manners` is a dependency of `rascal-core` and `rascal-mask`, pinned exactly
(`=0.1.0`); it carries types and schemas only, so this is not a code dependency between tools
(ADR 0009's rule stands: rascal depends on no tool). A CI step warns when the contract has a newer
tag; moving to it is a deliberate commit.

What changes in rascal, in the contract's words:

- the image reference (file digest, dimensions, projection, topology) on every image in and out;
- the contract's location on every location that crosses rascal's boundary, with `parts`,
  `straddles_wrap`, `reason` on `absent`, `score` and `score_kind` on the segmenter's findings, and
  a `mask` kind that carries rascal's greyscale rendering out;
- `rascal describe --json` and the `describe` MCP tool: one action per shipped recipe;
- `run.json` with the shared header, written atomically and last, in every run directory;
- in `serve`: one MCP tool per action beside the op tools, images by path or inline as an implicit
  journaled import, results with `isError` for `invalid` and `error`, progress as a heartbeat,
  cancellation that leaves `run.json`, run resources and the runs list, a per-call cap and a
  per-session budget;
- a fill cache keyed by the fill request's identity, so a retry never pays twice;
- exit codes 130 and 143 on SIGINT and SIGTERM;
- a `playground` subcommand that starts the shared playground program and carries no UI (since
  M9, Wafflehouse by its launcher agreement: `wafflehouse --tool rascal`, decision 14).

What does not change: the op tools (ADR 0015) keep their envelopes and their error mapping in
v0.1; the copied `manifest-v1` and `envelope-v1` schemas (ADR 0009) stay, because they are the
pack-facing shapes inside rascal, and the contract's result carries the envelope as its body; the
workspace and its ids stay as rascal's own convenience.

## Alternatives

- **Copy the contract's schemas by hash, as ADR 0009 does.** It is how the earlier copies went
  stale. The crate is pinned the same way (an exact version) and also catches structural drift at
  compile time; the Python pack still reads the schemas, which the crate embeds.
- **Make the contract's result the only MCP shape, op tools included.** The op tools serve an
  agent composing steps and their envelopes are tested against fixtures; wrapping them is a v0.2
  question once the action tools have run in anger.
- **Conform rascal before squint.** The contract's hard parts are long paid runs, which squint
  has and rascal does not; squint goes first so the contract is proven where it is riskiest, and
  rascal conforms in parallel against the same tag.

## Consequences

- An agent that knows only the contract can run a rascal recipe with files and references alone,
  and hand rascal's mask to squint as the region where change was allowed.
- The lab's long-standing note is closed by the fill cache: arms with identical masks share one
  fill, and a retried heal costs nothing.
- The contract's v0.2 is expected after squint's conformance; rascal updates the pin then.

## Amendment (M11): table-manners v0.2

v0.2 adds run association and the session ledger (the contract's 2.3 and 4.1 to 4.6), agreed
between squint and rascal from Wafflehouse's C4. rascal built against the draft by path in
milestone M11 and pins `=0.2.0`, tagged and published 2026-11-01: the run's reference
(`session_id`, `client_run_id`) in `run.json`, the runs list and progress `_meta`; correlation
ids claimed when a call is accepted, with the `client_run_id_reused` refusal; `spend.unresolved_usd`;
the paged `rascal://runs{?cursor}`; `rascal://session` under a UUID per `serve`; and fixtures for
the contract's `fixtures/rascal/` from rascal's own server.
- `SPEC.md` "External interface" and `docs/contract.md` describe the mapping; `MAPPING.md` in the
  contract repository is the field-by-field work list until both tools conform.

## Amendment (M12): table-manners v0.3

v0.3 lets a host set a tool up and pay for a run safely (the contract's 2.2, 3.1, 3.2, 4.4, 4.5
and 5). rascal builds against the draft by path in milestone M12, as in M11, and pins `=0.3.0`
once it is tagged and published (PLAN.md 12.10). The decisions, recorded here from PLAN.md M12:

- **No default configuration lookup.** `--config <path>` is the only way to choose one, for
  `describe`, `serve` and `estimate` alike (4.6 allows either). The configuration is read once
  into a state, `missing`, `invalid` or `loaded`, with an identity that is never null: with no
  file it is the built-in defaults' (ADR 0016), which are a configuration too.
- **`describe` and `serve` work with no configuration**; `serve`'s `--workspace` defaults to the
  configuration's `[workspace] dir`, else a data-directory workspace, so a host needs nothing but
  `rascal serve` and, if it likes, `--config`. A pack that does not start leaves its tools out
  instead of stopping the server.
- **An action's readiness is its default use** (3.1): the mask alone when no filler is
  configured (every action but `remove_prompt` `ready`), healing with the configured fillers
  otherwise, so every variable they read is named for the host. It is a local inspection that
  reads variable names, never values, and never starts a pack or sends a request; a model file
  is `present` when it is there and not empty (digests are the pack's to check), never `valid`.
- **A call fails only for what it needs**, before any run: the `not_ready` refusal (JSON-RPC
  `-32602`, no run, no spend, its `client_run_id` released), on the command line too (`rascal run`
  and `heal` exit 64 before the run directory exists). A missing key is therefore never a run;
  `credential` is a provider's 401 or 403 during one, never retryable, and the session remembers
  it, as it remembers a 2xx (`valid`).
- **The cap and the budget fail apart**, `cap` and `budget`, after the arguments and the named
  estimate and before any paid step; a `max_spend_usd` above the configured cap is `invalid`,
  never lowered.
- **`estimate` is served in every configuration** and priced by the function the limits check
  uses. A mask-only call with no file is allowed at 0 under the defaults' identity; a call that
  cannot be priced (a filler not configured, `remove_prompt` without its pack) is refused for
  `readiness` with `worst_case_usd` null, never a made-up zero. The binding covers the arguments
  (images by their bytes, delivery choices left out), the configuration's identity, the session
  and the version; the command line's estimate has no session, so no server accepts it.
- **Estimates expire after one hour.** A call naming one is checked at the limits check and
  fails `stale_estimate` when it is not one this session issued (for rascal another session and
  another configuration are the same answer, since a session reads its configuration once, so
  one fixture stands for both), when the call differs or when it expired.
- **A server render exits on stdin's end of file by default**, prints only its URL and is framed
  only for `--frame-ancestor`; the document render is one self-contained file.
- **No list image slots.** Every shipped recipe's inputs are single, so no action declares
  `many`; a test keeps it so until a recipe grows a list input and decides its slot shape.

The contract's `fixtures/rascal/` at v0.3 comes from rascal's own test
(`table_manners_fixtures.rs`), held to the contract exactly as the contract's own fixtures test
holds it, and `no_provider_call.rs` proves that `describe` and `estimate` make no provider call
in any configuration or credential state (`docs/contract.md`, "Conformance").

## Amendment (M10, task 10.6): operations are actions

What "does not change" above no longer holds for the op tools: on v0.4 each built-in op is an
operation of the description, served as an action tool whose call is a run (ports bound by
artifact reference before the run, the op run on the session's store, `artifacts` with previews
in `run.json`), and the op tools with their envelopes leave `serve` (ADR 0015's amendment). The
envelope stays rascal's own record of a step and is the operation's `body.envelope`.
