# rascal task checklist

Mirror of `PLAN.md` draft 4. Tick a task only when its acceptance criteria and verification pass
and the commit is on the milestone branch. Checkpoints need the owner's review, a pushed merge
with green CI on both platforms, and then the tag.

## Pre-M0
- [x] Owner confirms no conflicting obligation (dated in docs/owner-decisions.md, 2026-10-28)
- [x] Owner names the forbidden-word list location (`RASCAL_FORBIDDEN_WORDS`, 2026-10-28)
- [x] `git init`, remote added, bootstrap documentation commit on `main` → tag `m0-base`
- [x] Codex plan review, three rounds, under docs/reviews/plan-review-*.md

## M0 Scaffold
- [x] 0.1 Workspace, crate skeletons, checks (including the clean-room scan and `ci-local`)
- [x] 0.2 CI, hooks, ignore rules (CI run itself awaits the owner's first push)
- [x] 0.3 Docs skeleton and ADRs 0001 to 0014 (owner reads at the checkpoint)
- [x] 0.4 Copied schemas and the schema drift harness
- [x] 0.5 Owner readiness (rows 1 and 2 made; 3 to 7 pending their tasks)
- [x] M0 review and test review, findings fixed or recorded (docs/reviews/m0-checkpoint.md)
- [x] `just check` and `just ci-local` green on the final branch; hooks tested; schema harness green
- [ ] Checkpoint M0: owner reads the ADRs → merge to `main` → push → CI green on both platforms → tag `m0`

## M1 Core
- [x] 1.1 Walking slice → tag `m1.1`
- [x] 1.2 Frames, identity, canonical serialization, locations and mapping
- [x] 1.3 The mask record and effective weight
- [x] 1.4 Op contracts
- [x] 1.5 Renderings and the `render` op
- [x] 1.6 Recipes with typed bindings
- [x] 1.7a Workspace generations and the commit protocol
- [x] 1.7b Repair, recovery and the journal projection
- [x] 1.8a Primitives, `cut`/`put` and the Plan
- [x] 1.8b `line`, `stroke` and the full `rasterize`
- [x] 1.9 `inspect`, compatibility export, self-contained runs, docs coverage
- [x] 1.10 The run page and tracing → tag `m1.2`
- [x] Checkpoint M1 → tag `m1` (reviews in docs/reviews/m1-checkpoint.md)

## M2 Seam
- [x] 2.1 `fan` and the band profile
- [x] 2.2 `profile` and `feather`
- [x] 2.3 `ramp` and `apply`
- [x] 2.4 `window` and `unwindow`
- [x] 2.5 `paste` (feather and hard)
- [x] 2.6 `measure` v1
- [x] 2.7 Fillers and the fake fillers
- [x] 2.8 `heal`'s typed graph and the shipped `seam` recipe → tag `m2.1`
- [x] Checkpoint M2 → tag `m2` (reviews in docs/reviews/m2-checkpoint.md)

## M3 MCP
- [x] 3.1 The transport decision and the independent client
- [x] 3.2 `rascal serve` → tag `m3.1`
- [ ] 3.3 The pack client and the isolation suite
- [ ] Checkpoint M3 → tag `m3`

## M4 Refine
- [ ] 4.1 `grow`, `shrink`, `close`
- [ ] 4.2 `subtract`, `select`, `invert`
- [ ] 4.3 `tighten`
- [ ] 4.4 The `remove`, `repair`, `repair-area`, `protect` recipes and the acceptance script → tag `m4.1`
- [ ] 4.5 The segmenter decision
- [ ] Checkpoint M4 → tag `m4`

## M5 Pack
- [ ] 5.1 The pack skeleton, `echo_region`, `fetch` and the harness
- [ ] 5.2 `segment` under branch A, or branch B
- [ ] 5.3 The bundle and offline setup → tag `m5.1`
- [ ] Checkpoint M5 → tag `m5`

## M6 Lab
- [ ] 6.1 Corpus, protocol, matrix and `fan-thin-v1`
- [ ] 6.2 `lab run`
- [ ] 6.3 `lab pairs` and `lab decode` → tag `m6.1`
- [ ] 6.4 The owner's first lab round (criterion 6 gate)
- [ ] Checkpoint M6 → tag `m6`

## M7 Poisson and release
- [ ] 7.1 The solver and `paste --mode poisson`
- [ ] 7.2 The final timing run and the release
- [ ] Checkpoint M7 → tag `m7` → tag `v0.1.0`
