**No—draft 3 is substantially improved, but artifact identity, recoverable commits, and several executable contracts still need specification changes before it is ready to plan and build against.**

**Resolution of review 2**

| # | Verdict | Assessment |
|---|---|---|
| 1 | Partly resolves | Images and frames are separated correctly, but `heal` still lacks explicit bindings for additional recipe inputs and the image/plan outputs it consumes. |
| 2 | Resolves | The outward half-Gaussian removes the core-boundary discontinuity and specifies support and quantization. |
| 3 | Partly resolves | Generations, reader snapshots and staging ownership improve publication, but crash recovery cannot reconstruct all committed provenance from the specified durable records. |
| 4 | Partly resolves | The whitelist and embedded location reference are correct, but rejecting every `artifacts` field prevents normal native-envelope export, and standalone-location export lacks a defined wrapping contract. |
| 5 | Resolves | Feather-ratio ownership, ruler geometry, initial constants, clipping and the post-smoothing gate behavior are specified. |
| 6 | Resolves | Signed derivatives, sampling, pair scoring and smoothing confined to tightened runs fix the substantive algorithm contradiction. |
| 7 | Partly resolves | Baseline, channel and topology rules are substantially complete, but immediate convergence when `b = 0` is mathematically incorrect with the specified initial guess. |
| 8 | Resolves | The integer formula is executable, and all listed examples agree with it. |
| 9 | Resolves | Blend invalidation and protection postconditions are explicit, and binary thresholds exclude zero. |
| 10 | Resolves | `invert` creates an actual mask, excludes the original nonzero support and preserves existing protection. |
| 11 | Partly resolves | Crop-only requests eliminate resampling ambiguity, but expanded context can exceed the image and cannot then be “slid inside.” |
| 12 | Partly resolves | Measurement frames and profile bindings are fixed, but empty sampling domains and all-skipped summaries remain undefined. |
| 13 | Resolves | Runtime validation, image-block correspondence, the concurrency AND rule, affected-call failures and workspace reads are specified. |
| 14 | Partly resolves | Excluding provenance removes the direct self-hash cycle, but hashed frame and profile references still contain workspace-local identifiers and paths. |
| 15 | Resolves | Rasterization kinds are enumerated, and `repair-area` supplies the missing area branch. |
| 16 | Partly resolves | Artifact bindings, revisions and attempts improve reuse, but finder prompts, arguments and relevant configuration are not explicitly included in cell identity. |
| 17 | Resolves | Assignment and picks are versioned and bound together; denominators, insufficient evidence and pre-run protocol freezing are defined. |
| 18 | Partly resolves | Release branches and suite ownership are reconciled, but the bundle still lacks the dependency and model bytes needed for disconnected installation. |

**New findings**

1. **[blocker] Artifact identity still depends on allocation and can omit referenced content.**  
   **Artifacts and identity; approximately lines 198–208, 282–310, 315.** Identity “excludes ids,” yet includes `frame`, whose values include `crop:<n>` and allocated plan identifiers. Mask identity also includes sections containing `loc-2` and filenames for profiles and prescription series, without saying those references are replaced by the referenced identities. The same computation after unrelated workspace activity can therefore hash differently; conversely, equal reference strings can name different content.

   **Concrete change:** Define semantic frame identity from root content, projection, topology and parent mapping, independently of registry IDs. Hash artifact references through their semantic identities, including profile and prescription contents; define `Plan` identity too. Test equivalent runs in workspaces with different allocation histories, and changed referenced content under unchanged local names.  
   [SPEC.md:282](rascal/SPEC.md:282)

2. **[blocker] A committed generation can lose the information required for replay.**  
   **Workspace; approximately lines 636–660.** The registry contains frames, locations, artifacts, plans and a counter; the operation journal is appended after commit. A crash between those actions loses resolved parameters, input bindings and the envelope for operations whose artifacts do not carry a provenance copy. Journaling merely `recovered` cannot reconstruct that information. Calling the `CURRENT` rename “durable” also leaves the required persistence ordering unstated.

   **Concrete change:** Commit an immutable operation record containing the replay information alongside the artifacts, and reference it from the generation before publishing `CURRENT`. Make the JSONL journal a recoverable projection of those records. Require artifact, operation-record and generation durability before publication, followed by durable publication; test recovery through full replay, not just the presence of a `recovered` line.  
   [SPEC.md:646](rascal/SPEC.md:646)

3. **[important] The bundle inventory cannot satisfy disconnected installation.**  
   **Python pack, Testing and Success; approximately lines 795–805, 986–994, 1033–1036.** The bundle contains the pack wheel, a lockfile and model URLs/digests. It does not contain the transitive dependency wheels or model artifacts. A clean machine with only the stipulated interpreter cannot install those dependencies offline; branch A also cannot obtain its checkpoint. Branch B fixes the segmenter scope conflict but still needs dependency packages.

   **Concrete change:** Include a complete platform-specific dependency wheelhouse and, under branch A, model bytes, or explicitly require a separately supplied offline artifact directory with a complete manifest. Declare the `uv` prerequisite and use an installer mode that prohibits downloads. Test with empty caches. Branch A eligibility must also include the text-prompt capability required by criterion 5, not merely an exportable permissive checkpoint.  
   [SPEC.md:795](rascal/SPEC.md:795)

