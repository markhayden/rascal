**No—draft 2 is not ready to plan and build against: its typed execution path, feather equation, workspace publication protocol, and schema-compatibility tests still contain blocking contradictions.**

**Resolution of review 1**

| # | Verdict | Why |
|---|---|---|
| 1 | Partly resolves | Effective weight and protection precedence are explicit, but the normative profile formula contradicts the promised feather contour. |
| 2 | Partly resolves | Removing aspect conversion makes lossless inversion feasible, but integer strip selection remains ambiguous for odd and small widths. |
| 3 | Partly resolves | Typed artifacts and prescription creation exist, but the example recipe and `heal` orchestration violate their declared input types. |
| 4 | Partly resolves | Separate schemas are introduced, but copied location kinds still incorrectly determine compatibility of an entire extended envelope. |
| 5 | Partly resolves | Ordinary arguments and workspace discovery resolve public access, but result handling and the distinction between schema compatibility and harness interoperability remain incomplete. |
| 6 | Partly resolves | Rasterization and stroke-only repair remove the original dependency gaps, but advertised rectangle repair still has no recipe path. |
| 7 | Partly resolves | Canonical `B` and final `heal` verification are defined, but standalone paste preservation and channel handling remain inconsistent across modes. |
| 8 | Partly resolves | Mis-sized filler output is rejected, but window selection and integer resampling do not yet define a reproducible request transform. |
| 9 | Resolves | Mapping now supports multiple parts, frame-local topology, retained source geometry, and qualified round-trip guarantees. |
| 10 | Partly resolves | The Poisson problem is substantially specified, but numerical degeneracies, output range, and channel conventions remain open. |
| 11 | Partly resolves | Normative algorithms are added, but fan has conflicting parameter ownership and tightening asks a magnitude for negative extrema. |
| 12 | Partly resolves | Rendering roles are explicit, but inverse rendering cannot implement the promised second pass through `heal`’s original paste weights. |
| 13 | Partly resolves | Inputs, external outputs, and observational metadata are addressed, but output hashes embedded in their own artifacts lack a non-circular identity definition. |
| 14 | Partly resolves | Staging and locking are introduced, but publication is not atomic and opening a workspace can delete active work. |
| 15 | Partly resolves | Environment filtering and deadlines are executable, but runtime result validation and concurrency precedence remain unspecified. |
| 16 | Partly resolves | Before/after images and series exist, but some measures require unbindable geometry combinations or undefined sampling domains. |
| 17 | Partly resolves | Cells and protocols exist, but identity omits relevant inputs and pick files lack a binding to the exact comparison. |
| 18 | Resolves | Schema copying is explicitly excepted, model execution is confined to configured external tools, and templates are independently authored. |
| 19 | Partly resolves | Platforms and offline acceptance are named, but bundle contents remain undefined and optional segmenter delivery conflicts with mandatory acceptance. |
| 20 | Partly resolves | Acceptance coverage improves substantially, but suite ownership and several numerical and blind-scoring decision rules remain unresolved. |

**New findings**

1. **[blocker] The example recipe and `heal` pipeline do not satisfy their own typed contracts.**  
   **Artifacts, Operations, Recipes; approximately lines 191–201, 362–367, 459–509.** `cut` requires a raw `Frame`, but the recipe binds an `Image` through an input named `image`. Validation must reject the shipped example. Further downstream, `apply` returns an `Image`, while `window` requires a `Frame`; `render` accepts only a mask and therefore has no binding for the prescribed working pixels used by `alpha` or `tinted`. An implementation must invent conversions or silently use the original pixels.

   **Change:** Define image-payload versus coordinate-frame bindings explicitly, correct the example, and write the complete typed `heal` graph—including the working image supplied to windowing and image-based rendering. Validate and execute the literal example as an acceptance fixture.

