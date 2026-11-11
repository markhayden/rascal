# M14 checkpoint review

Branch `m14-fixes` from `main` at `88af8e6`, 2026-11-10. M14 (choosable segmenters, PLAN.md
14.0–14.14) is merged on `main`: track P at `m14.1`, the four models at `m14.2-birefnet`,
`m14.2-hqsam`, `m14.2-sam3` and `m14.2-evfsam2`, and the model choice at `m14.3`.

- **Contract:** table-manners `0.6.0`, published 2026-11-10. rascal, squint and Wafflery all pin
  `=0.6.0` from crates.io.
- **Base:** `m13` was never tagged (M13's 13.16 and its checkpoint are still open in
  `tasks/todo.md`). This review's base is therefore `661bd68`, `main`'s last M13 merge before
  M14.
- **Passes:** two ran over `git diff 661bd68..88af8e6` and Wafflery's `8d4ae89..8290b6f`.
  - A five-axis code review (correctness, readability, architecture, security, performance).
  - A test-coverage pass mapping each acceptance test in PLAN.md 14.2–14.11 to a test.
- **Fixes:** made on this branch, test first, one commit each. The rest are recorded below.

## Model dispositions

All four track-E models are merged. None was excluded.

| Model | Corpus gate (20 images, IoU ≥ 0.999) | Release | macOS download | Linux x86_64 gate |
|---|---|---|---|---|
| `birefnet` | 20/20, all 1.000000 | `models-2026-11-birefnet` | passed, `62824792…` | passed |
| `hqsam` | 20/20, lowest 0.999996 (`thin-02`) | `models-2026-11-hqsam` | passed, `6be1130e…` | passed |
| `sam3` | 20/20, lowest 0.999987 (`hole-01`) | `models-2026-11-sam3` | passed, `593e11a3…` | passed |
| `evfsam2` | 20/20, lowest 0.999989 (`parts-03`) | `models-2026-11-evfsam2` | passed, `7931bdfe…` | passed |

- **Linux gate:** `scripts/model-gate.sh <model>` installs the model from its release, then runs
  its tests with the network off, as linux/amd64 under emulation.
  - It ran out of memory (exit 137) with Docker's VM at 8 GB. All four passed at 32 GB, each
    with the same installation identity as on macOS.
  - The script now warns below 16 GB.
- **The adopted set** is `tiny`, `birefnet`, `hqsam`, `sam3` and `evfsam2`. Every one lists
  `macos-arm64` and `linux-x86_64`.

## Code review: fixed here

1. **A re-run of `rascal tools setup` was refused once any model was installed** (`bc650f6`).
   - Cause: `get` wrote its per-model lock to `<models>/.install-<id>.lock`. Setup's
     models-layout check treats that as a stray file.
   - Fix: the lock is now `<models>/<id>/.lock`, as SPEC.md says.
   - Test: `test_the_models_directory_holds_only_the_layout`.
   - Also: SPEC.md's staging name now includes the pid the code puts there.
2. **A call's `set` could pin `segment.installation`** (`be4a329`).
   - Cause: a pinned installation skipped the run's hold. Its name was also joined into a path
     without being checked.
   - Fix:
     - A pinned name must be a plain name, naming an installation of the model on disk.
     - `serve` and `rascal call` refuse `segment.installation` in `set`.
     - segment-lab's `--set` from the command line, which holds its own installation, still
       works.
   - Test: `remove_prompt::a_pinned_installation_must_be_a_real_one_and_a_call_cannot_pin_one`.
3. **Wafflery showed no install progress and still offered Get** (Wafflery `3be0f08`).
   - Cause: the page's first refresh after Get could land before the tool had marked the model
     installing.
   - Fix: `shownState` treats a Get the page just started as installing, and the list keeps
     polling, until any of these happens:
     - the tool reports a state other than not installed;
     - the tool records a new outcome;
     - 30 s pass.
   - Also re-indented a misindented prop (`onOpenSetup`) in `Workspace.tsx`.
4. **An install racing another came back `failed`, not `busy`** (`e5cc83d`).
   - Cause: the per-model lock's refusal didn't contain the word rascal looks for in the log.
   - Fix: every `Busy` refusal now says `busy:`.
5. **BiRefNet's licence read "MIT"** (`d7c4de1`). Its install also carries OWLv2, so it now reads
   "MIT and Apache-2.0", in the catalogue and in Wafflery's confirm dialog.
6. **Tiny's catalogued size was 749 MB; its files are 753 MB** (`8871934`).

## Test pass: fixed here

