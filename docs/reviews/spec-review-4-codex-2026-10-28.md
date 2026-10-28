**No—draft 4 closes most earlier gaps, but the root identity cycle and remaining durability, reuse, and second-pass contracts need correction before building against it.**

**Resolution of review 3**

| # | Verdict | Assessment |
|---|---|---|
| 1 | Partly resolves | References now use semantic identities, but root-frame identity depends on image identity, which depends on that frame’s identity. |
| 2 | Partly resolves | Durable operation records solve the replay-information gap, but the stated publication sequence omits persistence steps for directory entries and `CURRENT` contents. |
| 3 | Resolves | The wheelhouse, model bytes, explicit prerequisites, disconnected installation test and text-prompt condition cover the finding. |
| 4 | Resolves | Convergence now tests the actual residual, including the zero-RHS/nonzero-initial-guess case. |
| 5 | Resolves | Mandatory support and reducible context make full-height and capacity-limited crops feasible. |
| 6 | Partly resolves | Named outputs, typed inputs and the protect recipe are present, but the advertised second pass conflicts with `heal`’s original-baseline preservation check. |
| 7 | Resolves | Core changes invalidate profiles and prescriptions, and `apply` explicitly preserves every zero-weight pixel. |
| 8 | Partly resolves | Projection and standalone-location validation are defined correctly, but the schema-test requirement still rejects the `artifacts` field that projection must discard. |
| 9 | Partly resolves | Prompts and finder arguments enter identity, but relevant pack configuration and determination of the request identity before reuse remain unspecified. |
| 10 | Resolves | Empty windows, all-skipped results, finite serialization and circular hue differences are covered. |

**New findings**

1. **[blocker] Root image and frame identities are mutually recursive.**  
   **Artifacts and identity, approximately lines 283–293.** A raw frame hashes its root image’s identity; that image hashes its frame identity. Neither hash can be computed first. Replacing local references with semantic identities fixes allocation dependence but introduces this cycle.

   **Concrete change:** Define a separate root pixel-content digest independent of frames, roles and provenance. Derive raw-frame identity from that digest plus projection/topology, then derive image-artifact identity from pixels, frame identity and role. Require import and cross-workspace identity tests to exercise this dependency order.

2. **[important] The explicit commit sequence does not establish its promised durability.**  
   **Workspace, approximately lines 679–699.** Fsyncing `registry/<n+1>.json` does not persist its new directory entry; the sequence never fsyncs `registry/`. It also renames the temporary `CURRENT` file without first requiring its contents to be fsynced. Mask artifacts contain nested directories whose entries likewise need persistence. After a power failure, a published pointer can therefore lack the generation or payload it names.

   **Concrete change:** Require persistence of staged directory trees, the new generation’s directory entry, and the temporary pointer’s contents before replacing `CURRENT`; persist the containing directory afterward. Distinguish atomic visibility at rename from acknowledged durability after synchronization, and test failure boundaries accordingly.

3. **[important] The protect recipe cannot preserve an earlier edit through `heal` in the same workspace.**  
   **Canonical baseline and Recipes, approximately lines 215–217, 612–620 and 636–637.** Suppose the first pass changes a pixel from canonical value 10 to 20. `invert` protects that pixel during the second pass, so using the first composite as the second pass’s image correctly preserves 20. The final check nevertheless compares it with the root’s immutable `B = 10` and fails. Reimporting the composite creates a different frame, requiring an unspecified mask-rebinding operation.

   **Concrete change:** Define preservation relative to the image supplied to each `heal` invocation, captured before that invocation’s prescriptions. Keep root provenance separately, allow the prior composite as the protect recipe’s image, and test two consecutive passes in one workspace.

4. **[important] Finder reuse still lacks a complete, pre-execution identity.**  
   **The lab, approximately lines 801–810.** The request record contains pack name, bundle digest, tool and arguments, but omits answer-affecting configuration supplied through the configured command or environment. Moreover, “when a finder ran” does not explain how the key is obtained before deciding to reuse a completed cell. Running the finder to discover the key defeats reuse of recorded nondeterministic outputs.

   **Concrete change:** Define a canonical finder invocation descriptor before invocation, including resolved arguments, semantic input identities and relevant non-secret configuration or an explicit configuration revision. Compute the reuse key from that descriptor; execute the finder only on a miss or explicit redo. Test configuration-only invalidation and a cache hit that makes no external call.

5. **[suggestion] Compatibility-export acceptance still asserts the obsolete rejection rule.**  
   **Compatibility export and Testing strategy, approximately lines 831–840 and 1020–1022.** The export section drops `artifacts`, while the schema-test section requires its presence to fail. Both cannot hold for a normal native envelope. The copied schema supports the new projection and embedded-location validation approach.

   **Concrete change:** Replace the `artifacts` rejection test with a positive native-envelope projection test; retain negative tests for unsupported locations and mappings.

The plan can carry these task-level decisions:

- Exact integer rounding and centering conventions for optional window context.
- Tighten’s final tie-break, endpoint sampling and degenerate-tangent handling.
- Canonical serialization and PNG encoder settings, after fixing the identity dependency graph.
- MCP implementation choice and platform-specific synchronization mechanisms.

**Count: 1 blocker, 3 important findings, 1 suggestion.**
