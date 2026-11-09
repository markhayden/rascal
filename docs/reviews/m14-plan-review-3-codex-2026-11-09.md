| Round-2 open item | Resolution | Why |
|---|---|---|
| R1 #4 — `fetch` removal and release CI | **Resolved** | [PLAN.md:4187](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4187) puts removal, provisioning, bundle changes and readiness into one cutover commit. |
| R1 #5 — Dependencies and optional exits | **Partly** | The conditional `m14.2` checkpoint is fixed, but dropping 14.6 also drops creation of `model-gate.sh`, which 14.7 and 14.9 require. |
| R1 #9 — Parity-resource packaging | **Resolved** | [PLAN.md:4250](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4250) places both inputs and reference PNGs under `fixtures/`, assigns wheel/sdist mappings and verifies packaged execution. |
| R1 #11 — Real-adapter checker retries | **Partly** | [PLAN.md:4318](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4318) adds retries only for installed models; R can finish before S lands, and no convergence gate requires rerunning this coverage for every adopted model. |
| R2 #1 — Storage cutover before provisioning | **Resolved** | 14.3 now adds inactive primitives; 14.4 switches consumers together and requires fresh-install readiness and segmentation. |
| R2 #2 — Comparator through shipped adapters | **Partly** | The driver is assigned, but the promised frozen SAM 3 cores do not exist: [check.py:149](/Users/markhayden/go/src/github.com/markhayden/rascal-lab/segment-lab/onnxcheck/sam3/check.py:149) approximates a core by swapping union pixels and saves only scores in `check.json`; actual reference-core generation remains unassigned. |
| R2 #3 — Linux verification skips Python | **Resolved** | [PLAN.md:4239](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4239) explicitly provisions Linux amd64, Python and models, disables networking, and fails missing prerequisites or skipped required coverage. |

New finding:

1. **Major — The Linux gate provisions one model but selects every model’s tests.** [PLAN.md:4239](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4239)

   `model-gate.sh birefnet` installs BiRefNet, then runs unrestricted `pytest -m segmenter` with missing artifacts fatal. Existing selected tests require Tiny’s SAM 2 encoder and decoder ([test_sam2.py:39](/Users/markhayden/go/src/github.com/markhayden/rascal/tools/python/tests/test_sam2.py:39)), which BiRefNet does not install. A clean gate therefore fails on unrelated prerequisites; SAM 3 has the same problem.

   **Fix:** Assign explicit model-based test selection, pass the requested model into pytest, and fail if its required cases are missing or skipped. Alternatively, provision every model selected by the suite before disconnecting.

Static review; no gates run.

**Verdict: not ready to begin M14.**