2. **[blocker] The profile equation reintroduces the feather contradiction mathematically.**  
   **Algorithms and Testing; approximately lines 408–411, 793–795.** For every blend pixel, `d > half`, so
   `Φ((half−d)/σ) − Φ((−half−d)/σ) < 0.5`.
   Consequently, the core has weight 1 and every outside sample has weight below 0.5; there is no 50% contour inside the blend. With `half=10`, `σ=5.5`, the first outside sample at `d=10.5` is only `118/255`. This produces a sharp drop at the core boundary.

   **Change:** Specify an outward feather whose boundary limit is 1 and whose 50% contour is outside the core, with explicit truncation and quantization. Use the same definition for generic `feather` and replace the contradictory tests.

3. **[blocker] The workspace protocol neither publishes atomically nor safely identifies abandoned work.**  
   **Workspace and Testing; approximately lines 558–564, 812–814.** Renaming artifacts and appending several registry files under a writer lock is not one commit: a crash can leave partial publication, and readers have no stated snapshot protocol. More immediately, “On open, incomplete staging directories are removed” can delete another process’s active operation because the lock is held only during publication. Cancellation also lacks a defined race boundary with commit.

   **Change:** Define one durable commit point, reader visibility, transaction ownership, and recovery. Use a commit record or atomically switched registry generation; distinguish active staging from abandoned staging; serialize cancellation against commit. Test crashes between publication steps, opening during active work, and cancellation both before and after commit.

4. **[blocker] The compatibility predicate still makes the schema acceptance test impossible.**  
   **Decisions, Operations, Schema tests; approximately lines 106–112, 337–342, 806–809.** An envelope containing only `bbox` locations can still contain rascal’s mandatory `artifacts` extension or an unsupported image mapping. The copied [envelope schema](dejaview/schemas/envelope-v1.json:314) has `additionalProperties: false` and no `artifacts` property. Location kind alone cannot establish compatibility. Also, the sibling provides `Location` as an embedded definition, not a standalone location-schema file to copy unchanged.

   **Change:** Define a compatibility export with an exact field whitelist, supported mappings, and explicit rejection rules; validate that export separately from native envelopes. Reference the copied location definition directly. Distinguish data-schema compatibility from tool interoperability: the sibling’s `_dejaview` context and annotations still require an explicit adapter.

5. **[important] Fan and ramp do not yet have one reproducible parameter contract.**  
   **Algorithms; approximately lines 383–418.** `fan` writes sigma using `feather_ratio`, but `profile` independently accepts that parameter without saying which value wins. Ruler-window rounding, clipping, empty windows, Sobel borders, and several initial constant values remain unspecified. Ramp must reuse fan’s sampling columns, although the declared `BandProfile` does not preserve those windows. Finally, smoothing widths after gating can widen a closed-gate row, contradicting “A closed gate yields the minimum width.”

   **Change:** Assign feather-ratio ownership once; retain the resolved ruler geometry needed by ramp; freeze initial constants and sampling edge cases. Either enforce the gate after width smoothing or narrow the minimum-width guarantee to the unsmoothed result.

6. **[important] Tightening asks a nonnegative magnitude for negative extrema.**  
   **Algorithms; approximately lines 420–427.** The algorithm computes “gradient magnitude” and then selects “positive and negative gradient extrema.” Magnitude has no negative values, so the specified boundary pair cannot exist. Stroke sampling density and local direction are also undefined, and smoothing across fallback samples can alter the stroke extent that was supposed to remain unchanged.

   **Change:** Use a signed derivative, define sample spacing and interpolation, specify admissible boundary-pair scoring and deterministic ties, and state how smoothing preserves untightened sections and prevents crossed boundaries.

