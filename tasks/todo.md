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
- [x] Checkpoint M0: owner reads the ADRs → merge to `main` → push → CI green on both platforms → tag `m0` (tagged; the line was never ticked)

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
- [x] 3.3 The pack client and the isolation suite
- [x] Checkpoint M3 → tag `m3`

## Observations from the first real runs (2026-10-29, not gating)
- [ ] Speed of a heal is almost all output: a 5760×2880 result as PNG at the default level and a 63 MB run page embedding every image at full size (about 70 s of a 90 s run). Faster PNG level or an opt-in JPEG result, thumbnails in the page, parallel writes. Before the M6 lab rounds.
- [x] LaMa leaves a grey veil in smooth sky with the standard band (confirmed by seam-01 and seam-02); a filler entry's `overrides` now choose the band `heal` runs for it (docs/fillers.md); Poisson paste (M7) is the structural answer.

## M4 Refine
- [x] 4.1 `grow`, `shrink`, `close`
- [x] 4.2 `subtract`, `select`, `invert`
- [x] 4.3 `tighten`
- [x] 4.4 The `remove`, `repair`, `repair-area`, `protect` recipes and the acceptance script → tag `m4.1`
- [x] 4.5 The segmenter decision (branch A: OWLv2 + MobileSAM, ADR 0018, 2026-10-29)
- [x] Checkpoint M4 → tag `m4`

## M5 Pack
- [x] 5.1 The pack skeleton, `echo_region`, `fetch` and the harness
- [x] 5.2 `segment` under branch A (ADR 0018): OWLv2 + SAM 2.1 on onnxruntime, `remove-prompt`, the models in the bundle
- [x] 5.3 The bundle and offline setup (the model artifacts' bytes join the bundle with 5.2; a bundle URL for `setup` comes with the release, 7.2) → tag `m5.1`
- [ ] Checkpoint M5 → tag `m5` once the model files are published as the `models-2026-10` release (the owner runs the release command; everything else of M5 is merged)

## M6 Lab
- [x] 6.1 Corpus, protocol, matrix and `fan-thin-v1` (built on the example corpus over the fixtures; owner decision 6 names the private corpus for 6.4)
- [x] 6.2 `lab run`
- [x] 6.3 `lab pairs` and `lab decode` → tag `m6.1`
- [x] 6.4 The owner's first lab round: seam-01 (criterion 6 not met under LaMa), seam-02 (two masks by two fillers: the filler decided), seam-03 (forty-seven panoramas under Bria: sky preferred 18 of 19, structured preferred 14 of 16). **Criterion 6 met 2026-10-29** (`docs/results/seam-0{1,2,3}.md`)
- [x] Checkpoint M6 → tag `m6` (2026-10-29, criterion 6 met)

## M7 Poisson and release
- [x] 7.1 The solver and `paste --mode poisson`
- [x] 7.1b A scaled window per filler (`fill_max_side`, decision 10): the request downscaled, the fill scaled back, journaled and in the cell identity; `lama-scale-01` judged it: sky 20 of 21 for the scaled window, structured 12 to 14 (`docs/results/lama-scale-01.md`)
- [~] **The band swallows solid objects on a smooth surface** (the object cap, `fan` 1.1.0 and `fan-v2`, is built; `objects-01` judged it, `docs/results/objects-01.md`: preferred 10 to 3 on the structured group, worse 3 to 7 on sky where the cap took clouds for objects; `fan-v3` follows from its findings, an edge must run down the frame for `object_run` of the height, an object on the line is removed whole and the cap tapers; `objects-02` judged it against fan-v1, `docs/results/objects-02.md`: 4 wins, 1 loss, 18 same, the regressions gone, the taper's step fixed after the round in `fan` 1.2.1; fan-v3 is the seam recipe's and the op's default since 2026-10-30, decision 12) (owner, 2026-10-29, after the lama-scale-01 round): where a ceiling light, a track spot or a fitting sits on an otherwise smooth ceiling, the fan widens for the smooth surface, the object falls inside the band, and the filler removes it. Two cells to keep for the work: `lama-scale-01`, `sunburst-training-17-office-break-room` (cells `56af200d…` scaled and `683d99ad…` full: a ceiling light on a smooth ceiling, gone in both) and `str-10` (cells `262d246e…` scaled and `2d0edb13…` full: a track light removed, the rest of the row kept). The fix belongs to the mask builder, after the initial development: the fan (or a step before it) should recognise a solid object inside the smooth region and go around it, by pattern and by how close detail lies to the seam, so the band protects what it crosses rather than erasing it. A `fan-v2` with its own lab round; the lab pages and run directories are on the owner's machine under `lab/lama-scale-01/`.
- [ ] **An object on or near the line is kept, not removed** (owner, 2026-10-30, after objects-02): the harbour chimney on the wrap edge is covered whole by every band and the filler invents stubs; fan-v2's stripe through it doubled it. The band cannot settle this: the mask builder should recognise a solid object on or near the line and keep it, healing the seam through it (and, for a generated panorama whose halves do not meet, say so). Cells: `objects-02`, `sunburst-training-18-harbor-promenade` (identical masks under both arms, `neither`); `objects-01`, the break-room light.
- [ ] **An object's base and stand are left behind** (owner, 2026-10-29, the gumball-machine test with `remove-prompt`): the segmenter's plane covered the globe and body and stopped above the pedestal and base, so the fill left them standing. Two queries (`"gumball machine. black metal stand"`, `max_instances` 2) work around it; the improvement is to extend a standing object's plane down to where it meets the floor, the same family as the light fixture. The owner's photo is `~/Downloads/neon-retro.png`.
- [ ] **Thin parts escape the plane** (the eagle's far wingtip, `~/Downloads/penguin-eagle-test.png`): SAM's plane stops short of the thinnest feathers and a 4 px grow does not reach them, so a faint trace stays under either filler; `--set grow.px=8` is the knob, a plane that follows thin structure the answer.
- [~] 7.2 The final timing run and the release: timing recorded (`docs/results/suite-timing.md`, 2 min 41 s), the criteria table in `docs/architecture.md`, `setup` from an archive or URL with its digest, the `bundles` workflow, the README install section; left: the owner's word to push `v0.1.0` (and the models release)
- [ ] Checkpoint M7 → tag `m7` → tag `v0.1.0` when the owner calls the work ready (decision 11); the machinery is complete
