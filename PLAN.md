# Implementation plan: rascal

Status: draft 4, 2026-10-28, written from `SPEC.md` draft 8 (draft 6 approved 2026-10-28; drafts 7
and 8 add what the plan needed) and revised against Codex plan reviews 1 to 3
(`docs/reviews/plan-review-*.md`). Review 3 judged draft 3 ready to begin M0 with six items to
fix in their owning tasks; draft 4 makes all six. The task
checklist that tracks progress is `tasks/todo.md`; this file holds the reasoning, the order and
the commit strategy. When the spec changes, this plan changes in the same commit. Drafts 2 and 3 added to the spec, in the same commit: the `rect` utility recipe, `inspect --json`,
`workspace list`, `ops list` and `ops describe`, `tools call`, the `--out` rule, `fan-thin-v1`, the
tighten tangent and cap rules, and the Diagnostics section with `rascal dash`; nothing in the plan
is public behaviour the spec does not name.

## Overview

Build a Rust library, CLI and MCP server that turn a need (a seam, an object, a scratch, a
stroke) into a mask record with zones, a soft edge and prescriptions, render it for any
consumer, run a filler plugin, and composite the result back with every unmasked pixel
untouched. The order is a walking slice first (one recipe end to end through files), then the
contracts every op shares, then the seam use case to a real heal, then the MCP surfaces, then
the remove and repair ops, then the Python finder pack, then the lab that proves the
fan-and-ramp mask on the owner's corpus, and last the Poisson paste.

| Milestone | Delivers | Tag |
|---|---|---|
| M0 Scaffold | workspace, checks including the clean-room scan, CI, hooks, docs skeleton, ADRs 0001 to 0014, copied schemas and the drift harness, owner readiness | `m0` |
| M1 Core | walking slice; frames, identity, canonical serialization; the mask record; op contracts (manifests, native envelope, registry validation, exit codes); renderings; recipes with typed bindings; workspace commit protocol and recovery; primitives, `cut`/`put`; `line`, `stroke`, `rasterize`; `inspect`, compatibility export, self-contained runs, docs coverage; the run page and tracing | `m1` |
| M2 Seam | `fan`, `profile`, `feather`, `ramp`, `apply`, `window`, `unwindow`, `paste` (feather, hard), `measure`, fillers, `heal`'s typed graph, the shipped `seam` recipe end to end offline | `m2` |
| M3 MCP | the transport decision; `rascal serve` driven by an independent client; the pack client with result validation; the isolation suite | `m3` |
| M4 Refine | `grow`, `shrink`, `close`, `subtract`, `select`, `invert`, `tighten`; the `remove`, `repair`, `repair-area`, `protect` recipes; the acceptance script; the segmenter decision | `m4` |
| M5 Pack | the Python pack with `echo_region`, `fetch` and the parity harness; `segment` under branch A; the bundle with wheelhouse and model bytes; offline setup and launch | `m5` |
| M6 Lab | corpus manifest, protocol, matrix, cells with pre-execution identity, attempts, pairs, decode; the owner's first lab round | `m6` |
| M7 Poisson | the gradient-domain paste wired into `paste` and `heal`; the final timing run; the v0.1.0 release | `m7`, `v0.1.0` |
| M8 Table manners | conformance to the shared contract, `table-manners` v0.1.0 | `m8` |
| M9 Spend and launcher | spend that survives every status, the reserved budget, reads off the permit, runs listed from allocation, the Wafflehouse launcher | `m9` |
| M10 Operations | table-manners v0.4: the path rule first; every op an action with `kind`, `group` and typed ports, a call a run; artifacts by reference and identity, the `binding` refusal, previews, the artifact resource; `export` and `import` over the mask bundles (10.5); `invocation` on every result, `rascal call` and bundle replay (R3) | `m10` |
| M11 Table manners v0.2 | run association (ids in `_meta`, correlation ids, the paged runs list) and the session ledger | `m11` |
| M12 Table manners v0.3 | readiness without configuration, the estimate bound to its call, the cap and budget apart, `credential`, claims released with no run, renders a host can embed | `m12` |

Deferred past M7 with their own ADRs: the Node, Swift and WASM bindings; the path fan; scaled
windows; replace with homography; retouch; extend; isolate; `segment` under branch B.

## Crate dependency graph

```
rascal-core      no dependency on other workspace crates; frames, images, locations, artifacts and identity,
                 canonical serialization, the mask record, effective weight, renderings, recipes, workspace,
                 journal, config, the Op trait, manifest, envelope, registry, compatibility export
rascal-ops       depends on core; primitives, every op, the solvers, the constant sets
rascal-mcp       depends on core; the MCP client (packs) and server (`serve`)
rascal-testkit   depends on core; fake fillers, the fake pack binary, the independent minimal MCP client,
                 the filesystem shim, synthetic image builders
rascal-mask      directory crates/rascal-mask; package `rascal-mask`, binary `rascal`; every selector in this
                 plan is `-p rascal-mask`; depends on core, ops, mcp; optional dependency on testkit behind the
                 non-default feature `testkit`
```

