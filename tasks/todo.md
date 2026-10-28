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
- [ ] 0.4 Copied schemas and the schema drift harness
- [ ] 0.5 Owner readiness
- [ ] Checkpoint M0 → tag `m0`

## M1 Core
- [ ] 1.1 Walking slice → tag `m1.1`
- [ ] 1.2 Frames, identity, canonical serialization, locations and mapping
- [ ] 1.3 The mask record and effective weight
- [ ] 1.4 Op contracts
- [ ] 1.5 Renderings and the `render` op
- [ ] 1.6 Recipes with typed bindings
- [ ] 1.7a Workspace generations and the commit protocol
- [ ] 1.7b Repair, recovery and the journal projection
- [ ] 1.8a Primitives, `cut`/`put` and the Plan
- [ ] 1.8b `line`, `stroke` and the full `rasterize`
- [ ] 1.9 `inspect`, compatibility export, self-contained runs, docs coverage
- [ ] 1.10 The run page and tracing → tag `m1.2`
- [ ] Checkpoint M1 → tag `m1`

## M2 Seam
- [ ] 2.1 `fan` and the band profile
- [ ] 2.2 `profile` and `feather`
- [ ] 2.3 `ramp` and `apply`
- [ ] 2.4 `window` and `unwindow`
- [ ] 2.5 `paste` (feather and hard)
- [ ] 2.6 `measure` v1
- [ ] 2.7 Fillers and the fake fillers
- [ ] 2.8 `heal`'s typed graph and the shipped `seam` recipe → tag `m2.1`
- [ ] Checkpoint M2 → tag `m2`

## M3 MCP
- [ ] 3.1 The transport decision and the independent client
- [ ] 3.2 `rascal serve` → tag `m3.1`
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
