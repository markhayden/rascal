**No—the files currently on disk still lack the promised draft-7 spec changes, and the dependency corrections below should be recorded before M0 begins.**

**Resolution of review 1**

| Finding | Resolution | Assessment |
|---|---|---|
| 1 | Resolves | The bootstrap exception, scanner in 0.1, exact checks, local verification and post-push CI/tag sequence address the original bootstrap problems. |
| 2 | Partly resolves | Ownership is substantially complete, but boundary-schema enforcement, exit-code consistency and two pack verification cases remain incomplete. |
| 3 | Resolves | M5 now orders skeleton → model → bundle, with disconnected acceptance on both platforms. |
| 4 | Partly resolves | The listed corrections land, but 1.6’s replacement scenario still requires sibling task 1.5. |
| 5 | Resolves | Rollbacks use branches, `--no-commit`, dependent reversions and verification before merging. |
| 6 | Partly resolves | Serialization and synchronization have owners, but 4.3’s degenerate-tangent rule misses zero central differences between nonzero segments. |
| 7 | Resolves | Criterion 6 requires both groups, and release requires every applicable criterion or an explicit owner scope change. |
| 8 | Resolves | Readiness gates now precede the real-filler run, model work and M6 corpus use. |
| 9 | Resolves | The six specifically requested acceptance corrections are incorporated. |
| 10 | Resolves | 7.1 includes the Poisson CLI slice, and the broad primitives/geometry task is split. |
| 11 | Partly resolves | Benchmarks and private helpers improve, but the stated public additions are absent from the on-disk spec and non-empty-output refusal remains plan-only. |
| G1 | Resolves | 1.4 owns manifests, envelopes, shared validation, successful bindings and no-publication behavior. |
| G2 | Partly resolves | Schema files and drift checks have owners, but boundary-type rules and native-document validation lack explicit acceptance. |
| G3 | Partly resolves | Exit-code ownership exists, but 1.4 incorrectly assigns `invalid_args` exit 1 under `--strict`. |
| G4 | Partly resolves | Relocation and filler replay are covered, but 3.3 omits the pack-replay acceptance promised by 1.9. |
| G5 | Resolves | 3.3 explicitly owns isolation across filenames, prompts, tool output and registry IDs. |
| G6 | Partly resolves | The named tests cover timeout and other failures, but omit cancellation failing every in-flight call and shutting down the pack. |
| G7 | Resolves | 3.3 owns compatibility export of the fake pack’s native `segment` envelope. |
| G8 | Resolves | 5.1 owns fetch, digest rejection, the parity harness and Ruff verification. |
| G9 | Resolves | 2.7 covers field dispatch, `send_mask` and malformed/non-PNG responses. |
| G10 | Resolves | 4.4 explicitly covers packaged installation and recipes on JPEGs without configuration. |
| G11 | Resolves | 1.9 includes measures and rejects both undocumented entries and stale documented names. |
| G12 | Resolves | 7.2 owns the final three-minute test-execution gate on the owner’s named machine. |
| G13 | Resolves | 1.2 owns canonical serialization, hash encoding, PNG settings and stable identity vectors. |
| G14 | Partly resolves | Decisions and fixtures are assigned, but dropping zero-length segments does not handle every degenerate tangent. |
| G15 | Resolves | 1.7a owns platform synchronization, durability documentation and real-platform checks. |
| G16 | Resolves | Failed experiments leave the release gate open, with versioned refits and new experiment identities. |

**New findings**

All **41** Dependencies lines exactly match the edge list. That syntactic agreement does not make every scenario executable in the permitted order.

**Before M0**

1. **[blocker] Spec baseline: the stated draft-7 changes are not present.**  
   [SPEC.md:3](rascal/SPEC.md:3) still says draft 6; its contents lack the `rect` utility recipe, `inspect --json`, `workspace list`, `ops list/describe` and `tools call` additions claimed by the request and plan. Separately, [1.1](rascal/PLAN.md:290) still introduces non-empty-output refusal, contradicting the plan’s claim that all public behavior is specified.  
   **Change:** Incorporate the intended draft-7 changes and reconcile the version references; explicitly specify the refusal rule or remove that public acceptance requirement.

2. **[important] Dependencies and scenarios: sibling and future-task prerequisites remain.**  
   [1.6](rascal/PLAN.md:403) expects a type error involving `render.image`, but `render` is introduced by parallel task 1.5; without it, validation encounters an unknown op. [6.1](rascal/PLAN.md:974) places matrix rejection cases under its CLI scenario before `lab run` exists. Tasks 6.2–6.3 use the example comparison with `thin`, while `fan-thin-v1` is only introduced in 6.4.  
   **Change:** Add `1.6 ← 1.5` or use an already registered op for the mismatch; keep matrix validation unit tests in 6.1 and demonstrate their CLI failures in 6.2; introduce the thin-arm constants with the matrix in 6.1. Specify `wide_max` in fractional-width units so `wide_max × width = thin(width)`, rather than assigning the pixel-valued `thin` directly. Update the edges and parallel-track text accordingly.

**Fix in the owning task**

3. **[important] 0.4: schema drift does not prove the complete boundary contract.**  
   The [harness](rascal/PLAN.md:228) compares schemas for types already deriving `JsonSchema`; it does not establish that every boundary type participates, rejects unknown fields, carries a version, or produces native documents that validate. These were part of G2.  
   **Change:** Add an explicit boundary-type inventory and reusable acceptance checks for required derives/versioning, unknown-field rejection and native serialized fixtures validating against committed schemas; require subsequent owning tasks to register their types.

4. **[important] 1.4: the invalid-argument scenario contradicts the exit-code contract.**  
   [Task 1.4](rascal/PLAN.md:360) expects a bad parameter to produce exit 1 under `--strict`, while [SPEC.md](rascal/SPEC.md:944) reserves that exit for `not_applicable` and `measured_nothing`; usage errors are 64.  
   **Change:** Require `invalid_args` to map to 64 with and without `--strict`, and test both cases alongside the strict-status cases.

5. **[important] 3.3: two promised pack guarantees still lack acceptance tests.**  
   [1.9](rascal/PLAN.md:504) delegates external-pack replay verification to 3.3, but [3.3’s acceptance](rascal/PLAN.md:766) omits it. Cancellation appears in the description, but the enumerated all-in-flight failure test covers timeout only.  
   **Change:** Add a relocated-run replay test with original inputs and the pack executable unavailable, proving identical outputs without another call; add a concurrent-call cancellation test proving every in-flight call fails and shutdown completes before reuse.

6. **[important] 4.3: tangent handling is incomplete, and the edgeless equality assertion exceeds the algorithm.**  
   [Task 4.3](rascal/PLAN.md:824) drops zero-length segments, but a path that reverses direction can have nonzero adjacent segments and a zero central-difference tangent. Also, the specified untightened quadrilateral strip does not generally equal `rasterize`’s dilated path, notably at endpoint caps.  
   **Change:** Define and fixture a deterministic fallback for zero central differences after resampling. Test edgeless output against the specified untightened extents and quadrilateral construction; require equality with `rasterize` only if matching cap/join semantics are deliberately specified.

**New findings: 1 blocker, 5 important, 0 suggestions. Remaining coverage gaps: 5 — G2, G3, G4, G6 and G14.**