Dev-dependency edges (every crate's tests may use testkit) are declared separately from
production edges in `scripts/crate-graph.txt`, enforced by `just check`. Publication order:
`rascal-core`, `rascal-testkit`, `rascal-ops`, `rascal-mcp`, `rascal-mask`. The five names were
confirmed free on crates.io on 2026-10-28 (`rascal` and `rascal-cli` are taken by an unrelated
project); `rascal-tools` is free on PyPI; `@markhayden/rascal` is free on npm.

The ops crate depends on `image`, `imageproc` and `rayon` only; the binary alone depends on
`tokio` and `reqwest`. The MCP crate's transport dependency is decided in 3.1.

## Task dependency edges

Each task's **Dependencies** line is the authority; this list is generated from those lines and
must match them. Milestone barriers apply on top: no task in M(n+1) starts before `m(n)` is
tagged, with the one exception under parallel tracks.

```
M0  0.1 ← none        0.2 ← 0.1        0.3 ← none        0.4 ← 0.1        0.5 ← none
M1  1.1 ← 0.1, 0.4    1.2 ← 1.1        1.3 ← 1.2         1.4 ← 1.3    1.5 ← 1.4         1.6 ← 1.4, 1.5   1.7a ← 1.4       1.7b ← 1.7a
    1.8a ← 1.4        1.8b ← 1.8a      1.9 ← 1.5, 1.6, 1.7b, 1.8b          1.10 ← 1.9
M2  2.1 ← 1.8a        2.2 ← 2.1        2.3 ← 2.2         2.4 ← 1.4
    2.5 ← 1.4         2.6 ← 2.1        2.7 ← 1.6, 1.5    2.8 ← 2.3, 2.4, 2.5, 2.6, 2.7
M3  3.1 ← 1.4         3.2 ← 3.1, 1.7b, 1.9               3.3 ← 3.1, 1.7b, 2.7
M4  4.1 ← 2.2, 2.3    4.2 ← 4.1        4.3 ← 1.8b, 2.2   4.4 ← 4.2, 4.3, 2.8, 3.2
    4.5 ← 0.5
M5  5.1 ← 3.3         5.2 ← 5.1, 4.5   5.3 ← 5.2
M6  6.1 ← 1.6, 0.5, 2.1   6.2 ← 6.1, 2.8, 4.4, 5.3           6.3 ← 6.2        6.4 ← 6.3
M7  7.1 ← 2.5, 2.8    7.2 ← 7.1
```

Every executable task names a **CLI scenario** that demonstrates its behaviour through the
binary using only what precedes it, so each is a vertical slice and not a layer. Documentation
and readiness tasks (0.3, 0.5, 4.5, 6.4) have none.

## Parallel tracks

Within a milestone, tasks whose dependency lines allow it may run on separate task branches: in M1, `1.5`, `1.7a` and `1.8a` after `1.4`, and `1.6` after `1.5`; in M2, `2.4`, `2.5` and `2.7` beside the
`2.1 → 2.2 → 2.3` chain; in M4, `4.1 → 4.2` beside `4.3`. Across milestones the one exception to
the barrier: `3.1` and `3.2` depend only on M1, so when two sessions are available the branch
`m3-mcp` may be cut from `m1` while `m2-seam` is in progress; `3.3` waits for `2.7`; before the
M3 checkpoint `m3-mcp` merges `m2` and the combined tree passes `just check` and every M2 and M3
test, and `m3` is tagged after `m2`. M7 depends only on 2.5 and 2.8 and is last by choice.

## Commit strategy

**Pre-M0.** The directory is not yet a repository. The owner records the dated confirmation
that no conflicting obligation applies to the work (in `docs/owner-decisions.md`) before
anything else. Then `git init`, the remote `github.com/markhayden/rascal` (created empty by the
owner on 2026-10-25), and one **documentation-only bootstrap commit** on `main` with `LICENSE`
(Apache-2.0), `SPEC.md`, `PLAN.md`, `tasks/todo.md`, `docs/owner-decisions.md`,
`docs/reviews/`, a README stub and `.gitignore` (`target/`, `lab/`, `run/`, `ws/`, `*.mask/`
outside `fixtures/`, `.env`, `.venv/`). This is the one commit exempt from `just check`, since
no `justfile` exists yet; it is tagged `m0-base` and `m0-scaffold` branches from it. The clean-room
word list lives outside the tree at `RASCAL_FORBIDDEN_WORDS` (owner decision 2) and is applied to
the bootstrap commit by hand.

**Branches.** `main` holds only reviewed checkpoints. Each milestone is one branch `mN-<name>`
from the previous checkpoint tag. Tasks are committed on the milestone branch, or on short task
branches `mN/<task>` merged into the milestone branch when two tasks are in flight. A milestone
merges into `main` with a merge commit (no squash) once the checks pass. From the M0 checkpoint
on (owner decision 2026-10-28) the agent merges and pushes; since 2026-10-29 (decision 8,
widened) it does so at will whenever `just check` and CI are green, without waiting for the
owner's review, which happens at the owner's pace; `main` is never force pushed.

**Commits.** Conventional Commits with a scope (`feat(ops): …`, `test(core): …`,
`docs(adr): …`). From task 0.1 on, every commit leaves `just check` green. `just check` is,
exactly: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`
and the same with `--features rascal-mask/testkit`; `cargo nextest run --workspace --features
rascal-mask/testkit` and `cargo nextest run -p rascal-mask` (each crate's tests one binary,
`tests/it/`); `cargo deny check licenses bans sources`;
`scripts/check-crate-graph.sh`; `scripts/check-packaging.sh`; `scripts/scan-clean-room.sh`
(built in 0.1, passing trivially when `RASCAL_FORBIDDEN_WORDS` is unset, as in CI);
`scripts/test-hooks.sh` and `scripts/test-checks.sh` (the self-tests of the hook and the
checks); from 1.9 on `scripts/check-docs.sh`. A commit that cannot is not made. `Cargo.lock` and
`tools/python/uv.lock` are committed. The spec, plan, todo, README and docs are updated in the
same commit as the change they describe. Secrets, real photographs and corpus paths never enter
a commit; the pre-commit hook (0.2) scans for credential-shaped values and for image files
outside `fixtures/` and `.github/assets/`.

**Pre-merge verification and checkpoints.** The push policy is: `main` is pushed after a merge
whose checks are green (owner decision 8), and a checkpoint is a merge commit that has passed CI
on both platforms and closes a milestone. The
procedure: `just ci-local` on the milestone branch (`just check` natively on the owner's macOS
arm64 machine, then `just check` inside a Linux container with the pinned toolchain, run by
`scripts/ci-local.sh` through Docker; the container's architecture is the host's, so this is a
Linux check, not an x86_64 check); the owner's review; the merge commit created on `main`
locally; `just check` on the merge commit itself; `main` is pushed; GitHub Actions runs
`just check` on macOS arm64 and Linux x86_64 and, for the package and release workflows, the
publish dry run and `scripts/acceptance.sh`; only when that is green is the merge commit tagged
`mN` (annotated, with the checklist and the versions of external artifacts: bundle digests,
model artifact hashes, published package versions) and the tag pushed. A red CI on `main` is
fixed forward on the milestone branch and merged again before the tag; `main` is never force
pushed. Intermediate tags `mN.k` mark task groups on the milestone branch. `v0.1.0` is tagged on
`main` after `m7`.

**Rollback.** Every rollback is prepared on a branch, verified, reviewed and merged; nothing is
reverted directly on `main`.

1. Revert one task: `git revert --no-commit <sha>` on a branch from the current tip; when later
   tasks depend on it, revert the dependents too, in reverse dependency order, all `--no-commit`;
   run `just check`; commit once; review; merge.
2. Reset a milestone branch to an intermediate tag only with a clean tree and after
   `git branch rescue/<branch>-<date>`; never on `main`.
3. Abandon an unmerged milestone by deleting the branch after the rescue reference exists.
4. Undo a merged milestone: a branch from `main`, `git revert --no-commit -m 1 <merge commit>`
   together with the reverts of every later milestone that depends on it, `just check` on the
   result, review, merge. Checkpoint tags are immutable; the revert is a new commit.
5. A constant-set change is its own commit with the fixture update, so a bad refit reverts alone.

External state is not rolled back by git: published packages, bundles and model artifacts are
versioned and recorded in checkpoint tags; a lab run whose pairs page was opened keeps its
protocol and picks; `lab/`, `run/` and workspaces are never committed.

**Reviews.** This plan is reviewed by Codex (`gpt-6-astra`, read-only) before M0 begins, at
least two rounds, recorded under `docs/reviews/plan-review-*.md`. Each milestone's diff is
reviewed with `/agent-skills:review` and `/agent-skills:test` before the owner's checkpoint
review, with findings fixed or recorded in `docs/reviews/mN-checkpoint.md` with the task that
owns them. The owner reviews every checkpoint.

## Milestone M0: Scaffold

### Task 0.1: Workspace, crate skeletons, checks

**Description.** The Cargo workspace with the five crates as empty libraries and a binary that
prints its version; `rust-toolchain.toml`, `clippy.toml`, `deny.toml`, `dist-workspace.toml`,
the `justfile` with `bootstrap`, `check`, `ci-local`, `fmt-check`, `clippy`, `test`, `deny`,
`graph`, `packaging`, `clean-room`, `publish-dry-run`, `smoke`; `scripts/crate-graph.txt` and
`check-crate-graph.sh`; `scripts/check-packaging.sh`; `scripts/scan-clean-room.sh` (reads the
word list file named by `RASCAL_FORBIDDEN_WORDS`, case-insensitive, over every tracked file,
passing trivially when unset); `scripts/ci-local.sh`; `scripts/test-checks.sh` proving each
check fails when it should; the `testkit` feature wired but empty.

**CLI scenario.** `cargo run -p rascal-mask -- --version` prints `rascal 0.1.0`; `just check` is
green on the empty workspace; with a planted forbidden word in a tracked file `just check` fails
naming the file.

**Acceptance.**
- [ ] `just check` passes; `scripts/test-checks.sh` proves the crate-graph check, the packaging
      check and the clean-room scan each fail on a planted violation; `just ci-local` runs the
      Linux container leg.

**Verification.** `just check`; `scripts/test-checks.sh`; `just ci-local`.

**Dependencies.** None. **Files.** `Cargo.toml`, `crates/*/Cargo.toml`, `crates/*/src/lib.rs`,
`crates/rascal-mask/src/main.rs`, `justfile`, `scripts/`. **Size.** M.

### Task 0.2: CI, hooks, ignore rules

**Description.** `.github/workflows/ci.yml` running `just check` on macOS arm64 and Linux
x86_64; `package.yml` running the publish dry run (and from 4.4 the acceptance script); the
pre-commit hook from `scripts/hooks/` (credential scanner with `scripts/synthetic-tokens.txt`,
image-file guard, fmt, clippy, clean-room scan); `.gitignore` completed.

**CLI scenario.** A staged file containing a synthetic token is refused by the hook; a staged
JPEG outside `fixtures/` is refused.

**Acceptance.**
- [ ] `scripts/test-hooks.sh` proves both refusals in a scratch repository; CI is green on the
      scaffold on both platforms.

**Verification.** `scripts/test-hooks.sh`; CI run.

**Dependencies.** 0.1. **Files.** `.github/workflows/`, `scripts/hooks/`, `scripts/scan-secrets.sh`,
`.gitignore`. **Size.** S.

### Task 0.3: Docs skeleton and ADRs 0001 to 0014

**Description.** `docs/architecture.md` with the spec's diagram and an empty status table;
`docs/adr/README.md` and one ADR per spec decision (0001 Rust workspace, 0002 finds and masks,
0003 the act step and filler plugins, 0004 v1 use cases, 0005 surfaces, 0006 the mask record and
effective weight, 0007 ops as tools and typed recipes, 0008 no model loop, 0009 copied schemas
and the compatibility export, 0010 crate names, 0011 pure Rust pixels, 0012 the Python pack and
its branches, 0013 measures as evidence and the lab protocol, 0014 licence and conventions);
`docs/review-checklist.md`; stubs for `docs/{masks,recipes,fillers,measures,lab,tools,workspace,
export}.md` each stating what will fill it; README sections (what, install, the recipes, where
it sits, status).

**Acceptance.**
- [ ] Every ADR has context, decision, alternatives and consequences; `docs/architecture.md`
      matches the spec diagram; README builds the same picture as the spec's objective.

**Verification.** Owner reads. **Dependencies.** None. **Files.** `docs/`, `README.md`. **Size.** M.

### Task 0.4: Copied schemas and the schema drift harness

**Description.** `schemas/copied/manifest-v1.json` and `envelope-v1.json` copied unchanged with
`schemas/SOURCES.toml` (origin repository, commit, version, licence notice, SHA-256 of each
file); `schemas/rascal/location-v1.json` referencing the copied `Location` definition by `$ref`
and adding `line` and `stroke`; the reusable **drift harness** and the **boundary-type inventory** in `rascal-core`: a registry
every boundary type is added to, and a test helper that, for each registered type, regenerates
its schema and compares it to the committed file, asserts the three derives,
`deny_unknown_fields` and a version field, and validates a serialized fixture of it against the
committed schema; every later task that adds a boundary type registers it and adds its fixture.

**CLI scenario.** None (a test harness).

**Acceptance.**
- [ ] The copied files match their recorded SHA-256; the seven location kinds validate and an
      unknown kind fails; the harness fails on a planted schema change, on a type missing a
      derive or a version, and on an unregistered boundary type found by a grep for the derives.

**Verification.** `cargo test -p rascal-core -- schemas`.

**Dependencies.** 0.1. **Files.** `schemas/`, `crates/rascal-core/src/schema_drift.rs`,
`crates/rascal-core/tests/it/schemas.rs`. **Size.** S.

### Task 0.5: Owner readiness

**Description.** `docs/owner-decisions.md` with every owner decision as a row with the task
that needs it: the pre-M0 obligation confirmation (dated before the bootstrap commit); the
forbidden-word list location (before 0.1's first commit); a real local filler and a real
panorama available on the owner's machine (before 2.8's by-hand run); the segmenter and its
branch, including redistribution terms for the export (decided in 4.5, before 5.2); the private
corpus location, first contents and the sky and structured groups with at least `min_decided`
images each (before 6.1); the MCP transport (3.1, by ADR); the path fan (after M7).

**Acceptance.**
- [ ] Every row names its blocking task and the gate it must clear before; the pre-M0 row is
      dated.

**Verification.** Owner signs the rows. **Dependencies.** None. **Files.** `docs/owner-decisions.md`.
**Size.** XS.

### Checkpoint M0

- [ ] `just check` and `just ci-local` green; CI green on both platforms; hooks tested; ADRs
      reviewed; schema harness green.
- [ ] Owner review → merge to `main` → push → CI green → tag `m0`.

## Milestone M1: Core

### Task 1.1: Walking slice

**Description.** The thinnest end-to-end path: `rascal run` takes the one-step `rect` recipe (`inputs.location`
→ `rasterize`, with `--bbox` as the sugar that binds the location) and an image, imports the image as a canonical `Image` in a `raw:` frame,
executes the step through the Op trait and a registry holding `rect` (the find op, for later
use) and `rasterize`, writes a mask record with a `core` plane and empty `blend` and `protect`,
renders `grey`, and writes `journal.jsonl` with one line per invocation; the three `--out`
kinds (run directory, file destination, lab directory) as one shared helper. Minimal versions of the artifact types, the recipe parser (typed bindings for
`Image`, `Location`, `Mask` only), the mask record writer, the journal, and the CLI dispatch with
the four exit codes (0, 1 under `--strict`, 3, 64). Each stub is named in a `// REPLACED BY <task>`
comment and 1.9's acceptance includes a grep that none remain. The `rect` recipe is a shipped
utility recipe (added to the spec by this plan).

**CLI scenario.** `rascal run rect fixtures/images/floor.png --bbox 0.2,0.2,0.4,0.5 --out run/`
writes `run/artifacts/<mask>/{mask.toml,core.png,blend.png,protect.png}`, `run/rect-grey.png` and a one-line journal; the grey rendering is white inside the box and black
outside; `rascal render … --out t.png` twice overwrites the file; a missing
image exits 3, a bad flag exits 64, and `--bbox` wholly outside the image exits 1 under `--strict`.

**Acceptance.**
- [ ] The scenario passes as an integration test, the four exit codes included; the journal's
      input and output identities are stable across two runs; a run directory that is non-empty
      is refused with 64 and a file destination is overwritten (the spec's three `--out` kinds).

**Verification.** `cargo test -p rascal-mask --test it -- walking_slice::`.

**Dependencies.** 0.1, 0.4. **Files.** `crates/rascal-core/src/{artifact,recipe,mask,journal,op,registry}.rs`,
`crates/rascal-ops/src/{rect,rasterize}.rs`, `crates/rascal-mask/src/{main,run,exit}.rs`,
`crates/rascal-mask/recipes/rect.toml`. **Size.** L, accepted as the one deliberate walking slice; tag `m1.1`.

### Task 1.2: Frames, identity, canonical serialization, locations and mapping

**Description.** Frames as coordinate spaces with the three v1 kinds; the pixel digest, frame
identity, image identity and plan identity in the spec's acyclic order; **canonical
serialization** (G13): the hash encoding (`sha256:` lower-hex), the PNG encoder setting used for
stored images (8-bit, no interlace, fixed filter and compression level, named in ADR 0016), the
canonical TOML/JSON field order for hashed records, and reference substitution by identity;
stable test vectors committed under `fixtures/identity/`; `schemas/rascal/{frame,image}-v1.json`
through the drift harness; the seven location kinds with validity rules including the wrap-seam
rule; mapping toward the root with `parts`, `partial` and the splitting rule; `frames.json` and
`locations.json`.

**CLI scenario.** `rascal run rect` on a 2:1 image with no `--projection` journals the
equirectangular assumption; a `--bbox` straddling `x = 0` on it is refused naming the seam rule;
`rascal inspect --json` (1.3) later shows the same identities the vectors predict.

**Acceptance.**
- [ ] Identity vectors reproduce; two workspaces with different allocation histories agree; the
      dependency order is exercised on import; reference substitution changes the identity when
      the referenced content changes. Mapping tests: every frame kind, splitting at the cut
      centre and the wrap seam, clipping flagged `partial`, exact round trips only for unclipped
      mappings, wholly-outside is an error.

**Verification.** `cargo test -p rascal-core -- frames identity canonical locations mapping`.

**Dependencies.** 1.1. **Files.** `crates/rascal-core/src/{frame,identity,canonical,location,mapping}.rs`,
`schemas/rascal/`, `fixtures/identity/`, `docs/adr/0016-canonical-serialization.md`. **Size.** M.

### Task 1.3: The mask record and effective weight

**Description.** The full record: `mask.toml` with zones, `profile`, `prescriptions`, `source`,
`provenance`, `identity`; the effective-weight function; the invariants; the refinement
postcondition helpers; provenance node append; the BandProfile and Series artifact types;
`schemas/rascal/{mask,band-profile,series}-v1.json`; the first `inspect` command with `--json`
only (the record, zone statistics, identity; the overlay and chart come in 1.9).

**CLI scenario.** `rascal inspect run/artifacts/<mask> --json` prints the record and identity; a
hand-edited `blend.png` with a nonzero value inside the core is refused on load with the
invariant named.

**Acceptance.**
- [ ] Invariant tests on all three planes; effective weight exhaustively; identity excludes ids,
      filenames and provenance and includes referenced identities.

**Verification.** `cargo test -p rascal-core -- mask weight` and `cargo test -p rascal-mask --test it -- inspect_json::`.

**Dependencies.** 1.2. **Files.** `crates/rascal-core/src/{mask,weight,profile,series}.rs`,
`crates/rascal-mask/src/inspect.rs`, `schemas/rascal/`. **Size.** M.

### Task 1.4: Op contracts

**Description.** The complete `Op` trait and manifest (every field the spec lists), the native
envelope with `artifacts` and `schemas/rascal/envelope-v1.json`, and the registry's shared
validation: artifact types, frame agreement between a mask and an image, required bindings,
a requirement satisfied only by a successful step's output, `not_applicable` publishing nothing
and writing nothing, `invalid_args` for bad parameters; `rect` and `rasterize` brought up to the
full contract; the manifest rendered by `rascal ops list` (a plan addition the spec's `tools
list` already implies for packs; documented) and `rascal ops describe <op>`.

**CLI scenario.** `rascal ops describe rasterize --json` prints the full manifest; `rascal run`
of a recipe whose step requires a step that returned `not_applicable` fails naming the
requirement; a bad parameter gives `invalid_args` in the envelope and exit 64, with and without `--strict`;
`not_applicable` exits 1 under `--strict` and 0 without.

**Acceptance.**
- [ ] A table test over every manifest field required; rejection paths for each validation rule;
      `not_applicable` publishes nothing (a directory diff of artifacts and registry) and appends
      exactly one journal line; the four
      exit codes for `invalid_args` and `not_applicable` with and without `--strict`; the envelope
      schema through the harness.

**Verification.** `cargo test -p rascal-core -- op manifest envelope registry`.

**Dependencies.** 1.3. **Files.** `crates/rascal-core/src/{op,manifest,envelope,registry}.rs`,
`crates/rascal-mask/src/ops.rs`, `schemas/rascal/envelope-v1.json`. **Size.** M.

### Task 1.5: Renderings and the `render` op

**Description.** The five profiles with their roles, polarity and parameters; `render` as a
full-contract op taking a `Mask` and, for `image_input` profiles, an `Image` in the same frame;
the `rendering` role and `rendering_role`; the registry rule that no op accepts an `Image` where
a `Mask` is declared.

**CLI scenario.** `rascal render run/artifacts/<mask> --for tinted --image img-1 --out t.png`
tints the core; `--for binary --threshold 0` exits 64; passing a rendering where a mask is
expected is refused naming the artifact's role.

**Acceptance.**
- [ ] Every profile exhaustively on a synthetic mask with core, blend and protect; protected
      pixels are 0 in `binary` at `t = 1`; `alpha` and `tinted` leave protected core pixels alone.

**Verification.** `cargo test -p rascal-ops -- render`.

**Dependencies.** 1.4. **Files.** `crates/rascal-core/src/rendering.rs`, `crates/rascal-ops/src/render.rs`,
`crates/rascal-mask/src/render.rs`. **Size.** S.

### Task 1.6: Recipes with typed bindings

**Description.** The full recipe parser and validator: `[inputs]` with every artifact type,
`[[step]]` with ids and typed bindings, constants references, `[outputs]` with `mask`, `image`,
`plan`, `render`; every validation error path with step ids; default resolution at validation;
CLI binding of `--location`, `--stroke`, `--protect`, `--mask` (and `--prompt` reserved for M5);
embedded recipes addressable by name; `schemas/rascal/{recipe,config}-v1.json` (config holds
the general sections now and gains `[fillers]` in 2.7 and `[packs]` in 3.3).

**CLI scenario.** `rascal run rect floor.png --bbox …` works by name and by path with
semantically identical journals (op, params, identities; durations excluded); a recipe binding
`render.image` where a `Location` is declared fails with `step "mask": input location expects
Location, got Image from render.image`.

**Acceptance.**
- [ ] A table test covers every validation error with its step id; resolved defaults appear in
      provenance; the by-name and by-path journals agree on semantic content.

**Verification.** `cargo test -p rascal-core -- recipe` and `cargo test -p rascal-mask --test it -- recipes_by_name::`.

**Dependencies.** 1.4, 1.5. **Files.** `crates/rascal-core/src/{recipe,config}.rs`, `crates/rascal-mask/src/{args,run}.rs`,
`crates/rascal-mask/recipes/`, `schemas/rascal/`. **Size.** M.

### Task 1.7a: Workspace generations and the commit protocol

**Description.** The workspace layout; staging directories with a lifetime lock; operation records with
`duration_ms` and the commit sequence number outside their identity; journal lines for every
invocation including non-publishing and failed ones; the commit sequence with the spec's sync order; `CURRENT` generations; the id counter;
readers' snapshot; `--workspace` on every op; the private throwaway workspace under `--out`;
`rascal workspace list` (a plan addition, documented); **platform synchronization** (G15): the
sync primitive per platform (`fsync` on files and directories on Linux; `F_FULLFSYNC` on macOS,
decided and recorded in ADR 0017) with the supported durability statement in
`docs/workspace.md`; a sync-counting shim in the testkit that records the order of syncs and
renames.

**CLI scenario.** Two shells run `rascal rect --workspace ws/ …` concurrently fifty times; the
final generation lists one hundred locations and `rascal workspace list ws/` agrees.

**Acceptance.**
- [ ] Concurrent commits produce no torn generation; an op that fails after staging publishes
      nothing; the shim proves the spec's sync order and that `committed` is reported only after
      the final sync; a platform test performs a real sync on each CI platform.

**Verification.** `cargo test -p rascal-core -- workspace`.

**Dependencies.** 1.4. **Files.** `crates/rascal-core/src/workspace/{mod,staging,commit,registry,sync}.rs`,
`crates/rascal-testkit/src/fs_shim.rs`, `docs/adr/0017-durability.md`. **Size.** M.

### Task 1.7b: Repair, recovery and the journal projection

**Description.** `rascal workspace repair`: abandoned staging removal, orphan collection, the
registry-versus-disk check, rebuilding the journal from operation records; cancellation before
and after the writer lock; crash injection at each commit boundary through the shim;
`schemas/rascal/journal-v1.json` (G2).

**CLI scenario.** A run killed between the pointer rename and the journal append is repaired,
and the rebuilt journal's identities equal those of an uninterrupted run of the same recipe.

**Acceptance.**
- [ ] Crash at each of the four boundaries yields nothing or a complete generation; an abandoned
      staging directory is removed and a locked one kept; a generation naming a missing artifact
      refuses the workspace until repair; the journal schema through the drift harness.

**Verification.** `cargo test -p rascal-core -- workspace::recovery`.

**Dependencies.** 1.7a. **Files.** `crates/rascal-core/src/workspace/{repair,recovery}.rs`,
`crates/rascal-mask/src/workspace.rs`, `schemas/rascal/journal-v1.json`. **Size.** M.

### Task 1.8a: Primitives, `cut`/`put` and the Plan

**Description.** The primitives module (`smooth_rows`, Sobel with edge replication, disc dilate
and erode, Euclidean distance transform, sRGB to Lab, the normal CDF) each tested against brute
force or tabulated values; `cut` with the integer strip formula and `put`; the `Plan` artifact
and `schemas/rascal/plan-v1.json` (G2).

**CLI scenario.** `rascal cut pano.png --strip 0.07 --out run/` then `rascal put pano.png
run/artifacts/<canvas> --out back.png`; the decoded samples of `back.png` equal the decoded
samples of `pano.png`.

**Acceptance.**
- [ ] `cut`/`put` round trips at `W = 4, 5, 10, 5760` with the spec's `s` values, compared on
      decoded samples; every primitive's test; the plan schema through the harness.

**Verification.** `cargo test -p rascal-ops -- primitives cut put`.

**Dependencies.** 1.4. **Files.** `crates/rascal-ops/src/{primitives,cut,put}.rs`, `crates/rascal-core/src/plan.rs`,
`schemas/rascal/plan-v1.json`. **Size.** M.

### Task 1.8b: `line`, `stroke` and the full `rasterize`

**Description.** The `line` (column and path forms) and `stroke` finders; `rasterize` for every
supported kind with `not_applicable` for `view` and `absent`; `--column` and `--stroke` CLI
bindings.

**CLI scenario.** `rascal rasterize --stroke s.json --image i --out run/` draws the dilated
path; `rascal line --image <cut canvas> --column 0.5` then `rascal rasterize` gives a band
`width_px` wide at the centre.

**Acceptance.**
- [ ] `rasterize` per kind including a `line` path dilated by half its width; `view` and
      `absent` are `not_applicable` and write nothing.

**Verification.** `cargo test -p rascal-ops -- line stroke rasterize`.

**Dependencies.** 1.8a. **Files.** `crates/rascal-ops/src/{line,stroke,rasterize}.rs`. **Size.** S.

### Task 1.9: `inspect`, compatibility export, self-contained runs, docs coverage

**Description.** `inspect` writing the overlay JPEG and the per-row chart, `--replay` rebuilding
the record from the operation records and diffing planes; `rascal export --compat` as the
projection with the two failure cases and the standalone-location form, positive tests from a
real `rect` envelope (the `segment` envelope is added in 3.3, G7); **self-contained runs** (G4):
a run directory copied elsewhere replays to identical identities with the original inputs
absent, and a replay of a run that called an external filler or pack reuses the recorded
outputs and makes no call (proven in 2.8 and 3.3 against their fakes, with the mechanism built
here); `scripts/check-docs.sh` (G11): every op, profile, recipe field, config key and measure in
the registry is named in `docs/`, and every name under a documented heading exists in the
registry, so stale names fail too; `docs/{masks,recipes,workspace,export}.md` filled; the stub
grep from 1.1.

**CLI scenario.** `rascal export --compat run/journal.jsonl --step rect --out e.json` validates
against the copied envelope schema; the same for a `line` step fails naming the region; `cp -r
run/ elsewhere/ && rascal inspect elsewhere/artifacts/<mask> --replay` reports an empty diff.

**Acceptance.**
- [ ] Positive export tests; the two negative cases; `--replay` empty on an untouched run and a
      named plane on a tampered one; relocation replay; `check-docs.sh` fails on an undocumented
      op and on a stale documented name; no `REPLACED BY` comment remains.

**Verification.** `cargo test -p rascal-core -- export replay` and `cargo test -p rascal-mask --test it -- inspect::`; `scripts/check-docs.sh`.

**Dependencies.** 1.5, 1.6, 1.7b, 1.8b. **Files.** `crates/rascal-ops/src/inspect.rs`,
`crates/rascal-core/src/{export,replay}.rs`, `scripts/check-docs.sh`, `docs/`. **Size.** M.

### Task 1.10: The run page and tracing

**Description.** `rascal dash <run or workspace> [--out | --serve]` from an authored template in
`crates/rascal-mask/pages/dash.html`: the timing waterfall from the operation records, every
artifact as a thumbnail opening at full size, every step's envelope and resolved parameters, repeated, `not_applicable` and failed invocations marked; the filler and pack call rows (empty
until 2.8 and 3.3 fill them); the **panel registry** an op registers a renderer with (the chart
panel arrives in 2.1, measures in 2.6, filler calls in 2.8, pack calls in 3.3); `tracing` spans
per op with `RASCAL_LOG`, off by default; `docs/diagnostics.md`; `check-docs.sh` extended to
panels. Later owners: filler spans, the configured-key grep and `--no-dash` on `heal` in 2.8;
`workspace_dash` in 3.2; pack spans in 3.3; `--no-dash` on `lab run` in 6.2.

**CLI scenario.** `rascal dash run/ --out run/dash.html` on the walking-slice run writes a page
listing its one step with its duration and the mask's three planes as thumbnails; `RASCAL_LOG=debug
rascal run rect …` prints one span per op.

**Acceptance.**
- [ ] The page names every journal line and no other, on a fixture run with a repeated step and
      a `not_applicable` step, and on the same run after `workspace repair` (marked rebuilt);
      every artifact the journal names is embedded as a `data:` URI (parsed, not grepped);
      `RASCAL_LOG` output on a fixture run contains no pixel bytes; `--serve` binds loopback only.

**Verification.** `cargo test -p rascal-mask --test it -- dash::`.

**Dependencies.** 1.9. **Files.** `crates/rascal-mask/src/dash.rs`, `crates/rascal-mask/pages/dash.html`,
`crates/rascal-core/src/panel.rs`, `docs/diagnostics.md`. **Size.** M; tag `m1.2`.

### Checkpoint M1

- [ ] `cargo test --workspace` green in both feature sets (its duration recorded in the
      checkpoint file as a benchmark, not a gate); every executable M1 task has its CLI scenario in an integration test; docs coverage check
      green; no stub comments remain; the run page renders the walking-slice run.
- [ ] `/agent-skills:review` and `/agent-skills:test` passes applied; findings in
      `docs/reviews/m1-checkpoint.md`.
- [ ] Owner review → merge to `main` → push → CI green → tag `m1`.

## Milestone M2: Seam

### Task 2.1: `fan` and the band profile

**Description.** The two rulers and the half-width rule exactly as the spec's algorithm, the
`fan-v1` constant set as an embedded TOML, the `BandProfile` output with the resolved window
geometry; `not_applicable` when a window clips to fewer than two columns.

**CLI scenario.** `rascal fan --workspace ws/ --image <cut canvas> --line loc-1 --json` prints
the profile summary (`thin`, max half, rows widened); on the synthetic step-sky canvas the top
rows widen and the textured bottom stays at `thin`.

**Acceptance.**
- [ ] Monotonicity in texture and in step; `raw_half = thin` at a closed gate; the golden
      profile on the step-sky fixture to 0.5 px; the clipped-window `not_applicable`; the
      per-row chart panel registered and shown on the run page beside the strip.

**Verification.** `cargo test -p rascal-ops -- fan`.

**Dependencies.** 1.8a. **Files.** `crates/rascal-ops/src/fan.rs`, `crates/rascal-ops/constants/fan-v1.toml`,
`fixtures/images/step-sky.png`, `fixtures/masks/`. **Size.** M.

### Task 2.2: `profile` and `feather`

**Description.** The half-Gaussian feather by distance transform with finite support, per-row
sigma for `profile` and one sigma for `feather`; `profile-v1` and `feather-v1`; the `sigma_px`
series; protected pixels get zero blend.

**CLI scenario.** `rascal profile --band <profile> --out ws/` then `rascal inspect` shows the
white core edge at the half-width and the tint fading outside it.

**Acceptance.**
- [ ] Symmetry about the line; the core edge at `half`; the 50% contour at `1.1774 σ` outside
      the core within half a pixel; zero beyond `3 σ`; `feather` on a blob matches the distance
      transform brute force.

**Verification.** `cargo test -p rascal-ops -- profile feather`.

**Dependencies.** 2.1. **Files.** `crates/rascal-ops/src/{profile,feather}.rs`, constants. **Size.** S.

### Task 2.3: `ramp` and `apply`

**Description.** `ramp` estimating the per-row colour step on the profile's step windows and
storing the `tone_ramp` prescription; `apply` performing every prescription once in order with
the `w > 0` rule; `ramp-v1`.

**CLI scenario.** `rascal ramp --image <canvas> --mask <band>` then `rascal apply` yields a
canvas whose two sides meet in colour at the line on the step-sky fixture; pixels beyond the
half-width and protected pixels are byte-identical.

**Acceptance.**
- [ ] Zero at and beyond `half`; `share · Δ` at the line; protected pixels untouched; a
      hand-built mask with a prescription and a smaller support than the prescription's band
      proves `apply` changes no `w = 0` pixel (the `shrink`-based form of this test is added in 4.1).

**Verification.** `cargo test -p rascal-ops -- ramp apply`.

**Dependencies.** 2.2. **Files.** `crates/rascal-ops/src/{ramp,apply}.rs`, constants. **Size.** S.

### Task 2.4: `window` and `unwindow`

**Description.** Mandatory support, optional context, the capping and sliding order, integer
rounding (floor for `x0, y0`, ceil for `x1, y1`, context in pixels rounded half up), tie
placement toward the smaller offset, `not_applicable` for empty or oversized support; the `crop:`
frame and the cropped mask; `unwindow` exactness; `window-v1`.

**CLI scenario.** `rascal window --image img --mask m --max-side 1024` on the full-height band
returns a full-height window with context only sideways; with `--max-side 100` on a wide band it
is `not_applicable` naming the sizes.

**Acceptance.**
- [ ] The spec's three window tests plus fractional context and a tied placement;
      `unwindow(window(x)) = x` where nothing changed; the crop frame's identity derives from the
      parent and the bbox.

**Verification.** `cargo test -p rascal-ops -- window`.

**Dependencies.** 1.4. **Files.** `crates/rascal-ops/src/{window,unwindow}.rs`, constants. **Size.** S.

### Task 2.5: `paste` (feather and hard)

**Description.** The paste contract: base copy, writes only where `w > 0`, channel rules, RGBA
alpha kept; `feather` and `hard`; `poisson` registered as `not_applicable` naming M7 until 7.1.

**CLI scenario.** `rascal paste base.png fill.png <mask> --mode hard --out c.png`; a diff of
`c.png` against `base.png` is nonzero only inside the core.

**Acceptance.**
- [ ] Each mode writes only `w > 0` unprotected pixels; a fill equal to base returns base; a
      synthetic fill that differs from base at every pixel still leaves `w = 0` pixels equal to
      base; RGBA base keeps its alpha.

**Verification.** `cargo test -p rascal-ops -- paste`.

**Dependencies.** 1.4. **Files.** `crates/rascal-ops/src/paste.rs`. **Size.** S.

### Task 2.6: `measure` v1

**Description.** The five measures with `measures-v1`, Series outputs with row indices and
units, scalar summaries, skipped rows, `measured_nothing` when every row is skipped, circular
hue, finite serialisation.

**CLI scenario.** `rascal measure --before a.png --after b.png --mask m --set continuity,drift
--json` prints summaries; on a flat image `continuity` reports `measured_nothing` with
`rows_skipped = rows`.

**Acceptance.**
- [ ] Each measure on a constructed case with a known answer; empty-window and zero-denominator
      rows skipped and counted; hue wraparound; `drift` outside is 0 after a correct paste; the
      measures panel on the run page.

**Verification.** `cargo test -p rascal-ops -- measure`.

**Dependencies.** 2.1. **Files.** `crates/rascal-ops/src/measure/`, constants. **Size.** M.

### Task 2.7: Fillers and the fake fillers

**Description.** `[fillers.*]` in the config schema; the command filler with placeholder
substitution, timeout and exit handling; the HTTP filler with multipart POST, bearer key from
`key_env`, timeout; the request-field matrix (G9): `mask_input` profiles send `image` and
`mask`, `image_input` profiles send the rendering as `image` and no `mask` unless
`send_mask = true`; the dimension assertion; non-PNG and malformed responses rejected; the
journal entry without the key; testkit fake fillers as one binary with five behaviours (exact
blur, mis-sized, re-encoding, slow, failing) and a local HTTP test server.

**CLI scenario.** None of its own; the filler is reached through `heal` in 2.8. The task's
integration test drives the filler port directly with each fake.

**Acceptance.**
- [ ] Each fake behaviour produces its documented outcome; the field matrix for every profile
      and `send_mask` value; a non-PNG body is an operational error; the key never appears in any
      file under `--out` (grep in the test).

**Verification.** `cargo test -p rascal-mask --features testkit -- fillers`.

**Dependencies.** 1.6, 1.5. **Files.** `crates/rascal-core/src/config.rs`, `crates/rascal-mask/src/filler/{command,http}.rs`,
`crates/rascal-testkit/src/bin/fake-filler.rs`, `examples/rascal.toml`. **Size.** M.

### Task 2.8: `heal`'s typed graph and the shipped `seam` recipe

**Description.** `heal` executing the spec's fixed graph with the recipe's declared outputs and
the typed input map; the preservation check against the invocation's `B`; measures in the mask's
frame; the embedded `seam` recipe; the golden seam mask on the step-sky fixture; the end-to-end
offline tests through every fake filler, with and without `max_side`; replay of a healed run
reusing the recorded filler output with the fake filler absent (G4); `docs/{fillers,measures}.md`.

**CLI scenario.** `rascal heal seam fixtures/images/step-pano.png --filler fake --config
fake.toml --out run/` (the testkit fake as a command filler, `rascal fake-filler exact`) writes `run/result.png` whose `w = 0` pixels equal the input
and a journal with fan, ramp, filler and measures. By hand, with the owner's real local filler
and real panorama (owner decision), the same command on the panorama.

**Acceptance.**
- [ ] The graph is executed literally (the journal's op sequence); the mis-sized filler is
      refused; the re-encoding filler passes preservation; `continuity` falls from the raw canvas
      to the composite on the fixture; replay without the filler binary present succeeds; `heal` writes the run page by default with
      the filler call row filled and `--no-dash` skips it; the filler span carries request size
      and status; `RASCAL_LOG` output and every file under `--out` on a run with a configured
      `key_env` contain no key.

**Verification.** `cargo test -p rascal-mask --features testkit --test it -- heal::`.

**Dependencies.** 2.3, 2.4, 2.5, 2.6, 2.7. **Files.** `crates/rascal-mask/src/heal.rs`,
`crates/rascal-mask/recipes/seam.toml`, `fixtures/masks/seam-step-sky.mask/`, `docs/`. **Size.** M; tag `m2.1`.

### Checkpoint M2

- [ ] The seam recipe heals the fixture through every fake filler; the owner has run one real
      panorama through a real filler by hand and looked at it in a 360 viewer of their choice.
- [ ] Review and test passes applied; `docs/reviews/m2-checkpoint.md`.
- [ ] Owner review → merge to `main` → push → CI green → tag `m2`.

## Milestone M3: MCP

### Task 3.1: The transport decision and the independent client

**Description.** ADR 0015: the official MCP crate if it supports revision `2025-11-25` with
pagination, cancellation, structured content and image blocks, else an in-house JSON-RPC over
stdio; the shared message types in `rascal-mcp`; the testkit's **independent minimal client**
written against the protocol document only (no rascal types imported), able to initialise,
list, call and read image blocks; a fake server in the testkit it is proven against.

**CLI scenario.** None public; the client is test-only. Its integration test initialises the
fake server, lists, calls and reads an image block.

**Acceptance.**
- [ ] The independent client talks to the fake server; the ADR records the choice and the
      version pin; the client's crate imports nothing from `rascal-core`'s MCP-facing types (a
      crate-graph line).

**Verification.** `cargo test -p rascal-mcp -- transport` and `cargo test -p rascal-testkit -- mcp_client`.

**Dependencies.** 1.4. **Files.** `crates/rascal-mcp/src/{transport,jsonrpc}.rs`,
`crates/rascal-testkit/src/mcp_client.rs`, `docs/adr/0015-mcp-transport.md`. **Size.** M.

### Task 3.2: `rascal serve`

**Description.** The server over a workspace: one tool per op from the registry with ordinary arguments, the
four workspace tools plus `workspace_dash` (writes the run page into the workspace, returns its
path), tracing to stderr only, envelope as `structuredContent`, image blocks in exact
correspondence, error mapping, serial by default and `--concurrency n`; `docs/tools.md` server
section.

**CLI scenario.** `rascal serve --workspace ws/` driven by the independent client with M1 ops
only: import the floor fixture, `rect`, `rasterize`, `render grey`, read the rendering; then
`rascal inspect --workspace ws/ <mask> --json` from the shell sees the same mask. (The seam ops
are exercised through `serve` in 4.4's acceptance once M2 is merged.)

**Acceptance.**
- [ ] Every registered op is reachable through the independent client (a loop over the
      registry); `workspace_read` on a mask is `invalid_args`; an operational failure arrives as a
      JSON-RPC error with the class; the CLI/serve interchange test passes.

**Verification.** `cargo test -p rascal-mask --features testkit --test it -- serve::`.

**Dependencies.** 3.1, 1.7b, 1.9. **Files.** `crates/rascal-mcp/src/server/`, `crates/rascal-mask/src/serve.rs`.
**Size.** M; tag `m3.1`.

### Task 3.3: The pack client and the isolation suite

**Description.** `[packs.*]` config; the environment allowlist with removal winning (including
every filler's `key_env`); startup, listing, manifest validation (both schemas, the annotation
block, bounds); `_harness` with `schemas/rascal/pack-context-v1.json`; result validation before
commit (schema, bounds, block correspondence, frame references); the concurrency AND rule;
timeouts, cancellation failing every in-flight call, late responses, process death, shutdown
escalation; stderr capture with redaction; `rascal tools call <pack> <tool> …` (a plan addition,
documented) for driving one pack tool from the shell; the testkit fake pack binary with each
misbehaviour selectable; the compatibility export of a real `segment` envelope (G7); the
**isolation suite** (G5): instruction-like text in filenames, prompts and tool output never
alters control flow across recipes, workspaces, fillers and packs; ids never resolve outside the
registry.

**CLI scenario.** `rascal tools list --config rascal.toml` prints the fake pack's manifest as a
client sees it; `rascal tools call fake segment --image i --prompt x` returns two instance
planes; with the pack told to return three blocks for two images the call is an operational
error and nothing is stored.

**Acceptance.**
- [ ] Each of these is a named test: schema failure, bounds breach, block mismatch, foreign
      frame reference, serial under either `false`, concurrent under both `true`, timeout failing
      every in-flight call and shutting the pack down, a late response for a tombstoned id
      ignored, process death, a credential-named variable removed despite declaration, stderr
      redacted; a concurrent-call cancellation failing every in-flight call with shutdown completed
      before the next use; a relocated `run` of a test-only recipe (`segment` → `rasterize` of
      the first region → `feather`, in the testkit; `select` arrives in M4)
      replaying to identical outputs with the pack executable absent and no call made; the pack
      span carries request size and status; the pack call row on the run page; the
      isolation suite; the `segment` export validates.

**Verification.** `cargo test -p rascal-mcp --test it -- client::` (the pure parts) and `cargo test -p rascal-mask --features testkit --test it -- packs:: isolation::` (the fake runs as `rascal fake-pack`).

**Dependencies.** 3.1, 1.7b, 2.7. **Files.** `crates/rascal-mcp/src/client/`, `crates/rascal-core/src/pack.rs`,
`crates/rascal-testkit/src/fake_pack.rs`, `crates/rascal-mask/src/{packs,tools}.rs`,
`schemas/rascal/pack-{context,annotation}-v1.json`, ADR 0019. **Size.** M (landed as L: the
manifest-in-`_meta` protocol, `many` ports and journal-backed replay were decided here).

### Checkpoint M3

- [ ] Both MCP sides green; the independent client proves the server; the isolation suite
      green; if M3 ran in parallel with M2, `m2` is merged into `m3-mcp` and the combined tree
      passes every M2 and M3 test.
- [ ] Review and test passes applied; `docs/reviews/m3-checkpoint.md`.
- [ ] Owner review → merge to `main` → push → CI green → tag `m3`.

## Milestone M4: Refine

### Task 4.1: `grow`, `shrink`, `close`

**Description.** Disc morphology on the core with `down_px` shadow growth; the postconditions
(blend and profile cleared, protect preserved); the envelope flags; the
`ramp → shrink → feather → apply` test in its final form.

**CLI scenario.** `rascal grow <mask> --px 4 --down-px 20` on the chair-blob fixture extends the
core downward; `rascal inspect --json` shows the profile gone and `blend_cleared` in the journal.

**Acceptance.**
- [ ] Geometry on a point and a blob against brute force; postconditions on all three planes;
      `ramp → shrink → feather → apply` changes no pixel outside the new support.

**Verification.** `cargo test -p rascal-ops -- grow shrink close`.

**Dependencies.** 2.2, 2.3. **Files.** `crates/rascal-ops/src/{grow,shrink,close}.rs`. **Size.** S.

### Task 4.2: `subtract`, `select`, `invert`

**Description.** `subtract` setting protect from a mask or location with growth; `select` over
instance planes with keep, drop and exclude boxes; `invert` with the target frame and the
mapping of support and protect through the frame chain.

**CLI scenario.** `rascal invert <cut-frame mask> --to raw` on the seam fixture produces a raw
frame mask whose protect covers the two edge spans (the first mask's support) and whose core is
the whole middle, now editable.

**Acceptance.**
- [ ] `subtract` precedence; `select` keep/drop/exclude on three planes; `invert` from a `cut:`
      mask: protect on the edge spans, core on the middle; from a `crop:` mask: protect on its
      bbox.

**Verification.** `cargo test -p rascal-ops -- subtract select invert`.

**Dependencies.** 4.1. **Files.** `crates/rascal-ops/src/{subtract,select,invert}.rs`. **Size.** S.

### Task 4.3: `tighten`

**Description.** The signed-derivative boundary-pair search, scoring, untightened samples,
run-wise smoothing, crossing reversion, the polygon core; the **remaining decisions** (G14):
the final tie-break after midpoint and width ties (lower sample index wins), endpoint sampling
(one-sided direction at the ends, no extrapolation beyond the path), degenerate tangents as the spec now states (a zero central difference after resampling takes
the previous sample's direction, the first such the next nonzero one; a path with no nonzero
direction is `not_applicable`), with a reversing-path fixture; round joins and end caps so an
entirely untightened stroke equals `rasterize` of the same stroke; `tighten-v1`; the
scratch fixtures (smooth and textured ground); deterministic fixtures for each decision.

**CLI scenario.** `rascal tighten --stroke s.json --image scratch-smooth.png` returns a mask
whose edges lie within one pixel of a drawn scratch's edges; on an edgeless stroke every sample
is untightened and the mask equals `rasterize`.

**Acceptance.**
- [ ] Edges within one pixel on both fixtures; reverts crossing samples; the reversing-path,
      tie-break and endpoint fixtures; an edgeless stroke equals `rasterize` exactly (caps and
      joins); untightened runs keep the stroke extent.

**Verification.** `cargo test -p rascal-ops -- tighten`.

**Dependencies.** 1.8b, 2.2. **Files.** `crates/rascal-ops/src/tighten.rs`, constants, `fixtures/images/scratch-*.png`.
**Size.** M. Landed 2026-10-29: the textured fixture draws its scratch 140 levels deep so its
edges outrank the planks' joins (about 100), which is what the scoring rule requires of a
real scratch on a textured ground; the crossing reversion cannot fire under one smoothing
kernel and is tested as a guard.

### Task 4.4: The `remove`, `repair`, `repair-area`, `protect` recipes and the acceptance script

**Description.** The four embedded recipes with their input bindings and goldens; `heal seam →
heal protect` end to end in one workspace; `scripts/acceptance.sh` first full version (G10):
install `rascal-mask` from the packaged crates into a clean directory with the sibling crates
supplied from the packages (as `cargo publish` would), the six recipes by name on JPEG fixtures
with no config file producing every required output, `serve` with the independent client
through the seam ops, CLI/serve interchange, docs coverage; wired into the package workflow;
`docs/recipes.md` complete.

**CLI scenario.** `rascal heal remove room.jpg --location chairs.json --filler exact --out run/`
then `rascal heal protect --image run/result.png --mask run/artifacts/<mask> --filler exact
--out run2/`; `scripts/acceptance.sh` passes on a clean checkout.

**Acceptance.**
- [ ] Goldens for the four recipes; the two-pass test preserves the first edit and allows a
      middle pixel to change; the acceptance script is green locally and in the package workflow.

**Verification.** `cargo test -p rascal-mask --features testkit --test it -- recipes::`; `scripts/acceptance.sh`.

**Dependencies.** 4.2, 4.3, 2.8, 3.2. **Files.** `crates/rascal-mask/recipes/`, `fixtures/masks/`,
`scripts/acceptance.sh`, `docs/recipes.md`. **Size.** M; tag `m4.1`.

### Task 4.5: The segmenter decision

**Description.** A survey in `tools/python/MODEL.md` (draft) of permissively licensed public
segmenters with text-prompt input and an exportable checkpoint: licence, redistribution terms
for an export, prompt kinds, size, export path; the owner's decision recorded in
`docs/owner-decisions.md` as branch A with the model named and its redistribution terms
confirmed, or branch B.

**Acceptance.**
- [ ] The survey cites each model's licence text; the decision row is signed and dated; the
      spec's success criterion 5 or 5b is marked as the one in force.

**Verification.** Owner signs. **Dependencies.** 0.5. **Files.** `tools/python/MODEL.md`,
`docs/owner-decisions.md`. **Size.** S.

### Checkpoint M4

- [ ] Six recipes run by name; the acceptance script is green; the segmenter decision is in.
- [ ] Review and test passes applied; `docs/reviews/m4-checkpoint.md`.
- [ ] Owner review → merge to `main` → push → CI green → tag `m4`.

## Milestone M5: Pack

### Task 5.1: The pack skeleton, `echo_region`, `fetch` and the harness

**Description.** `tools/python` as a uv project: the MCP server with the manifest annotation
block, `echo_region` (returns the region it was given as an instance plane), the `rascal-tools
fetch` command (downloads artifacts named in `artifacts.toml` into the cache and verifies
digests; a tampered digest is refused), the parity harness (`segmenter`-marked tests comparing a
tool's output to stored outputs within a stated tolerance, skipping when the artifact is not
fetched), `ruff` check and format as executable gates, `uv.lock`; `rascal.toml`
`[packs.rascal-tools]` example with `revision`.

**CLI scenario.** `rascal tools list --config examples/rascal.toml` shows `echo_region` from the
real pack; `rascal tools call rascal-tools echo_region --image i --bbox …` round-trips a region.

**Acceptance.**
- [ ] `pytest` offline green including the harness on `echo_region`; `ruff` gates green; the
      Rust pack client accepts the real pack's manifest; the env allowlist is observed by the
      pack (a test tool echoes its environment); `fetch` refuses a bad digest.

**Verification.** `uv run --frozen --offline --no-sync pytest`; `uv run --frozen --offline --no-sync ruff check . && … ruff format --check .`; `cargo test -p rascal-mask --features testkit --test it -- packs_real::`.

**Dependencies.** 3.3. **Files.** `tools/python/`, `examples/rascal.toml`. **Size.** M.

### Task 5.2: `segment` under branch A, or branch B

**Description.** Branch A: the ONNX export procedure and provenance in `MODEL.md` (upstream
source, licence, export steps, artifact URL and SHA-256, preprocessing version, onnxruntime
version, parity tolerance), `segment` with text prompts (and points where supported), parity
fixtures, `artifacts.toml`; `--prompt` binding in the `remove` recipe through the pack. Branch
B: no `segment`; the README names it as a later milestone; a test asserts the pack's tool list
does not contain `segment` and `rascal run remove --prompt` fails naming the milestone;
criterion 5b in force.

**CLI scenario.** Branch A: `rascal heal remove room.png --prompt "chair" --filler exact --config
rascal.toml` removes the chairs on the chair-blob fixture.

**Acceptance.**
- [x] Branch A: parity within `MODEL.md`'s tolerance; the export's SHA-256 recorded; no test
      downloads anything. (The export took ADR 0018's drop-in decoder, SAM 2.1 Hiera-Tiny;
      `--prompt` binds a recipe `Text` input and the prompt recipe is `remove-prompt`.)

**Verification.** `uv run --frozen --offline --no-sync pytest -m segmenter`; `cargo test -p rascal-mask --features testkit --test it -- remove_prompt::`.

**Dependencies.** 5.1, 4.5. **Files.** `tools/python/src/rascal_tools/segment.py`, `tools/python/MODEL.md`,
`tools/python/src/rascal_tools/artifacts.toml`, `crates/rascal-mask/recipes/remove.toml`. **Size.** M.

### Task 5.3: The bundle and offline setup

**Description.** `scripts/build-bundle.sh` producing one bundle per platform with the wheel,
the wheelhouse, the provisioning project and lock, `MODEL.md`, and under branch A the model
bytes with `artifacts.toml`, all digested in `bundle.toml`; `rascal tools setup` (download or
`--bundle`, digest verification, `uv sync` offline from the wheelhouse with downloads
prohibited); launch offline; the disconnected acceptance step added to `scripts/acceptance.sh`
for both tools on both platforms with the network disabled and empty caches; `docs/tools.md`
complete.

**CLI scenario.** On a machine with the network disabled and empty uv and pip caches, `rascal
tools setup --config rascal.toml --bundle ./bundle-macos-arm64` installs; `rascal tools call
rascal-tools echo_region …` works, and under branch A `rascal heal remove … --prompt "chair"`.

**Acceptance.**
- [ ] The bundle build test installs into a clean directory offline with empty caches; a
      tampered digest is refused; CI runs the disconnected step on both platforms.

**Verification.** `scripts/acceptance.sh` (bundle step); the bundle test in `pytest`.

**Dependencies.** 5.2. **Files.** `scripts/build-bundle.sh`, `crates/rascal-mcp/src/provision.rs`,
`crates/rascal-mask/src/tools.rs`, `docs/tools.md`. **Size.** M; tag `m5.1`.

### Checkpoint M5

- [x] The pack installs offline on both platforms; the branch in force is proven by acceptance.
- [x] Review and test passes applied; `docs/reviews/m5-checkpoint.md`.
- [x] Owner review → merge to `main` → push → CI green → tag `m5` (2026-11-01, after the
      `models-2026-10` release).

## Milestone M6: Lab

### Task 6.1: Corpus, protocol and matrix

**Description.** `corpus.toml` by id, SHA-256, group, licence note; `rascal corpus verify`;
`protocol.toml` with groups, arms, seed, `min_decided`, rules; `matrix.toml` with per-image bindings and arms; their schemas through the harness; the
`fan-thin-v1` constant set (the spec's fixed-width arm); `examples/`. Requires owner decision 6
(the corpus and its groups) to be recorded first. Matrix validation, including the
dependent-finder rejection, is unit-tested here; its CLI form is 6.2's scenario.

**CLI scenario.** `rascal corpus verify examples/corpus.toml` reports a missing file by id.

**Acceptance.**
- [ ] Schema tests; verify on a tampered file; matrix validation unit tests including a missing
      `protocol` and the dependent-finder rejection naming the binding; `fan-thin-v1` yields `thin` on every row of the step-sky fixture at three image widths
      (the effective upper width is `max(thin, wide_max · w)`).

**Verification.** `cargo test -p rascal-mask --test it -- lab_config:: corpus_cli::` and `cargo test -p rascal-ops --test it -- fan_thin_widths::`.

**Dependencies.** 1.6, 0.5, 2.1. **Files.** `crates/rascal-mask/src/lab/{corpus,protocol,matrix}.rs`,
`crates/rascal-ops/constants/fan-thin-v1.toml`, `schemas/rascal/`, `examples/`. **Size.** S.

### Task 6.2: `lab run`

**Description.** Cells with the pre-execution identity (including finder invocation
descriptors), attempts, `latest`, the ledger, resume by identity, `--redo`, the protocol digest
recorded; runs through `heal` with the arm's filler.

**CLI scenario.** `rascal lab run examples/matrix.toml --out lab/demo/` over the fixtures with
the fake filler; a second invocation makes no external call (the fake filler counts calls); a
`revision` bump makes a new cell; a matrix without `protocol`, and one with a dependent-finder
arm, are refused naming the field or the binding; `lab run` writes the run page per cell and
`--no-dash` skips it; a second `lab run --out lab/demo/` resumes rather than refusing the
directory.

**Acceptance.**
- [ ] Identity changes on prompt, parameter or revision changes only; resume skips complete
      cells; `--redo` adds an attempt and keeps the old one.

**Verification.** `cargo test -p rascal-mask --features testkit --test it -- lab_run:: lab_identity::`.

**Dependencies.** 6.1, 2.8, 4.4, 5.3. **Files.** `crates/rascal-mask/src/lab/{run,cell,ledger}.rs`. **Size.** M.

### Task 6.3: `lab pairs` and `lab decode`

**Description.** The static pairs page from an authored template, `assignment.json`, the
seed-drawn order and sides, `picks.json`, decode with the rejection rules and the protocol
outcome; `schemas/rascal/{assignment,picks}-v1.json` (G2); `docs/lab.md` complete.

**CLI scenario.** `rascal lab pairs lab/demo/ --arms thin,fan-ramp` writes a page; picking
writes `picks.json`; `rascal lab decode lab/demo/ picks.json` prints the per-group table and
`preferred | not_worse | inconclusive`.

**Acceptance.**
- [ ] Decode rejects a foreign or stale picks file; the outcome rules on constructed pick sets;
      the page embeds images and no key; the two schemas through the harness.
- [ ] The page shows each pair at its focus (the region both results changed, through the
      wrap edge of a panorama) at full size, whole, and from inside the sphere for an
      equirectangular frame; the attempt record names the source image.

**Verification.** `cargo test -p rascal-core --test it -- lab_decode::` and `cargo test -p rascal-mask --features testkit --test it -- lab_pairs::`.

**Dependencies.** 6.2. **Files.** `crates/rascal-mask/src/lab/{pairs,decode}.rs`, `crates/rascal-mask/pages/pairs.html`,
`schemas/rascal/`, `docs/lab.md`. **Size.** M; tag `m6.1`.

### Task 6.4: The owner's first lab round

**Description.** The owner's private corpus under `RASCAL_CORPUS` with the sky and structured
groups (owner decision 2), the `fan-thin-v1` arm from 6.1, the protocol committed, `lab run` with the owner's real local filler, the pairs page judged blind, decode
recorded in `docs/results/seam-01.md` with the numbers beside the outcome (the `thin` arm is
`fan-thin-v1` from 6.1). **Gate** (G16):
success criterion 6 is met only when the sky group is `preferred` **and** the structured group
is `not_worse`. If either fails, the results document records the failed experiment; a refit is
a new constant-set version, a new lab name and a new protocol, recorded as `seam-02`; the M7
release gate waits on criterion 6 passing or on the owner's recorded, dated scope change in
`docs/owner-decisions.md`.

**Acceptance.**
- [x] The results document exists with the protocol digest, the outcome per group and the
      measures; criterion 6 is marked met, or the gate is recorded as open. (Three rounds on
      2026-10-29: `seam-01` under LaMa failed the gate, `seam-02` showed the filler decided
      it, `seam-03` under Bria met it, sky `preferred` 18 of 19 and structured `preferred`
      14 of 16; `docs/results/seam-03.md`.)

**Verification.** Owner. **Dependencies.** 6.3. **Files.** `docs/results/seam-01.md`. **Size.** S
(code), the owner's time otherwise.

### Checkpoint M6

- [ ] The lab runs end to end on fixtures in CI and on the corpus by hand; the results document
      is in.
- [ ] Review and test passes applied; `docs/reviews/m6-checkpoint.md`.
- [ ] Owner review → merge to `main` → push → CI green → tag `m6`.

## Milestone M7: Poisson and release

### Task 7.1: The solver and `paste --mode poisson`

**Description.** The discrete Poisson problem as specified: `Ω`, source gradients, Dirichlet
from base, border handling, component anchoring, conjugate gradients in `f32` with the residual
evaluated before every test, clamping, the blend-zone fade after the solve; `poisson-v1`; wired
into `paste` and reachable through `heal` with `paste = "poisson"`; `wrap` frames refused.

**CLI scenario.** `rascal paste base.png fill.png <mask> --mode poisson --out c.png` on the
spec's constant-fill case returns `base`; `rascal heal seam … --config poisson.toml` (a filler
entry with `paste = "poisson"`) heals the step-sky fixture through the fake filler.

**Acceptance.**
- [x] The spec's cases plus the single-pixel zero-RHS case; the no-boundary component is
      `not_applicable`; non-convergence at a tiny `max_iter` is an operational error; a `wrap`
      frame is refused; a **benchmark** (not a gate) records the solve time for a 1000×1000 domain
      in release on the owner's machine in `docs/results/poisson-bench.md`.

**Verification.** `cargo test -p rascal-ops -- poisson paste::poisson` and `cargo test -p rascal-mask --features testkit --test it -- heal_poisson::`.

**Dependencies.** 2.5, 2.8. **Files.** `crates/rascal-ops/src/{poisson,paste}.rs`, constants,
`crates/rascal-mask/src/heal.rs`. **Size.** M.

### Task 7.2: The final timing run and the release

**Description.** The final timing run (G12): `cargo test --workspace` in both feature sets on
the owner's Apple silicon machine, test execution time recorded in `docs/results/suite-timing.md`
with the machine named; `cargo-dist` release archives for macOS arm64 and Linux x86_64 with the
archive smoke; the publish dry run; `scripts/acceptance.sh` from the unpacked archive;
`README.md` install section final; every success criterion's outcome and proving command
recorded in `docs/architecture.md`; the release gate: all seven criteria (5 or 5b as in force)
met, or an owner-recorded scope change for any that is not.

**CLI scenario.** `cargo install rascal-mask` from the packaged crates works in a clean
directory; `rascal run seam` from the unpacked release archive runs on the fixture.

**Acceptance.**
- [x] Timing under three minutes recorded (`docs/results/suite-timing.md`: 30.8 s and
      129.9 s); the release workflow dry run green (`dist generate --check`, `dist plan`,
      the package workflow); the archive's `rascal` runs `seam` on the fixture and the bundle
      installs from its packed archive with `segment` answering (the same document); the
      criteria table complete (`docs/architecture.md`). The release itself, the `v0.1.0` tag,
      waits on the owner's word, with the model files' release.

**Verification.** `just publish-dry-run`; `just dist-check`; `scripts/acceptance.sh`.

**Dependencies.** 7.1. **Files.** `dist-workspace.toml`, `.github/workflows/release.yml`, `README.md`,
`docs/architecture.md`, `docs/results/suite-timing.md`. **Size.** M.

### Checkpoint M7

- [x] All seven success criteria met with their proving command and outcome in
      `docs/architecture.md`, or a dated scope change recorded for any exception (checked
      against `41e09c2` at the checkpoint, 2026-11-01; no exception).
- [x] Review and test passes applied; `docs/reviews/m7-checkpoint.md` (2026-11-01).
- [ ] Owner review → merge to `main` → push → CI green → tag `m7` → tag `v0.1.0` → the owner
      publishes. (`v0.1.0` released 2026-11-01 on the owner's word; `m7` follows the review
      record above.)

## Milestone M8: Table manners

rascal conforms to the shared contract (`table-manners` v0.1.0, ADR 0020, decision 13): the
images profile and the core. The contract's text is the specification; this milestone is the
mapping. Each task is a branch merged on a green check; the checkpoint tags `m8`.

### Task 8.1: The contract pinned, and the image reference

**Description.** `table-manners = "=0.1.0"` in the workspace dependencies; `scripts/check-contract.sh`
warns when the contract has a newer tag (a CI step, never a failure). The image reference at
import: `ImageRecord` gains the source file's SHA-256 for an imported image, and
`ImageReference` is built from an image and its frame (projection and topology). The twelve
pixel vectors copied to `fixtures/contract/pixels/` with their digests, reproduced by a test.

**Acceptance.**
- [x] Every reference rascal emits validates against `urn:table-manners:images:image-reference`.
- [x] The vectors reproduce; an import records the file digest; `workspace import` prints it.

**Files.** `Cargo.toml`, `crates/rascal-core/src/image.rs`, `crates/rascal-core/src/contract.rs`
(new), `scripts/check-contract.sh`, `.github/workflows/ci.yml`, `fixtures/contract/pixels/`.
**Size.** M.

### Task 8.2: Locations across the boundary

**Description.** Conversion both ways between the contract's location and rascal's: `parts` ↔ the
`group` of a split location, every kind (`bbox`, `polygon` to 256 vertices, `line`, `stroke`,
`mask`, `whole_image`, `absent` with `reason`, `view` accepted and never emitted), `score` and
`score_kind` (`owlv2-detection` for the segmenter). The reference is verified against the imported
image (`invalid`, code `input`, on a mismatch). `--location` and `--stroke` accept a contract
location file (recognised by its `image` field) beside rascal's own. `locations.used` and
`locations.found` are recorded per run: `remove-prompt`'s findings from `segment`, the seam line.

**Acceptance.**
- [x] The contract's valid location examples round-trip through rascal's types; rascal's split
      location leaves as one two-part location. Inbound, a shape in several parts, a polygon over
      rascal's 64 vertices (its schema is the copied envelope's, ADR 0009) and a `mask` in a
      location slot are refused with the reason: rascal v0.1's limits, recorded here.
- [x] `rascal run remove img.png --location finding.json` with a squint-shaped finding masks it,
      byte-identically to rascal's own location; a finding on another image is refused (64).
- `locations.used` and `locations.found` are recorded with the header in 8.4.

**Files.** `crates/rascal-core/src/location.rs`, `contract.rs`, `crates/rascal-mask/src/run.rs`,
tests. **Size.** M.

### Task 8.3: `describe --json` and the `describe` tool

**Description.** The description document: one action per shipped recipe (`seam`, `remove`,
`remove_prompt`, `repair`, `repair_area`, `protect`, `rect`) plus `estimate`; `params_schema` from
the recipe's overridable step parameters (`set`), `filler`, `strict`, `projection`; image slots
from the recipe's inputs with roles; location slots with the kinds each takes; `outputs` (`mask`,
`result`); `cost` from the config's fillers (`may_spend`, `basis: estimated`, `needs_env`);
`duration`; `renders` (`rascal dash {run.dir} --serve --port 0`); `vocabulary`; `limits`;
`profiles: ["images"]`. The MCP tool `describe` returns the same document.

**Acceptance.**
- [x] The document validates against `urn:table-manners:core:description`; `ops list` is unchanged.
- [x] A fixture under `fixtures/contract/describe.json` is compared in the test suite.
- The `describe` MCP tool is wired with the action tools in 8.5, and `estimate` with spend in 8.6.

**Files.** `crates/rascal-mask/src/describe.rs` (new), `main.rs`, `crates/rascal-mcp/src/server.rs`.
**Size.** M.

### Task 8.4: `run.json` and the header

**Description.** Every `run` and `heal` writes `run.json` atomically and last: `status` from the
outcome (`ok`, `measured_nothing`, `not_applicable` → `completed` with the step statuses in the
body; `invalid_args`, usage and recipe errors → `invalid`; operational errors → `error` with the
code map: io → `io`, a filler endpoint → `provider`, a filler command or pack → `tool_process`, a
timeout → `timeout`, the rest → `internal`; a signal → `cancelled`), `input_identity` (the run's
identity over the recipe, the inputs and the overrides), `spend` (`estimated` from the filler's
`spend_usd_per_call`), `cache`, `images.in` and `images.out` with roles and path locators,
`locations`, `notes` from the journal, and a body with the envelope statuses, the outputs by role
and the heal report. SIGINT exits 130 and SIGTERM 143, after `run.json` is written.

**Acceptance.**
- [x] `run.json` validates against `urn:table-manners:core:result` on completed, invalid, error
      and cancelled runs; a heal whose filler fails keeps the mask in `images.out`.
- [x] The step-sky seam under the fake filler is checked field by field (its times and paths
      change per run, so a byte golden would pin nothing useful); SIGTERM and SIGINT mid-fill
      give 143 and 130 with `run.json` `cancelled`.

**Files.** `crates/rascal-mask/src/run.rs`, `heal.rs`, `main.rs`, `crates/rascal-core/src/contract.rs`.
**Size.** L, split into the header (8.4a) and the signals (8.4b) if it runs long.

### Task 8.5: `serve` under the contract

**Description.** One MCP tool per action beside the op tools (ADR 0015 unchanged for those):
image slots by `path` or `inline` with `expect`, an implicit journaled import; `return_images`,
`image_blocks` (opt-in blocks), `max_spend_usd`; results as `structuredContent` with `isError`
for `invalid` and `error`; JSON-RPC errors only before a run exists; progress on every journal
step and a heartbeat under `progress_interval_s` with a strictly increasing value; cancellation
writes `run.json` as `cancelled` and returns no result; resources `rascal://runs`,
`rascal://run/{id}/run.json`, `/journal`, `/images/{role}`; a runs root for `serve` (`--runs`,
default `$XDG_DATA_HOME/rascal/runs`); `--budget-usd` per session; paths refused over a network
transport (none in v0.1, the check is there). The testkit client grows the calls it needs.

**Acceptance.**
- [x] The independent client runs the `seam` action end to end with the fake filler, reads the
      run resource and the runs list, and sees strictly increasing progress.
- [x] A cancelled call leaves `run.json` as `cancelled`; a digest mismatch is `invalid`/`input`.
- The `rect` recipe's action is `mask_rect` (an action never shadows an op); the per-call cap and
  the session budget move to 8.6, where runs carry a price.

**Files.** `crates/rascal-mcp/src/server.rs`, `jsonrpc.rs`, `crates/rascal-testkit/src/mcp_client.rs`,
tests. **Size.** L.

### Task 8.6: The fill cache and spend

**Description.** A cache of fills keyed by the fill request's identity, shared across runs and
lab arms (`[cache] dir` in `rascal.toml`); a hit reports `cache: hit` and spend 0;
`spend_usd_per_call` on a filler entry; `estimate` returns the worst case (fills × price) and the
session budget left. The lab's identical-mask arms share one fill (the objects-01 and objects-02
finding).

**Acceptance.**
- [x] Two identical heals: the second is a hit with no filler call; a lab run with two arms whose
      masks are identical makes one external call per image.
- [x] `serve` refuses a call past the per-call cap (lowered by `max_spend_usd`) or the session
      budget with code `budget`; `estimate` reports both before anything runs.

**Files.** `crates/rascal-mask/src/heal.rs`, `filler/`, `crates/rascal-core/src/config.rs`,
`lab/`. **Size.** M.

### Task 8.7: The `playground` subcommand

**Description.** `rascal playground` finds the shared playground program (`PATH`, then the data
directory), starts it with `--tool rascal`, and when it is absent says where to get it and exits
69 (unavailable). No UI, no server.

**Acceptance.**
- [x] With a fake `playground` on `PATH` the subcommand runs it with `--tool rascal` and the
      arguments and keeps its exit code; without, the message and exit 69.

**Files.** `crates/rascal-mask/src/main.rs`, `playground.rs` (new). **Size.** S.

### Task 8.8: Fixtures, the acceptance half, docs

**Description.** rascal's conformance fixtures (`describe.json`, a `run.json` per status, the
locations) under `fixtures/contract/`, validated in the test suite against the crate's embedded
schemas, and copied to the contract repository's `fixtures/rascal/` by a pull request there; the
first half of the cross-tool acceptance test (a squint-shaped finding to `rascal run remove`) run
in rascal's CI against the tagged contract. `docs/contract.md` (new), `SPEC.md` "External
interface", `README.md`, `docs/architecture.md`.

**Acceptance.**
- [x] `just check` green; the conformance fixtures (`fixtures/contract/results/`, `describe.json`)
      are generated, masked and compared; rascal's half of the acceptance test
      (`contract_locations.rs`) passes in CI against the pinned contract.

**Size.** M.

### Checkpoint M8
- [x] Tasks 8.1 to 8.8 merged, `just check` green, CI green on both platforms.
- [x] Tag `m8` (2026-10-31, `ebb773f`). Then the contract's pin moves to v0.2 when it is tagged after squint's M7.

### Owner decisions for M8
- Actions are the shipped recipes, one each, with the filler as a parameter (a run without a
  filler is the mask alone); not one action per recipe and filler. Recommended, assumed.
- The op tools keep their envelopes and mapping in v0.1 (ADR 0020). Recommended, assumed.
- The runs root for `serve` defaults under `$XDG_DATA_HOME/rascal/runs`. Recommended, assumed.

## Milestone M9: Spend and launcher

Wafflehouse's review of rascal against the shared contract (decision 14) found two spending bugs
and asked for the launcher it defines (H1). None of this waits on table-manners v0.2: the shared
names for run association and the session (C4) are adopted after v0.2 is tagged, and rascal says
the unresolved part of a spend in its own `body` until then. The spending bugs come first. Each
task is a branch merged on a green check; the checkpoint tags `m9`.

### Task 9.1: Spend survives the status

**Description.** A filler call is *not sent* (refused before its request left rascal: no key,
connection refused, the command could not start), *known* (the filler answered: a 2xx, or a
command that exited 0; from M12, a 401 or 403 too, counting zero, table-manners v0.3 4.5), or
*unresolved* (sent with no
answer that settles it: timed out, killed, a failure status, a non-zero exit). The call record
says which (`charge`) and what it costs (`spend_usd`: the entry's price unless not sent). `heal`
notes `filler call started` before the call, so a run killed mid-call still shows it. A run's
spend is the journal's: every known and unresolved call at its price, and a started call with no
record as unresolved, whatever the run's status. `run.json` reports the total in `spend.usd` and
the unresolved part in `body.spend_unresolved_usd` with a note; never zero for a call that may
have been charged.

**Acceptance.**
- [x] A heal cancelled after the fill (serve, before `apply`) reports the price; a filler that
      times out or exits non-zero reports it as unresolved; a filler that cannot start reports 0.
- [x] `rascal heal` killed by SIGTERM during the filler call writes `run.json` with the price,
      unresolved.

**Files.** `crates/rascal-mask/src/filler/{mod,http,command}.rs`, `heal.rs`, `run_json.rs`,
`actions.rs`, tests, `docs/contract.md`, `docs/fillers.md`. **Size.** M.

### Task 9.2: The budget reserved, then settled

**Description.** `serve`'s ledger holds `spent`, `reserved` and `unresolved`. The cap and budget
check and the reservation of the call's worst case are one step under the ledger's lock; the
run's end settles it from the run's spend (9.1): the reservation released, the spend added, the
unresolved part kept as such. A run that ends without settling (a panic) settles its reservation
as unresolved spend. `estimate` reports the reserved and unresolved parts.

**Acceptance.**
- [x] Under `--concurrency 2` with a budget for one call and a slow fake filler, two calls give
      one run and one refusal before any filler call; the session total is the one price.
- [x] `estimate` during a run shows the reservation; after it, the spend.

**Files.** `crates/rascal-mask/src/actions.rs`, tests, `docs/contract.md`. **Size.** S.

### Task 9.3: Reads never wait on a run

**Description.** `describe` and `estimate` answer while a run holds the call permit, as the
resources already do: the `Actions` trait says which calls are exclusive.

**Acceptance.**
- [x] With a slow run holding the permit, `estimate`, `describe` and `rascal://runs` answer
      before it ends.

**Files.** `crates/rascal-mcp/src/server.rs`, `crates/rascal-mask/src/actions.rs`, tests. **Size.** S.

### Task 9.4: Runs listed from allocation

**Description.** `serve` creates a run's directory when it allocates the run, before the first
progress line and any paid work, so `rascal://runs` lists it as `running` with its action from
that moment.

**Acceptance.**
- [x] A slow run is in the runs list, `running`, while its first step works.

**Files.** `crates/rascal-mask/src/actions.rs`, tests. **Size.** XS.

### Task 9.5: The Wafflehouse launcher

**Description.** `rascal playground` hands off to the shared program by Wafflehouse's H1, as
squint's 7.9 does: it finds `wafflehouse` (`WAFFLEHOUSE_BIN`, absolute, never falling through when
it is wrong; then `PATH`; then `~/.local/bin`), runs it with exactly `--tool rascal` without a
shell, and keeps its exit code (128 + N for a signal). Missing on a terminal (stdin and stderr),
it offers to install: no exits 0 and changes nothing; yes runs the installer into
`~/.local/bin`, then that path; an installer failure exits 74 and runs nothing. Off a terminal,
69 with instructions. The production installer waits for an approved release artifact, so a
release build exits 69 with no offer; the installer is a testkit-only seam (`--installer`, with
`--terminal` to take the answer from stdin). The subcommand takes no arguments (64).

**Acceptance.**
- [x] Fake programs show the exact argv, a path with spaces, exit codes and signals kept, an
      invalid override 69 without fall-through, the lookup order, a symlinked program; missing:
      no 0, yes installs and runs, installer failure 74, off a terminal 69, extra arguments 64.

**Files.** `crates/rascal-mask/src/playground.rs`, `main.rs`, `tests/it/playground.rs`,
`docs/contract.md`, `SPEC.md`, `README.md`. **Size.** S.

### Checkpoint M9
- [x] Tasks 9.1 to 9.5 merged, `just check` green, CI green on both platforms; tag `m9` (2026-10-31, `a461801`).

The shared names for run association and the session are adopted in M11.

## Milestone M10: Operations (table-manners v0.4)

rascal adopts table-manners v0.4 (CONTRACT.md draft `b1ae034`, its "Changes from v0.3"): a host
runs one step alone on artifacts it already holds, so a client tests one piece at a time: Line on
an image, its Location to Rasterize, the saved Mask to Feather with a new sigma, with no finder or
filler and nothing upstream rerun (R3, decision 14). Every op becomes an ordinary contract action
with `kind: "operation"`, a `group` and typed `ports`, so an op call is a run with `run.json`, a
run reference, readiness, an estimate, progress and cancellation (4.7); the op tools and the
`workspace_*` tools of ADR 0015, with rascal's own envelope as their result, leave `serve` (no
shims). Artifacts cross by **reference** (type, schema version, identity, opaque frame, the image
they were made on, a resource), never by workspace id, and resolve by identity in the session's
store, which is how a saved binding reopens after a restart or a workspace move. A Mask leaves and
enters through the reserved `export` and `import` tools over 10.5's mask bundles (5.1). Every
result records its `invocation` (from 10.2), `rascal call <action> --arguments <file>` runs it again, and
`rascal call --bundle` replays a host's invocation bundle (4.8, 4.9). And first, ahead of all of
it: the path rule (5.2), since today `serve` reads any absolute path a caller names, which is a
live gap on v0.3 as well.

