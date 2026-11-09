| Open item | Resolution | Why |
|---|---|---|
| R1 #1 — Parity definition | **Resolved** | Fixed settings, reference planes, matching, positive cases and empty-plane IoU are now specified. |
| R1 #5 — Installed converter resources | **Resolved** | Input images and references ship in the wheel, with acceptance from an installed wheel without a checkout. |
| R1 #6 — Unavailable configured default | **Resolved** | Configuration explicitly rejects an unavailable default or a pack that does not declare the requested model. |
| R1 #7 — Installer lifecycle | **Partly** | `last_outcome` enables reconciliation, but its `installed / failed / cancelled` states cannot represent successful uninstallation. |
| R1 #8 — Atomic publication | **Resolved** | Publishing through an atomic `current` replacement closes the previously identified old-directory rename window. |
| R1 #9 — Selection precedence and third-party packs | **Resolved** | Step-local model selection is prohibited, and forwarding is conditional on the pack declaring `model`. |
| R1 #12 — Installation evidence | **Resolved** | Adapter version, installation-sensitive estimate binding, preserved manifests and model-free recorded replay are specified. |
| R1 #17 — Premature generalization | **Resolved** | The catalogue is flat, and 0.6.0 defines only model installables. |
| R2 #1 — Explicit unsupported parameters | **Resolved** | Applicability is checked before default expansion; only applicable effective parameters are forwarded and recorded. |
| R2 #2 — Invocation extension shape | **Resolved** | Estimates, invocations and Wafflery consistently use `effective["x-rascal"].segmenter`. |
| R2 #3 — Tiny diagnostics | **Resolved** | Optional diagnostics explicitly preserve Tiny’s existing measurements. |

Round 2 contains **eight** rows marked “Partly,” not nine; all eight are covered above.

The following findings are new.

1. **Major — A run can switch installations between segmentation calls.** [SPEC.md:1196](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1196)  
   A reinstall is picked up by the next pack call, while only the model **ID** is resolved once per run. A reinstall between initial segmentation and an M13 crop retry can therefore combine masks from two installations. The single installation identity recorded in the invocation and used for estimate/lab binding cannot describe that execution accurately. Removal between those calls can likewise interrupt a run after it has already paid for a fill.

   **Fix:** Resolve and pin the installation identity at run admission, use it for every retry, and retain that installation until the run finishes. Either defer conflicting removal/replacement or retain the referenced generation. Test a reinstall and removal between initial segmentation and a checker-triggered retry.

2. **Major — Publication fails when reinstalling the same installation identity.** [SPEC.md:1227](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1227)  
   An unchanged release-model manifest can produce the same identity as the current installation. Renaming staging onto that existing, nonempty directory fails. Furthermore, the unconditional instruction to delete the “previous installation” would delete the current target if an implementation reused the existing directory without distinguishing equal identities.

   **Fix:** Define identical-install handling explicitly: verify and reuse an existing matching installation, discard only staging, and delete the previous generation only when its identity differs from the newly published one. Add acceptance for two consecutive Gets of an unchanged release model.

3. **Major — Removing SAM 3’s `pad` leaves the checker’s crop padding undefined.** [SPEC.md:1107](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1107)  
   SAM 3 rejects `pad` and excludes it from effective parameters, but M13 still constructs checker-directed retry crops using that setting ([SPEC.md:970](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:970)). An ordinary SAM 3 run therefore has no specified padding value for this required retry path.

   **Fix:** Define host-side retry crop padding separately from adapter box padding, including its default and recorded value. Preserve Tiny’s existing behavior and keep unsupported settings out of SAM 3’s pack arguments. Test a SAM 3 checker retry with an asserted crop rectangle.

**Verdict: not ready to plan.**
