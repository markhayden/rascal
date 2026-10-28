# 0014: Apache-2.0 and the family's repository conventions

Date: 2026-10-28. Status: accepted.

## Context

rascal is a single-owner open-source project meant to be used inside other people's pipelines
and alongside an evaluation harness in the same family. It should be easy to adopt, easy to
contribute to, and recognisably built the same way as its sibling.

## Decision

Apache-2.0. Rust 2024 workspace; `just check` as the one definition of what a commit must pass
(fmt, Clippy with warnings denied in both feature configurations, tests in both, `cargo deny`,
the crate-graph, packaging and clean-room checks, and the self-tests of the hook and the
checks); `cargo-dist` for release archives; dated ADRs in this directory; `docs/owner-decisions.md`
for decisions only the owner can make; a pre-commit hook that scans every staged blob for
credential-shaped values and refuses image files outside `fixtures/`; Conventional Commits with
a scope; milestone branches merged into `main` only at reviewed checkpoints. No backwards
compatibility and no shims: a breaking change is a version bump and an ADR.

## Alternatives

- **MIT.** Shorter, but without the patent grant that makes Apache-2.0 the safer default for a
  tool others embed.
- **Ad hoc conventions.** Every rule here exists because its absence cost something once.

## Consequences

- The clean-room rule is a check, not a promise: every tracked file is scanned against a word
  list kept outside the tree.
- A commit that cannot pass `just check` is not made.