Task numbers follow the contract's priorities; 10.5 (portable masks, CLI and library) is done and
keeps its number, and the MCP tools it waited for are 10.10. The old outline's 10.1 (drafting the
profile) is done in table-manners: the coordinator wrote the operations profile from the v0.4
design memo, so nothing here drafts the contract.

**The pin.** 10.1 builds on `main` against `table-manners = "=0.3.0"` and merges on its own. From
10.2 on, the milestone branch `m10-operations` (cut from `main` after 10.1 merges) builds by path
against the v0.4 draft, as M11 and M12 did:

```toml
# M10 builds against the v0.4.0 draft by path until it is tagged and published (PLAN.md 10.15).
table-manners = { path = "../table-manners", version = "=0.4.0" }
```

where `../table-manners`, from the rascal checkout being built, resolves to a table-manners
checkout on branch `v0.4` at `b1ae034` (the settled questions) or later, and at least at the
commit that lands the v0.4 schemas, types and examples (at `b1ae034` `schemas/` is still
v0.3's, so 10.2 starts when they land). `scripts/check-contract.sh` already reads the exact
version of a path pin and warns that the build is against a draft. Each commit message names the
table-manners commit it was built against. The milestone branch merges into `main` only at 10.15,
after the owner tags and publishes v0.4.0 and the pin is `=0.4.0`. Nothing here waits on squint's
or Wafflery's v0.4 work except the tag itself.

**What rascal does not take from v0.4.** Bundles for `rascal:Plan`, `rascal:Series` and
`rascal:BandProfile` (declared `portable: none`, so a host labels them workspace-dependent;
PROPOSALS "Left for later than v0.4"). Operations served by a pack (`segment`): a description is
a local inspection (3.1, 12.2) and a pack's tools are known only by starting it, so pack tools
leave `serve` with the op tools and the segmenter stays reachable through `remove_prompt`; pack
operations are out of v0.4 for rascal (question 6, settled), since listing them from an install
would need the install to record the pack's manifests. Workspace ids in MCP arguments (CONTRACT 10.1 allows them):
references are the one form, so the id-shaped arguments go with the tools that took them.

**Order and parallel tracks.** 10.1 first, on `main`, alone: it touches the path handling and
`serve`'s flags and nothing else, so it can ship on v0.3 while the contract is reviewed. Then the
chain **10.2 → 10.3 → 10.4 → 10.6** (the pin and shapes; references, the store and the binding
refusal; operations in the description; an operation call is a run). After 10.4, **10.10** (export
and import, in a new module) runs beside 10.6. After 10.6 four tracks: **A** 10.7 (the artifact
resource, reopening after a restart and a move); **B** 10.8 (tasks list their Masks); **C** 10.9
(estimates for operations); **D** 10.12 (`rascal call`; the invocation is 10.2's), then 10.13
(the bundle replay) once 10.10 is in. A and 10.10 live mostly in new modules (`artifact_resource.rs`,
`transfer.rs`) and run in parallel with anything; B, C and D all edit `actions.rs`, `estimate.rs`
and `run_json.rs`, so they run in one session in the order 10.8, 10.9, 10.12 unless two sessions
take care to merge 10.8 first. 10.14 (the end-to-end path, fixtures, ADR 0021, docs) waits for
every track; 10.15 for 10.14, squint's v0.4 fixtures and the owner's release.

**Commits and rollback.** One commit per task (a task marked "split" may be two), `just check`
green, conventional scope (`feat(mcp)`, `feat(core)`, `feat(mask)`, `test(mask)`, `docs`), the
checklist in `tasks/todo.md` ticked in the task's own commit. Rollback checkpoints are annotated
tags: `m10.1` on `main`'s merge of 10.1 (the path rule on v0.3, the one M10 change `main` carries
before the release); on the milestone branch `m10.2` after 10.2 (a v0.4 rascal whose results
carry their invocation), `m10.3` after 10.3, 10.4 and 10.6 (operations run, bound by reference), `m10.4` after
10.7 to 10.10 (artifacts reopen by identity, tasks' Masks bind, estimates cover operations, Masks
export and import), `m10.5` after 10.12 and 10.13 (`rascal call` and the bundle replay). A task group
that goes wrong is reverted on the branch (rule 1, `git revert --no-commit` newest first: 10.13
before 10.12; 10.9, 10.8 and 10.7 before 10.6; 10.6 before 10.4 before 10.3), or the
branch is reset to the last tag after a `rescue/` reference (rule 2). 10.1 on `main` is undone
alone by rule 4 (revert its merge), which reopens the gap and is therefore a last resort. If v0.4
changes under rascal before it is tagged, the affected task is amended forward on the branch; if
v0.4 is abandoned, the milestone branch is never merged and `main` stays on `=0.3.0` with 10.1
(rule 3). After the merge, undoing M10 is rule 4, which returns the pin to `=0.3.0` and keeps 10.1;
the published crate and the contract's `fixtures/rascal/` are external state and stay.

### Task 10.1: The path rule

**Description.** `serve` reads a caller's path only inside the directories the session allows
(CONTRACT 5.2, 10.1; squint's rule, which rascal lacked: before this task `estimate::image_in`
took any absolute path that is a file, said "is not a file" otherwise, and `workspace_import` and
the recipe actions' `mask` and `protect` arguments read any path at all). `rascal serve --allow-dir
<abs>` is repeatable; each value must be absolute and name an existing directory, else `serve`
exits 64 before it answers anything, naming the flag (it is the host's argv, not a caller's
path; question 3, settled). Each is resolved once at start (`std::fs::canonicalize`: `.`, `..`
and every symlink), then narrowed by the configuration's new `[serve] allow_dirs` (absolute paths, resolved the same way),
which may only take away: the allowed set is the overlap of the two trees (question 4, settled: a
flag directory inside a configured one stays; a configured directory inside a flag one replaces
it; anything else goes), with no `[serve] allow_dirs` the flags as given, `allow_dirs = []`
allowing nothing, a configured directory that does not resolve allowing nothing (stderr names
it), and a configuration that does not load allowing nothing (fail closed, the coordinator's
review: what it would narrow is unknown; stderr says why, and `serve` still starts, its estimates
working on inline images).
With no `--allow-dir` every path is refused, closed by default; inline images keep working. The
set is fixed for the session, printed on stderr at start (the v0.3 session schema has no field for
it; question 2, settled), and from 10.2 shown as the session's `allowed_dirs`.

One function, `rascal_core::paths::PathRule::admit(slot, given, want) -> Result<PathBuf>` in a
new module (in the core, so the MCP server holds the same rule for `workspace_import`; a
person's own command line is `PathRule::Own`, which takes any path), is the only way a caller
path is read: the path must be absolute, its canonical form (every symlink resolved, the final
component included) must exist, be a regular file (for `mask` and `protect`, a stored mask
directory every entry of which also resolves inside) and lie under an allowed directory (a
component-wise prefix of a canonical path, so `/a/bc` is not under `/a/b`); the file is then read
through the canonical path, never the given one. So a symlink inside pointing out is refused and
a symlink outside whose target is inside is read: the rule is on what is read. Every failure is
one refusal with one text, `<slot>: this path is not readable here (only files inside the
directories rascal serve allows; CONTRACT 5.2)`, which names neither the path's
existence nor its kind, so a missing file, a directory, a symlink out of the tree and a path
outside it are indistinguishable. In a run it is `invalid` with code `input` (before, such paths
were `usage`); in `estimate` it is the `invalid_arguments` refusal with code `input`; in
`workspace_import` (until 10.6 removes it) a JSON-RPC invalid-params error. It is a new core
error, `Error::Input` (exit 64 on the command line, `invalid`/`input` in the contract). The rule
covers every caller path `serve` takes: image slots, `mask` and `protect` (stored mask directories until 10.3 makes them
references) and `workspace_import`. It governs `serve` only: `rascal run`, `heal` and every other
command keep taking the person's own paths (5.2); `rascal call` and `rascal import` take the flag
in 10.12 and 10.10. Writes are audited: `serve` writes only under its runs root, its workspace and
its cache (images out, inline inputs and renders already are), never at a caller path, and a test
proves it by listing. A check-then-read race (a symlink swapped between `canonicalize` and the
read) is outside the threat this guards (a caller on the host's own stdio, 5.2's purpose is to
keep an agent from naming files like a key) and is recorded as such in `docs/contract.md`.

This builds on v0.3.0 and is mergeable alone: the session resource's schema is closed at v0.3, so
`allowed_dirs` waits for 10.2; nothing else in the v0.3 shapes changes. Closing paths by default
breaks any host that passes paths today without `--allow-dir` (Wafflery's v0.3 client); the
coordinator settled that 10.1 lands now, closed by default, and Wafflery passes `--allow-dir`
(question 1).

**CLI scenario.** `rascal serve --runs r/ --allow-dir "$PWD/in"` answers a `seam` call with
`{"image": {"path": "$PWD/in/pano.png"}}` as before; the same call with `/etc/hosts`, with
`$PWD/in/../secret.png`, with `$PWD/in/link.png` pointing outside, and with `$PWD/in/missing.png`
each ends `invalid`/`input` with the same reason; `rascal serve --allow-dir in` (relative) exits 64.

**Acceptance.**
- [x] With `--allow-dir D`: a file in D (and in a subdirectory, and through a symlink that stays
      inside D) runs; a path outside D, a `..` escape, a symlink inside D to a file outside, a
      missing file inside D, a missing file outside, a directory, and a relative path each end
      `invalid`/`input` with byte-identical reasons and no import into the workspace.
- [x] With no `--allow-dir` every path is refused the same way, and an inline image still runs.
- [x] `[serve] allow_dirs` narrows (a flag directory outside it allows nothing; a configured
      subdirectory of a flag directory is the only one allowed; `[]` allows nothing) and never
      widens (a configured directory with no flag allows nothing).
- [x] A relative `--allow-dir`, one that does not exist and one that is a file each exit 64 naming
      the flag, before `initialize` is answered.
- [x] `estimate` with a refused path is `-32602` with `invalid_arguments`, code `input`, the same
      reason; `workspace_import` and the `mask` and `protect` arguments are held to the rule.
- [x] After a session of calls (images out by path, inline inputs, the dash render), every file
      `serve` created lies under its runs root, workspace or cache.
- [x] `rascal run` and `heal` still take any path the person names.

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- paths::
serve_contract:: estimate::`; `cargo nextest run -p rascal-core --test it -- config::`; `cargo
nextest run -p rascal-core --lib -- paths::`; `just check`. Done 2026-11-01 (branch
`m10-allow-dir`).

**Dependencies.** M12 (`m12`, `main` at `a80b023`); table-manners `=0.3.0`. **Files.**
`crates/rascal-core/src/paths.rs` (new: `PathRule`, `admit`, the refusal, the flags and the
overlap), `error.rs` (`Error::Input`), `config.rs` (`ServeConfig { allow_dirs }`),
`schema_drift.rs`, `crates/rascal-mask/src/estimate.rs` (`image_in` through `admit`),
`actions.rs` (`mask`, `protect`, the rule in `ContractActions`), `serve.rs` (the set resolved,
narrowed and printed), `main.rs` (`--allow-dir`), `run_json.rs` and `exit.rs` (`Error::Input`),
`crates/rascal-mcp/src/server.rs` (`workspace_import` through the rule the server is given,
none by default), `crates/rascal-testkit/src/mcp_client.rs` (`serve_command`: `serve` with the
repository and the temporary directory allowed, for the tests), `bin/independent-walk.rs`,
`crates/rascal-mask/tests/it/paths.rs` (new), the suites that start `serve`,
`crates/rascal-core/tests/it/config.rs`, `schemas/rascal/config-v1.json`,
`fixtures/schemas/config-v1/full.json`, `docs/contract.md` ("serve": the path rule),
`docs/tools.md`, `docs/fillers.md`, `docs/architecture.md`, SPEC "The MCP server and client" and
"Commands", README. **Size.** M.

### Task 10.2: The pin to v0.4 and the shapes

**Description.** The pin moves to the v0.4 crate by path (above); `contract` is `0.4.0`
everywhere. The shapes the v0.4 types force: every action (the seven recipe actions) carries
`kind: "task"` and no `group`; `vocabulary` moves to the core's object with the images profile's
four lists in it as before; `rascal://session` gains `allowed_dirs` (10.1's set, `[]` with no
`--allow-dir`); and every result carries its real `invocation` (4.8), the old 10.11 folded in
here (the coordinator's review: a stand-in would be a fabricated replay record, and the built
v0.4 schema allows `null` only on an `invalid` result, question 5). Tasks and the command line
are every result there is at this point; 10.3 and 10.6 extend the same function to what they add
(references, operations). The invocation is `action`; `arguments` as resolved, with every
default of the action's `params_schema` applied, each image as `{"expect": <image reference>}`
with no `path` or `inline`, each location as given, `max_spend_usd` kept, and `return_images`
and `image_blocks` left out; `effective` the estimate's object from the same function (12.5):
`model`, `config_identity`, and `x-rascal` (the filler and its revision, paste and
`fill_max_side`, and the constant sets in force by step); `requirements` the names of the
readiness requirements the call used (12.2's `for_call`), never a value. It is `null` only on an
`invalid` run whose arguments did not resolve. On the command line (`rascal run`, `heal`) the
invocation is written as the matching action's arguments (the recipe's action name;
`--location` and `--stroke` files as locations, `--set` as `set`), so a CLI run is callable
through `serve` and `rascal call` alike. `mask` and `protect` are still paths to stored mask
directories here; 10.3, the next task in the chain, makes them references, in the invocation as
everywhere (the milestone branch merges nothing before 10.15).
`profiles` stays `["images"]` until 10.4 adds `operations`. The description's text and
`docs/contract.md` stop mentioning MCP roots (rascal never used them). `fixtures/table-manners/`
and `fixtures/contract/describe.json` are regenerated at v0.4.

As built: the defaults applied are those `params_schema` now declares (`strict: false`, and
`seam`'s `projection: "equirectangular"`, its recipe's), and `projection` is always named as the
projection the image ran in, so a replay never depends on the 2:1 guess; the constant sets are
those of the recipe's steps whose op has sets (the recipe's, a filler's override, then `set`;
`estimate::constants`), not the heal graph's fixed ones; a requirement whose name is a path (a
filler command given by path) is recorded by its file name; a recipe file run by `rascal run` has
no action and records `custom`; a command-line run whose flags do not resolve ends `invalid`
before it starts (exit 64), so a signal can only cancel a run that records its invocation.
The split could not keep the invocation out of the first commit: the moment the pin moves, the
v0.4 result schema rejects every completed `run.json` without one, and a stand-in was ruled out.
So the first commit carries the pin, the shapes and the invocation on every result, and the
second adds the constant sets to the shared `effective` and the proofs (path and inline equal,
the estimate's `effective` equal, the credential by name, a command-line invocation replayed
through `serve`).

