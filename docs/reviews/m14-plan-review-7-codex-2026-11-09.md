Static review; no gates run. `SPEC.md` changed to draft 7 during the review; finding 6 identifies coverage gaps introduced by that update.

1. **Major — The disconnected bundle gate loses its asset source.** [PLAN.md:4177](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4177)  
   The bundle acceptance must install Tiny using local `--from` assets, but the workflow explicitly drops provisioning. The new, separate model-test job does not make its files available to the bundle jobs. The prescribed acceptance command supplies no asset directory either. On clean runners, segmentation therefore lacks its prerequisites. This regresses the previously settled required-model bundle gate.  
   **Fix:** Assign acquisition and verification of Tiny’s assets before disconnection, outside the bundle. Specify an argument/environment variable carrying that directory into acceptance, and provision it in both platform jobs. Missing assets or skipped segmentation must fail.

2. **Major — The release inventory omits required non-graph assets.** [PLAN.md:4123](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4123), [PLAN.md:4211](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4211)  
   Tiny needs downloadable licence files and bounded artifact sizes, but its current [manifest](/Users/markhayden/go/src/github.com/markhayden/rascal/tools/python/src/rascal_tools/artifacts.toml:8) contains only four inference files, without sizes. No task explicitly owns publishing Tiny’s licence assets. Meanwhile, the model-release checklist names graphs, external data and licences but omits SAM 3’s and EVF-SAM2’s tokenizers/configuration. The referenced SAM 3 exporter saves graphs only; the EVF lab adapter obtains its tokenizer through Transformers. Those releases alone cannot support clean offline inference.  
   **Fix:** Assign Tiny’s metadata/licence publication before cutover. For 14.7/14.8, explicitly export, publish, hash and install tokenizer/configuration assets using the pack’s runtime dependencies. Verify installation with empty upstream caches and networking disabled. Update the existing four-entry manifest assertion in the same commit.

3. **Major — The prepublication gate requires catalogue entries added only afterward.** [PLAN.md:4207](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4207)  
   The branch’s shipped pack must run the candidate before publication, but the subsequent “Release” step adds its artifact and catalogue entries. After 14.4, lookup accepts verified installations only; the plan provides no way to install or select that unpublished candidate for the gate.  
   **Fix:** Add candidate manifests and the adapter on the isolated model branch first, install the export using `get <model> --from <export-dir>` into an isolated directory, and run corpus parity. Publish only after it passes, then verify the release-download route before merging.

4. **Major — The convergence gate can finish before model tasks finish.** [PLAN.md:4032](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4032), [PLAN.md:4343](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4343)  
   Depending only on track-E tasks “that landed” permits tagging `m14` while any of 14.5–14.8 remains unstarted or in flight. A later independent model merge then escapes the final checked-retry gate. The spec permits omission after an export fails, not simply because its task has not finished. This weakens the settled convergence requirement.  
   **Fix:** Require a recorded disposition for all four tasks before 14.14: passed and merged, or attempted and excluded with failing-gate evidence. Freeze that set before convergence. Reflect those dispositions in [tasks/todo.md:186](/Users/markhayden/go/src/github.com/markhayden/rascal/tasks/todo.md:186).

5. **Major — Lab installation pinning remains unassigned.** [PLAN.md:4102](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4102), [PLAN.md:4251](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4251)  
   The harness records an installation identity, and 14.10 adds that identity to the lab key, but neither explicitly acquires the lab cell’s hold **before computing the key**. Testing that the key changes after reinstall does not exercise replacement between key computation and execution. Rascal’s later run-admission hold cannot protect that earlier window.  
   **Fix:** Assign the lab-side lock/resolve/hold operation, forwarding of the held identity to every call, and terminal cleanup. Add the spec’s deterministic reinstall-between-key-and-call test; require output to retain the held identity.

6. **Major — The updated spec’s instance-parity gate is not concretely covered.** [PLAN.md:4205](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4205), [PLAN.md:4262](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4262)  
   Newly updated [SPEC.md:1232](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1232) requires frozen instance references, specified fixtures/prompts, and count, score, region and plane tolerances. Generic export parity plus corpus-core IoU does not establish those assertions. The new EVF-SAM2 retry assertion excluding both `threshold` and `pad` also lacks explicit acceptance.  
   **Fix:** Assign fixture/reference generation and required-model instance comparisons to each model task, including their exact failing command. Add the EVF-specific retry assertion to 14.10 and convergence.

7. **Minor — SAM 3’s frozen baseline needs explicit empty-result handling.** [PLAN.md:4107](/Users/markhayden/go/src/github.com/markhayden/rascal/PLAN.md:4107)  
   The existing `sam3-own` round has 20 runs but only 19 output masks: `thin-05` legitimately returns `not_applicable`. Copying “the round’s cores” therefore fails the mandatory 20-image comparison. The existing lab runner treats that result as an image-sized empty plane.  
   **Fix:** Materialize and hash empty reference planes for verified `not_applicable` outcomes, do the same in `pack-round`, and test empty/empty, empty/non-empty and genuinely missing results separately.

The separate branches, atomic cutover, per-model markers, provisioned Linux gate, isolated restore drill and coordinated squint bump remain intact. The todo numbering and tags match the plan; it needs the completion/disposition clarification above.

**Verdict: not ready to begin M14.**
