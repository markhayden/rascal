**No—M0 should wait until the bootstrap, contract ownership, and dependency corrections below are incorporated into the plan.**

The edge list matches every task’s Dependencies line. Every catalogue op, rendering profile and shipped recipe has a named task. Task 1.1 is buildable from 0.1 and 0.4 because it owns its minimal implementations; the unresolved issue is who completes several of those contracts.

**Before M0**

1. **[blocker] Commit strategy, 0.1–0.2: the initial gates cannot run in the stated order.**  
   `just check` must include the clean-room scanner before every milestone commit, but 0.1 precedes its implementation in 0.2. M0 also requires green CI before merging, while pushes occur only after checkpoint tagging. There is no local CI equivalent. M0’s branch has no preceding checkpoint tag.  
   **Change:** Define the documentation-only bootstrap exception and initial branch point; implement the scanner prerequisite in 0.1 or explicitly stage its activation in 0.2. Provide a pre-merge CI procedure consistent with the push policy, including both platforms. Specify the exact two feature configurations and checks, and validate the actual merge result before tagging. [Commit strategy](rascal/PLAN.md:87)

2. **[blocker] M1 onward: substantial contracts lack complete ownership and verification.**  
   “A later task replaces a stub” does not assign the full Op, manifest or native-envelope implementation. The following are the missing or incomplete ownership groups; add each to an explicit task’s acceptance and verification, with one accountable owner per item.

   | Gap | Contract or verification missing | Concrete assignment |
   |---|---|---|
   | G1 | Full manifests, native envelopes, shared type/frame validation, successful-binding requirements, and `not_applicable` publishing nothing | Add an M1 contract-completion task before consumers; test required fields and rejection paths. |
   | G2 | Native `envelope-v1`, `journal-v1`, `assignment-v1`, `picks-v1`; explicit Plan schema coverage; generated-schema drift checks and boundary-type rules | Assign envelope to the contract task, journal to 1.6b, Plan to 1.7, assignment/picks to 6.3; 0.4 owns the reusable drift harness. |
   | G3 | CLI exit codes and `--strict` | 1.1 owns dispatch/error mapping; exercise exits 0, 1, 3 and 64. |
   | G4 | Self-contained runs containing all referenced inputs; replay reusing recorded external outputs | 1.8 owns relocation/replay acceptance, including replay with original inputs and external executables unavailable. |
   | G5 | Isolation tests for filenames, prompts and tool output; IDs unable to escape the registry | 3.3 owns the complete isolation suite, covering the already-built recipe, workspace and filler paths. |
   | G6 | MCP tests for late responses, process death, serial/concurrent AND-rule behavior, and cancellation failing every in-flight call | Enumerate these in 3.3’s acceptance instead of relying on “switchable misbehaviours.” |
   | G7 | Compatibility export of a real native `segment` envelope | 3.3 adds this using its fake pack; 1.8 cannot prove it before that producer exists. |
   | G8 | Explicit Python `fetch` command, branch-B parity harness, and executable Ruff verification | 5.1 owns the harness/lint gates; the reordered model task owns fetch and digest failures. |
   | G9 | Filler field dispatch for `image_input`, `send_mask`, and non-PNG rejection | 2.7 tests the request-field matrix and malformed responses. |
   | G10 | Installed recipes running on JPEGs without configuration and producing all required outputs | 4.4 makes these literal acceptance cases, using packaged crates without relying on unpublished registry dependencies. |
   | G11 | Documentation coverage includes **measures** and rejects undocumented registry entries **and stale extra names** | Extend 1.8’s coverage checker; later tasks extend the registry, not duplicate ownership. |
   | G12 | Final offline suite under three minutes on the owner’s Apple silicon machine, excluding compilation | 7.2 owns a recorded final timing run; M1’s one-minute checkpoint does not prove this. |

   [Walking slice and subsequent tasks](rascal/PLAN.md:246)

3. **[blocker] 5.2–5.3: branch-A provisioning is ordered before its contents exist.**  
   5.2 builds the bundle and performs “artifact fetch for branch A”; 5.3 subsequently creates the export, segment implementation and parity fixtures. The spec requires model bytes inside the local bundle and a disconnected acceptance call to `segment`. Setup cannot repair an incomplete bundle by fetching models.  
   **Change:** Order pack skeleton → model export/tool/parity → final bundle/setup. Make the final bundle task depend on the model task under branch A, include `MODEL.md`, artifact bytes and `artifacts.toml`, and test both tools with network disabled and empty caches on both platforms. Keep development fetching separate from installation. [Tasks 5.2–5.3](rascal/PLAN.md:795)

4. **[important] Dependency and parallel-track claims omit real prerequisites.**  
   The lists agree syntactically, but:

   - 1.5’s expected type-mismatch scenario uses `cut`, introduced in 1.7.
   - 1.6b invokes `inspect --replay`, introduced in its dependent 1.8.
   - 2.5 tests the re-encoding fake filler introduced in parallel task 2.7.
   - 3.2’s scenario requires M2’s `fan` and `profile`, despite permitting M3 to run alongside M2.
   - 3.3’s credential filtering needs filler configuration introduced in 2.7.

   **Change:** Replace early scenarios with available operations, explicitly move minimal functionality earlier, or add the missing edges and narrow parallelism. Explicitly assign 1.3’s initial `inspect --json` implementation. For concurrent M3, specify branching from `m1`, integration of M2 before final validation, and testing that combined tree; merely delaying `m3` tagging does not establish correctness. [Dependencies and parallel tracks](rascal/PLAN.md:55)