**Acceptance.**
- [x] `rascal describe --json` validates against the v0.4 description schema: `kind: "task"` on
      every action, `vocabulary` with the four lists, no `group`, no `artifact_types`.
- [x] `rascal://session` validates with `allowed_dirs` equal to 10.1's resolved set, and `[]`
      for a server started with no `--allow-dir`.
- [x] Every `run.json`, the runs pages, the estimates and the refusals validate against v0.4;
      `UPDATE_FIXTURES=1` regenerates `fixtures/table-manners/` and the check passes after.
- [x] Every fixture and every result in the suite has a non-null `invocation` validating against
      v0.4, but an `invalid` run whose arguments did not parse, which has `null`.
- [x] The invocation names no path anywhere but `mask` and `protect` until 10.3 (a test walks it
      for strings starting with `/`), and two calls on the same image given by path and inline
      give equal invocations.
- [x] `invocation.effective` equals the estimate's `effective` for the same call; `requirements`
      lists the credential's name and never its value (the sentinel search of 12.2).

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- describe::
serve_contract:: run_json:: table_manners_fixtures:: estimate::`; `scripts/test-checks.sh`;
`just check`. Done 2026-11-01 (branch `m10-operations`, built against table-manners `d9e6058`).

**Dependencies.** 10.1; a table-manners checkout on branch `v0.4` with the v0.4 schemas and types.
**Files.** `Cargo.toml`, `Cargo.lock`,
`crates/rascal-mask/src/{describe,actions,run_json,estimate,main}.rs`,
`crates/rascal-mask/src/invocation.rs` (new),
`crates/rascal-mask/tests/it/{describe,serve_contract,run_json,paths,estimate,table_manners_fixtures}.rs`,
`fixtures/table-manners/`, `fixtures/contract/`, `docs/contract.md`, SPEC, README,
`docs/architecture.md`. **Size.** M (split: the pin and shapes, then the
invocation).

### Task 10.3: References, the store and the binding refusal

**Description.** The artifact reference (3.4) and its resolution, in `rascal-core`, used first by
the one place today's actions take a native artifact: `protect`'s `mask` argument and the
`protect` argument of `remove` and `remove_prompt` (paths to stored mask directories since M8).

- **Types.** `ArtifactType::contract_type()` maps rascal's types to the contract's names: `Image`
  → `image`, `Location` → `location`, the rest `rascal:Mask`, `rascal:BandProfile`,
  `rascal:Series`, `rascal:Plan`; every record's schema version is 1. A type string with another
  tool's prefix, an unknown name or an unprefixed rascal name is no type of rascal's.
- **The reference.** `contract::artifact_ref(workspace, artifact)` builds `{type,
  schema_version: 1, tool: "rascal", identity, frame, on, resource}`: `identity` is the
  artifact's identity (`sha256:<hex>`, SPEC "Artifacts and identity"), except an `image`, whose
  identity is `sha256:<file_sha256>` (3.4; rascal's own image identity stays internal); `frame` is
  the artifact's frame identity (`sha256:<hex>`; null for no type rascal has today, since every
  type is framed); `on` is the image reference (8) of the image at the root of the frame chain when
  the workspace holds one, else left out; `resource` is `rascal://artifact/<identity>`, carried on
  every reference as the built v0.4 schema has it (question 5; served from 10.7). Workspace ids
  never appear.
- **The store.** The session's store is `serve`'s workspace (`--workspace`, its default
  unchanged). `Workspace::resolve(type, identity)` finds an artifact by identity in the current
  generation (an index built from the registry once per generation, not a scan per call); an
  `image` resolves by the source file digest every import records (8.1, M8).
- **Binding.** `bind(port, expected type, reference) -> Result<Artifact, Binding>` checks, in
  order, the type (`wrong_type`), the schema version (`wrong_version`) and resolution
  (`unresolved`), and the caller checks frames across ports (`wrong_frame`, 10.6). Each problem
  carries `port`, `index` for an item of a `many` port, `expected` ("rascal:Mask v1"), `got` (the
  type and version given, or the identity) and, for `unresolved`, a `remedy` by type: a Mask,
  "Import its bundle with the import tool (rascal import), then call again."; a location, "Pass the
  location itself, or import it with the import tool."; an image, "Pass the image by path or
  inline, with this reference as expect."; a `portable: none` type, "This artifact cannot move
  between workspaces; start rascal serve on the workspace that made it." A call with several
  problems names them all, in argument order.
