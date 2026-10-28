# 0010: Crate and package names

Date: 2026-10-28. Status: accepted.

## Context

`rascal` and `rascal-cli` on crates.io belong to an unrelated project. The command must still
be `rascal`, and the family of crates, the Python pack and a later Node binding need names that
are free and obviously related. Checked on 2026-10-28.

## Decision

The binary crate is `rascal-mask` (installed command `rascal`). Libraries: `rascal-core`,
`rascal-ops`, `rascal-mcp`, `rascal-testkit`. The Python pack is `rascal-tools` on PyPI. The
Node binding, later, is `@markhayden/rascal`. Publication order: core, testkit, ops, mcp, mask.

## Alternatives

- **A different command name.** The project is rascal; the command should be too.
- **One crate.** Would force every library consumer to compile the binary's dependencies
  (`tokio`, `reqwest`).

## Consequences

- `cargo install rascal-mask` installs `rascal`; the README says so in its first lines.
- Every selector in the plan is `-p rascal-mask`.