4. **[important] The Poisson zero-RHS shortcut returns an incorrect solution.**  
   **Paste; approximately lines 508–520.** The solver starts at `fill` and declares convergence “at once when `b = 0`.” Consider a single editable pixel, a black constant base and a constant fill of 100. Source gradients and boundary values give `b = 0`, but the initial solution is 100 with a nonzero residual. Immediate convergence returns 100 instead of the required black base, contradicting the constant-field test.

   **Concrete change:** Always evaluate the residual of the current iterate, or explicitly set the solution to zero for the anchored homogeneous system before returning. Add this exact zero-RHS/nonzero-initial-guess case.  
   [SPEC.md:514](rascal/SPEC.md:514)

5. **[important] Window placement is impossible for ordinary full-height seam support.**  
   **Window; approximately lines 491–497.** With default `context = 0.5`, support spanning height `H` expands to height `2H`. That rectangle cannot be “slid inside the image rather than clipped,” regardless of filler capacity. Full-height support is normal for the seam recipe. The contract also does not distinguish mandatory support from context that may be reduced to meet `max_side`.

   **Concrete change:** Keep support mandatory, cap optional context to the available image extent, and specify whether context is reduced to meet `max_side` or causes rejection. Define integer rounding and placement ties. Test full-height support, support touching each border, and support that fits while its requested context does not.  
   [SPEC.md:491](rascal/SPEC.md:491)

6. **[important] The fixed `heal` graph still relies on implicit recipe bindings.**  
   **Recipes; approximately lines 564–604.** The example declares only a mask output, while `heal` consumes an image in the mask’s frame and a cut plan. It identifies that image as `cut.image`, coupling execution to a step name rather than an output contract. `run(recipe, image)` also omits the location, stroke, protect and existing-mask bindings needed beyond seam. Separately, the promised recipe per v1 use case has no protect recipe—the four listed recipes include two repair variants.

   **Concrete change:** Declare the recipe outputs consumed by `heal`: mask, source image in its frame and any required restoration plan. Pass a typed input map through `heal`, and validate those outputs without relying on step names. Add an explicit existing-mask/invert recipe or narrow the “one recipe per v1 use case” promise.  
   [SPEC.md:578](rascal/SPEC.md:578)

7. **[important] Refinements can leave prescriptions that modify pixels outside the new mask.**  
   **Mask postconditions and ramp application; approximately lines 334–342, 470–475.** Clearing blend after a core change does not invalidate the old band profile or tone-ramp prescription. After shrinking a ramp-bearing mask, the prescribed formula still modifies unprotected pixels inside the old half-width, including pixels now having `w = 0`. This conflicts with the rule that prescriptions use effective weight and can make `heal` fail its preservation check.

   **Concrete change:** Explicitly prohibit `apply` from modifying any `w = 0` pixel. Define whether core-changing refinements invalidate, recompute or retain profile-dependent prescriptions, and whether retained profiles remain eligible for band measures. Test `ramp → shrink → feather → apply` against the new support.  
   [SPEC.md:338](rascal/SPEC.md:338)

8. **[important] Compatibility export has no usable normal-envelope path.**  
   **Operations and Compatibility export; approximately lines 373–375, 785–791.** Every native operation envelope includes `artifacts`, but its presence makes export fail. Thus even an otherwise compatible rectangle result is rejected. The advertised location input is also underspecified: a bare copied `Location` does not validate as an envelope, whose required fields are `status`, `summary` and `provenance`; an enclosing region additionally requires its own metadata.

   **Concrete change:** Define an explicit projection that omits native artifact bookkeeping while rejecting unsupported content selected for export, or define a separate exportable result selection. Specify whether standalone locations validate against the embedded `Location` definition or are wrapped, including the required wrapper fields. Add positive tests from actual native outputs. The copied schema permits the stated top-level whitelist; the remaining problem is constructing a usable export under it.  
   [SPEC.md:785](rascal/SPEC.md:785), [copied-envelope source:361](dejaview/schemas/envelope-v1.json:361)

9. **[important] Lab reuse still lacks the complete finder request identity.**  
   **Lab; approximately lines 758–764.** Bound artifact identities and a pack bundle digest do not identify a finder invocation’s prompt, points, selected tool, resolved arguments or relevant configuration. The same image and bundle can produce different masks after those values change, yet the listed cell key need not change.

   **Concrete change:** Hash all resolved recipe inputs, including scalar inputs, and a canonical finder request/configuration record. Specify how the finder identity is determined before deciding whether a cell can be reused. Test that changing only a prompt or finder parameter produces a new cell.  
   [SPEC.md:758](rascal/SPEC.md:758)

10. **[important] Measures still lack finite results for empty domains.**  
    **Measures; approximately lines 712–726.** Skipping a zero denominator does not define a mean over an empty outside window, or summaries when every row is skipped. Wide bands can leave `transition` with no outside samples; flat images can leave no valid continuity rows. Hue differences also need a circular convention to avoid treating angles near opposite sides of the angular boundary as far apart.

    **Concrete change:** Specify clipping and empty-domain behavior, the outcome and omitted summaries when no valid samples remain, and signed or unsigned circular hue differences with units. Require finite serialized values and tests for empty windows, all-skipped series and hue wraparound.  
    [SPEC.md:712](rascal/SPEC.md:712)

The plan can carry the MCP implementation choice, concrete encoder settings, platform-specific durability implementation, and tighten’s final tie-break and endpoint/degenerate-tangent handling as explicit task decisions. Minor stale wording such as “three shipped recipes” can accompany those tasks. The findings above affect promised behavior or acceptance and need specification changes.

**Count: 2 blockers, 8 important findings, 0 suggestions.**