- **The refusal.** A call whose references do not bind is refused before any run: JSON-RPC
  `-32602` with refusal data `{"reason": "binding", "run": null, "bindings": [...]}` (4.7), through
  M12's structured `CallError`, so it allocates nothing, writes no run, spends nothing and
  releases its `client_run_id` (12.7). The recipe actions' `mask` and `protect` arguments take a
  `rascal:Mask` reference and nothing else (the path form goes, no shim; the `params_schema` names
  the reference schema); the resolved mask enters the run's own workspace through 10.5's
  import (in memory, with its frame chain), so the recipe runs as today and a mask in a `crop:`
  frame now works where M12's path form needed the making run's `--workspace`. The estimate's
  binding (12.5) digests the reference by its identity, as it digested the stored mask's identity
  before. Until 10.10, a test puts a Mask
  into the store with `rascal workspace import-mask` (10.5).

**CLI scenario.** With a store holding a mask from `rascal workspace import-mask`, a served
`protect` call with that mask's reference runs; the same call naming an identity the store does not
hold is refused `binding`/`unresolved` with the import remedy and its `client_run_id` is accepted on
the corrected retry.

As built: `bind` returns what the store holds under the identity (`Workspace::Resolved`: type,
local id, rascal's identity, frame), not a loaded artifact, and `bind_all` binds a call's
references naming every problem; the caller loads or exports what it needs. A `rascal:` type a
tool other than rascal made is `wrong_type` too. `on` is the root image's file digest with the
root frame's size, projection and topology and no pixel digest, so every store gives an artifact
the same reference (one holding the image and one holding only the bundle that names it agree).
The Mask argument's `params_schema` writes the reference's shape out (the schema's id in
`$comment`) instead of a `$ref`, since every tool's input schema must stand alone; its `type` is
any type name, so a wrong one reaches the `wrong_type` refusal. The estimate checks a
reference's shape and digests its identity without reading the store. With `mask` and `protect`
no longer paths, the path rule's mask-directory kind goes (`PathRule::admit` admits regular
files only). On the command line, `--mask` and `--protect` stay stored mask directories (a
person's own paths); the invocation records each as the reference the workspace around it (or
`--workspace`) gives, or, for a directory no workspace holds, one built from the mask, which a
raw frame alone allows. No task takes two references, so "a call with two bad references names
both" is proven on `bind_all`; a served call refuses each problem as its own entry, `port` the
argument's name.

**Acceptance.**
- [x] `artifact_ref` for an image, a location, a Mask in a raw frame and a Mask in a `crop:` frame
      validates against `urn:table-manners:operations:artifact-ref`; the image's identity is
      `sha256:<file_sha256>`; `on` is the root image's reference; no id appears in any reference.
- [x] `Workspace::resolve` finds each type by identity; a workspace reopened (a restart) and a
      workspace directory moved and reopened resolve the same identities (the library half of the
      old 10.6).
- [x] A served `protect` call with the mask's reference runs and gives the same mask identity as
      M12's path form did on the same inputs; `wrong_type` (a location's reference, a reference of
      type `squint:Thing`), `wrong_version` (schema version 2) and `unresolved` (with its remedy)
      are each a `-32602` `binding` refusal validating against `refusal.json`, with no run
      directory and the `client_run_id` accepted on the retry; a call with two bad references
      names both.
- [x] A path where a reference is expected is `-32602` invalid params (the input schema rejects
      it); an estimate's binding is unchanged for the same mask.

**Verification.** `cargo nextest run -p rascal-core --test it -- artifact_ref:: workspace::`;
`cargo nextest run -p rascal-mask --features testkit --test it -- binding:: serve_contract::
estimate::`; `just check`. Done 2026-11-01 (branch `m10-operations`, built against table-manners
`d9e6058`).

**Dependencies.** 10.2, 10.5. **Files.** `crates/rascal-core/src/{artifact,contract}.rs`,
`crates/rascal-core/src/workspace/mod.rs` (`resolve`, the identity index),
`crates/rascal-core/src/binding.rs` (new), `crates/rascal-mask/src/{actions,estimate,describe}.rs`,
`crates/rascal-core/tests/it/artifact_ref.rs` (new), `crates/rascal-mask/tests/it/binding.rs`
(new), `serve_contract.rs`, `estimate.rs`, `docs/contract.md`. **Size.** M.

### Task 10.4: Operations described

**Description.** `describe` emits every built-in op as an action (3, 4.7), and the description
becomes an operations tool's: `profiles: ["images", "operations"]`, top-level `artifact_types`,
`vocabulary.group`. For each of the 24 ops of `rascal_ops::registry()` (SPEC "Operations" but
`segment`, which is a pack's):

- `name` the op's name (no op shares a name with a recipe action or a reserved name: the `rect`
  recipe is already `mask_rect`; a test keeps it so), `title` the name in words, `purpose` from
  the manifest (its `purpose`, with `when_to_use` and `when_not_to_use` appended as sentences),
  `kind: "operation"`,
  `group` one of `find`, `refine`, `prep`, `out`, `inspect` (SPEC's table, carried by a new
  `Op::group()` so a new op cannot forget it), listed in `vocabulary.group`;
- `ports`: `in` and `out` from the manifest's ports, each with `name`, `type` (10.3's mapping),
  `required`, `many` and `purpose`;
- `params_schema` the manifest's parameters (the same JSON Schema `serve` builds from them today)
  plus `constants` as an `enum` of the op's constant sets when it has any, with the default set the
  op uses; no `max_spend_usd` and no image or location slot (ports only: `images: []`,
  `locations: []`), `outputs` naming the roles of the images its out-ports make and `locations:
  ["found"]` when an out-port is a `location`;
- `cost: {"may_spend": false}`, `readiness` `ready` with no requirements (a built-in op reads no
  configuration), `duration` from the manifest's `cost.seconds`, and `renders: []` (the built
  v0.4 schema's form, question 5; the dash reads a task's run workspace; an operation's picture
  is its preview).

The seven recipe actions stay `kind: "task"` with no group. `artifact_types` lists `location`
(`portable: "json"`, `preview: true`), `rascal:Mask` (`bundle`, `true`), `rascal:BandProfile`,
`rascal:Series` and `rascal:Plan` (`none`, `false`), each with `schema_versions: [1]` and a
purpose, and never `image` (3.3). Every result now carries `artifacts: {"in": [], "out": []}`, so
every document agrees with `profiles` before 10.6 fills them. This task changes the description
only: `tools/list` keeps today's op tools, whose names the actions now share, until 10.6 swaps
them in one step (the description and `tools/list` disagree on the branch for one task).

As built: operations follow the tasks, by group in pipeline order (`find`, `refine`, `prep`,
`out`, `inspect`) and then by name. `title` is the op's name capitalized; `purpose` is the
manifest's with "When to use: …" and "When not to use: …" appended (the op tool's text, without
its output guidance). `params_schema` is the one `Manifest::params_schema` gives both this and
`serve`'s op tools, so the op tools' `constants` gains the same `default`. `outputs.images`
names each image out-port by its port name and adds `preview` when an out-port is a location or
a Mask, since 4.7 lists every preview in `images.out` and 11.2 has `outputs` name every role
returned. `duration.max_s` is a hundred times `cost.seconds`, at least 60 seconds. The pack
client's ops take `find` (a pack's ops are finders and are never described). `rascal describe`
without `--json` prints each action's kind and group. In `serve`, the action tools are the
tasks only, so `tools/list` is unchanged until 10.6.

**Acceptance.**
- [x] `rascal describe --json` (no configuration, and the M12 test configuration) validates
      against v0.4 with `operations` in `profiles`: `ports` on every operation and on no task,
      `group` on every operation and in `vocabulary.group`, `artifact_types` with `bundle`, `json`
      and `none` types, with and without previews, and no `image`; no action name is reserved or
      repeated.
- [x] Each operation's `params_schema` accepts the manifest's documented defaults and rejects an
      unknown parameter; its port names never clash with a parameter.
