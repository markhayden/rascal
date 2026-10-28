# rascal

The find-and-mask step of an image pipeline. rascal turns a need (a seam to heal, an object to
remove, a scratch to repair, a region to protect) into a high-quality mask and the plumbing a
visual model needs to blend its result back: zones, soft edges, a tone ramp, per-consumer
renderings, and cut/put geometry for wrapped images such as 360 panoramas.

Pure Rust library, CLI and MCP server. Fillers (the models that paint) are plugins; ML finders
are a Python tool pack over MCP.

Status: pre-release. The design is in [SPEC.md](SPEC.md) and the build order in
[PLAN.md](PLAN.md). Nothing is built yet.

Licence: Apache-2.0.

## Developing

One manual step, then everything is a `just` recipe:

```
cargo install just --version 1.59.0 --locked    # or: brew install just
just bootstrap                                  # pinned cargo-deny and cargo-dist
just hooks                                      # the pre-commit hook
just check                                      # what every commit must pass
```

`just check` runs formatting, Clippy with warnings denied in both feature configurations,
the tests in both, `cargo deny` for licences and sources, the crate-graph and packaging checks,
the clean-room scan, and the self-tests of the hook and the checks. The pre-commit hook scans
every staged blob for credential-shaped values, refuses image files outside `fixtures/` and
`.github/assets/`, runs the clean-room scan on the staged files, then formatting and Clippy. `just ci-local` runs the same inside a Linux
container with the pinned toolchain (Docker). `just smoke <binary>` is the offline smoke against
any built binary.

The clean-room scan reads a forbidden-word list from the file named by `RASCAL_FORBIDDEN_WORDS`.
The list lives outside the repository and is never committed; with the variable unset, as in
CI, the scan passes and says so.
