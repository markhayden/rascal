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
and the same with `--features rascal-mask/testkit`; `cargo test --workspace` and the same with
`--features rascal-mask/testkit`; `cargo deny check licenses bans sources`;
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
`crates/rascal-core/tests/schemas.rs`. **Size.** S.

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

**Verification.** `cargo test -p rascal-mask --test walking_slice`.

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

**Verification.** `cargo test -p rascal-core -- mask weight` and `cargo test -p rascal-mask --test inspect_json`.

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

**Verification.** `cargo test -p rascal-core -- recipe` and `cargo test -p rascal-mask --test recipes_by_name`.

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

**Verification.** `cargo test -p rascal-core -- export replay` and `cargo test -p rascal-mask --test inspect`; `scripts/check-docs.sh`.

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

**Verification.** `cargo test -p rascal-mask --test dash`.

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

**Verification.** `cargo test -p rascal-mask --features testkit --test heal`.

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

**Verification.** `cargo test -p rascal-mask --features testkit --test serve`.

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

**Verification.** `cargo test -p rascal-mcp --test client` (the pure parts) and `cargo test -p rascal-mask --features testkit --test packs --test isolation` (the fake runs as `rascal fake-pack`).

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

**Verification.** `cargo test -p rascal-mask --features testkit --test recipes`; `scripts/acceptance.sh`.

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

**Verification.** `uv run --frozen --offline --no-sync pytest`; `uv run --frozen --offline --no-sync ruff check . && … ruff format --check .`; `cargo test -p rascal-mask --features testkit --test packs_real`.

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
- [ ] Branch A: parity within `MODEL.md`'s tolerance; the export's SHA-256 recorded; no test
      downloads anything. Branch B: the absence test.

**Verification.** `uv run --frozen --offline --no-sync pytest -m segmenter`; `cargo test -p rascal-mask --features testkit --test remove_prompt`.

**Dependencies.** 5.1, 4.5. **Files.** `tools/python/src/rascal_tools/segment.py`, `tools/python/MODEL.md`,
`tools/python/artifacts.toml`, `crates/rascal-mask/recipes/remove.toml`. **Size.** M.

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

- [ ] The pack installs offline on both platforms; the branch in force is proven by acceptance.
- [ ] Review and test passes applied; `docs/reviews/m5-checkpoint.md`.
- [ ] Owner review → merge to `main` → push → CI green → tag `m5`.

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

**Verification.** `cargo test -p rascal-mask --test lab_config --test corpus_cli` and `cargo test -p rascal-ops --test fan_thin_widths`.

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

**Verification.** `cargo test -p rascal-mask --features testkit --test lab_run --test lab_identity`.

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

**Verification.** `cargo test -p rascal-core --test lab_decode` and `cargo test -p rascal-mask --features testkit --test lab_pairs`.

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
- [ ] The results document exists with the protocol digest, the outcome per group and the
      measures; criterion 6 is marked met, or the gate is recorded as open.

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
- [ ] The spec's cases plus the single-pixel zero-RHS case; the no-boundary component is
      `not_applicable`; non-convergence at a tiny `max_iter` is an operational error; a `wrap`
      frame is refused; a **benchmark** (not a gate) records the solve time for a 1000×1000 domain
      in release on the owner's machine in `docs/results/poisson-bench.md`.

**Verification.** `cargo test -p rascal-ops -- poisson paste::poisson` and `cargo test -p rascal-mask --features testkit --test heal_poisson`.

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
- [ ] Timing under three minutes recorded; the release workflow dry run green;
      `scripts/acceptance.sh` green from the unpacked archive; the criteria table complete.

**Verification.** `just publish-dry-run`; `just dist-check`; `scripts/acceptance.sh`.

**Dependencies.** 7.1. **Files.** `dist-workspace.toml`, `.github/workflows/release.yml`, `README.md`,
`docs/architecture.md`, `docs/results/suite-timing.md`. **Size.** M.

### Checkpoint M7

- [ ] All seven success criteria met with their proving command and outcome in
      `docs/architecture.md`, or a dated scope change recorded for any exception.
- [ ] Review and test passes applied; `docs/reviews/m7-checkpoint.md`.
- [ ] Owner review → merge to `main` → push → CI green → tag `m7` → tag `v0.1.0` → the owner
      publishes.

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