- [x] Every result validates with `artifacts` present and empty.

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- describe::
run_json::`; `cargo nextest run -p rascal-ops --test it`; `just check`. Done 2026-11-01 (branch
`m10-operations`, built against table-manners `4dd1d2f`, whose schemas and types are
`d9e6058`'s).

**Dependencies.** 10.3. **Files.** `crates/rascal-core/src/op.rs` (`Op::group`, `Group`),
`crates/rascal-ops/src/*.rs` (each op's group), `crates/rascal-mask/src/{describe,run_json}.rs`,
`crates/rascal-mask/tests/it/describe.rs`, `fixtures/contract/describe.json`. **Size.** M.

### Task 10.5: Native artifacts portable

**Description.** A Mask leaves one workspace and enters another with the same identity, as a
file a person can hand over: a **mask bundle**, an uncompressed ustar archive (`tar`, already in
the lock for `tools setup`; ADR 0022) whose members are, in this order, `bundle.json` (the
manifest: `mask_bundle_version`, the mask's identity, its frame chain from the root raw frame to
the mask's frame, and every other member's path, size and SHA-256) and `mask/<file>` for
`mask.toml` and each file it names (the three planes, the band profile, the sigma series, each
prescription's series), sorted by name. Every header is fixed (mode 0644, uid and gid 0, mtime 0,
no user or group name), so the same stored mask always gives the same bytes and the bundle's
SHA-256 is a stable name for it. Export reads nothing but the workspace; import trusts nothing in
the file: it reads at most 256 MiB, at most 16 members, `bundle.json` at most 1 MiB, member names
at most 64 bytes and plain; each member's size is checked against the bytes that remain before
it is read; then it checks the manifest against the members, the archive against the canonical
bytes rebuilt from them, each frame's identity recomputed from its fields and its parent, the
record's frame against the chain's last, every mask invariant, and the identity recomputed from
the pixels against both the record and the manifest. Only then does it commit, as one `import`
record (the bundle's SHA-256 and file name in its parameters), registering any frame of the chain
the workspace does not know: a frame the workspace has under another id keeps the workspace's id
(the record's `frame` follows; ids are not identity), and a `crop:` id held by a different frame
takes the next free crop number. A mask whose identity is already present publishes nothing and
answers with the existing id. A file that is a PNG, a JPEG, a directory or any archive that is not
a mask bundle is refused naming what it is: a rendering is an `Image` and never a Mask (SPEC
"Frames and images"). Locations import from JSON (rascal's own location or a table-manners one,
the two shapes `--location` takes, through one function shared with `run`), validated against a
named image of the workspace. The commit itself gains the guard the import relies on: a frame id
already bound to a different identity is refused, never silently kept (concurrent `window`s could
otherwise both take `crop:<n>`). CLI and library only; the MCP tools are 10.10's, over the
operations profile's artifact reference, which this does not presume.

**CLI scenario.** `rascal run seam pano.png --out a/`, then `rascal workspace export-mask a/
<mask id> --out seam.mask.tar` prints the identity and `sha256:<bundle digest>`; `rascal workspace
import-mask b/ seam.mask.tar --sha256 <digest>` into a fresh workspace prints a new id, the same
frame and the same identity; `rascal inspect --json` on both agrees; `rascal workspace import-mask
b/ a/seam-grey.png` exits 64 naming a PNG; `rascal workspace import-location b/ finding.json
--image img-1` prints a location id.

**Acceptance.**
- [x] Export, then import into a fresh workspace, yields the same mask identity, the same frame
      identities for the whole chain (raw, cut, crop), and identical planes, profile and
      prescriptions; exporting the imported mask gives a byte-identical bundle.
- [x] The same mask exported twice gives the same bytes and the same bundle digest.
- [x] A tampered bundle is refused with an error naming the fault and nothing is committed (the
      generation number unchanged, no new file under `artifacts/`): any byte of any member
      flipped, a member missing, an extra member, a member renamed, a truncated archive, bytes
      after the end, a frame chain that is wrong (a frame's identity, a broken parent link, a
      chain that does not end at a raw frame, a mask frame other than the chain's last), a
      manifest identity other than the record's, a `--sha256` that does not match.
- [x] A PNG, a JPEG, a directory, an empty file, a text file and a tar that is not a mask bundle
      are each refused as not a mask bundle, exit 64, with nothing committed; `workspace export-mask`
      of an `Image` or a `Location` is refused naming its type.
- [x] Hostile input is bounded: a file over 256 MiB is refused before it is read, a header
      claiming more bytes than remain is refused before any allocation of that size, a
      `bundle.json` over 1 MiB, more than 16 members and a member name that is not plain are refused.
- [x] Importing a bundle whose identity the workspace already holds publishes nothing (no new
      generation) and prints the existing id; importing into a workspace holding a different
      `crop:<n>` registers the crop under a free number with the identity unchanged.
- [x] A commit naming a frame id the generation binds to another identity is refused.
- [x] `import-location` takes rascal's location and a table-manners location (8.2's shapes),
      validates it against the named image (a location on another file, a frame other than the
      image's, a seam-straddling bbox: refused, 64) and registers it; the same file twice gives
      the same id.
- [x] `schemas/rascal/mask-bundle-v1.json` through the drift harness; `docs/workspace.md`, SPEC
      "Workspace" and "Commands", README.

**Verification.** `cargo nextest run -p rascal-core --test it -- mask_bundle:: frames_identity::
workspace::` and `cargo nextest run -p rascal-mask --features testkit --test it --
portable_cli::`; `just check`.

**Dependencies.** M1 (1.2, 1.3, 1.7a), 8.2. **Files.** `crates/rascal-core/src/bundle.rs`
(new), `mask.rs` (`files`, one reader for a directory and for members), `frame.rs` (`verify`),
`contract.rs` (`location_from_json`, moved from `crates/rascal-mask/src/run.rs`),
`workspace/{mod,commit}.rs`, `crates/rascal-core/Cargo.toml` (`tar`), `schema_drift.rs`,
`schemas/rascal/mask-bundle-v1.json`, `fixtures/schemas/mask-bundle-v1/`,
`crates/rascal-mask/src/{main,workspace_cmd}.rs`, tests
`crates/rascal-core/tests/it/mask_bundle.rs`, `crates/rascal-mask/tests/it/portable_cli.rs`,
`docs/adr/0022-mask-bundles.md`, `docs/workspace.md`, `SPEC.md`, `README.md`. **Size.** M.

**Commits.** (1) this plan, the SPEC lines and ADR 0022; (2) `feat(core)`: frame verification
and the commit's frame-id guard; (3) `feat(core)`: the mask's files in memory, the bundle
writer and reader, `Workspace::export_mask` and `import_mask` (one commit: the writer's
determinism is tested through the reader); (4) `feat(mask)`: `workspace export-mask` and
`import-mask`; (5) `feat(mask)`: `workspace import-location`, the checklist ticked. Each leaves
`just check` green. The rollback point is commit (1): the plan and the ADR stand alone, and
reverting (2) to (5) together (`git revert --no-commit`, newest first)
removes the feature with nothing else depending on it; (2) alone is a fix worth keeping.

### Task 10.6: An operation call is a run

**Description.** `serve` offers one MCP tool per action of the description and nothing else
(plus the reserved tools): the op tools, the pack tools and the five `workspace_*` tools of ADR
0015 leave `rascal_mcp::server` (no shims; `workspace_import`'s job is an image port's path or
inline image, `workspace_list` is the host's own references, `workspace_describe` and
`workspace_read` are the artifact resource of 10.7, `workspace_dash` is the dash render). An
operation's MCP input schema is its `params_schema` plus one property per in-port (4.7): an
artifact reference, a non-empty array of them for a `many` port, and for an `image` port also an
image in (10.1, through 10.1's path rule) and for a `location` port also a location (9), which is
imported into the store as `workspace import-location` does, validated against the call's image.

A call runs through the machinery the recipe actions already use (`ContractActions::call`): the
claim, readiness, the permit (an operation is exclusive like any run), cancellation, the run
directory under the runs root with its journal and `run.json` written atomically and last,
progress with the run's reference, and the limits check (an operation's worst case is 0, so it
only checks the arguments). Before allocation it binds every in-port with 10.3's `bind` and then
checks frames across ports with a new `Op::check_frames(inputs)` (default: every framed input
shares one frame identity; `unwindow`, `put`, `invert`, `window` and `measure` state their own
relations), so a bad binding of any kind is the `binding` refusal, never a run; the op's own
validation stays as a second line. The run invokes the op with `Workspace::invoke_with` on the
store, so its outputs commit there (one operation record, as for the op CLI) and resolve by
identity at once; the run directory holds the journal line and the previews, not a workspace.
Upstream steps never rerun: an operation works on what it is given or refuses.

The result is a v0.4 result: `status` from the envelope (`ok` and `measured_nothing` are
`completed`; `not_applicable` is `completed` with the reason in `notes` and empty
`artifacts.out`, since nothing was published; `invalid_args` is `invalid`/`usage`; an operational
failure `error` with its code); `artifacts.in` one entry per bound input (`preview: null`) and
`artifacts.out` one per output, a `many` port once per item in order, each with its reference and,
for a type with `preview: true`, a **preview**: an image out with role `preview`, PNG, written into
the run directory and located as `return_images` asks. A Mask's preview is `inspect`'s overlay of
the mask on its frame's image when the store holds that image, else its grey rendering
(`render`); a location's is its rasterized outline drawn on its image. One function,
`preview::draw(store, artifact) -> Option<Image>`, deterministic, so the same artifact always gives
the same preview bytes (10.7 reuses it). Images an operation takes appear in `images.in` with the
port's name as `slot`, images it makes and every preview in `images.out`, locations taken in
`locations.used` and made in `locations.found` (4.7, 11.1); the native envelope goes in `body`
(`{"envelope": …}`). `input_identity` is the operation record's input digest.

As built: one commit (not split). `rascal_mcp::server::Server::new(actions, concurrency)`
serves the binary's `Actions` and nothing else (a test reads `server.rs` for any op, registry,
pack, workspace or envelope); `rascal_mcp::jsonrpc` keeps only what the server and the pack
client share, so the envelope's tool-result shape and the error-class mapping went with the op
tools. `tools/list` is the description's actions in its order, then `estimate` and `describe`.
`serve` still starts each configured pack once at start, only to learn whether it starts (the
segmenter's readiness requirement, 12.2), then stops it. An in-port value with an `identity` is a
reference; an `image` port also takes an image in (its projection `expect`'s, else the 2:1 guess)
and a `location` port a location, read against the call's first image, else against the root
image (`on`) of an input in a raw frame; a missing required port, an empty `many` list and a
malformed reference are plain invalid params. The binding refusal holds every problem of the
call (type, version and resolution per reference, then `wrong_frame` over the bound ones) in
port and item order. Values are read only once the run exists, so a refused path is
`invalid`/`input`, and the frame check runs again over every input in the run (a mismatch
`invalid`/`usage`); whatever stops the run before its invocation resolves is `invalid`, the one
status 4.8 allows a null invocation. `Op::check_frames` defaults to `same_frame` (by frame
identity); `op::derived_frame` gives `put` (the canvas in the `cut:` frame of the image's, the
plan for the image's frame) and `unwindow` (the crop in a `crop:` frame of the image's); `measure`
exempts its line; `invert` has one input; `window` states the default. `op::resolve_step` is the
one parameter resolution (the constant set's values, the manifest's defaults, `constants` named)
that `Workspace::invoke_with` and the invocation share, so `invocation.arguments` are the
parameters as the op ran them and a replay passing them explicitly runs the same values;
`effective` is `{model: null, config_identity, x-rascal: {constants}}`. The op's step id is the
run's id; the run writes its own journal line (the store's journal has the same op line). An
image out is copied into the run directory as `<output>.png`, role its port's name, and its
identity is the digest of the PNG the store wrote; for such an image to bind again by
reference, `Workspace::resolve` now also indexes every image rascal made by the digest of its
stored PNG (each read once per image per process; an imported image keeps its import's digest).
A preview is `preview-<output>.png`; `preview::draw` finds a Mask's or location's image through
the records that made it (the producing record's outputs, then its inputs, nearest first), else
the one imported image of its frame, and draws a location on a plain canvas of its frame when
the store holds none, or the image alone for a location with no interior. `not_applicable`'s note
is `<op>: not_applicable: <reason>`; `spend` is a measured zero. A call naming an estimate in
`_meta` is `stale_estimate` until 10.9 issues estimates for operations. Under the testkit,
`RASCAL_TESTKIT_OPERATION_DELAY_MS` holds an operation's run before its op runs (after its
invocation resolves), so a test can cancel one queued and one running. The independent walk
(`independent-walk`, the acceptance script) runs `cut` on the image by path and the seam
operations by reference, and prints the mask's identity, which the script finds with
`workspace list`.

**CLI scenario.** Through the independent client: `line` on `{"image": {"path": …}, "column":
0.5, "width_px": 24}` completes with a `location` out and its preview; `rasterize` on
`{"location": <that reference>, "image": <the image's reference>}` completes with a `rascal:Mask`
out and its preview; `feather` on `{"mask": <that reference>, "sigma_px": 6}` completes with a new
Mask whose identity differs from the input's; `line` was not rerun (one `line` record in the
store's journal).

**Acceptance.**
- [x] `tools/list` equals the description's actions plus `describe`, `estimate` (and from 10.10
      `export` and `import`); no op, pack or `workspace_*` tool remains, and the server crate no
      longer knows ops.
- [x] Line → Rasterize → Feather runs through the independent client one call at a time; every
      `run.json` validates against v0.4 with `artifacts.in` and `artifacts.out`, a preview on every
      Mask and location out and none on another type, and the same images and locations in
      `images` and `locations`; the run resources and the runs list include them.
- [x] A `many` port (`select`'s planes) takes an array of references and lists each in
      `artifacts.in` in order; a bad item is refused naming its `index`.
- [x] `wrong_frame`: `paste` given a mask drawn in a `crop:` frame and a base in the raw frame is
      the `binding` refusal naming the port, with no run; each of the four problems from a call
      validates against `refusal.json` and releases its `client_run_id`.
- [x] An op's `not_applicable` (`rasterize` on a `view` location) completes with a note and no
      out; an `invalid_args` is `invalid`/`usage`; cancellation while queued leaves no run and
      during the run a `cancelled` `run.json`.
- [x] The same artifact gives byte-identical previews in two runs.
- [x] `isolation.rs` and `serve.rs` (which drove the op tools) drive the operations instead,
      proving the same isolation and protocol properties.

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- operations::
serve:: serve_contract:: isolation::`; `cargo nextest run -p rascal-mcp --test it`; `cargo nextest run -p rascal-ops --test it -- frames::`;
`cargo nextest run -p rascal-core --test it -- artifact_ref::`; `just check`. Done 2026-11-01 (branch
`m10-operations`, built against table-manners `4dd1d2f`).

**Dependencies.** 10.4. **Files.** `crates/rascal-mcp/src/server.rs` (the op, pack and workspace
tools removed; the server serves `Actions` only), `crates/rascal-core/src/op.rs`
(`check_frames`), the five ops with frame relations, `crates/rascal-mask/src/operations.rs` (new:
an operation as a run), `preview.rs` (new), `actions.rs`, `run_json.rs`, `serve.rs`, `packs.rs`
(no pack tools in `serve`), `crates/rascal-testkit/src/mcp_client.rs`,
`crates/rascal-mask/tests/it/operations.rs` (new), `serve.rs`, `isolation.rs`,
`serve_contract.rs`, `docs/tools.md`, `docs/contract.md`. **Size.** L, split into the serving
swap with Line, Rasterize and Feather (10.6a) and the remaining ops' frame relations, `many`
ports and the rewritten isolation suite (10.6b) if it runs long.

### Task 10.7: The artifact resource; reopening after a restart and a move

**Description.** `rascal://artifact/{identity}` (4.7) returns `{contract, tool, artifact:
<reference>, preview: <image out or null>, record}` for any artifact the store holds (the record
is what `workspace_describe` returned: the tool's own, which a host never reads), and
`rascal://artifact/{identity}/preview` the preview PNG itself, for a type with `preview: true`.
The preview is 10.6's `preview::draw`, written once into rascal's cache
(`$XDG_CACHE_HOME/rascal/previews/<identity>.png`, outside the store and every run directory) and
located by path on stdio. Both templates are in the description's `resources`. A read of an
identity the store does not hold (or of `/preview` for a type without one) fails with refusal
data `{"reason": "binding", "run": null, "bindings": [{"port": null, "problem": "unresolved",
"expected": …, "got": <identity>, "remedy": …}]}` under JSON-RPC `-32602`, like every refusal
(question 10, settled). Reads never wait for the permit (4.5).

This is the old 10.6, persistence and relocation, end to end: a host stores references, restarts
`serve`, and reads and binds them again; after the workspace directory moves, the host starts
`rascal serve --workspace <new>` and the same identities resolve. rascal never searches folders
for an artifact.

As built: the identity alone finds the artifact (`Workspace::resolve` for each type in turn, so
the read needs no type); a malformed identity is the same `unresolved` refusal. The refusal's
`expected` reads "an artifact this store holds (any rascal type, v1)" (for `/preview`, one whose
type has a preview), and the remedy names the import tool or starting `serve` on the workspace
that holds it; `/preview` of a Plan, Series or band profile says its type has none. `record` is
rascal's own record as `workspace_describe` gave it (an image's, a Mask's or a location's record;
a band profile, series or plan as stored). The cached preview is named by identity, as specified,
and reused once written: a Mask previewed first in a store that lacked its frame's image keeps the
grey rendering when another store reads it (a preview is for people and never stands for the
artifact). The preview's image out is built by `preview::image_out`, which the operation results
now share, and the cache root by `dash::cache_root`, which the dash document shares.
`Actions::read` became fallible (`Result<Option<ResourceBody>, CallError>`) so a read can carry
the refusal, sent like a call's. The fixtures with the description's `resources` were
regenerated; an artifact-resource fixture for table-manners waits for 10.14.

**Acceptance.**
- [x] A Mask's and a location's resources validate against the v0.4 artifact-resource shape with
      a preview; a Plan's (from `cut`) has `preview: null`; `/preview` returns a PNG equal to the
      run's preview.
- [x] After Line → Rasterize, a new `serve` on the same workspace reads both references and a
      `feather` call bound to the saved Mask reference completes; after the workspace directory is
      moved and `serve` started on the new path, the same.
- [x] An identity the store does not hold is the `binding`/`unresolved` refusal with `port: null`
      and a remedy; a read while a slow run holds the permit answers at once.

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it --
artifact_resource:: serve_contract::`; `just check`. Done 2026-11-01 (on `m10-operations`).

**Dependencies.** 10.6. **Files.** `crates/rascal-mask/src/artifact_resource.rs` (new),
`actions.rs` (`resources`, `templates`, `read`), `describe.rs` (`resources`),
`crates/rascal-mask/tests/it/artifact_resource.rs` (new), `docs/contract.md`,
`docs/workspace.md`. **Size.** S.

### Task 10.8: Tasks list their Masks

**Description.** Every action fills `artifacts`, tasks included (4.7): a recipe action lists in
`artifacts.out` each Mask its recipe outputs (`port: "mask"`, the recipe's output name), with its
reference and preview, on `completed` and also on `error` and `cancelled` when the mask was made
(2.3: a salvaged mask can be bound to another filler's call or to an operation); a `protect` or
`mask` argument appears in `artifacts.in`. A task still runs in its own run directory with its
own workspace (self-contained runs, 1.9; the dash and `--replay` read it), so when the run ends
`serve` publishes each output Mask into the session's store through 10.5's import (in memory, the
same checks, idempotent by identity, its frame chain registered), and the reference resolves
there. `rascal run` and `heal` on the command line fill `artifacts` the same way, with references
into the run's own workspace (the resource resolves only in a server whose store holds it, which
the reference does not promise).

As built (the coordinator's decisions, 2026-11-01): the run records its own Mask. When the step
that makes the recipe's `outputs.mask` completes, fresh or from the cache (the same identity, so
rightly this run's), `Run::record_output` writes its identity into `outputs.json` in the run
directory; `run.json` lists exactly that, on `completed`, `error` and `cancelled` alike, and no
Mask when the step never completed. A workspace's records are never searched for "the latest
record of the step", which a shared `--workspace` would get wrong. The preview is drawn in the run's
workspace, which holds the image, so it is the overlay; it is `preview-mask.png` in the run
directory and in `images.out`. `serve` publishes before `run.json` is written: the image file
the run read (skipped when the store resolves its digest; imported in the projection of the
Mask's root frame), then the Mask's bundle exported from the run's workspace and imported with
its frame chain; the run's preview then seeds 10.7's identity-named cache, so the resource's
preview equals the result's (a test also clears the cache and finds the store's redraw equal).
No `publish_mask` was added: `import_mask_bytes` already applies 10.5's checks and is idempotent
by identity (an `already_present` import adds no record). A failed publication is a note in the
result, never a lost run. `artifacts.in` comes from the invocation's `mask` and `protect`
references, which 10.3 already writes on both paths. `outputs.json` and `preview-mask.png` are
reserved run-directory names (a render may not take them). The testkit's `serve_command` now
sets `XDG_CACHE_HOME` under the temporary directory, so tests never write the person's cache.
The contract result fixtures and the table-manners `run-*` fixtures were regenerated (their
Masks and previews); 10.6's test helpers that held a task's `artifacts` empty now check the
10.8 shape.

**Acceptance.**
- [x] A served `remove` lists its Mask in `artifacts.out` with a preview, validating against v0.4;
      a `feather` call bound to that reference, in the same session, completes.
- [x] A heal whose filler fails (`error`) still lists the Mask, and it binds.
- [x] Two identical `seam` calls publish one Mask (one import record, the second none).
- [x] `rascal run remove … --out r/` writes `run.json` with the same `artifacts.out` reference as
      the served call on the same inputs.

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- run_json::
serve_contract:: operations::`; `just check`. Done 2026-11-01 (on `m10-operations`).

**Dependencies.** 10.6. **Files.** `crates/rascal-mask/src/{actions,run_json,run,heal}.rs`,
`crates/rascal-core/src/workspace/mod.rs` (`publish_mask` over `import_mask`'s checks),
tests `run_json.rs`, `serve_contract.rs`, `operations.rs`. **Size.** M.

### Task 10.9: Estimates for operations

**Description.** `estimate` answers for every action, operations included (4.5): an operation's
estimate is allowed, `worst_case_usd` 0, `cap_usd` as configured, `effective.model` null,
`x-rascal: {"constants": <the set it would use>}`, its readiness `ready`. Its binding digests each
in-port's reference by identity and each image by file digest (12.5's recipe, extended), so an
estimate and the call it binds agree. A call whose references do not bind gets the same `binding`
refusal from `estimate` as from the call (4.5), from one function. A served operation carrying
`_meta["table-manners/estimate"]` is checked as in 12.6.

As built: the operation's estimate reads its ports with the call's `items` and binds them with
the call's `bind_ports`, so the refusal and its `bindings` are the call's; its parameters go
through `resolve`, which the run now shares (the parameters as given and as resolved). A port
value of the wrong shape, plain invalid params on a call, is `invalid_arguments` (`usage`) from
`estimate`, whose input schema cannot reject it. The binding (`operation_bound`) enters a
reference as `{"artifact": <identity>}`, a `many` port as the list in order, an image by value as
`{"file_sha256": …}` after its `expect` is checked, a location and each parameter as sent, and
the configuration as `config_identity`; it reads nothing into the store. The call checks a named
estimate after allocation (as before 10.9, so a mismatch is the run's `invalid`/`stale_estimate`)
through `check_estimate`, now shared with tasks: the caller passes the call's binding and its
image slots or ports (`estimate::changed` takes those names instead of a recipe). `cap_usd` is
the configured per-call cap (an operation takes no `max_spend_usd`). `rascal estimate --json` on
the command line still prices tasks only: it has no store for an operation's references to
bind in.

**Acceptance.**
- [x] `estimate` for `feather` on a held Mask validates and is allowed at 0; the binding is the
      same for the same reference and changes with `sigma_px`, the Mask's identity and the
      configuration.
- [x] `estimate` for each binding problem is the `binding` refusal, validating, byte-equal in its
      `bindings` to the call's.
- [x] A `feather` call carrying its estimate's binding runs; with another `sigma_px` it ends
      `invalid`/`stale_estimate`.
- [x] `no_provider_call.rs` covers operation estimates.

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- estimate::
no_provider_call::`; `just check`. Done 2026-11-01 (on `m10-operations`).

**Dependencies.** 10.6. **Files.** `crates/rascal-mask/src/{estimate,operations}.rs`, tests
`estimate.rs`, `no_provider_call.rs`. **Size.** S.

### Task 10.10: Export and import

**Description.** The reserved `export` and `import` tools and the commands `rascal export --json
<file|->` and `rascal import --json <file|->` (5.1), wrapping 10.5's mask bundles and location
JSON. They are controls: no run, never in `actions`, never waiting for the permit; failures are
JSON-RPC errors, and on the command line a refusal exits 64 with the refusal JSON on stdout.

- **Export** `{artifact: <reference>}`: the reference is bound (10.3; `port: "artifact"`); a
  `rascal:Mask` is written as its bundle (`Workspace::export_mask`, ADR 0022), a `location` as
  the contract's location JSON (8.2's `location_out`), into rascal's export area
  (`$XDG_DATA_HOME/rascal/exports/<identity with : as ->.mask.tar` or `.location.json`), outside
  every run directory and the store, and answered with the reference and `file {path, sha256,
  bytes, media_type}` (`application/x-tar`, `application/json`). The same artifact always gives
  the same bytes, and a file already there with the right digest is reused. An unresolved
  reference is the `binding` refusal; a `portable: none` type (Plan, Series, BandProfile) is
  `invalid_arguments` (`usage`) naming the type; an artifact export would refuse by 10.5's bounds
  is `invalid_arguments` (`input`) with 10.5's message.
- **Import** `{file: {path}, sha256?}` or `{inline: {media_type, data}}` (up to
  `limits.inline_bytes`): a path under 10.1's rule (`rascal import` takes `--allow-dir` the same
  way, none refusing every path); `sha256` checked against the bytes; a mask bundle is imported
  through 10.5's checks whole, a location JSON validated against an image the store holds (an
  image it lacks is `invalid_arguments` naming the image's file digest and the remedy, passing
  the image to any action first), and answered with the reference and `already_present`. A PNG,
  a JPEG or any file that is neither a mask bundle nor a location is `invalid_arguments`
  (`input`) naming what it is: a rendering is never a Mask (5.1). A refused path says nothing
  about the file.
- Both are in `tools/list` exactly when a type is portable (always, for rascal) and are listed in
  the description's text (`docs/contract.md`), not in `actions`. The contract takes `rascal
  export` (question 9, settled): today's compatibility export becomes `rascal compat-export`,
  with no fallback under the old name or flag, and SPEC, README and `docs/export.md` follow.
  The commands take `--workspace` (the store, `serve`'s default) and `--config`.

**CLI scenario.** `echo '{"artifact": <mask ref>}' | rascal export --json - --workspace a/`
prints the file's path and digest; `echo '{"file": {"path": "<that path>"}}' | rascal import
--json - --workspace b/ --allow-dir <exports dir>` prints the same reference with
`already_present: false`, and again `true`; the same with a PNG exits 64 with the refusal JSON.

As built: an artifact type that is no type of rascal's (another tool's, an unknown name) is the
`binding` refusal (`wrong_type`, `expected` the portable types), and `image` is
`invalid_arguments` (`usage`: it travels as a file). A location has the JSON form only in an
image's own frame and on an image file the store can name (`on`); one in a `crop:` or `cut:`
frame is `input`, and rascal's `group` does not travel. Import tells a bundle from a location by
its content; an inline `media_type` that contradicts it is `usage`, and a request's shape faults
(unknown fields, a malformed `sha256` or base64, inline bytes over the limit) are `usage`. The
export's `file.sha256` is plain hex, as the schema has it. `rascal import`'s `--allow-dir` is
narrowed by `[serve] allow_dirs` like `serve`'s, and a refused path has the rule's one text. No
core change was needed: `contract::location_out` and `Workspace::import_mask_bytes` already did
the work. The tests are `tests/it/transfer.rs` (through `serve`) and `portable_cli.rs` (the
commands), leaving `serve_contract.rs` to 10.6, which ran beside this task. Merged into `m10-operations` after
10.6: the two tools are listed after `estimate` and before `describe`, reached through
`ContractActions` like every other call (`handles`, not `exclusive`), and 10.6's tool-list tests
(`operations.rs`, `serve.rs`) count them.

**Acceptance.**
- [x] Through `serve`: a Mask exported, imported into an empty store (`already_present: false`),
      imported again (`true`), the reference equal throughout and the exported bytes equal to
      `workspace export-mask`'s; a location exported and imported inline; each output validating
      against v0.4.
- [x] Refusals, each validating against `refusal.json`: a PNG given as a mask bundle, a
      `portable: none` export, an unresolved export (`binding`, `port: "artifact"`), a digest
      mismatch, a path outside the allowed directories (indistinguishable from a missing file).
- [x] Both answer while a slow run holds the permit; neither writes a run or touches the runs
      list; export writes only under the export area.
- [x] The commands print the same documents as the tools and exit 0, or 64 with the refusal JSON.

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- transfer::
portable_cli:: inspect::`; `just check`. Done 2026-11-01 (branch `m10-10.10-export`, built
against table-manners `4dd1d2f`).

**Dependencies.** 10.4 (and through it 10.3 and 10.1), 10.5. **Files.**
`crates/rascal-mask/src/transfer.rs` (new), `main.rs` (`export --json`, `import --json`,
`compat-export`), `export.rs`, `actions.rs` (the two tools registered),
`crates/rascal-mask/tests/it/transfer.rs` (new), `portable_cli.rs`, `inspect.rs` and
`packs.rs` (the renamed compatibility export), `docs/workspace.md`, `docs/export.md`,
`docs/contract.md`, `docs/tools.md`, `docs/architecture.md`, `docs/adr/0022-mask-bundles.md`
(the MCP tools now exist), `docs/adr/0009-copied-schemas-compat-export.md` (the new name), SPEC,
README. **Size.** M.

### Task 10.11: Folded into 10.2

The invocation on every result is part of 10.2 (the coordinator's review, 2026-11-01): a v0.4
result cannot carry a stand-in, so the real invocation lands with the pin. 10.3 writes `mask` and
`protect` into it as references and 10.6 writes an operation's in-ports, each through 10.2's
function. The number stays so the others keep theirs.

### Task 10.12: `rascal call`

**Description.** `rascal call <action> --arguments <file.json|-> [--config <path>] [--allow-dir
<abs>]… [--workspace <dir>] [--runs <dir>]` runs any action from exactly the arguments an MCP
call would send (4.8): the same parsing, readiness, binding, limits and run as `serve`, through
the same `ContractActions` with no session (`session_id` null, no budget ledger, the cap still
applied). Paths in the arguments follow 10.1's rule with the given `--allow-dir` (none refusing
every path). The run goes under `--runs` (`serve`'s default runs root); the store is
`--workspace` (`serve`'s default). It prints the result on stdout and exits as in 6 (0, 3, 64,
130, 143); a refusal that is a JSON-RPC error in `serve` (`not_ready`, `binding`) prints the
refusal JSON and exits 64. So an invocation's `arguments`, with a `path` added to each image, are
the file for `rascal call <invocation.action>`.

**CLI scenario.** Take a served `feather` result's `invocation.arguments` into `args.json`;
`rascal call feather --arguments args.json --workspace <the store>` prints a result whose
`invocation` equals the original and whose out reference is the same.

As built: `serve` and `rascal call` build their `ContractActions` through one function,
`serve::contract_actions`, which takes the session (`serve`'s, or none) and the command's name
for stderr; the session field became optional, so without one `session_id` is null, the budget
is not applied and no `rascal://session` exists. The call goes through `Actions::call` itself
with an empty `CallMeta` (no correlation id, no estimate). The result is printed compact on one
line (so 10.13's JSON Lines are the same printing); a cancelled run's `run.json` is found through
the run reference its progress carried and printed. SIGINT and SIGTERM set the call's cancel flag
and the exit code (130, 143) rather than exiting at once, so the run writes `cancelled` itself
before the next commit, as a cancelled MCP call does. Only the description's actions are called:
a reserved tool or an unknown name exits 64 with the reason on stderr. A refusal with data
(`not_ready`, `binding`, `invalid_arguments`) prints the data and exits 64; a call error without
data (malformed arguments) is said on stderr and exits 64. `exit::INTERRUPTED` and
`exit::TERMINATED` name 130 and 143.

**Acceptance.**
- [x] For a task (`seam` with the command filler, an image path added) and an operation
      (`feather`), `rascal call` of a result's invocation gives a result whose `invocation`
      equals the original's and whose outputs have the same identities.
- [x] Each exit code: 0 completed, 3 error (a failing filler), 64 invalid, a refused path, a
      `binding` refusal (refusal JSON on stdout), 130 on SIGINT.
- [x] A path outside `--allow-dir` is refused as in 10.1; inline images need no flag.

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- call_cli::`;
`just check`. Done 2026-11-01 (on `m10-operations`).

**Dependencies.** 10.2 (its `binding` refusal comes with 10.3, its `feather` scenario with
10.6). **Files.** `crates/rascal-mask/src/{main,call}.rs` (the second new),
`actions.rs` (a session-less entry), `crates/rascal-mask/tests/it/call_cli.rs` (new),
`README.md`, SPEC "Commands", `docs/contract.md`. **Size.** S.

### Task 10.13: `rascal call --bundle`

**Description.** `rascal call --bundle <path> [--config] [--allow-dir <abs>]… [--workspace]
[--runs]` replays an invocation bundle (4.9) built by a host. The bundle path follows 10.1's rule.
The reader is bounded as 10.5's is (256 MiB, a member count, a manifest size, plain member names,
each size checked against what remains) and checks the whole file before acting: the manifest
against `urn:table-manners:core:invocation-bundle`, every member it names present and no other,
each `artifacts/` member's `sha256` and each `inputs/` member's `file_sha256`. Then for each step
in order: a step of another tool is refused (exit 64, naming the step; question 11, settled); each
artifact member is imported into the store (10.10's import, so it is checked whole); each input
member is copied into rascal's own replay area (`$XDG_DATA_HOME/rascal/replay/<bundle digest>/`,
a directory rascal owns, so it is not a caller path); the step is called with its
`invocation.arguments` plus a `path` on each image, as 10.12 does. A later step that names an
earlier step's output binds to that step's replayed output by port, whatever its identity
(CONTRACT 4.9 at `d9e6058`; question 12, settled), and its result reports the change: the port,
the identity the bundle named and the one replayed (a filler's output, say, differs). A step that lists `reference_only` inputs or `unportable`
artifacts is refused before anything runs, naming them. It prints one result per line (JSON
Lines), stops at the first step that does not complete, and exits with that step's code.

As built: the bounds are 256 MiB, 256 members, an 8 MiB `invocation.json` (it holds every
step's result) and member names of at most 96 bytes of lowercase letters, digits, `-` and `.`.
The reader checks the manifest against the contract's schema through
`contract::schema_errors`, which resolves the crate's embedded schemas. It does not insist on the
writer's canonical byte layout (only that `invocation.json` comes first): any well-formed bundle
whose digests hold replays; the testkit's writer is the byte-stable one. Another tool's step is
refused up front with the `reference_only` and `unportable` checks, so nothing is imported before
a refusal. An image a step bound by reference (Rasterize's `image`) keeps its reference in the
arguments, so the invocation stays the original's; its input member is imported into the store
by file digest when the store lacks it. A rebinding by port matches an earlier step's original
`artifacts.out` entry to the replayed one at the same position and port. Its report (CONTRACT
4.9 says only that the replay reports each such change and that it is not an error; the
coordinator's ruling fixed the place) is a `replay:` note in the rebound step's own result,
handed to the run through `CallMeta::notes` and `Pending::notes` before it runs, so it is in the
run's `run.json` and the printed line is that file's document (a test asserts each printed line
equals its run's `run.json`). Artifact imports go through `transfer::import_bytes` (the `import` control's checks
without its inline size limit). The idempotency test replays a one-step bundle twice into a
fresh store (one `import_mask` record); in the chain the replayed steps make the exported
artifacts first, so their imports are already present on the first pass.

**Acceptance.**
- [x] A bundle of Line → Rasterize → Feather, written by the testkit's bundle writer from the
      results and `export`, replays from another directory into a fresh store with the original
      workspace deleted; each replayed `invocation` equals the bundle's and the final Mask has
      the original identity.
- [x] A tampered bundle (an artifact member's byte, an input's byte, a missing member, an extra
      member, an oversized manifest) is refused with nothing imported and nothing run; a bundle
      with `unportable` or `reference_only` entries is refused naming them.
- [x] Two calls of `--bundle` on the same file are idempotent in the store (imports
      `already_present`).

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- call_bundle::`;
`just check`. Done 2026-11-01 (on `m10-operations`).

**Dependencies.** 10.12, 10.10. **Files.** `crates/rascal-mask/src/call.rs`,
`crates/rascal-core/src/invocation_bundle.rs` (new: the bounded reader),
`crates/rascal-testkit/src/invocation_bundle.rs` (new: a host-side writer for tests, since no tool
builds one), `crates/rascal-mask/tests/it/call_bundle.rs` (new), `docs/contract.md`, README.
**Size.** M.

### Task 10.14: Line → Rasterize → Feather end to end, fixtures, ADR 0021, docs

**Description.** The old 10.7. The path through the independent client as one test
(`operations_path.rs`): import nothing by hand; `line` on an image path, `rasterize` on its
location reference, the Mask exported, `serve` restarted on a moved workspace, the Mask reference
read back, `feather` with a new sigma bound to it, its result's invocation run by `rascal call`,
and the chain replayed from a bundle in another directory; plus `remove`'s Mask bound by a later
`feather`. Then rascal's v0.4 fixtures for table-manners `fixtures/rascal/` (CONTRACT 7, item 7,
v0.4), produced by rascal's own server and CLI in `table_manners_fixtures.rs`, never by hand,
normalised as in 11.6, written under `UPDATE_FIXTURES=1` and compared otherwise, each held to the
contract as its `tests/fixtures.rs` holds it:

- `describe.json` (and the v0.3 set's describes, now with `kind`, `group`, `ports`,
  `artifact_types`);
- `run-op-line.json`, `run-op-rasterize.json`, `run-op-feather.json`; `run-remove-mask.json` (a
  task listing its Mask) and `run-op-feather-from-remove.json` (binding it);
- `refusal-binding-wrong-type.json`, `-wrong-version.json`, `-wrong-frame.json`,
  `-unresolved.json`, each from a call, the same four from `estimate`
  (`refusal-binding-estimate-*.json`), `refusal-binding-many-index.json`, and
  `run-binding-claim-released.json` (the retry accepted under the released id);
- `artifact-mask.json`, `artifact-location.json`, `artifact-plan.json` (no preview),
  `refusal-artifact-unresolved.json`;
- `export-mask.json`, `import-mask.json` (`already_present: false`), `import-mask-again.json`
  (`true`), `import-location-inline.json`, `refusal-import-png.json`,
  `refusal-export-not-portable.json`, `refusal-export-unresolved.json`;
- `invocation` on every result, and `run-call-feather.json` (a `rascal call --arguments` run of
  `run-op-feather.json`'s invocation, the invocations equal);
- `bundle-line-rasterize-feather/invocation.json` with its members, and the results of its
  replay by `rascal call --bundle` from another directory with the original workspace gone;
- `session.json` with `allowed_dirs`, `session-no-allow-dir.json` (`[]`), and
  `run-path-outside.json`, `-dotdot.json`, `-symlink.json`, `-missing.json` (the same refusal as
  the existing file outside), `-no-allow-dir.json`, each `invalid`/`input`;
- the M11 and M12 sets kept at v0.4.

rascal copies them into the table-manners branch `v0.4` and commits there, and fills rascal's
column of MAPPING's v0.4 table. **ADR 0021**, "Operations are actions": every op an action with
`kind`, `group` and `ports`; artifacts by reference and identity, resolved in the session's store;
the `binding` refusal; op runs commit into the store and tasks publish their Masks into it; the
previews; export and import over the mask bundle; the op and `workspace_*` tools removed,
superseding ADR 0015's op surface (its transport decision stands); the path rule; pack operations
deferred. ADR 0020 gains an M10 amendment, ADR 0015 a superseded-in-part note, ADR 0022 its MCP
tools. Docs move to v0.4: `docs/contract.md` (operations, references, binding, artifacts,
previews, the artifact resource, export and import, the invocation, `rascal call`, bundles, the
path rule), `docs/tools.md` (the operations as actions; the op CLI unchanged), `docs/workspace.md`
(the store, the export area, relocation), SPEC "Operations", "The MCP server and client",
"Workspace", "External interface" and "Commands", README (`serve --allow-dir`, `rascal call`,
`export --json`, `import --json`), `docs/architecture.md`.

As built: the fixtures come from `v04_set` in `table_manners_fixtures.rs` beside the M11 and
v0.3 sets, from servers whose data and cache sit under the scratch directory (so the export area
and the preview cache normalise to `/work/data/…` and `/work/cache/…`). The file name chooses the
schema as table-manners' test does, now also `artifact-*` (`operations:artifact`), `export-*`
and `import-*` (the controls' responses) and `bundle-*/invocation.json` (the bundle manifest),
whose members are written and compared byte for byte beside it; the comparison walks
subdirectories. `wrong_frame` comes from `subtract` on Masks of two images, the `many` index from
`select` with a wrong-typed and an unresolved plane. `estimate-op-feather.json` (added at the
coordinator's request before table-manners v0.4.0) is the allowed estimate of the Feather call,
its Mask port bound by reference, made just before that call. The replay's results are
`run-replay-line.json`, `-rasterize.json` and `-feather.json` (the plan named no files for them).
Results whose arguments did not resolve keep a null invocation: `run-invalid.json` and the
`run-path-*.json` refusals. `session.json` (the M11 set's) carries the suite's two allowed
directories. The second acceptance item's table-manners half (`cargo test --test fixtures` in
the contract's checkout, and MAPPING's v0.4 column) is the coordinator's: this task does not
touch the table-manners checkout. ADR 0015 is marked superseded in part, ADR 0020 notes 0021,
and ADR 0022's MCP tools note was already in from 10.10.

**Acceptance.**
- [x] `operations_path.rs` passes through the independent client, end to end as above.
- [ ] table-manners' fixtures test (`cargo test --test fixtures` there) passes for
      `fixtures/rascal/`; rascal's own check passes without `UPDATE_FIXTURES`.
- [x] The docs name v0.4; none describes the op tools, the `workspace_*` tools, workspace ids in
      MCP arguments, a path form of a mask argument, or MCP roots.

**Verification.** `UPDATE_FIXTURES=1 cargo nextest run -p rascal-mask --features testkit --test
it -- table_manners_fixtures::`, then the same without it; `cargo nextest run -p rascal-mask
--features testkit --test it -- operations_path::`; in the table-manners checkout `cargo test`;
`just check`. Done 2026-11-01 on `m10-operations` (the table-manners side waits for the coordinator).

**Dependencies.** 10.7 to 10.13. **Files.** `crates/rascal-mask/tests/it/{operations_path,
table_manners_fixtures}.rs` (the first new), `crates/rascal-mask/tests/it/main.rs`,
`fixtures/table-manners/`, `docs/adr/0021-operations-are-actions.md` (new), `docs/adr/README.md`,
`docs/adr/{0015,0020,0022}-*.md`, `docs/contract.md`, `docs/tools.md`, `docs/workspace.md`,
`docs/architecture.md`, `SPEC.md`, `README.md`, table-manners `fixtures/rascal/` and `MAPPING.md`.
**Size.** M.

### Task 10.15: The pin to the release (owner)

**Description.** Once squint's v0.4 fixtures pass too, the table-manners branch merges, the owner
tags `v0.4.0` and publishes the crate, and rascal pins `table-manners = "=0.4.0"` in place of the
path; `scripts/check-contract.sh` reports it as the newest. The fixtures are regenerated if the
published contract differs from the branch rascal built against, and the milestone branch merges
into `main` by the checkpoint procedure.

As built (2026-11-02, after the owner tagged and published v0.4.0): the workspace pins
`table-manners = "=0.4.0"` from crates.io in place of the path (`rascal-core` takes it through
the workspace; no other manifest names it), and `Cargo.lock` records the registry crate and its
checksum. The draft-only wiring is gone: `scripts/check-contract.sh` reads only the exact
`"=X"` pin and no longer warns about a path draft, and its self-tests in
`scripts/test-checks.sh` dropped the draft cases (a path pin is still refused as no exact pin).
No CI workflow referred to the draft checkout. The published contract matches the branch rascal
built against: the table-manners fixtures, the contract fixtures and the description fixture
compare equal, and `UPDATE_FIXTURES=1` rewrites nothing. `scripts/check-contract.sh` reports
0.4.0 as the newest. What remains of the acceptance below is the coordinator's: CI on both
platforms, the merge into `main` and the tag `m10`.

**Acceptance.**
- [ ] `just check` and CI green on both platforms on the pinned release; the milestone branch
      merged; tag `m10`.

**Verification.** `scripts/check-contract.sh`; `just ci-local`; CI on the merge commit.

**Dependencies.** 10.14; squint's v0.4 fixtures; the owner's tag and publish. **Files.**
`Cargo.toml`, `Cargo.lock`, `PLAN.md`, `tasks/todo.md`. **Size.** XS.

### Checkpoint M10
- [ ] Tasks 10.1 to 10.15 done (10.5 done 2026-11-01), CI green on both platforms; tag `m10`.

### Owner decisions for M10
- The path rule ships ahead of v0.4 on `=0.3.0` and is closed by default (no `--allow-dir`, no
  path), as v0.4 requires; hosts that pass paths add `--allow-dir`. Settled (question 1): 10.1
  lands now; Wafflery passes `--allow-dir`.
- The configuration narrows the allowed directories with `[serve] allow_dirs`, to the overlap of
  the trees. Settled (question 4).
- References are the only form of a native artifact over MCP: workspace ids and mask paths leave
  the MCP surface. Recommended, assumed.
- The session's store is `serve`'s workspace; operation runs commit into it, and tasks keep their
  self-contained run workspaces and publish their Masks into the store when they end. Recommended,
  assumed.
- Previews for `rascal:Mask` (an overlay on its image, else its grey rendering) and `location` (its
  outline on its image); none for BandProfile, Series and Plan. Recommended, assumed.
- Operations declare `renders: []`; their preview is their picture. Settled (question 5).
- Pack operations (`segment`) are not served in M10. Settled (question 6): out of v0.4.
- Exports go to `$XDG_DATA_HOME/rascal/exports/`, previews to `$XDG_CACHE_HOME/rascal/previews/`,
  bundle replays to `$XDG_DATA_HOME/rascal/replay/`. Recommended, assumed.

### Questions for the coordinator (M10)

Raised about the v0.4 draft (`b1ae034`); the coordinator settled every one on 2026-11-01, and
each ruling is folded into the task it governs.

1. 10.1 ships the path rule on `=0.3.0`, closed by default. Any host passing paths without
   `--allow-dir` (Wafflery's v0.3 client) then gets `invalid`/`input`. May 10.1 merge to `main`
   before Wafflery passes `--allow-dir`, or should the two land together? Recommended: together.
   **Ruling:** land 10.1 now, closed by default; Wafflery will pass `--allow-dir`.
2. On v0.3 the session schema is closed, so `allowed_dirs` is printed on stderr only until 10.2.
   Acceptable? Recommended: yes. **Ruling:** yes, stderr until 10.2.
3. A `--allow-dir` that is relative, missing or not a directory: the contract is silent. rascal
   exits 64 at start naming the flag. Confirm, or should it be dropped with a warning?
   **Ruling:** exit 64.
4. "Narrowed by the configuration, which may only take away": rascal intersects the trees (a
   configured subdirectory of a flag directory replaces it). Confirm, versus keeping only flag
   directories that are themselves listed. **Ruling:** the overlap.
5. The v0.4 schemas, types and examples are not at `b1ae034`. 10.2 assumes `invocation` may be
   `null` in the schema (filled in 10.11), that `resource` and `on` in the artifact reference are
   optional or required as the text's example marks them, and that an action may omit `renders`.
   **Ruling:** as the built v0.4 schemas have it: `invocation` is `null` only on an `invalid`
   result; artifact references carry `resource`; operations declare `renders: []`.
6. Pack operations: a description is a local inspection and a pack's tools are known only by
   starting it, so `segment` cannot be described without the install recording the pack's
   manifests. Leave pack operations out of v0.4 for rascal (planned), or have `tools setup` record
   the manifests so `describe` lists them? **Ruling:** pack operations stay out of v0.4.
7. A task's artifact argument (`protect`'s `mask`): 4.7 defines the `binding` refusal for an
   operation's ports. rascal uses it for task arguments too, with `port` the argument's name.
   Confirm. **Ruling:** yes: the binding covers a task's artifact arguments, `port` the
   argument's name.
8. "A task lists each native artifact it produced": rascal lists the recipe's outputs (the Mask),
   not intermediates such as a `fan`'s BandProfile. Confirm. **Ruling:** outputs only.
9. `rascal export` already exists as the compatibility export (`--compat`). 5.1 reserves
   `<tool> export --json`. rascal keeps one subcommand with two exclusive modes. Acceptable, or
   rename the compatibility export? **Ruling:** the contract takes `rascal export`; the
   compatibility export becomes `rascal compat-export`, with no fallback.
10. A missed artifact read carries the `binding` refusal data; which JSON-RPC code: `-32602` like
    every refusal (planned), or MCP's resource-not-found `-32002`? **Ruling:** `-32602`, with
    the binding data.
11. `rascal call --bundle` on a bundle holding another tool's step: refuse it (planned) or skip
    it? And the output of a multi-step replay: one result per line (planned), stopping at the
    first step that does not complete, with that step's exit code? **Ruling:** refuse another
    tool's step; one result per line; stop at the first step that does not complete, with its
    exit code.
12. A bundle chain whose later step names an earlier step's image output (not an artifact) by
    reference: the replay maps it only when the replayed output has the same file digest, which a
    filler's output does not. Is that the intended limit (only the call repeats, 4.9), and should
    `inputs` then carry such intermediates? **Ruling:** a replayed step naming an earlier
    step's output binds to the replayed output by port and reports the change (CONTRACT 4.9 at
    `d9e6058`).
13. For `location`, the identity is each tool's recipe (unlike `image`), so squint and rascal give
    the same location different identities. Intended? **Ruling:** yes.
14. An `image` port given an artifact reference with no locator: rascal resolves it by file digest
    in its store (planned). Confirm the contract means this, beside the image-in forms.
    **Ruling:** yes.

## Milestone M11: Table manners v0.2

rascal adopts table-manners v0.2: run association and the session ledger (Wafflehouse's C4, as
squint and rascal agreed it; the contract's 2.3, 4.1 to 4.6). The contract is drafted on
table-manners' `v0.2-contract` branch and is tagged only when squint's and rascal's fixtures
validate against it, so rascal builds against the crate by path (`../table-manners`, on that
branch) on one milestone branch, `m11-table-manners-v02`. Each task is a commit there with
`just check` green locally. The branch cannot merge while the pin is a path (CI has no sibling
checkout), so it merges at 11.7, after the owner tags and publishes v0.2.0 and the pin is
`=0.2.0`. Nothing here waits on squint: its fixtures come in parallel under its own M8.

### Task 11.1: The pin and the shapes

**Description.** The pin moves to the v0.2 crate by path. The result's `run` carries
`session_id` and `client_run_id` (null for a CLI run and for a call that sent no id);
`spend.unresolved_usd` replaces M9's `body.spend_unresolved_usd`; runs entries are run references
with `action`, `status` and `started`; `contract` is `0.2.0` everywhere.

**Acceptance.**
- [x] A CLI `run` and `heal` write `run.json` that validates against the v0.2 result schema, with
      both ids null.
- [x] A heal whose filler call is unresolved reports it in `spend.unresolved_usd`, never in the
      body; a known call gives 0 there.
- [x] `fixtures/contract/` regenerated and valid against v0.2.

**Files.** `Cargo.toml`, `crates/rascal-mask/src/{run_json,actions}.rs`, tests,
`fixtures/contract/`. **Size.** M.

### Task 11.2: The session

**Description.** `serve` starts a session: a random UUID (v4, from the `uuid` crate that rmcp
already brings in, with its `v4` feature) and the start time. `rascal://session` returns one
atomic observation of the ledger (contract 4.6): budget and remaining null together when
uncapped, `remaining = max(0, budget - spent - reserved)`, amounts at most six decimals. Served
runs carry the session id. `estimate` keeps the worst case, the effective per-call cap and
whether the call fits, and drops M9's private `session_*` fields: the session is the resource's
until C1 places it in the estimate.

