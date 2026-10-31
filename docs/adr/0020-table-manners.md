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
- a `playground` subcommand that starts the shared playground program and carries no UI.

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
- `SPEC.md` "External interface" and `docs/contract.md` describe the mapping; `MAPPING.md` in the
  contract repository is the field-by-field work list until both tools conform.