5. **[important] Rollback can create broken commits on `main`.**  
   Plain `git revert` commits immediately, contradicting verification “before committing.” Reverting an older merged milestone while retaining dependent milestones can leave `main` non-working.  
   **Change:** Prepare rollback on a branch, apply task or milestone reversions with `--no-commit` in reverse dependency order, validate the combined result, then review and merge it atomically. Retain rescue references and immutable checkpoint tags. [Rollback](rascal/PLAN.md:121)

6. **[important] Three decisions explicitly delegated by the spec reviews remain insufficiently assigned.**  
   **Change:** Add these named deliverables and acceptance cases:

   - **G13 — 1.2:** canonical serialization, hash encoding and PNG encoder settings; stable vectors covering ordering and reference substitution.
   - **G14 — 4.3:** final tie-break after midpoint/width ties, endpoint sampling, and degenerate tangents; deterministic fixtures.
   - **G15 — 1.6a:** platform-specific synchronization mechanisms and supported durability guarantees; platform checks supplementing the simulated filesystem.

   MCP transport is properly assigned to 3.1. Window rounding is assigned to 2.4, but its acceptance should explicitly cover fractional context and tied placements. A risk-table reference to “a task-level decision” is insufficient ownership for synchronization. [Risks](rascal/PLAN.md:970)

7. **[important] 6.4 and M7: recording failure can still satisfy the release checklist.**  
   **G16:** 6.4 accepts criterion 6 being marked “met or not”; M7 requires criteria merely marked with outcomes. The refit trigger checks sky preference but ignores structured-group regression.  
   **Change:** Gate release on all applicable success criteria passing. For criterion 6, require both sky `preferred` and structured `not_worse`; otherwise record the failed experiment and stop that gate pending further work or an approved scope change. Refitting must use a new constant-set version and the required new lab/protocol identity. [Task 6.4](rascal/PLAN.md:899)

8. **[important] Owner-readiness dates are later than the work requires.**  
   The spec requires the corpus decision before M6, while the plan defers it to 6.4. M2 requires a real local filler and panorama, neither listed as a readiness dependency. Branch-A redistribution/export readiness is needed before the final bundle, not just before the current 5.3.  
   **Change:** Update 0.5 and the owner-decision table with those earlier gates, corpus group capacity sufficient for the frozen protocol, and the real-filler prerequisite for 2.8. Record the dated pre-M0 obligation confirmation before the initial documentation commit. [Owner decisions](rascal/PLAN.md:983)

**Fix in the owning task**

9. **[important] Several acceptance statements contradict the intended behavior.**  
   **Change:**

   - **1.5:** compare semantic journal content, excluding duration and other observations; complete journals need not be identical.
   - **4.2:** correct “middle untouched”: inversion makes the untouched middle editable; mapped prior support becomes protected.
   - **4.4:** obtain `result.png` from `heal`, not the preceding `run remove`.
   - **5.3:** use `heal remove` for a scenario claiming actual removal through a filler; `run` builds the mask.
   - **5.3, branch B:** remove the contradictory stub acceptance. The spec defers `segment`; test its absence and the named later milestone.
   - **1.7:** compare decoded channel samples, not a PNG file against decoded bytes.

   [Recipe acceptance](rascal/PLAN.md:336)

10. **[important] 7.1 is explicitly horizontal despite the universal vertical-slice claim.**  
    It has no CLI scenario until 7.2. Documentation/readiness tasks also intentionally have none. Size labels otherwise have one L, with the harmless additional XS for 0.5; labels alone do not make the broad 1.7 scope small.  
    **Change:** Wire `paste --mode poisson` in 7.1 alongside the solver; reserve 7.2 for end-to-end and release work. Narrow the CLI-scenario promise to executable tasks. Split 1.7 if necessary into independently demonstrable cut/put and geometry work, each carrying its needed primitives. [Task 7.1](rascal/PLAN.md:926)

11. **[suggestion] Additional public behavior and thresholds should be identified as plan additions.**  
    The spec does not promise the built-in `rect` recipe/`run --bbox`, `inspect --json`, CLI `workspace list`, `fill`, `mcp probe`, `segment --tool echo_region`, non-empty-output refusal, M1’s one-minute limit, a fixed two-pixel tightened scratch, the million-pixel/two-second Poisson bound, or Poisson producing lower interior drift than feather.  
    **Change:** Keep demonstration helpers private where possible, or document the intended public extensions. Treat performance bounds as explicitly chosen benchmarks with hardware/domain definitions. Replace unsupported numerical assertions with the spec’s prescribed cases; lower drift measures less change, not necessarily better healing. [Poisson acceptance](rascal/PLAN.md:926)

**Findings: 3 blockers, 7 important, 1 suggestion. Coverage gaps: 16 numbered contract, verification or decision groups (G1–G16).**