**Acceptance.**
- [x] `rascal://session` validates against v0.2, is listed in `resources/list` and in describe's
      `resources`, and answers while a run holds the permit, showing its reservation.
- [x] Two `serve` processes give two ids; a run's `session_id` is its server's.
- [x] Uncapped, `budget_usd` and `remaining_usd` are both null; after an unresolved heal,
      `unresolved_usd` is the price and is included in `spent_usd`.

**Files.** `crates/rascal-mask/src/{actions,serve}.rs`, `Cargo.toml`, tests. **Size.** S.

### Task 11.3: Correlation ids

**Description.** A `tools/call` may carry `_meta["table-manners/client_run_id"]` (contract 4.4).
`serve` checks its form (1 to 128 of `[A-Za-z0-9._:-]`; otherwise `-32602` before any run) and
claims it atomically when it accepts the request, before the permit wait. A claimed id is
refused with `-32602` and `data: {reason: "client_run_id_reused", run}`, where `run` is the
earlier request's run reference, or null while that request has none. A claimed id stays
claimed for the session, a cancelled-while-queued one included. The id is echoed in the run's
reference and persisted with the session id and action in the run's inputs directory at
allocation, so a `running` entry carries it.

**Acceptance.**
- [x] Two concurrent calls with one id give one run and one refusal that validates against
      `refusal.json`; at most one filler call is made.
- [x] A reuse after the first run ended carries that run's reference; a reuse of an id whose
      request was cancelled while queued carries `run: null`.
- [x] A malformed id is refused before any run; a call without one has `client_run_id: null`.
- [x] The runs list shows a running run's ids before its first step ends.

**Files.** `crates/rascal-mcp/src/server.rs`, `crates/rascal-mask/src/actions.rs`,
`crates/rascal-testkit/src/mcp_client.rs`, tests. **Size.** M.

### Task 11.4: The run in progress `_meta`

**Description.** From allocation, every progress notification, heartbeats included, carries the
run's reference under `_meta["table-manners/run"]`; none before (a call waiting on the permit).
The run leaves `message`: no `rascal://` there. Cancellation is checked again after the permit
and just before allocation, so a cancel landing between them leaves no run (contract 4.2). The
`Actions` trait gains a way to hand the server the run reference at allocation, which the server
attaches to every later notification.

**Acceptance.**
- [x] Through the independent client: a queued call's heartbeats carry no `_meta`; after
      allocation every notification carries a reference equal to the result's `run` without
      `dir`; no message contains `rascal://`.
- [x] A cancel while queued leaves no run directory and no runs entry; the check once the permit
      is held covers the window before allocation, too narrow to time in a test.

**Files.** `crates/rascal-mcp/src/server.rs`, `crates/rascal-mask/src/actions.rs`,
`crates/rascal-testkit/src/mcp_client.rs`, tests. **Size.** M.

### Task 11.5: The runs page

**Description.** `rascal://runs{?cursor}` (contract 4.3): newest first by (started, id), 50 a
page, `next_cursor` null on the last. The cursor is the last entry's run id, which is already
URI-unreserved and ordered; clients treat it as opaque. `rascal://runs` stays the listed first
page; an unknown cursor is a resource error, not an empty page.

**Acceptance.**
- [x] 120 runs read as three pages with no gap or repeat; the last `next_cursor` is null; every
      page validates.
- [x] A run started after page one was read appears on a fresh page one, not on page two.

**Files.** `crates/rascal-mask/src/actions.rs`, tests. **Size.** S.

### Task 11.6: Fixtures and docs

**Description.** rascal's v0.2 fixtures for table-manners `fixtures/rascal/`, produced by
rascal's own server through its independent client, never written by hand: `describe.json`;
`run-*.json` for completed, cancelled, error, invalid and an unresolved spend; `runs.json` and a
second page; `session.json`; `refusal-*.json` with an earlier run and with none. A test writes
them under `UPDATE_FIXTURES=1` and checks them otherwise; rascal copies them into the
table-manners branch and commits there. Docs move to v0.2: `docs/contract.md`, SPEC's external
interface, README, ADR 0020, `docs/architecture.md`, and rascal's column in table-manners'
MAPPING.md.

**Acceptance.**
- [x] table-manners' fixtures test passes for `fixtures/rascal/`.
- [x] The docs name v0.2 and no longer describe the v0.1 body field or the run in the message.

**Files.** `crates/rascal-mask/tests/`, `fixtures/contract/`, docs, table-manners
`fixtures/rascal/` and `MAPPING.md`. **Size.** M.

### Task 11.7: The pin to the release (owner)

**Description.** Once squint's fixtures pass too, the table-manners branch merges, the owner
tags `v0.2.0` and publishes the crate, and rascal pins `=0.2.0` in place of the path.

**Acceptance.**
- [x] `just check` and CI green on the pinned release; the milestone branch merged; tag `m11`.

**Files.** `Cargo.toml`, `Cargo.lock`, `PLAN.md`, `tasks/todo.md`. **Size.** XS.

### Checkpoint M11
- [x] Tasks 11.1 to 11.7 done, CI green on both platforms; tag `m11`.

## Milestone M12: Table manners v0.3

rascal adopts table-manners v0.3: a host can set rascal up and pay for a run safely. `describe`
and `serve` work with no configuration and say per action what is missing; `estimate` becomes
the reserved tool, priced on exactly the call's arguments and bound to them, the session and the
configuration; the per-call cap and the session budget fail with their own codes; a rejected or
missing key is `credential`; a request refused with no run releases its correlation id; and the
dash is a render a host can embed (the contract's 2.2, 3.1, 3.2, 4.4, 4.5 and 5; MAPPING.md's
v0.3 table). The contract is drafted on table-manners' branch `v0.3` and is tagged only when
squint's and rascal's fixtures validate against it, so rascal builds against it as M11 did:
one milestone branch, `m12-table-manners-v03`, cut from `main` after `m11`, with the pin

```toml
# M12 builds against the v0.3.0 draft by path until it is tagged and published (PLAN.md 12.10).
table-manners = { path = "../table-manners", version = "=0.3.0" }
```

on a sibling checkout of table-manners at branch `v0.3`, at `e503a23` or later (the commits that
settled the questions this plan raised, and Wafflery's three rulings folded in below at 12.5 and
12.8) (`scripts/check-contract.sh` already
reads the exact version of a path pin). Each task is a commit (or a short task branch
`m12/<task>` merged into the milestone branch when tasks run in parallel) with `just check` green
locally against that checkout; the commit message names the table-manners commit it was built
against. The milestone branch merges into `main` only at 12.10, after the owner tags and
publishes v0.3.0 and the pin is `=0.3.0`. Nothing here waits on squint (its M9 runs in
parallel) or on M10's open tasks (those wait on the v0.4 operations profile).

What rascal does not take from v0.3: list image slots. Every shipped recipe's inputs are single
(an `Image`, a `Location`, a `Mask`, a `Text`: `crates/rascal-mask/recipes/*.toml`), so no action
declares `many` and the MAPPING row is n/a; the ops' many ports reach the contract with the
operations profile, not here. 12.1 adds a test that no action declares `many`, so a recipe that
grows a list input has to decide its slot shape.

**Order and parallel tracks.** 12.1 first: it moves the pin and makes the shapes compile and
validate. Then three tracks: **A**, the spending chain `12.7 → 12.2 → 12.3 → 12.5 → 12.6`
(12.7, the claim release in `rascal-mcp`, is small and comes first because 12.2's `not_ready`
refusal releases its id through it), with **12.4** beside 12.7, 12.2 and 12.3 (it needs only
12.1) and joining before 12.5; **B**, 12.4; **C**, 12.8 (the renders, `dash.rs` only). B and C
touch no file of each other's, and C none of A's, except `tasks/todo.md`. 12.9 waits for every track; 12.10 waits for 12.9, squint's
fixtures and the owner's release.

**Commits and rollback.** One commit per task, `just check` green, conventional scope
(`feat(mask)`, `feat(mcp)`, `test(mask)`, `docs`); the checklist in `tasks/todo.md` ticked in the
task's own commit. Rollback checkpoints are annotated intermediate tags on the milestone branch:
`m12.1` after 12.1 (the pin and the shapes: a v0.3 rascal with no new behaviour), `m12.2` after
12.7 and 12.2 to 12.4 (the claim release, readiness and the `not_ready` refusal, `credential`, the
split limits: everything observable without the new estimate), `m12.3` after 12.5 and 12.6 (bound
estimates), `m12.4` once 12.8 is merged in. A task group that goes wrong is reverted on the branch
(rule 1, `git revert --no-commit` newest first: 12.6 before 12.5, 12.3 before 12.2, 12.2 before
12.7), or the branch is reset to the last tag after a
`rescue/` reference (rule 2). If v0.3 changes under rascal before it is tagged, the affected task
is amended forward on the branch; if v0.3 is abandoned, the milestone branch is never merged and
`main` stays on `=0.2.0` (rule 3). After the merge, undoing M12 is rule 4 (revert the merge
commit), which returns the pin to `=0.2.0`; the published crate and the contract's
`fixtures/rascal/` are external state and stay.

### Task 12.1: The pin and the shapes

**Description.** The pin moves to the v0.3 crate by path; `contract` is `0.3.0` everywhere.
`scripts/check-contract.sh` also prints, as a CI warning, that the build is against a draft
when the pin is a path. The shapes the v0.3 types force, with no new behaviour: `estimate` leaves
the description's `actions` (it stays an MCP tool with its v0.2 input and output until 12.5, as a
reserved name); `cost.needs_env` is gone, and each name it held becomes a `credential`
requirement in the action's `readiness` (`present` or `missing` by the variable's existence,
never its value; `ready` or `needs_setup` from them), the rest of readiness coming in 12.2; the
dash render gains `kind: "server"` and `ready_timeout_s: 10` (its behaviour moves in 12.8); every
action's `params_schema` declares `max_spend_usd` as `{"type": "number", "exclusiveMinimum": 0}`
(every action takes a filler, so every action may be paid), moved out of `serve`'s added input
schema (`actions.rs`, today `minimum: 0`) into the description, so the description and the MCP
input schema agree. A value of 0 or below is refused as `invalid` (`usage`) in the run, as any bad
argument is today. The error map compiles against the v0.3 `ErrorCode`, still sending both limits
to `budget` until 12.4. `fixtures/table-manners/` and `fixtures/contract/describe.json` are
regenerated at v0.3.

**Acceptance.**
- [x] `rascal describe --json` with the M11 test configuration validates against the v0.3
      description schema: no `estimate` in `actions`, no `needs_env`, `readiness` on every action,
      the dash render with `kind` and `ready_timeout_s`, `max_spend_usd` with `exclusiveMinimum: 0`
      on every action; no action declares `many`.
- [x] `tools/list` still offers `estimate` and `describe`; each action tool's input schema has the
      same `max_spend_usd` as its `params_schema`.
- [x] A served call with `max_spend_usd: 0` ends `invalid`/`usage` with zero spend.
- [x] Every `run.json`, the runs pages, the session and the refusals validate against v0.3;
      `UPDATE_FIXTURES=1` regenerates `fixtures/table-manners/` and the check passes after.

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- describe::
serve_contract:: run_json:: table_manners_fixtures::`; `scripts/test-checks.sh`; `just check`.

**Dependencies.** M11 (`m11`); a checkout of table-manners branch `v0.3` at `../table-manners`.
**Files.** `Cargo.toml`, `Cargo.lock`, `scripts/check-contract.sh` (and its self-test in
`scripts/test-checks.sh`), `crates/rascal-mask/src/{describe,actions,run_json}.rs`,
`crates/rascal-core/src/contract.rs`, `crates/rascal-mask/tests/it/{describe,serve_contract}.rs`,
`fixtures/table-manners/`, `fixtures/contract/`. **Size.** M.

### Task 12.2: Readiness without configuration

**Description.** Configuration is read once into a **config state**: `missing` (no `--config`,
or a path that does not exist), `invalid` (it does not parse or check; the reason kept, never a
value), or `loaded` (the `Config` and its **identity**, the SHA-256 of its canonical
serialization, ADR 0016; with no file, the digest of rascal's built-in defaults, which are a
configuration too, so the identity is never null). rascal
keeps having no default lookup: the only way to choose a configuration is `--config <path>`, as
4.6 allows. `rascal describe [--json] [--config <path>]` never fails on configuration and exits
0; `rascal serve [--config <path>]` starts in every config state, and `--workspace` becomes
optional (the config's `[workspace] dir`, else `$XDG_DATA_HOME/rascal/workspace`, beside the
runs root's default), so a host needs nothing but `serve` and, if it likes, `--config`. In
`serve`, a pack that does not start no longer stops the server: its tools are absent, stderr says
why, and its requirement says so; a run started with such a pack notes it in its journal and
leaves its tools out, so a mask-only call still runs and a recipe that needs the pack fails
with the pack's own error.

Readiness is one function, `readiness::for_call(setup, action, use)` in a new module (`setup`
holds the config state, the session's answers and the packs `serve` could not start; `use` is
the call's one filler, none, or the description's default use),
used by `describe` and later by the estimate and the limits check (the call's one filler, 3.1: an
estimate's readiness is the call's own). In `describe` an action's readiness describes its
**default use** (3.1: the action as configured now, its requirements naming every variable that
use reads), which for rascal is what its configuration offers: with no filler
configured, the mask alone, which needs nothing (every action but `remove_prompt` is `ready`
with no requirements); with fillers configured, healing with any of them (the union of their
requirements), so every variable a configured filler reads is named and a host passes it (4.6).
It is a local inspection: it
reads the environment's variable names, the configuration and the file system, and never starts a
pack, sends a request or writes anything. Requirements, by kind:

- `config`, name `rascal.toml`: `present` when it loads, `invalid` when it does not parse or
  check (with the error, never a value), `missing` when `--config` names no file or is absent
  (3.1). It is listed on every action whose default use reads the configuration (every action
  once a file is given; `remove_prompt` always, since its pack is configured there), with the
  setup sentence "Start rascal with `--config <path>` naming the segmenter pack." when missing;
  unconfigured, the mask-only actions list nothing.
- `credential`, one per configured HTTP filler's `key_env`: `present` or `missing` from the
  environment, setup "Set `<NAME>` in the environment rascal is started with." (`valid` and
  `invalid` come from the session in 12.3).
- `model`, one per configured filler, name `fillers.<name>`: `present` (configured, not checked).
- `dependency`, one per command filler, its `command[0]` found as an executable (by path or
  `PATH`): `present` or `missing`.
- For `remove_prompt` only, the segmenter pack: `dependency` named `packs.<name>` for the entry
  whose command is `<dir>/.venv/bin/rascal-tools` (the rule `tools setup` already uses), `missing`
  with "Run `rascal tools setup --bundle <…> --config <path>`." when there is no such entry or its
  launcher does not exist, `invalid` when `serve` failed to start it; and one `file` per model
  artifact the install's `artifacts.toml` names (the detector, its tokenizer, the segmenter's
  encoder and decoder), looked up exactly where the pack looks (`RASCAL_TOOLS_MODELS`
  when the pack entry passes it, else the install's `models/`, else the pack's default cache),
  `present` when the file exists and is not empty (`bundle.toml` records digests, not sizes), else
  `missing` with the setup naming the directory looked in and `rascal-tools fetch`. Digests are
  not hashed here (749 MB on every `describe`; 3.1 allows `present` for an unchecked file); the
  pack still verifies them before use, and rascal does not see that check, so it never reports a
  file `valid` in v0.3.
- `env`, for the non-secret variables rascal or a pack reads (3.1): each name a configured pack
  entry passes through (`[packs.*] env`, such as `RASCAL_TOOLS_MODELS`), on the actions that use
  the pack (a filler entry passes no variables of its own); `present` when set. When unset it
  is `missing` only if the action needs it (for the models directory: the files are not found by
  the pack's fallbacks either); otherwise it is omitted, since nothing stands in the way.
  Credentials stay `credential`, never `env`.

State: `needs_setup` when any requirement is `missing` or `invalid`, else `ready`; rascal never
reports `unavailable` (every action runs on every platform rascal ships). `cost.may_spend` stays
as it is (a configured filler with a price), so an unconfigured rascal describes free actions. A
call is refused for a requirement only when that call needs it (3.1), **before any run**: the
action's arguments are read for the filler first, the call's own readiness is computed, and when
any requirement it needs is `missing` or `invalid` (a filler with no configuration, an unknown
filler, the filler's key not set or already rejected in this session, `remove_prompt` without its
pack, a model file or a needed `env`), the call is a JSON-RPC `-32602` with refusal data
`{"reason": "not_ready", "run": null, "requirements": [...]}` (`refusal.json`) listing exactly
those requirements. It allocates nothing, writes no run, spends nothing and releases its
`client_run_id` (12.7). To carry it, `Actions::call`'s error becomes structured (`CallError {
message, data }`) in place of a string. On the command line, `rascal run` and `heal` make the same
check before creating the run directory and exit 64 naming the requirements (not under
`--replay`, which answers from a recorded run). A mask-only call
needs nothing and runs in every config state.

**CLI scenario.** `rascal describe --json` with no flags prints a valid description with
`seam` `ready` (no requirements) and `remove_prompt` `needs_setup` (config `missing`) and exits 0; `rascal describe --json --config bad.toml`
prints `invalid` with the parse error and exits 0; `rascal serve` with no flags answers
`initialize`, `describe` and a mask-only `seam` call.

**Acceptance.**
- [x] `describe` exits 0 and validates with no configuration, a missing file and an invalid file;
      the requirement states are as above; no credential value appears in any output (a test sets
      a key to a sentinel and searches stdout, stderr, the document and the refusal for it).
- [x] `serve` starts with no flags but `--runs`, with an invalid configuration, and with a pack
      whose launcher is missing; its `describe` agrees with the CLI's; a mask-only call completes;
      a call naming a filler without configuration is refused `not_ready` naming `rascal.toml`,
      validating against `refusal.json`, with no run directory, no runs entry and its
      `client_run_id` accepted on the next request; a call naming a filler whose key is unset is
      refused `not_ready` naming the `credential`, with no request to the filler.
- [x] `remove_prompt`'s readiness follows an install: missing launcher, launcher without model
      files, launcher with every file (`needs_setup`, `needs_setup`, `ready`), against a fake
      install tree; the model directory resolution matches the pack's (the pack's resolution and
      rascal's both check one table, `fixtures/readiness/models-dir.json`); a pack entry
      passing `RASCAL_TOOLS_MODELS` lists it as `env` (`present` when set; `missing` when unset
      and the fallbacks hold no model files; absent when they do).
- [x] With a priced HTTP filler configured, every action lists its `credential` and `model`; with
      none, only `remove_prompt` has requirements.
- [x] Neither `describe` nor readiness starts a pack or makes a request (the fake pack's start
      marker is absent; the fake HTTP filler saw nothing).

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- describe::
readiness:: serve_contract::`; `cargo nextest run -p rascal-core --test it -- config::`; `just
check`.

