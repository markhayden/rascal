Static review against the revised documents and named implementation sources; no gates were run. No new spec findings.

Plan round 1 resolutions:

| # | Finding | Resolution | Why |
|---|---|---|---|
| 1 | Missing squint adoption | **Resolved** | 14.12 includes squint’s pin, generated fixtures, conformance and coordinated publication. |
| 2 | Contract-dependent work contaminates pack checkpoints | **Resolved** | P, S and R now have separate branches; the fallback correctly promises the pack CLI. |
| 3 | Premature BiRefNet artifact activation | **Resolved** | 14.5 publishes without activating the entry; 14.6 follows catalogue-aware readiness and bundle selection. |
| 4 | Release CI calls removed `fetch` | **Partly** | Migration is assigned, but 14.4’s installer/CI slices contradict its requirement to remove `fetch` and update CI in the same commit. |
| 5 | Dependencies and optional exits | **Partly** | Lifecycle primitives are assigned, but dropping 14.6 still leaves `m14.2` defined after that task while S and R require that tag. |
| 6 | Rollback cannot restore external state | **Resolved** | An isolated restore installs matching source, models and configuration, with an explicit `m13` restore drill. |
| 7 | Admission-versus-deletion race | **Resolved** | Hold acquisition and deletion share `.lock`, with a barrier-controlled race test and terminal cleanup. |
| 8 | Wafflery credentials and converter provisioning | **Resolved** | Tasks explicitly own installable requirements, Keychain forwarding, session restart and deterministic `uv` discovery. |
| 9 | Parity images conflict with commit hook | **Partly** | Input images move under `fixtures/`; the reference planes, also PNGs, still lack an explicit allowed source location and packaging mapping. |
| 10 | Unusable verification instructions | **Resolved** | 14.1 creates the comparator and frozen baseline; verification now supplies bundle arguments and required-model modes. |
| 11 | Weakened BiRefNet gate and missing runtime cases | **Partly** | Parity now fails decisively and query cases are included, but no task requires an offline real-adapter checker retry for **each** adopted model. |
| 12 | Unassigned spec requirements | **Resolved** | HQ-SAM refusal, step-model prohibition, unloading, resource checks, identity changes and UI readiness transitions have owners and acceptance. |
| 13 | Oversized tasks lack slices | **Resolved** | Resolution/evidence/retries, describe/supervision/CLI and backend/setup/form work now have named slices. |

Spec round 4 open-item resolutions:

| Open item | Resolution | Why |
|---|---|---|
| R3 #1 — Installation changes during a run | **Resolved** | Admission acquires the hold atomically, and lab cells now pin before computing their identity. |
| R4 #1 — Hold acquisition races deletion | **Resolved** | Both operations use the same cross-process lock; the deterministic race case is specified. |
| R4 #2 — Lab output can use the wrong installation key | **Resolved** | The held installation determines both the cell key and every pack call, with replacement-between-key-and-call coverage. |
| R4 #3 — Invalid existing generation prevents repair | **Resolved** | Publication explicitly replaces an invalid unheld directory and refuses `busy` while it is held. |

New findings only:

1. **Major — The storage cutover precedes its provisioning command.** [PLAN.md:4148](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4148)

   14.3 switches `cache.py` to installation directories and explicitly stops reading the flat layout. The replacement installer arrives in 14.4. At the intervening pushed commit, `fetch` and release CI still provision flat files, which the pack can no longer read. Separately, 14.2 changes readiness’s artifact selection, but 14.3 does not assign its corresponding layout change: today [readiness.rs:450](/Users/markhayden/go/src/github.com/markhayden/rascal/crates/rascal-mask/src/readiness.rs:450) checks `<models>/<filename>`.

   **Fix:** Introduce inactive installation primitives in 14.3, then switch runtime lookup, readiness, provisioning and bundle/CI consumers together in 14.4. Require a fresh Tiny installation to pass both `describe` readiness and segmentation at that commit.

2. **Major — The new corpus comparator lacks a route through the shipped non-Tiny adapters.** [PLAN.md:4103](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4103), [PLAN.md:4215](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4215)

   14.1 freezes `birefnet` and `sam3-own`, but those are the lab’s PyTorch bridge cells, not its recorded ONNX outputs. The existing round runner selects a fixed cell and supplies no model argument ([runner.py:95](/Users/markhayden/go/src/github.com/markhayden/rascal-lab/segment-lab/src/segment_lab/runner.py:95)). Neither 14.1 nor the adapter tasks assigns new cells or a driver that exercises the shipped adapters. Re-running those cells can therefore compare the lab implementation against itself. Host model resolution in 14.9 also arrives after these independent pack gates.

   **Fix:** Assign a corpus driver that explicitly invokes the branch’s pack with the selected model and installation before 14.6/14.7 acceptance. Freeze the actual ONNX references separately from PyTorch export references, and record the invoked pack/model plus complete corpus coverage in the comparison result.

3. **Major — The specified Linux real-model verification skips Python entirely.** [PLAN.md:4218](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4218)

   14.6 names `just ci-local` as its Linux required-model run. Its [Dockerfile](/Users/markhayden/go/src/github.com/markhayden/rascal/scripts/ci/Dockerfile:1) installs neither `uv` nor the pack environment; [check-python.sh](/Users/markhayden/go/src/github.com/markhayden/rascal/scripts/check-python.sh:8) consequently exits successfully after skipping Python. The container invocation also passes neither the required-model flag nor installed models, and uses the host architecture. On the owner’s Mac this establishes neither Linux x86_64 adapter execution nor offline model coverage.

   **Fix:** Assign a dedicated Linux x86_64 model gate with a provisioned Python environment and model installations, then disable networking and run `RASCAL_TOOLS_REQUIRE_MODELS=1 … pytest -m segmenter`. Make missing prerequisites and skipped required coverage fail; require this gate for every adapter claiming that platform.

**Spec: ready to build; decision 15 remains the explicit SAM 3 prerequisite.**  
**Plan: not ready to begin M14.**
