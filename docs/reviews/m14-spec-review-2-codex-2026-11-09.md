| # | Round 1 finding | Resolution | Why |
|---|---|---|---|
| 1 | Parity cannot be computed from digests | **Partly** | Reference planes, matching and tolerances are defined; frozen test settings and empty-versus-empty mask handling remain unspecified. |
| 2 | External weights escape integrity checking | **Resolved** | The installation manifest covers external data and ancillary files, with containment checks before loading. |
| 3 | Credential route conflicts with process/session boundaries | **Resolved** | Installation has a separate process, credential-free dependency provisioning, and an explicit session restart after credential entry. |
| 4 | Parity mistaken for trust | **Resolved** | Upstream inputs are pinned, unsafe loading is excluded, converter provenance is recorded, and parity is explicitly a correctness gate. |
| 5 | Converter unavailable to installed users | **Partly** | Converter resources now ship in the wheel, but packaging the input images and acceptance without a checkout remain unspecified. |
| 6 | Per-choice readiness contradiction | **Partly** | The default/selected-choice distinction is defined, but excluding an unavailable configured default from `enum` leaves its description and readiness unresolved. |
| 7 | Installer lifecycle undefined | **Partly** | Controls and attachment behavior exist; `describe` has no terminal outcome/error field despite promising reconnect reconciliation. |
| 8 | Nonatomic installation and loaded-model coordination | **Partly** | Locks and identity-based loading help, but renaming the old directory aside before publishing the new one leaves a crash window that staging cleanup cannot recover. |
| 9 | Selection disconnected from recipes/retries/public surface | **Partly** | Binding, retries and public scope are settled; step-override precedence and the exception to unconditional model forwarding for third-party packs remain unclear. |
| 10 | Adapter inference behavior unspecified | **Resolved** | The draft defines common ordering and pairing plus model-specific scoring, masks, crops and parameter applicability. |
| 11 | String model identity used as a measurement | **Resolved** | Identity moves to resolved parameters and invocation evidence; measurements remain numeric. |
| 12 | Installation identity and replay missing | **Partly** | Run evidence and recorded replay are covered; adapter version and installation-sensitive estimate binding remain unspecified. |
| 13 | Fetch/bundle membership contradicts optional models | **Resolved** | Model selection replaces fetch-all, and bundle contents follow the catalogue’s permissive allowlist. |
| 14 | Licence conclusion too broad | **Resolved** | The policy is narrowed, applicable terms and licence provenance are acknowledged, and owner approval remains an explicit gate. |
| 15 | Acceptance does not prove real adapters/offline runtime | **Resolved** | Gated real-model tests cover shipped adapters, corpus parity, non-square inputs, queries and retries offline. |
| 16 | Catalogue displays lack semantics | **Resolved** | Metrics, timing context, missing measurements, platform support and separate resource figures are defined. |
| 17 | Premature generic installer/component framework | **Partly** | The component catalogue is flattened, but generic installables still explicitly anticipate future fillers. |

The following findings are new in draft 2.

1. **Major — “Explicit” unsupported settings cannot be distinguished after existing default resolution.** [SPEC.md:1102](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1102)  
   The new rule rejects explicitly supplied `pad`, `outline` and `min_cover` for SAM 3 while ignoring their defaults. However, rascal fills defaults before execution ([SPEC.md:677](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:677)), and the pack client forwards the resolved parameter map. Consequently, an ordinary SAM 3 call reaches the pack carrying all three keys, indistinguishably from explicit overrides. Presence-based validation rejects normal calls; value-based validation silently accepts explicit overrides equal to defaults.  
   **Fix:** Validate applicability against the original supplied parameters before default expansion, then resolve and forward only the selected adapter’s applicable settings. Record that same effective map. Test both an ordinary SAM 3 recipe call and an explicit unsupported argument equal to its usual default.

2. **Major — The proposed invocation field violates the contract’s extension shape.** [SPEC.md:1289](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1289)  
   `invocation.effective.segmenter` is not an allowed field. [CONTRACT.md:775](/Users/markhayden/go/src/github.com/markhayden/table-manners/CONTRACT.md:775) requires the estimate’s effective object, with tool-specific fields under `x-<tool>`; its schema rejects other properties. The listed 0.6.0 changes do not amend this object, and Wafflery is instructed to read the incompatible path.  
   **Fix:** Use `effective["x-rascal"].segmenter = {model, installation}` in both estimates and invocations, and update Wafflery’s lookup. Reuse the existing extension mechanism.

3. **Minor — The new adapter record loses evidence required by “Tiny unchanged.”** [SPEC.md:1158](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1158)  
   `{query, score, region, plane}` cannot carry Tiny’s selected outline index, predicted mask IoU, retry-attempted flag or retry-kept flag. Those measurements remain required by [SPEC.md:825](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:825) and are emitted from the existing `Traced` record. A wrapper receiving only the proposed instance record cannot preserve them.  
   **Fix:** Allow optional per-instance diagnostics alongside the four common fields. Preserve Tiny’s existing measurements and omit inapplicable diagnostics for other adapters.

**Verdict: not ready to plan.**