1. **Holds on failure and cancel** (PLAN.md 14.10's acceptance), in `8bac0f3`.
   - The new test found a real bug: a run cancelled by SIGINT or SIGTERM left its hold behind.
     `watch_signals` exits without unwinding, so the hold's drop never ran.
   - Fix: holds now sit in a registry that the signal path releases before it exits.
   - The fake pack's `models` mode gains a prompt `die` (exits mid-call) and a prompt `slow`
     (waits until cancelled).
   - Test: `remove_prompt::a_run_that_fails_or_is_cancelled_releases_its_hold`.
2. **EVF-SAM2's retry receiving neither `threshold` nor `pad`.**
   - `checked_retry_real_model` now holds every `segment` call, the retry's included, to the
     model's catalogue `params`.
   - The convergence gate runs it on the real EVF-SAM2.

## Recorded for later

| # | Finding | Where |
|---|---|---|
| R1 | SAM 3 resizes every candidate above the threshold to full size before `order` cuts to `max_instances`. A low threshold on a large panorama holds about 200 full-size planes at once. Cut first. | `tools/python/src/rascal_tools/models/sam3.py` |
| R2 | A model never shows as out of date. Nothing compares `install.json`'s catalogue revision with the pack's. `adapter_version` is the pack's version (0.1.0), so lab cell identities do not change when an adapter does. | `installs.py`, `segmenters.rs` `complete`, `tools/python/pyproject.toml` |
| R3 | Uninstall signals the pid in the status file. If the installer crashed and the pid was reused, that pid could belong to an unrelated process. A second install also truncates the shared log. | `crates/rascal-mask/src/install.rs` |
| R4 | `[segment] model` naming an uncatalogued model. `effective()` takes it as is, `default_model()` silently falls back, and readiness can say ready while the run refuses. Readiness should report it. | `crates/rascal-mask/src/segmenters.rs` |
| R5 | A hold is judged live only by its pid being alive, so a recycled pid keeps an installation held. | `installs.py` `held` |
| R6 | Untested: SIGINT/SIGTERM during `rascal-tools get`; a reinstall between a run's first `segment` and its retry; the estimate binding and lab identity following a reinstall; `describe` making no network call; a `remove` racing a `get`; external data through a symlinked directory; a corrupt `install.json` reported as "gone". | PLAN.md 14.4, 14.10, 14.11 |
| R7 | `claims::a_request_cancelled_while_queued_keeps_its_id_with_no_run` hung to nextest's 180 s timeout once in CI (run 42860089235, on `55d0236`, which touched only an ignored test). It passes locally in under 0.1 s, and the same test passed on `8bac0f3`. A rare hang in the queued-cancel path, or the runner. | `crates/rascal-mcp/tests/it/claims.rs` |

## The convergence gate

`checked_retry_real_model` heals the chair fixture with each real model and the scripted
checker, which fails the first try, so every model runs a `segment` retry. Each model was
installed from its release into a fresh models directory with `rascal-tools get`.

- **macOS arm64:** `RASCAL_TEST_MODEL=<id> cargo nextest run -p rascal-mask --features
  rascal-mask/testkit --run-ignored only -E 'test(checked_retry_real_model)'`, with
  `RASCAL_TEST_NO_THRESHOLD=1` for `evfsam2`. All five passed: `tiny`, `birefnet`, `hqsam`,
  `sam3`, `evfsam2`.
  - The first run failed on every model at the new catalogue check (`55d0236`). It read
    `segmenters.toml` as a TOML value, not a document. Fixed in `e665961`. Every assertion
    before it had passed.
- **Linux x86_64:** `scripts/model-gate.sh --heal <id>` for each model, with Docker's VM at
  32 GB and the network off. All five passed, each installed with the same identity as on macOS:
  `tiny` (`61238e7a…`), `birefnet` (`62824792…`), `hqsam` (`6be1130e…`), `sam3` (`593e11a3…`)
  and `evfsam2` (`7931bdfe…`). Tiny's first run was built before `e665961` and failed the same
  way. It passed when rerun.

## The restore drill

Against `661bd68` (M13 has no tag), following PLAN.md's "Restoring a working install". Every
step ran in isolation under `../m14-restore/m13/`.

1. `git worktree add --detach ../m14-restore/m13/rascal 661bd68`. It pins table-manners
   `=0.5.0` from crates.io, so no sibling checkout is needed.
2. `cargo install --path crates/rascal-mask --locked --root ../m14-restore/m13` (29 s).
3. `uv sync --frozen` in that worktree's `tools/python`, then `rascal-tools fetch --cache
   ../m14-restore/m13/models` (M13's own command, three files).
4. A `rascal.toml` naming that worktree's launcher with `RASCAL_TOOLS_MODELS` in the pack's env,
   and no `[segment]` table.

Then `rascal workspace import` of `fixtures/images/chair-blob.png`, and `rascal tools call ...
rascal-tools segment --prompt chair --threshold 0.01`. It returned two planes and two regions,
committed, in 4 s. The live checkouts and models directory were not touched.

## The owner's check

In isolation under `~/rascal-manual/m14/`: rascal (`4eee8d2`), Wafflery (`c8ef141`) and squint
(`f288738`) built from their merged mains, an empty models directory, a fresh Wafflery library,
and a `rascal.toml` with the pack, LaMa and squint's `source_fidelity` as the checker.

**First attempt.** Get on the four new models showed *Installing*, and the bars barely moved for
several minutes. Three bugs, fixed on `m14-handcheck`, test first, one commit each:

1. **The installs went to the wrong directory** (`1e1cade`).
   - Cause: Wafflery passes a tool only the variables its description names, learned from a
     probe run with none set. With Tiny already in `~/.cache/rascal-tools` (the pack's
     fallback), readiness omitted `RASCAL_TOOLS_MODELS`, so Wafflery never passed it.
   - Fix: a variable the pack is configured to receive is always listed; unset, it is
     `missing` and `optional`.
   - The partial downloads were cancelled, which deleted their staging. The empty model
     directories and status files they left were removed, so `~/.cache/rascal-tools` holds
     Tiny only, as before. Its stale `.install-tiny.lock` (pre-`bc650f6` layout) was removed
     too.
2. **Progress sat at 0** (`88d7591`). The status file a host reads was written only between
   files. SAM 3's and EVF-SAM2's first files take many minutes. It is now written at every 1%.
3. **A hold could be left by a signal** (`edb852b`). The full gate's hold test failed once under
   load: a SIGTERM after the pack wrote the hold, but before it answered, found nothing
   registered. Holds are now registered before they are written and released by run.

**Second attempt.** PENDING