**Dependencies.** 12.1, 12.7. **Files.** `crates/rascal-core/src/config.rs` (`ConfigState`,
`Config::identity`), `crates/rascal-mask/src/readiness.rs` (new), `describe.rs`, `actions.rs`,
`serve.rs`, `packs.rs`, `main.rs`, `run.rs`, `heal.rs`, `crates/rascal-mcp/src/server.rs`
(`CallError`, the refusal), `crates/rascal-testkit/src/mcp_client.rs`, `crates/rascal-mask/tests/it/readiness.rs` (new),
`describe.rs`, `serve_contract.rs`, `tools/python/tests/`. **Size.** L, split into the config
state with `describe` and `serve` (12.2a) and the pack and file requirements (12.2b) if it runs
long.

### Task 12.3: `credential`

**Description.** A typed `Error::Credential { filler, reason }` replaces today's text matching
for a provider's 401 or 403 during a run (today `provider`, retryable): status `error`, code
`credential`, `retryable: false`, exit 3 on the command line. `credential` is only that (2.2): a
key that is not set is refused `not_ready` before any run (12.2); if one still goes missing at the
call (`filler/http.rs`, today a `usage`), that is an `internal` error, since 12.2's check should
have caught it. A rejection is a known call that counts zero (table-manners 4.5: the provider
answered, refusing before any work), not a call not sent: its record says `charge: known`, the
reservation is settled at zero, and the run's `spend.basis` is `measured`. The session
keeps what providers said about each credential: `serve`'s `ContractActions` holds `answers`
(`key_env` → `valid` | `invalid`), updated when a run ends from its filler call records (a 2xx →
`valid`, a 401 or 403 → `invalid`, anything else leaves it as it was; a cache hit says nothing),
and readiness (12.2) reads it, so a fresh session shows `present` until a real call settles it.
A later call whose filler's key is `invalid` in this session is refused `not_ready` by 12.2's
check, without sending (the process environment cannot change within a session).

**Acceptance.**
- [x] Through the fake HTTP filler: a 401 and a 403 each end `error`/`credential`,
      `retryable: false`, spend 0, the mask kept in `images.out` (2.3); `rascal heal` exits 3.
- [x] After a 2xx the credential reads `valid` in `describe` and after a 401 `invalid`; a new
      `serve` shows `present` again; the next call after a 401 is refused `not_ready` naming the
      `invalid` credential, with no run and no request (the fake filler's `seen` unchanged).
- [x] `provider` stays retryable for a 5xx and a timeout stays `timeout`.

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- fillers::
heal:: serve_contract:: readiness::`; `just check`.

**Dependencies.** 12.2. **Files.** `crates/rascal-core/src/error.rs`,
`crates/rascal-mask/src/filler/{mod,http}.rs`, `heal.rs`, `run_json.rs`, `actions.rs`,
`readiness.rs`, `crates/rascal-mask/src/exit.rs`, tests `fillers.rs`, `heal.rs`,
`serve_contract.rs`, `readiness.rs`. **Size.** M.

### Task 12.4: The cap and the budget apart

**Description.** `Error::Budget` splits: `Error::Cap` (the call's worst case is over its
effective per-call cap) and `Error::Budget` (the session cannot reserve it), codes `cap` and
`budget`, both `error`, never retryable. The effective cap is the configured `call_cap_usd`
lowered by `max_spend_usd`; a `max_spend_usd` above the configured cap is now `invalid`
(`usage`) naming both amounts, never silently lowered (today `min(cap, asked)`); with no
configured cap the caller's value is the cap. The order at the limits check (`actions.rs`
`reserve`) is fixed and documented: arguments, then (from 12.6) the estimate, then the cap, then
the budget (requirements are checked before allocation, 12.2) and the reservation in the same step under the ledger's lock (9.2 unchanged).

**Acceptance.**
- [x] With `call_cap_usd = 0.10` and a filler at 0.04: `max_spend_usd: 0.2` is `invalid`/`usage`;
      `max_spend_usd: 0.03` is `error`/`cap`; with `session_budget_usd = 0.05` the second call is
      `error`/`budget`; each with zero spend, no filler call, and the ledger's `reserved_usd` back
      at 0.
- [x] Uncapped, `max_spend_usd: 0.03` gives `cap` and `0.05` runs.
- [x] The 9.2 race test still gives one run and one refusal, now `budget`.

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it --
serve_contract::`; `just check`.

**Dependencies.** 12.1 (track B; parallel with 12.7, 12.2 and 12.3). **Files.**
`crates/rascal-core/src/error.rs`, `crates/rascal-mask/src/{actions,run_json}.rs`,
`crates/rascal-mask/tests/it/serve_contract.rs`, `docs/contract.md` (the order). **Size.** S.

### Task 12.5: The estimate tool

**Description.** `estimate` becomes the contract's reserved tool. Its input is `{action,
arguments}`, where `arguments` is exactly what the paid `tools/call` would send (images,
locations, `filler`, `set`, `max_spend_usd`, `return_images`, …); today it takes `{action,
filler, max_spend_usd}`. It answers with `estimate.json` as `structuredContent`, built by one
function shared with the limits check so the two cannot disagree:

- `binding`: `sha256:` over the canonical serialization (ADR 0016) of `{action, arguments,
  tool_version, config_identity, session_id}`, where `return_images` and `image_blocks` are left
  out of `arguments` (they only choose how results are delivered, 4.5), each image slot's value is
  replaced by `{"file_sha256": …}` (a path read and hashed, inline bytes decoded and hashed, in memory; `expect`
  verified as the call would and then dropped) and a `Mask` input by the stored mask's identity.
  Every other argument (`max_spend_usd` included) enters as sent.
- `effective`: `model` is `fillers.<name>` for the call's filler (null for a mask-only call);
  `config_identity` from 12.2 (the loaded file's, or the built-in defaults' with no file; null
  only when the call needs a configuration that is missing or invalid); `x-rascal`: `{filler: {name, kind: "http" | "command", revision,
  paste, fill_max_side}}` or `{filler: null}`.
- `worst_case_usd` (the reservation the limits check would make: the filler's
  `spend_usd_per_call` for the one call a heal makes, 0 for a mask-only call) and `cap_usd`
  (12.4's effective cap, null when uncapped). A mask-only call with no file is priced: allowed,
  `worst_case_usd` 0, `config_identity` the built-in defaults' digest. When the call cannot be
  priced, `worst_case_usd` is null and `refused` is `readiness`, never a made-up zero (4.5): a
  filler that is not configured, or `remove_prompt` without its pack; `config_identity` is null
  there only when what is missing is the configuration itself (no file, or an invalid one).
- `allowed` and `refused`: `readiness` when the call's readiness (12.2, for its filler) is not
  `ready`, else `cap`, else `budget`, else allowed. A call that is not ready is not an error: it
  is an estimate refused for `readiness`. A wrong request is not an estimate (an estimate is not a
  run), and is a JSON-RPC `-32602` in one of two forms (4.5, settled at table-manners `e503a23`):
  arguments the `estimate` input schema rejects (no `action`, `arguments` not an object) are plain
  invalid params with no refusal data (`action` is a string there, not an enum, so an unknown
  action reaches rascal's own check); any other wrong request (an unknown action, an action's bad
  argument, `max_spend_usd` above the configured cap or not above zero, an `expect` that does not
  match its image) carries refusal data `{"reason": "invalid_arguments", "run": null, "error":
  {"code": "usage" | "input", "retryable": false, "reason": …}}` (`refusal.json`), the code as
  the run would have given it (`input` for an `expect` mismatch or an image rascal cannot take,
  `usage` otherwise) and the reason naming the argument.
- `readiness` (the call's own, 3.1), `session` (one atomic observation, the same function as
  `rascal://session`; null on the command line), `session_id` (null on the command line), and
  `expires`: one hour after the estimate (rascal's prices
  are configuration, fixed for the session; the hour bounds what a host may hold), a constant a
  testkit build may shorten through `RASCAL_TESTKIT_ESTIMATE_TTL_MS`.

The server records every binding it issues with its expiry and the parts it was made from (the
image digests by slot, the other arguments' digests by name), bounded to the newest 1024, for
12.6. An estimate never waits for the permit (9.3), never allocates, imports, writes a run or
touches the workspace, never starts a pack and never calls a filler. `rascal estimate --json
[--config <path>] <request.json | ->` prints the same document with no session (4.5):
`session_id` and `session` are null, the binding is computed over a null session id so no server
accepts it, and `budget` can never be the refusal (there is no ledger; the cap and readiness still
apply); it exits 0, or 64 on a bad request. M9's `estimate` fields (`call_cap_usd`) are gone.

**CLI scenario.** `echo '{"action":"seam","arguments":{"image":{"path":"/abs/pano.png"},"filler":"fake"}}' | rascal estimate --json --config rascal.toml -`
prints an allowed estimate with `worst_case_usd` 0.04, a binding and `session: null`; with the
image changed by a byte the binding changes.

**Acceptance.**
- [x] Every estimate validates against `urn:table-manners:core:estimate`: allowed; refused for
      `cap`, `budget` and `readiness`; a mask-only call with a configuration (`model` null, worst
      case 0, allowed), and the same with no file (`config_identity` the defaults' digest);
      `remove_prompt` with no file (`refused: "readiness"`, `worst_case_usd` and
      `config_identity` null).
- [x] The binding is stable for the same call (path or the same bytes inline give one binding,
      and so do calls differing only in `return_images` or `image_blocks`), and changes with the
      image's bytes, any other argument, the configuration, the session and the version (the
      version through a unit test of the recipe).
- [x] `rascal estimate --json` prints an estimate with `session_id` and `session` null that
      validates; a served call carrying its binding fails `stale_estimate` (12.6).
- [x] `estimate` is in `tools/list` with the `{action, arguments}` schema, never in `actions`; it
      answers while a run holds the permit; it leaves the runs root, the workspace's generation and
      the session's `reserved_usd` unchanged.
- [x] An unknown action, an action's bad argument, or `max_spend_usd` over the configured cap is a
      JSON-RPC `-32602` with `invalid_arguments` refusal data (code `usage`) naming it, and an
      `expect` mismatch the same with code `input`, each validating against `refusal.json`;
      `estimate` called without `action` is a plain `-32602` with no refusal data; a call that is
      not ready is an estimate with `refused: "readiness"`, not an error.

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- estimate::
serve_contract::`; `just check`.

**Dependencies.** 12.2, 12.3, 12.4. **Files.** `crates/rascal-mask/src/estimate.rs` (new:
binding, effective, the issued table), `actions.rs`, `describe.rs`, `main.rs`,
`crates/rascal-mask/tests/it/estimate.rs` (new), `serve_contract.rs`. **Size.** L.

### Task 12.6: A call bound to its estimate

**Description.** `serve` reads `_meta["table-manners/estimate"]` on a paid `tools/call` and hands
it to the action with the correlation id: `Actions::call` takes a `CallMeta { client_run_id,
estimate }` in place of the bare id. At the limits check, first of all, a named estimate is
recomputed for the call as it arrived (12.5's function, over the run's resolved inputs) and
compared: a binding this session never issued (another session, which for rascal also means any
other configuration, since a session reads its configuration once), a binding that differs (an
image's bytes or an argument changed: the reason names the slot or argument, from the issued
parts; a command-line estimate's binding is never issued by a server), or one past its `expires`, fails the run with `stale_estimate` (`invalid`, not retryable,
spend 0, before any step and any reservation). A malformed value (not `sha256:<64 hex>`) is the
same `stale_estimate`, not a JSON-RPC error, since the run exists. A call that names no estimate
is checked against the limits alone, and a matching estimate is no reservation: the cap and the
budget are checked again as in 12.4.

**Acceptance.**
- [x] A call carrying its estimate's binding runs; the same call after a byte of its image
      changed, with another argument, under a restarted server (a new session) or under a server
      with another configuration, and after the testkit expiry, each ends
      `invalid`/`stale_estimate` naming why, with no filler call and `reserved_usd` unchanged.
- [x] An allowed estimate followed by a call that the budget can no longer hold (another call
      spent in between) ends `budget`, not `stale_estimate`.
- [x] The testkit client sends any `_meta`; a call with no estimate behaves as in 12.4.

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- estimate::
serve_contract::`; `cargo nextest run -p rascal-mcp --test it`; `just check`.

**Dependencies.** 12.5. **Files.** `crates/rascal-mcp/src/server.rs` (`CallMeta`, the
`table-manners/estimate` key), `crates/rascal-mask/src/{actions,estimate,run_json}.rs`,
`crates/rascal-core/src/error.rs` (`Error::StaleEstimate`),
`crates/rascal-testkit/src/mcp_client.rs`, tests `estimate.rs`, `serve_contract.rs`. **Size.** M.

### Task 12.7: A claim released when the request gets no run

**Description.** Today a claim is never released (`server.rs` `claims`). From v0.3 (4.4, 5): a
request that ends in a JSON-RPC error while its claim still has no run reference (the action's
`Err` before allocation, a panic before allocation, any refusal before a run) releases its id, so
the client retries the corrected request under the same id; 12.2's `not_ready` refusal is one
such error. A request cancelled before
allocation (in the permit wait, or at the recheck once the permit is held) keeps its id claimed
with `run: null`; a request that reached allocation keeps it with its reference, whatever its
outcome. The release and the claim are under the same lock, so a concurrent request with the id
sees either the claim or nothing.

**Acceptance.**
- [x] A request with id X that fails before a run (an actions double in `rascal-mcp`'s tests,
      `tests/it/claims.rs`, returns `Err` or panics before allocation; the testkit cannot hold it,
      since it does not depend on `rascal-mcp`) gets a plain `-32602` with no refusal data (only
      rascal's `class`); a second request with X is accepted and its run carries X; a request
      that fails after allocation keeps X with its run.
- [x] A request with id Y cancelled while queued behind a slow run leaves no run; a retry with Y
      gets `client_run_id_reused` with `run: null`.
- [x] M11's reuse tests still pass (an id whose run ended is refused with that run).

**Verification.** `cargo nextest run -p rascal-mcp --test it`; `cargo nextest run -p
rascal-mask --features testkit --test it -- serve_contract::`; `just check`.

**Dependencies.** 12.1 (first in track A; parallel with 12.4 and 12.8). **Files.**
`crates/rascal-mcp/src/server.rs`, `crates/rascal-mcp/tests/it/`,
`crates/rascal-mask/tests/it/serve_contract.rs`. **Size.** S.

### Task 12.8: Renders a host can embed

**Description.** `rascal dash <run dir> --serve` becomes the contract's server render (3.2): its
stdout is exactly one line, `http://127.0.0.1:<port>/`, once it is listening, and nothing else
ever (tracing stays on stderr); it binds port 0 unless `--port` says otherwise; it serves until
its stdin reaches end of file or it receives SIGTERM, then exits 0 (3.2) (a thread reads stdin to EOF;
a person at a terminal keeps it open as before, and the docs note that `< /dev/null` now exits at
once); `--frame-ancestor <origin>` sends `Content-Security-Policy: frame-ancestors <origin>` on
every response, the origin checked as `http://` or `https://`, a host and an optional port, no
path (otherwise 64), and without the flag the page sends `frame-ancestors 'none'`. A document
render is added: `rascal dash <run dir> --document` writes ONE self-contained file (3.2, settled at
table-manners `e503a23`): the page with every image inline as a `data:` URI and its styles and
script inline, nothing it needs beside it, so a host reads that one file and nothing next to it.
rascal chooses the path, outside the run directory: `$XDG_CACHE_HOME/rascal/dash/<run id>.html`
(a run directory is never written after its `run.json`; 3.2: a render never modifies the run
directory). It prints only that absolute path, as its one stdout line, and exits 0 once the file
exists. `{run.dir}` and `{frame_ancestor}` appear only as whole arguments (3.2). Every render's
`command[0]` is the tool's own name, `rascal` (the description's `tool.name`), and the host runs
the same executable it serves, never another program or another install found on `PATH` (3.2,
settled at `e503a23`); the renders below already do this, and rascal adds nothing for it. The description's
renders become `{name: "dash", kind: "server", ready_timeout_s: 10, command: ["rascal", "dash",
"{run.dir}", "--serve", "--port", "0", "--frame-ancestor", "{frame_ancestor}"]}` and `{name:
"page", kind: "document", command: ["rascal", "dash", "{run.dir}", "--document"]}`.

**Acceptance.**
- [x] Spawned with piped stdin, the server prints one URL line within the timeout and no other
      stdout; a request gets the CSP header with the given origin; closing stdin ends it with exit
      0; SIGTERM ends it with exit 0; a bad origin exits 64 before listening.
- [x] `--document` prints one absolute path, and nothing else, to a file that exists outside the
      run directory and leaves the run directory's listing unchanged; the file is self-contained
      (no `src`, `href` or `url(` names anything but a `data:` URI or an in-page anchor).
- [x] The description's renders validate (server with `ready_timeout_s`, document without), and
      each render's `command[0]` is the description's `tool.name`.

**Verification.** `cargo nextest run -p rascal-mask --features testkit --test it -- dash::
describe::`; `just check`.

**Dependencies.** 12.1 (track C; parallel with everything after 12.1). **Files.**
`crates/rascal-mask/src/{dash,main,describe}.rs`, `crates/rascal-mask/tests/it/dash.rs`,
`docs/diagnostics.md`. **Size.** S.

### Task 12.9: Fixtures, the no-call proof, docs

**Description.** rascal's v0.3 fixtures for table-manners `fixtures/rascal/` (CONTRACT 7, item
7), produced by rascal's own server and CLI through the independent client in
`table_manners_fixtures.rs`, never written by hand, normalised as in 11.6, written under
`UPDATE_FIXTURES=1` and compared otherwise. The configuration gains an HTTP filler served by the
testkit's fake (its `key_env` set or unset per server) beside the command fillers, and a cap. The
set: `describe.json` (configured, `ready`); `describe-unconfigured.json` (no `--config`);
`describe-credential-missing.json`, `-present.json` (also the `ready` action), `-valid.json`
(after a 2xx) and `-invalid.json` (after a 401); `estimate-allowed.json`,
`estimate-refused-cap.json`, `estimate-refused-budget.json`, `estimate-refused-readiness.json`;
`run-stale-estimate-image.json`, `-argument.json`, `-session.json` (a server restarted under
another configuration: for rascal one fixture stands for both the configuration and the session,
as the coordinator settled) and `-expired.json`; `estimate-cli.json` (no session); `estimate-unconfigured.json` (a mask-only call with no
file: allowed, `worst_case_usd` 0, the defaults' identity), while `estimate-refused-readiness.json` above is
`remove_prompt` without its pack (`worst_case_usd` null); `run-cap.json`, `run-budget.json`, `run-credential-rejected.json` (a 401 during the run);
`refusal-not-ready.json` (a call naming a filler whose key is unset, refused before any run; a
missing credential is no longer a run); `run-claim-released.json` (the accepted retry, carrying the id
the refused request released) and `refusal-cancelled-before-allocation.json` (`run: null`);
and M11's set kept at v0.3. The no-provider-call proof is a test of its own
(`no_provider_call.rs`): across every config state and credential state above, `describe` (CLI
and tool), `estimate` (CLI and tool, allowed and each refusal) and every `not_ready` refusal leave the fake HTTP filler's
`seen` empty, the fake pack's start marker absent, the runs root and the workspace generation
unchanged. rascal copies the fixtures into the table-manners branch `v0.3` and commits there,
and updates rascal's column of MAPPING's v0.3 table. Docs move to v0.3: `docs/contract.md`
(readiness, the `not_ready` refusal, the estimate and its binding recipe, the limits check's
order, `credential`, the claim release, the renders), SPEC "External interface", README (`describe` and `serve` with no
configuration, `rascal estimate`), `docs/fillers.md` (`credential`), `docs/architecture.md`,
and an M12 amendment to ADR 0020 recording the decisions below.

**Acceptance.**
- [x] table-manners' fixtures test (`cargo test --test fixtures` there) passes for
      `fixtures/rascal/`; rascal's own check passes without `UPDATE_FIXTURES`.
- [x] `no_provider_call.rs` passes and fails when an estimate is made to call the filler (checked
      once by hand, recorded in the commit message).
- [x] The docs name v0.3 and no longer describe `needs_env`, the old `estimate` arguments or
      `budget` for the per-call cap.

**Verification.** `UPDATE_FIXTURES=1 cargo nextest run -p rascal-mask --features testkit --test
it -- table_manners_fixtures::`, then the same without it; `cargo nextest run -p rascal-mask
--features testkit --test it -- no_provider_call::`; in the table-manners checkout `cargo test`;
`just check`.

**Dependencies.** 12.2 to 12.8. **Files.** `crates/rascal-mask/tests/it/{table_manners_fixtures,
no_provider_call}.rs` (the second new), `crates/rascal-mask/tests/it/main.rs`,
`fixtures/table-manners/`, `docs/contract.md`, `SPEC.md`, `README.md`, `docs/fillers.md`,
`docs/architecture.md`, `docs/adr/0020-table-manners.md`, table-manners `fixtures/rascal/` and
`MAPPING.md`. **Size.** M.

### Task 12.10: The pin to the release (owner)

**Description.** Once squint's v0.3 fixtures pass too, the table-manners branch merges, the owner
tags `v0.3.0` and publishes the crate, and rascal pins `table-manners = "=0.3.0"` in place of the
path; `scripts/check-contract.sh` reports it as the newest. The fixtures are regenerated if the
published contract differs from the branch rascal built against, and the milestone branch merges
into `main` by the checkpoint procedure.

**Acceptance.**
- [ ] `just check` and CI green on both platforms on the pinned release; the milestone branch
      merged; tag `m12`.

**Verification.** `scripts/check-contract.sh`; `just ci-local`; CI on the merge commit.

**Dependencies.** 12.9; squint's v0.3 fixtures; the owner's tag and publish. **Files.**
`Cargo.toml`, `Cargo.lock`, `PLAN.md`, `tasks/todo.md`. **Size.** XS.

### Checkpoint M12
- [x] Tasks 12.1 to 12.10 done, CI green on both platforms; tag `m12` (2026-11-01, table-manners 0.3.0 from crates.io).

### Owner decisions for M12
- No default configuration lookup: `--config <path>` is the only way to choose one, for
  `describe`, `serve` and `estimate` alike (4.6 allows either). Recommended, assumed.
- `serve`'s `--workspace` becomes optional with a data-directory default, so a host starts
  `rascal serve` with nothing but `--config`. Recommended, assumed.
- An action's readiness describes its default use (3.1, settled at table-manners `6fbe6fa`):
  for rascal, what the configuration offers: the mask alone when no filler is configured (so an
  unconfigured rascal is `ready` but for `remove_prompt`), healing with the configured fillers
  otherwise, so their variables are named for the host. A call fails only for what it needs.
  Recommended, assumed.
- `estimate` is served in every configuration (with no file a mask-only call estimates at 0
  under the built-in defaults' identity, 4.5); the contract lets any tool serve it and requires it of one
  that may spend (settled). Assumed.
- A missing key is refused `not_ready` before any run on the command line too (exit 64, no run
  directory), mirroring `serve`. Recommended, assumed.
- Estimates expire after one hour. Recommended, assumed.
- A server render exits on stdin's end of file by default, not behind a flag. Recommended,
  assumed.

## Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| The fan constants, designed on one canvas scale, do not transfer to the owner's corpus | High: criterion 6 fails | Constants are versioned sets; 6.4 budgets a refit as `seam-02`; the thin arm is a constant set, so the comparison is apples to apples; the release gate waits |
| `tighten` finds texture edges instead of the scratch on real prints | Medium | The textured-ground fixture gates 4.3; untightened fallback keeps the stroke usable; the path fan is the named follow-up |
| No segmenter meets licence plus text-prompt plus export | Medium | Branch B is specified in full; the decision is taken in 4.5 before any pack work; the bundle task comes after the model task |
| The Poisson solve is slow or unstable on large bands | Medium | M7 is last and isolated; the benchmark is recorded, `feather` remains the default |
| The official MCP crate lags the pinned revision | Low | 3.1 decides by ADR; the in-house transport is small and the independent client proves either |
| Cross-platform wheelhouses drift between Python patch versions | Medium | One bundle per platform with the interpreter version pinned in `bundle.toml`; CI builds and installs both with empty caches |
| The offline suite grows past three minutes | Low | Golden images are small synthetics; the shim replaces real sync in tests; 7.2 records the final timing |
| Workspace durability differs by filesystem | Low | ADR 0017 names the primitive per platform; the shim tests the order; a real-sync test runs on each CI platform |
| `just ci-local`'s Linux leg runs the host's architecture, not x86_64 | Low | GitHub CI on the pushed merge is the x86_64 gate before any tag |

## Owner decisions needed

| # | Decision | Needed before |
|---|---|---|
| 1 | Confirmation of no conflicting obligation | the bootstrap commit |
| 2 | The forbidden-word list location | 0.1's first commit |
| 3 | A real local filler and a real panorama on the owner's machine | 2.8's by-hand run |
| 4 | The MCP transport | 3.1 (by ADR) |
| 5 | The segmenter, its branch and the export's redistribution terms | 5.2 (decided in 4.5) |
| 6 | The private corpus location, contents and groups (at least `min_decided` images each) | 6.1 |
| 7 | The path fan before or after v1 | after M7 unless 4.3 shows the need |
