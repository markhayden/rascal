# M0 checkpoint review

Branch `m0-scaffold`, 2026-10-28. Two reviews ran before the owner's checkpoint review, as the
plan requires: a five-axis code review against `docs/review-checklist.md` and a test-coverage
review, both by Claude Code agents, read-only, with the acceptance criteria of tasks 0.1 to
0.5 as the brief. Both ran `just check`, the self-tests and the publish dry run; the code
review also ran `just ci-local` and drove the harness and the clean-room scan with planted
cases in scratch repositories.

Verdicts: the code review asked for two small commits before the checkpoint (the clean-room
guard's fail-open paths; the harness predicates against what schemars generates) and would
then approve; the test review judged coverage adequate once five must-have items landed. Both
commits landed the same day; every must-have item is in them.

## Findings and dispositions

| # | Finding | Disposition |
|---|---|---|
| 1 | The hook scanned worktree files, not staged blobs, so a staged forbidden word edited out of the file was committed | fixed, `fix(repo)`: `scan-clean-room.sh --staged` reads the index; self-test case "staged but removed from the worktree" |
| 2 | A word in a file name was not scanned, and a content hit printed the path containing it | fixed: tracked and staged paths scanned; a matching path reported by position; self-tests assert the word is never printed |
| 3 | A word list with CRLF endings disabled the scan silently | fixed: list normalised (CR and whitespace stripped, entries under three characters refused); self-test |
| 4 | `has_version` rejected every enum, including a correctly versioned tagged one | fixed, `fix(core)`: structural through `oneOf`, `anyOf`, `allOf` and local `$ref`s; probe enums in the tests |
| 5 | `is_closed` did not descend into `properties`, so an open struct variant of an externally tagged enum passed | fixed: descends into `properties`, `items`, `prefixItems`, schema-valued `additionalProperties`; `ExternalOpen` probe |
| 6 | Authored schemas skipped the closed and version checks, and the location cannot carry a version field at all | fixed and refined: both checks apply to both sources; versioning by schema id for a shape shared with a copied schema (ADR 0009 refined 2026-10-28; SPEC "Code style" updated); the location schema is checked that way |
| 7 | A derive list wrapped by rustfmt, or a `cfg_attr`, escaped the inventory grep | fixed: attributes read across lines, `cfg_attr` seen, `pub(...)` stripped, block comments skipped; cases in the test |
| 8 | Integer-valued floats failed the fixture round trip | fixed: numbers compared by value |
| 9 | The hook's credential report named a line in a concatenated stream | fixed: per-blob scan, report prefixed with the file |
| 10 | A symlinked word list inside the tree was accepted | fixed: real path resolved; self-test |
| 11 | Commit messages were not scanned | fixed: `scripts/hooks/commit-msg`; self-test |
| 12 | Two workspace dependencies outside the spec's list | fixed: `tracing-subscriber` and `rustls` dropped until the tasks that need them (1.10 adds the subscriber with a spec note) |
| 13 | CI workflows without `permissions:`; actions pinned by tag; `dist` installed by `curl \| sh` | `permissions: contents: read` added; SHA pinning and a checksum for the `dist` installer are deferred to task 7.2 (release) |
| 14a | Any read error reported as `MissingSchema` | fixed: `Unusable` with the message |
| 14b | Scratch directories never removed | fixed: a guard removes them |
| 14c | CLI test hard-coded the version | fixed: `CARGO_PKG_VERSION` |
| 14d | `cargo package` inside a `while read` loop; `read_text()` without an encoding | deferred to task 0.1's next touch; both are robustness against conditions `just check` does not meet today |
| 14e | The registry will need to embed schemas that live outside the crate | recorded in `docs/export.md` "Where the schemas live"; decided in task 1.4 |
| 14f | Negative location fixtures missing for stated rules | fixed: seven added (point outside the unit square, single-point path, zero width and radius, missing frame, extra fields on line and polygon) |
| 14g | ASCII-only case folding for non-ASCII words | documented in the scan's header |
| T1 | `commit_bad` accepted any refusal | fixed: every refusal asserts the refusing guard's message |
| T2 | The tracked-files mode of the scan was never run with a list | fixed: scratch-repository cases for tracked, staged and file modes |
| T3 | `test-hooks.sh` depended on the developer's `commit.gpgsign` | fixed: the scratch repository sets it false; fixed bytes replace `/dev/urandom` |
| T4 | The copied directory was not swept for unlisted files | fixed: `UnlistedCopiedSchema`; test |
| T5 | `SOURCES.toml` with an unknown field untested; retriever rejections untested one by one; `Unusable`, `FixtureUnreadable`, bare `version`, `Option` version, map fields, render's newline, formatting-is-not-drift | fixed: all have tests |
| T6 | `.svg` not in the image guard | left out by decision: SVG is text and goes through the credential and clean-room scans; the hook says so |
| T7 | `compile_fail` doctest for `boundary_type!` | deferred to task 1.2, with the first real boundary type |
| T8 | A docs check that every ADR has its four sections | deferred to task 1.9 (`scripts/check-docs.sh`) |
| T9 | CI green on both platforms | pending the owner's first push, by construction |

## Checkpoint state

- `just check` green on macOS arm64 (bash 3.2) and `just ci-local` green in the Linux
  container (bash 5) on the final branch.
- Hooks tested: 47 cases. Checks tested: 24 cases. Rust: 22 schema tests, 4 CLI tests.
- ADRs 0001 to 0014 written; 0009 refined. Owner reads at the checkpoint.
- Schema harness green on the location schema; the inventory is empty until task 1.2.
- Open: the CI run on both GitHub runners after the owner pushes `main`.