7. **[important] Paste still lacks a complete baseline and numerical output contract.**  
   **Paste and Boundaries; approximately lines 429–449, 843–846.** Paste preserves `base`, whereas the global invariant requires preservation against `B`; standalone paste has neither a baseline binding nor a precondition establishing their equality outside the editable region. Only feather defines RGBA behavior. Poisson leaves color/alpha handling, out-of-range solutions, and the relative-residual test when the right-hand side is zero unspecified.

   **Change:** Bind or verify the immutable baseline for every paste, define channel compatibility and alpha behavior for all modes, and specify Poisson’s color space, clamping, zero-residual convention, and adjacency for every supported topology. Clarify the flat-field comparison test: differing constant base and fill images generally produce different Poisson and feather results.

8. **[important] The integer cut rule is not executable consistently on the promised inputs.**  
   **Images and frames; approximately lines 228–234.** For odd `W`, the stated cap `W / 2 − 1` can be fractional—for example, `1.5` at `W=5`—despite `s` being an integer. At `W=4`, the cap defeats the preceding minimum of 2. “Rounded to even” also needs to distinguish ties-to-even from forcing an even strip width.

   **Change:** Give one integer formula, a valid range for `f`, and an explicit rounding convention; reconcile the minimum supported width with the intended minimum strip width. Include expected `s` values for the smallest widths and odd-width cases.

9. **[important] Refinements and binary rendering do not fully preserve the new mask invariants.**  
   **Mask rules, Renderings, Operations; approximately lines 302–310, 325, 355–361.** Growing core into existing blend leaves nonzero blend inside core unless another rule clears or recomputes it. Shrinking and closing likewise lack a blend policy. Separately, the binary formula marks zero-weight—including protected—pixels white when `threshold=0`, unless that value is explicitly rejected.

   **Change:** Define each refinement’s postconditions for all three planes, including blend invalidation or recomputation. Require protected pixels to render zero independently of threshold and specify threshold bounds.

10. **[important] The protect profile cannot drive the advertised inverse second pass through `heal`.**  
    **Renderings and Recipes; approximately lines 328–333, 505–509.** The filler receives inverse weights, but paste still uses the original mask’s weights. Edits outside the first mask are therefore discarded. Moreover, `1−w` remains positive throughout fractional blend pixels, contrary to the claim that the first mask’s “region” is excluded.

    **Change:** Either restrict `protect` to export and reject it as a `heal` profile, or introduce a typed inverse-mask operation whose weights govern both rendering and paste. Specify whether exclusion covers the full nonzero support or only the fully editable core.

11. **[important] The window transform does not guarantee `max_side` or reproducible alignment.**  
    **Operations and Fillers; approximately lines 366, 542–550.** A window with `floor_px=max_side` has a minimum size, not a stated maximum; the `scale` parameter has no defined selection rule. “If the mask fits” does not identify core versus full effective-weight support. A uniform scale also rarely gives integer dimensions on both axes, so recording a factor and filter alone does not fix the pixel-center mapping.

    **Change:** Specify support bounds, context expansion, integer crop placement, target dimensions, pixel-center transforms, and mask/image sampling separately. Enforce the maximum on both request dimensions and define behavior for empty masks and seam-spanning support.

12. **[important] Measures still cannot run consistently on the main seam pipeline.**  
    **Recipes and Measures; approximately lines 505–509, 607–620.** After `put`, `before=B` and `after=result` are raw-frame images, while the recipe’s mask/profile remains in the cut frame; `measure` requires one frame. Its `Mask or line` interface also does not explain how measures bind both geometry and profile. `texture_match` uses `half` even for general blob masks, and flat rows or achromatic colors leave ratio and hue cases undefined.

    **Change:** Specify the measurement frame and explicit geometry/profile bindings. Measure seam continuity in corresponding cut images and preservation in raw coordinates, or define equivalent mappings. Restrict band-only measures appropriately and freeze zero-denominator, empty-domain, hue, gradient, and sampling conventions.

