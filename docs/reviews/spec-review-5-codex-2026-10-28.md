**No—draft 5 fixes the identity, durability and export issues, but derived-frame protection and pre-execution lab identity still need small contract changes before building against it.**

**Resolution of review 4**

| Finding | Verdict | Assessment |
|---|---|---|
| 1. Identity cycle | Resolves | Frames no longer depend on image content, so pixel, frame and image identities can be computed without recursion. |
| 2. Commit durability | Resolves | The sequence now persists staged trees, destination directories, the generation, registry directory and pointer contents before acknowledging durability after the final sync. |
| 3. Second-pass baseline | Partly resolves | Invocation-local `B` fixes preservation, but the protect recipe still lacks compatible image/mask bindings when the earlier mask occupies a derived frame. |
| 4. Finder reuse | Partly resolves | Revisions and pre-execution descriptors cover independent finder calls, but descriptors containing downstream input identities cannot always be computed before any finder runs. |
| 5. Export acceptance | Resolves | Positive projection tests replace the obsolete rejection rule and agree with the copied envelope schema. |

**New findings**

1. **[important] `protect` cannot consume the principal seam workflow’s outputs as specified.**  
   **Operations and Recipes, approximately lines 407–408, 428, 618–649.** After `heal seam`, the composite occupies the raw frame while the mask remains in the cut frame. `invert` preserves that cut frame; the protect recipe supplies neither a matching image nor the required plan output. `apply` therefore fails frame validation. Simply cutting the composite again would edit only the strips, missing the untouched middle despite the promise to cover everything the first mask left alone.

   **Concrete change:** Define how protect projects the earlier mask’s support and protection into the raw frame, treats pixels outside its mapped extent as previously untouched, and inverts there. Specify compatible image/restoration bindings and paste support for the resulting topology. Require a `heal seam → heal protect` test that preserves the first edit and permits changes in the panorama’s middle.

2. **[important] The lab key requires information that a valid recipe can produce only through execution.**  
   **Recipes and The lab, approximately lines 103–105, 606–615, 817–827.** Typed bindings allow a finder to consume an earlier finder’s output—for example, `segment → select → render → segment`. The second invocation’s input identity depends on the first finder’s nondeterministic result, so its descriptor cannot be computed “before any finder runs.” The promised key and cache-hit behavior therefore require an unstated restriction.

   **Concrete change:** Either restrict v1 lab recipes so every finder input comes from initial inputs or deterministic preprocessing independent of external results, rejecting violations during validation; or key cells from the initial inputs and resolved invocation graph, recording concrete intermediate identities within each attempt. Test the chosen behavior with a dependent-finder recipe.

The plan can carry these task-level decisions:

- Canonical serialization, hash encoding and PNG encoder settings.
- Integer rounding and centering for optional window context.
- Tighten’s final tie-break, endpoint sampling and degenerate-tangent handling.
- MCP implementation choice and platform-specific synchronization mechanisms.

**Count: 0 blockers, 2 important findings, 0 suggestions.**
