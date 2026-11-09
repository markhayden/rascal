| Round 4 open item | Resolution | Evidence |
|---|---|---|
| R1 #11 — Retry coverage after all adapters land | **Resolved** | [PLAN.md:4408](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4408) requires convergence coverage for every adopted model after merging S, on both supported platforms. |
| R4 #1 — Final gate requires unconfirmed HQ-SAM | **Resolved** | [PLAN.md:4202](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4202) defines the adopted set; [PLAN.md:4405](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4405) keeps HQ-SAM in refusal coverage. |
| R4 #2 — Linux retry gate lacks rascal and host-test invocation | **Resolved** | [PLAN.md:4336](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4336) assigns the Linux build, checker, offline filler and explicit host test, which requires second-try crop evidence. |

1. **Major — The bundle dispatch command tests the wrong branch and cannot request required models.** [PLAN.md:4223](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4223)

   `gh workflow run bundles.yml` dispatches the default branch, but this gate precedes `m14.1`, while the cutover remains on unmerged `m14p-pack`. It therefore exercises the old workflow and code. Additionally, the existing [workflow:14](/Users/markhayden/go/src/github.com/markhayden/rascal/.github/workflows/bundles.yml:14) exposes no dispatch inputs, and [workflow:56](/Users/markhayden/go/src/github.com/markhayden/rascal/.github/workflows/bundles.yml:56) enables required models only for `v*` tags. “With required models” has no executable mechanism for this manual gate.

   **Fix:** Assign a required-model dispatch input—or make manual runs always require models—in 14.4’s cutover. Specify `gh workflow run bundles.yml --ref m14p-pack` with that input, and require both platform jobs to pass at the cutover’s recorded commit SHA before tagging `m14.1`.

No new minor findings. Static review; no gates run.

**Verdict: not ready to begin M14.**