13. **[important] MCP startup validation is specified, but runtime result validation is not.**  
    **Workspace/MCP; approximately lines 566–603.** Manifest validation does not establish that a pack result obeys its schema, bounds, frame references, or image-block correspondence. Pack configuration and manifests can disagree on concurrency without a precedence rule. Cancelling one concurrent call shuts down the process without defining outcomes for its other calls. Public `workspace_read` also promises image blocks for arbitrary artifacts, including JSON records and mask directories.

    **Change:** Define validation before any returned data is committed, concurrency precedence, terminal outcomes for all affected calls, and complete result/error serialization. Restrict image reads to image artifacts and provide an appropriate representation for other artifact types.

14. **[important] Content identity is circular for artifacts containing their own output hashes.**  
    **Mask provenance and Recipes; approximately lines 290–315, 497–503.** Every zone-changing operation appends input/output hashes to the mask record it produces. If a mask’s hash covers its semantic metadata, that metadata includes its own output hash. Excluding timestamps and durations does not resolve this cycle.

    **Change:** Define canonical hashing explicitly—for example, hash payload and semantic parameters while keeping provenance edges in an external immutable node. State treatment of allocated IDs and registry references, and test equivalent outputs across separate workspaces.

15. **[important] The accepted location kinds exceed the executable v1 recipe contract.**  
    **Use cases, Operations, Recipes; approximately lines 67, 353–354, 516–521.** Repair advertises rectangles and stain blobs, but the only recipe accepts a stroke and invokes `tighten`. `rasterize` accepts “any kind,” including `absent`, which has no interior, and `view`, whose rasterized footprint is unspecified. Line dilation also needs to distinguish full width from radius.

    **Change:** Enumerate supported rasterization kinds and their exact geometry semantics; return `not_applicable` for non-geometric locations. Add the rectangle/blob repair branch or narrow the advertised v1 repair scope consistently.

16. **[important] Lab cell identity can reuse results after answer-affecting inputs change.**  
    **Lab; approximately lines 624–650.** Identity includes image bytes and recipe information, but no explicit location/protect artifacts or finder identity. A command path or URL also remains unchanged when its implementation, model, or relevant configuration changes. The matrix has no per-image input-binding mechanism for the remove and repair recipes.

    **Change:** Define resolved cell inputs and hash all bound artifacts plus relevant finder/filler configuration and an explicit implementation/model revision. Add per-image recipe bindings. Preserve separate attempts when `--redo` repeats an external call rather than overwriting an existing completed result.

17. **[important] Blind picks and the success rule are not bound tightly enough to a particular experiment.**  
    **Lab and Success; approximately lines 651–656, 875–879.** Assignment and pick files have no specified version, stable pair IDs, or experiment digest, so stale picks can be decoded against regenerated pairs. The two-thirds/one-third rule does not define zero decided pairs or the denominator for structured-group losses. Freezing the protocol only before opening the pairs page still permits inspecting results and tuning the rule beforehand.

    **Change:** Version assignment/pick schemas and bind them to exact cell results and a protocol digest. Reject missing, duplicate, foreign, or incomplete comparisons; define `same`, `neither`, denominators, and insufficient-evidence outcomes. Freeze the protocol before inspecting experiment outputs.

18. **[important] Optional segmenter delivery and mandatory offline acceptance remain incompatible.**  
    **Decisions, Testing, Success, Open questions; approximately lines 118–121, 825–839, 867–874, 901–903.** M5 may ship without `segment`, but acceptance and success unconditionally require text-prompt segmentation on both platforms. The bundle still lacks a concrete inventory and offline installation contract. Separately, `cargo test` claims to cover acceptance cases, while the actual acceptance script is assigned to release CI.

    **Change:** Resolve or explicitly branch the release scope before committing to M5 acceptance. Specify the bundle’s interpreter/runtime prerequisites, locked packages, model artifacts, digests, installer, and disconnected launch procedure. Separate the timed offline Rust suite from release acceptance and state which command proves each success criterion.

**Count: 4 blockers, 14 important findings, 0 suggestions.**
