| Round 7 finding | Resolution | Evidence |
|---|---|---|
| 1. Disconnected bundle loses assets | **Resolved** | [PLAN.md:4186](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4186) assigns verified external assets, mandatory `--models`, and provisioning in both platform jobs. |
| 2. Missing non-graph release assets | **Resolved** | [PLAN.md:4174](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4174) owns Tiny’s licences, sizes and manifest assertion; [4219](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4219) owns tokenizer/configuration assets and offline checks. |
| 3. Prepublication gate cannot install candidate | **Resolved** | [PLAN.md:4225](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4225) adds candidate entries before `get --from`, followed by gates, publication and download verification. |
| 4. Convergence can precede model completion | **Resolved** | [PLAN.md:4364](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4364) requires all four dispositions and freezes the adopted set; [tasks/todo.md:195](/Users/markhayden/go/src/github.com/markhayden/rascal/tasks/todo.md:195) agrees. |
| 5. Lab installation pinning unassigned | **Resolved** | [PLAN.md:4281](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4281) assigns hold-before-identity, forwarding and cleanup; [4297](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4297) adds the deterministic reinstall test. |
| 6. Instance parity and EVF retry coverage | **Partly resolved** | [PLAN.md:4223](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4223) covers the four new models; EVF assertions appear in 14.10 and convergence. Tiny’s [14.2 acceptance](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4139) still lacks the frozen instance suite required for each catalogued model. Assign it before `m14.1`. |
| 7. SAM 3 empty baseline handling | **Resolved** | [PLAN.md:4114](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4114) materializes empty planes and distinguishes empty comparisons from missing results. |

1. **Major — The prescribed positive fixture fails the new parity gate before segmentation.** [PLAN.md:4223](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4223), [SPEC.md:1234](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1234)

   The adopted spec check requires a nonempty instance on `chair-blob.png` with `"chair. table"` at threshold 0.2. A targeted local run using the digest-verified shipped OWLv2 artifacts produced maximum scores **0.01917 for chair** and **0.02425 for table**, leaving zero detections. BiRefNet and HQ-SAM use this same detector, so faithful exports cannot pass that positive-instance requirement. The gate would exclude them for fixture suitability rather than export correctness.

   **Fix:** Amend the spec and plan together to use a verified positive fixture at the prescribed settings, retaining the chair blob as an empty-result parity case if useful. Verify positive and negative reference outcomes before freezing them; retain the numerical parity tolerances.

Verification: document/source review plus the targeted detector run; full gates were not run.

**Verdict: not ready to begin M14.**
