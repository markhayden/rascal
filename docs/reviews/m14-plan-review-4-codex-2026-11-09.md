| Round 3 open item | Resolution | Evidence |
|---|---|---|
| R1 #5 — Optional exit drops the shared model gate | **Resolved** | [PLAN.md:4202](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4202) assigns `model-gate.sh` to mandatory task 14.4. |
| R1 #11 — Retry coverage can finish before SAM 3 lands | **Partly** | [PLAN.md:4399](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4399) adds the convergence gate after merging S. Its scope and execution prerequisites still have the new defects below. |
| R2 #2 — Actual SAM 3 reference-core generation unassigned | **Resolved** | [PLAN.md:4117](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4117) assigns ONNX cells producing full recipe cores, frozen references and failing comparisons. |
| R3 #1 — Single-model installation selects unrelated tests | **Resolved** | [PLAN.md:4046](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4046) introduces per-model markers; the Linux command selects the requested model explicitly. |

New findings only:

1. **Major — The final gate requires installing an explicitly un-installable catalogue entry.** [PLAN.md:4396](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4396), [PLAN.md:4400](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4400)

   The owner must get and run “each catalogued model,” and convergence requires “all catalogued ids.” Task 14.2 explicitly catalogues `hqsam` as `unconfirmed` and requires installation refusal. Rollback rule 3 removes optional BiRefNet/SAM 3 entries, not HQ-SAM. The prescribed checkpoint therefore cannot pass.

   **Fix:** Define the required inference set as adopted, installable models supported on the target platform. Provision and test every member; keep HQ-SAM in separate refusal/UI coverage.

2. **Major — The Linux retry gate lacks the rascal executable and host-test invocation.** [PLAN.md:4202](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4202), [PLAN.md:4330](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4330)

   `model-gate.sh` provisions Python and the pack, then runs pack pytest cases. Tasks 14.9 and 14.13 also rely on it for a checked `heal remove-prompt`, but no task extends that container with a Linux rascal build or connects the host retry test to its pytest selection. Pack tests can pass without exercising the promised retry; adding a subprocess test alone encounters a missing executable.

   **Fix:** Assign 14.9 an extension that builds the branch’s Linux amd64 rascal binary, provisions the scripted checker and offline filler, and explicitly invokes the checked-retry case for the selected model. Require evidence that the retry executed.

Static review; no gates run.

**Verdict: not ready to begin M14.**
