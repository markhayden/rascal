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
- [x] Checkpoint M5 → tag `m5` (2026-11-01: the model files published as the `models-2026-10` release, digests checked against `artifacts.toml`; `m5` on the M5 merge `ddf2a3e`)

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
- [x] 7.2 The final timing run and the release (released as `v0.1.0`, 2026-11-01): timing recorded (`docs/results/suite-timing.md`, 2 min 41 s), the criteria table in `docs/architecture.md`, `setup` from an archive or URL with its digest, the `bundles` workflow, the README install section; the owner called it ready on 2026-11-01
- [x] M7 criteria checked against `41e09c2` (all seven met, `docs/architecture.md`) and the review and test passes applied (`docs/reviews/m7-checkpoint.md`, 2026-11-01)
- [ ] Checkpoint M7 → tag `m7`: `v0.1.0` released 2026-11-01 on the owner's word (decision 11); `m7` follows the owner's review and merge of the checkpoint record

## M8 Table manners (the shared contract, `table-manners` v0.1.0; ADR 0020, decision 13; PLAN.md M8)
- [x] 8.1 The contract pinned (`=0.1.0`, a warn-on-newer CI step) and the image reference: the source file's digest at import, projection and topology, the twelve pixel vectors reproduced (`rascal-core::contract`, `fixtures/contract/pixels/`, `scripts/check-contract.sh`; `workspace import` prints the file digest)
- [x] 8.2 Locations across the boundary: the contract's location to and from rascal's (`parts` ↔ `group`, every kind, `reason`, `score`/`score_kind`), verified against the imported image; `--location` takes a contract file (done: `contract::location_in`/`location_out`, the contract's location examples round-trip, a squint-shaped finding masks byte-identically to rascal's own location; rascal's v0.1 limits refused with the reason: several parts, over 64 vertices, `mask` in a location slot); `locations.used` and `found` move to 8.4 with the header
- [x] 8.3 `rascal describe [--json] [--config]`: one action per shipped recipe, validated against the contract's schema, pinned by `fixtures/contract/describe.json`, `docs/contract.md`; the `describe` MCP tool moves to 8.5 with `serve`, `estimate` to 8.6 with spend
- [x] 8.4 `run.json` with the shared header on every `run` and `heal`, atomically and last, on completed, invalid, error and cancelled runs; exit 130 and 143 on signals (done: `run_json` in the binary, the error-code map in `docs/contract.md`, a failed heal keeps its mask, an occupied `--out` untouched, both signals tested against the slow fake filler)
- [x] 8.5 `serve` under the contract: action tools beside the op tools, images by path or inline, `isError` results, progress heartbeat, cancellation, run resources and the runs list (done: `rascal_mcp::server::Actions`, `actions.rs`, `--runs`; the `rect` recipe's action is `mask_rect` so no action shadows an op; the cap and budget move to 8.6 with prices)
- [x] 8.6 The fill cache keyed by request identity (closes the lab's pay-twice note), `spend_usd_per_call`, `estimate` (done: `[cache]`, `[limits]`, the lab's `fills/`, `Error::Budget`; arms with identical requests share one call, a repeated heal is a hit with no spend, serve refuses past the cap or budget)
- [x] 8.7 `rascal playground`: finds and starts the shared playground, no UI
- [x] 8.8 Fixtures under `fixtures/contract/`, the first half of the cross-tool acceptance test in CI, `docs/contract.md`, SPEC "External interface", README, architecture (done: `fixtures/contract/results/` masked and compared; rascal's half of the acceptance test is `contract_locations.rs`; rascal's v0.1 limits written up for the contract's MAPPING in `docs/contract.md`)
- [x] Checkpoint M8 → tag `m8` (2026-10-31, `ebb773f`, CI green on both platforms)

## M9 Spend and launcher (Wafflehouse's review; decision 14; PLAN.md M9)
- [x] 9.1 Spend survives the status: not sent, known and unresolved calls; `filler call started`; the journal's spend in `run.json`, the unresolved part in the body (done: `filler::Charge`, `filler::spent`; tests in `fillers.rs`, `run_json.rs`, `serve_contract.rs`)
- [x] 9.2 The budget reserved, then settled: one atomic check and reservation in `serve` (done: `actions::Ledger` and its reservation; found and fixed with it: two runs allocated in one millisecond shared an id and a directory, now `run_json::allocate_ms`)
- [x] 9.3 Reads never wait on a run: `describe` and `estimate` off the permit (done: `Actions::exclusive`)
- [x] 9.4 Runs listed from allocation (done: the run directory is made at allocation; both in `serve_contract.rs`)
- [x] 9.5 The Wafflehouse launcher by H1 (done: `playground.rs`, the testkit seam `--installer`/`--terminal`; `tests/it/playground.rs`)
- [x] Checkpoint M9 → tag `m9` (2026-10-31, `a461801`, CI green on both platforms)

## M10 Operations (table-manners v0.4; R3, decision 14; PLAN.md M10; 10.1 on `main` at `=0.3.0`, then the branch `m10-operations` by path against table-manners branch `v0.4`, then `=0.4.0`)
- [x] 10.1 The path rule: `serve --allow-dir <abs>` (repeatable, absolute, resolved at start), narrowed by `[serve] allow_dirs`, none refusing every path; one `paths::admit` for every caller path (image slots, `mask`, `protect`, `workspace_import`), canonical form inside an allowed directory, read through the canonical path; one refusal text that says nothing about the file (`invalid`/`input`, `invalid_arguments` in `estimate`); writes only under rascal's own directories; mergeable alone on v0.3 (← M12) → tag `m10.1` on `main` (done 2026-11-01: `rascal_core::paths`, `tests/it/paths.rs`; landing now, closed by default, Wafflery passes `--allow-dir`)
- [x] 10.2 The pin to v0.4 by path and the shapes: `kind: "task"`, the core `vocabulary`, `allowed_dirs` in `rascal://session`, the real `invocation` on every result (the old 10.11: resolved arguments with no path, `effective` from the estimate, requirement names; `null` only on `invalid`), fixtures regenerated (← 10.1; the v0.4 schemas and types) → tag `m10.2`
- [x] 10.3 References, the store and the binding refusal: `contract_type`, `artifact_ref` (identity, frame, `on`, resource; never an id), `Workspace::resolve` by identity, `bind` (`wrong_type`, `wrong_version`, `unresolved` with a remedy), the `-32602` `binding` refusal releasing the id; `mask` and `protect` take references (← 10.2, 10.5) (done 2026-11-01: `rascal_core::binding`, `contract::artifact_ref`, `Workspace::resolve`; `tests/it/artifact_ref.rs`, `tests/it/binding.rs`)
- [x] 10.4 Operations described: the 24 built-in ops as actions (`kind: "operation"`, `group` via `Op::group`, `ports`, `params_schema` with `constants`, ready, free, `renders: []`), `artifact_types` (location json, Mask bundle, the rest none), `profiles` with `operations`, empty `artifacts` on every result (← 10.3)
- [x] 10.5 Native artifacts portable (PLAN.md Task 10.5, ADR 0022; library and CLI; the MCP tools are 10.10)
  - [x] Frames verified from their fields and parent; a commit never rebinds a frame id
  - [x] The mask bundle written: canonical ustar, `bundle.json` with the frame chain and member digests, the same mask giving the same bytes
  - [x] The mask bundle read: bounded, every member and the frame chain checked, refused whole on any fault, a PNG or any non-bundle never taken as a Mask, idempotent on a known identity
  - [x] `rascal workspace export-mask` and `import-mask` (`--sha256`), docs and SPEC
  - [x] `rascal workspace import-location`: rascal's or a table-manners location, validated against the named image
  - [x] Review follow-up (ADR 0022 accepted 2026-11-01): export refuses a bundle import would refuse; a version mismatch says to upgrade or that the format is gone; the bundle names its image, and an op on an imported mask whose image is missing names it by file digest with the `workspace import` remedy
- [x] 10.6 An operation call is a run: the op, pack and `workspace_*` tools removed from `serve`; in-ports bound before allocation with `Op::check_frames` (`wrong_frame`); a run committing into the store; `artifacts` in and out with previews (Mask, location); images and locations mirrored; the envelope in `body`; Line → Rasterize → Feather through the independent client (← 10.4) → tag `m10.3` (done 2026-11-01: `crates/rascal-mask/src/{operations,preview}.rs`, `Op::check_frames`, `op::resolve_step`, `Server::new(actions, concurrency)`; an image rascal made resolves by the digest of its PNG; `tests/it/operations.rs`, `serve.rs` and `isolation.rs` drive the operations)
- [x] 10.7 The artifact resource and its preview; reopening after a restart and after the workspace moves; a missed read the `binding` refusal with `port: null` (← 10.6)
- [x] 10.8 Tasks list their Masks in `artifacts.out` (also on `error` and `cancelled`), published into the store through 10.5's import; CLI runs too (← 10.6)
- [x] 10.9 Estimates for operations: allowed at 0, references in the binding by identity, the same `binding` refusal as the call (← 10.6)
- [x] 10.10 `export` and `import` (reserved tools, `rascal export --json`, `rascal import --json` with `--allow-dir`; the compatibility export renamed `rascal compat-export`, no fallback) over the mask bundles and location JSON; refusals for a PNG, `portable: none`, unresolved, a digest mismatch, a refused path (← 10.4, 10.5) → tag `m10.4` after 10.7 to 10.10
- [x] 10.11 Folded into 10.2 (the coordinator's review, 2026-11-01: no stand-in invocation)
- [x] 10.12 `rascal call <action> --arguments <file>`: the MCP arguments, no session, `--allow-dir`, exit codes, refusals as JSON with 64 (← 10.2)
- [x] 10.13 `rascal call --bundle`: the bounded reader, imports, inputs into rascal's replay area, steps in order (another tool's refused), JSON Lines out, stopping at the first step that does not complete; a later step binds to an earlier step's replayed output by port and reports the change; replayed from another directory with the original workspace gone (← 10.12, 10.10) → tag `m10.5`
- [x] 10.14 Line → Rasterize → Feather end to end (`operations_path.rs`), v0.4 fixtures for table-manners `fixtures/rascal/` validated as table-manners validates them, ADR 0021, the 0015, 0020 and 0022 notes, docs (← 10.7 to 10.13)
- [ ] 10.15 The pin to v0.4.0 (owner: squint's fixtures pass, tag and publish) → merged → tag `m10` (← 10.14) (pinned `=0.4.0` from crates.io 2026-11-02, fixtures unchanged; CI, the merge and `m10` remain)
- [ ] Checkpoint M10 → tag `m10`

## M11 Table manners v0.2 (contract C4; PLAN.md M11; pinned `=0.2.0`)
- [x] 11.1 The pin and the shapes: run ids in `run`, `spend.unresolved_usd`, runs entries as run references (done: `fc819f7`; `check-contract.sh` reads a path pin)
- [x] 11.2 The session: UUID v4, `rascal://session`, the estimate without private session fields (done: `8759223`)
- [x] 11.3 Correlation ids: claimed on acceptance, the `client_run_id_reused` refusal, persisted at allocation (done with 11.4: `c1f0d4d`, `rascal_mcp::server::Progress`)
- [x] 11.4 The run in progress `_meta`; cancellation rechecked before allocation (done: `c1f0d4d`)
- [x] 11.5 The runs page: `rascal://runs{?cursor}` (done: `b2ade08`)
- [x] 11.6 Fixtures for table-manners `fixtures/rascal/`, and docs (done: `fixtures/table-manners/`, committed to the table-manners branch as `0aa506c`)
- [x] 11.7 The pin to v0.2.0, tagged and published 2026-11-01 → merged → tag `m11`
- [x] Checkpoint M11 → tag `m11` (2026-11-01)

## M12 Table manners v0.3 (readiness, bound estimates, renders; PLAN.md M12; built by path against table-manners branch `v0.3`, then `=0.3.0`)
- [x] 12.1 The pin and the shapes: v0.3 by path, `estimate` out of `actions`, `needs_env` → credential requirements, the dash render's `kind`, `max_spend_usd` `exclusiveMinimum: 0` on every action, fixtures regenerated (← M11) → tag `m12.1` (done against table-manners `e503a23`; `check-contract.sh` warns on a path pin; Wafflery's three rulings folded into PLAN.md 12.5 and 12.8)
- [x] 12.2 Readiness without configuration: the config state and identity, `describe` and `serve` in every state, `--workspace` optional, an action's readiness its default use (the mask alone unconfigured, healing with the configured fillers otherwise), requirements by local inspection (config, credentials by name, fillers, the segmenter pack, its model files, `env` for non-secret variables such as the models directory); a call that needs a missing or invalid requirement is refused `not_ready` before any run (JSON-RPC `-32602`, no run, no spend, id released) (← 12.1, 12.7) (done against table-manners `e503a23`; a model file is `present` when it is there and not empty, since `bundle.toml` records no sizes; a run leaves out a pack that does not start, so a mask-only call runs; `run` and `heal` skip the check under `--replay`; the model directory table `fixtures/readiness/models-dir.json` is checked by both suites)
- [x] 12.3 `credential`: a provider's 401/403 during a run, never retryable (a missing key is 12.2's `not_ready`); the session's answers make a credential `valid` or `invalid` (← 12.2) (done against table-manners `e503a23`; the answers are learnt from the run's filler call records when it ends; a key unset at the call is `internal`; a 401/403 is a known call at zero, `spend.basis: measured`, not a call not sent)
- [x] 12.4 The cap and the budget apart; `max_spend_usd` over the configured cap is `invalid` (← 12.1; parallel with 12.7, 12.2, 12.3) → tag `m12.2` after 12.7 and 12.2 to 12.4 (done against table-manners `e503a23`; the order is in `docs/contract.md`; `estimate` answers a `max_spend_usd` above the cap with a JSON-RPC error until 12.5 makes it the `invalid_arguments` refusal)
- [x] 12.5 The estimate tool: `{action, arguments}` in, `estimate.json` out, the binding, `effective`, `expires`, the call's own readiness; the binding leaves out `return_images` and `image_blocks`; a mask-only call with no file allowed at 0 under the built-in defaults' identity; an unpriceable call (a filler not configured, `remove_prompt` without its pack) refused for readiness with `worst_case_usd` null; `rascal estimate --json` with `session_id` and `session` null; never a run (← 12.2, 12.3, 12.4) (done against table-manners `e503a23`; one pricing function, `estimate::price`, shared with the limits check; a run checks its arguments as the estimate does, so an argument the action does not take is an `invalid` run; the issued-binding table moves to 12.6, where it is read)
- [x] 12.6 A call bound to its estimate: `_meta["table-manners/estimate"]` at the limits check, `stale_estimate` (← 12.5) → tag `m12.3` (done against table-manners `e503a23`; the issued-binding table, the newest 1024, lands here with its reader; `Actions::call` takes `CallMeta`; a binding is past its `expires` from that instant on)
- [x] 12.7 A claim released when the request gets no run; kept when cancelled before allocation; first in the chain, since 12.2's `not_ready` releases through it (← 12.1) (done against table-manners `e503a23`; the actions double is in `crates/rascal-mcp/tests/it/claims.rs`, in process over a pipe; a run reference sent just before the action returns now always reaches the claim)
- [x] 12.8 Renders a host can embed: `dash --serve` URL-only, stdin EOF and SIGTERM, `--frame-ancestor`; exit 0; `dash --document` (← 12.1; parallel) → tag `m12.4` once 12.8 is in (done against table-manners `e503a23`; the page opens the run read-only through `Workspace::open_read_only`, so no render writes into it)
- [x] 12.9 Fixtures for table-manners `fixtures/rascal/` at v0.3 (with `refusal-not-ready.json`; no missing-credential run), the no-provider-call proof, docs and the ADR 0020 amendment (← 12.2 to 12.8) (done against table-manners `e503a23`; 31 files in `fixtures/table-manners/`, each held to the contract as its `tests/fixtures.rs` holds it, and that test passes on them; `refusal-no-run.json` kept beside `refusal-cancelled-before-allocation.json`; the copy into the contract's branch and rascal's MAPPING column are the coordinator's)
- [x] 12.10 The pin to v0.3.0 (owner: squint's fixtures pass, tag and publish) → merged → tag `m12` (← 12.9) (2026-11-01: `table-manners = "=0.3.0"` from crates.io)
- [x] Checkpoint M12 → tag `m12` (2026-11-01)

## M13 The self-correcting remove (PLAN.md M13; issues #14, #15, #16)
- [ ] 13.0 Merge the walk (rascal, table-manners, Wafflery)
- [x] 13.4a The baseline snapshot (before 13.1 publishes)
- [x] 13.1 The decoder re-export, parity, release `models-2026-11`
- [x] 13.2 The whole-object outline, padding and the retry (`outline.py`, `segment` settings)
- [x] 13.3 `plug` and the recipes; owner's collar check → merge `m13a-mask`, tag `m13.1`
- [ ] 13.4 The candidates and their licences
- [ ] 13.5 The corpus: flat generator, scenes (owner: spend), owner's masks, manifest
- [ ] 13.6 The harness and the round; results page
- [ ] 13.7 The pick (owner); ADR 0018 section, export if needed
- [ ] 13.8 table-manners v0.5 draft: verdict, optional requirements, image out labels
- [ ] 13.9 squint answers with a verdict; real `squint call` integration test
- [x] 13.10 The checker's configuration, readiness, `check`, the reservation; ADR 0023
- [x] 13.11 `heal`, one try at a time (refactor) → tag `m13.2`
- [x] 13.12a One checked try
- [x] 13.12b The located retry
- [x] 13.12c N tries, the selection and the outputs
- [x] 13.13 Replay and cancellation → tag `m13.3`
- [ ] 13.14 Wafflery shows checks → tag `m13.4`
- [ ] 13.15 Version 0.5.0 everywhere at once; publish; pins from crates.io
- [ ] 13.16 Merge and the by-hand check
- [ ] Checkpoint M13 → tag `m13`
