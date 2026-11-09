| Open item | Resolution | Evidence |
|---|---|---|
| R1 #7 — Successful uninstall outcome | **Resolved** | `last_outcome` now separates `action: install/uninstall` from `state: succeeded/failed/cancelled` ([SPEC.md:1363](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1363)). |
| R3 #1 — Installation changes during a run | **Partly** | Installation pinning and retained generations cover retries, but acquiring the hold still races deletion; the lab also explicitly bypasses pinning. See new findings 1–2. |
| R3 #2 — Reinstalling an identical installation | **Resolved** | A verified existing generation is reused, staging is discarded, and equal identities are excluded from previous-generation deletion ([SPEC.md:1236](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1236)). |
| R3 #3 — SAM 3 checker crop padding | **Resolved** | Host-side `crop_pad` is defined, recorded, excluded from unsupported pack arguments, and covered by a crop-rectangle assertion ([SPEC.md:1093](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1093), [SPEC.md:1435](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1435)). |

New findings only:

1. **Major — Hold acquisition is not atomic with installation deletion.** [SPEC.md:1244](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1244)  
   `get` and `remove` take `.lock`, but run admission only reads the identity and writes `.held/<run id>`. A remover can observe no hold and delete the generation between those operations—or begin deleting just before the hold appears. The new retention mechanism therefore does not guarantee that an admitted run owns a usable installation.

   **Concrete fix:** Under the same cross-process lock used by deletion, resolve `current`, check the generation, and register the hold before admitting the run. Require deletion’s hold check and deletion to occur under that lock. Add a deterministic race test that pauses admission between identity resolution and hold creation.

2. **Major — The lab exception can put results under the wrong installation’s cache key.** [SPEC.md:1207](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1207)  
   Lab calls are explicitly allowed to omit `installation` and use `current`. However, [SPEC.md:1331](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1331) includes installation identity in the cell key, which is computed before execution ([SPEC.md:1667](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1667)). A reinstall between key computation and inference can record installation B’s output under installation A’s key.

   **Concrete fix:** Require lab cells to acquire a hold before computing their key and pass that exact installation to every pack call. Keep the `current` fallback only for ad hoc calls without a precomputed installation-bound identity. Test replacement between cell-key computation and inference.

3. **Minor — An existing but invalid generation still makes publication fail.** [SPEC.md:1236](/Users/markhayden/go/src/github.com/markhayden/rascal/SPEC.md:1236)  
   The new branch handles an existing directory that verifies. If that directory exists but a file is missing or corrupt, it falls through to renaming staging onto the existing nonempty directory. That rename fails, so fetching verified replacement bytes cannot repair the installation.

   **Concrete fix:** Define a separate occupied-but-invalid branch: refuse `busy` while the generation has live holds; otherwise retire the damaged directory under the lock before publishing verified staging. Add an installed-file corruption followed by `get` acceptance case.

**Verdict: not ready to plan.**
