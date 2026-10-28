**Yes—M0 can begin against draft 3; the remaining issues should be corrected before implementing their owning tasks.**

**Resolution of review 2**

| Finding | Resolution | Assessment |
|---|---|---|
| 1 | Resolves | Draft 7 and the promised public additions, including the output-directory refusal rule, are now on disk. |
| 2 | Partly resolves | The dependency, matrix-test ordering and thin-arm introduction are corrected, but 6.4 still says `wide_max = thin`, and the new spec value creates inverted clamp bounds. |
| 3 | Resolves | Task 0.4 now assigns the boundary inventory, trait/version checks, unknown-field enforcement and schema-validating native fixtures. |
| 4 | Resolves | Task 1.4 explicitly tests `invalid_args` as exit 64 with and without `--strict`. |
| 5 | Partly resolves | Both requested tests are present, but the healed-run replay test can precede the implementation of `heal` under the permitted parallel schedule. |
| 6 | Resolves | The tangent fallback, reversing-path fixture, round caps/joins and explicit untightened equality contract address the finding. |
| G2 | Resolves | Schema ownership and the reusable boundary-contract checks are now explicit. |
| G3 | Resolves | The invalid-argument exit mapping now agrees with the spec. |
| G4 | Partly resolves | Pack replay has acceptance coverage, but its prerequisite ordering still needs correction. |
| G6 | Resolves | Cancellation explicitly fails every in-flight call and completes shutdown before reuse. |
| G14 | Resolves | Tangent, endpoint, tie-break and cap/join decisions have named fixtures and ownership. |

**New findings**

All **42 Dependencies lines exactly match the edge list**. Semantic prerequisites and contradictory contracts remain below.

**Before M0**

None. These issues do not prevent scaffolding, checks or the schema harness.

**Fix in the owning task**

1. **[important] 1.1, output-producing commands and 6.2: the global `--out` rule contradicts file outputs and lab resume.**  
   [SPEC.md:984](rascal/SPEC.md:984) requires every `--out` to name a new or empty directory. Consequently, the plan’s `render --out t.png`, `put`, `paste`, compatibility export and `dash --out run/dash.html` scenarios cannot have their stated behavior. A second `lab run --out lab/demo/` also encounters the newly prohibited nonempty directory instead of resuming.

   **Change:** Before 1.1 establishes shared output handling, distinguish fresh run directories, single-file destinations and resumable lab directories in the spec. Define file replacement behavior and explicitly exempt validated lab resume/redo from fresh-directory refusal; test each category.

2. **[important] 1.1 and 1.10: the walking slice executes a different shipped `rect` recipe from the spec.**  
   [Task 1.1](rascal/PLAN.md:277) requires `rect → rasterize`, `--bbox` and two journal steps; [the spec](rascal/SPEC.md:653) defines an input `Location` followed by `rasterize`. Task 1.10 then fixes its page expectation at two steps. These cannot all describe the literal shipped recipe.

   **Change:** Choose one contract before 1.1: specify the two-step recipe and `--bbox` binding, or make the walking slice consume `--location` and expect one recipe step. Update the diagnostics fixture consistently.

3. **[important] 1.4, 1.7a–b and 1.10: Diagnostics lacks a consistent durable source for every displayed invocation.**  
   [Diagnostics](rascal/SPEC.md:899) says every op writes a record containing duration, and the page reads those records to reproduce journal steps. The [commit-record contract](rascal/SPEC.md:704) does not include duration or explicit invocation ordering, while `not_applicable` must write nothing and 1.4 verifies an unchanged workspace/output directory. The happy-path two-step page test does not resolve this contradiction.

   **Change:** Define how durable invocation observations relate to artifact commits, including repeated calls, unsuccessful/no-output steps and external calls. Reconcile observation recording with “writes nothing”; assign the necessary fields and recovery behavior to 1.7a–b before 1.10 consumes them. Test page reconstruction after repair and with repeated and `not_applicable` steps.

4. **[important] 1.10 and later diagnostics owners: controls and call tracing remain unassigned, and the configured-key test is premature.**  
   [Task 1.10](rascal/PLAN.md:531) covers the page, panel registry and op spans. It does not explicitly assign `--no-dash`, page generation on request from `serve`, or filler/pack spans with request size and status. Its configured-key acceptance precedes `[fillers]` and `key_env`, introduced in 2.7. Checking for external URLs also does not prove that every artifact is embedded.

   **Change:** Keep op-span fields, default-off behavior and embedded-artifact checks in 1.10; assign configured-key and filler-span tests to 2.7–2.8, pack-span tests to 3.3, `serve` diagnostics to 3.2, and `--no-dash` to 2.8/6.2. Include stderr-only tracing during MCP operation. The panel registry and its M2 extensions are otherwise ordered correctly through the M1 barrier.

5. **[important] 3.3: the new healed-pack replay acceptance requires an undeclared prerequisite.**  
   [Task 3.3](rascal/PLAN.md:797) now requires a relocated healed-with-`segment` run, but its dependencies and parallel-track instructions let it start after 2.7; `heal` arrives in 2.8. Using the shipped prompt-driven `remove` path would additionally require later work.

   **Change:** Add `3.3 ← 2.8` and update the edge list and parallel-track text. Specify a test-only recipe/binding using the fake pack and already available ops, or use a non-healed pack run for replay and retain the existing dependencies.

6. **[important] 2.1, 6.1 and 6.4: `fan-thin-v1` has an invalid clamp interval.**  
   [The new constant set](rascal/SPEC.md:460) sets `wide_max = 0.0`, but [the algorithm](rascal/SPEC.md:493) clamps between positive `thin` and `wide_max × w = 0`. That interval cannot enforce the promised fixed width. [Task 6.4](rascal/PLAN.md:1065) also retains the superseded pixel-valued `wide_max = thin`.

   **Change:** Before implementing 2.1, define an effective upper width such as `max(thin, wide_max × w)` and use it consistently in interpolation and clamping. Test the zero-width-fraction case across image widths in 6.1, and remove 6.4’s conflicting definition.

**New findings: 0 blockers, 6 important, 0 suggestions. Remaining coverage gaps: 3 — G4, the durable diagnostics source, and diagnostics controls/call tracing.**
