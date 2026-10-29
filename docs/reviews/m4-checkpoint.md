# M4 checkpoint review

Branch `m4-refine`, 2026-10-29. Two reviews ran before the owner's checkpoint review, as the plan
requires: a five-axis code review of commits `m3..8df007e` against SPEC "Operations", "The mask
record", "Recipes", the M4 tasks in PLAN.md and `docs/review-checklist.md`, and a test-coverage
review mapping every M4 acceptance bullet and CLI scenario to its test, both by Claude Code
agents, read-only.

Verdicts: the code review requested changes with one must-fix (closing eroded from the image
border, so an object cut off by the frame edge lost a strip of mask and a thin strip along the
border vanished), seven should-fix items (bounded cost and bounded inputs among them) and nine
nits, and found the postconditions, the inversion geometry and the many-port rules sound; the
test review found 252 workspace tests green and stable across five runs of the recipe suites,
named six tests whose names promised more than they proved, and listed eight tests worth adding.
Every must-fix and should-fix item and the proposed tests landed the same day in one commit; the
rest are recorded with owners.

**Benchmark.** `cargo test --workspace --features rascal-mask/testkit`: 90 s warm-ish, 252 tests
before the checkpoint, 268 after. `cargo test -p rascal-mask --features testkit --test recipes`:
about 5 s, dominated by two heals through the fake. `scripts/acceptance.sh`: about four minutes
locally, almost all of it the install from the packaged crates. Not a gate.

**The owner's item.** Decision 5, the segmenter, is surveyed in `tools/python/MODEL.md` and awaits
the owner's signature; ADR 0018 follows it. M5's `segment` task does not start until then. It
does not gate this merge.

## Findings and dispositions

| # | Finding | Disposition |
|---|---|---|
| 1 | must-fix: `close` eroded with outside-the-image counting as unset, so a shape against the border lost `radius_px` of core along it and a 2 px strip along the top closed to nothing, against the catalogue's "extent unchanged" | fixed: morphology runs through the exact distance transform with two erosion conventions, outside unset for `shrink` (documented) and outside set for `close`; tests: a corner box and a border strip survive closing unchanged, `shrink` erodes from the border, the transform agrees with the offset disc at five radii |
| 2 | `grow`, `shrink`, `close`, `subtract` accepted `px` up to 1e4 with a cost of pixels × disc; one call could tie up `serve` | fixed by the same change: linear in the plane whatever the radius |
| 3 | a stroke's `radius_px` had no upper bound; `tighten` walked `2r + 1` samples per path sample, so a location file with `1e12` hung the op | fixed: `Location::validate` refuses a width or radius above the frame's diagonal; `tighten` clamps its radius to the diagonal as well; tests through the op and the validation |
| 4 | `tighten` read the perpendicular at 1 px steps with an odd last step when `2r` was not an integer, skewing the end derivative | fixed: `⌈2r⌉` equal steps symmetric about the path, the derivative per pixel; SPEC amended |
| 5 | `tighten` reported a stroke/image frame mismatch as operational; `select` reported a plane of another size as operational | fixed: both `invalid_args` |
| 6 | the run-time many expansion ignored whether the port was `many`, so a recipe declaring `protect_0` beside an optional `protect` fed a single port a `<port>_0` | fixed: expansion only for a declared many port and only from a step's output; test with the hostile recipe, which now passes the mask through |
| 7 | a manifest could declare a many `plane` beside a single `plane_3` | fixed: `Manifest::check` refuses a port named as another port's numbered artifact; test |
| 8 | nits: `select` rebuilt its port, `tighten`'s bounds were a match in a closure, `subtract` assembled its output twice, three ops published `mask` where the others publish `<op>.mask`, the cap and tube normals differ at a sharp bend, `subtract`'s row in tools.md contradicted the pass-through | all fixed (the normals note is in the module doc) |
| 9 | test review: `edges_within_a_pixel_*` were statistical; the untightened stroke was compared to the dilation function, not the `rasterize` op; `remove` without protect checked a journal string only; the journal form of the refinement report and the CLI form of the M4 ops were untested; no `heal remove`; no JPEG input; no replay of the many port through a real pack or over MCP; no constant-set tripwire | fixed: the worst extent is bounded at two pixels and the op-to-op equality is asserted; `grow` through the CLI with the journal's `blend_cleared`/`profile_cleared` and `inspect --json`; `heal remove --location` through the exact fake on a JPEG of the chair blob, reading back the untouched outside and the repainted shadow; `select` over twelve real pack planes on the CLI (`plane_10` after `plane_1`, `--keep 1,10`, `--exclude`) and as an array of ids over `serve`; `ramp → subtract(location) → apply` proving the protect is honoured; `subtract` from another frame and a grown box at the edge; `invert` through a crop of a cut to both targets; the optional-input validation rule; `tighten-v1` pinned |
| 10 | test review: `UPDATE_FIXTURES` set in CI would rewrite goldens and pass; the two-pass test ignored the `mask:` line it captured | fixed: the recipe goldens refuse the variable under `CI`; the captured line is asserted against the listing |
| 11 | test review: the acceptance script installed with `--locked` but no lockfile beside the extracted crate, discarded five of six masks unread, ran the docs check against the checkout's binary, interpolated ids into Python source, did not say it needs `python3` or that the client is built from the checkout; the plan's "JPEG fixtures" are PNG | fixed: the workspace's `Cargo.lock` is copied beside the crate, every mask is read back with `inspect --json`, `RASCAL_BIN` points the docs check at the installed binary, ids go in as arguments, the header says so; a JPEG input runs in `tests/recipes.rs` |
| 12 | survey honesty: the plan says "cites each model's licence text" where tags and pages were read; Hub tags are uploader-declared; CLIPSeg's count rounded up | fixed in `tools/python/MODEL.md`: the header says what was read, the mirror-sourced row says secondary, "about 0.15 B" |
| 13 | `--mask`'s derived-frame check compares frame ids, and raw frame ids carry no image, so a cut mask from another same-size panorama in the same workspace passes | recorded, by the frame model's design (frames carry sizes and projections); docs/recipes.md says to give the mask the run that made it; the preservation check in `heal` still holds |
| 14 | `stroke_core` and `dilate_path` test every pixel in the bbox against every segment and cap | recorded: a per-segment rasterisation is the fix when the lab's timing run (M7) shows it matters |
| 15 | the plan's `ramp → shrink → feather → apply` bullet is only provable in its degenerate form, since a refinement drops the prescription by design | recorded: the test says so in its name; the plan's wording stands as the postcondition it proves |

## Verification

`just check` green after the fixes (both feature sets, `cargo deny`, the crate graph, packaging,
the clean-room scan, the docs coverage, the hook and check self-tests); `scripts/acceptance.sh`
green locally and in the package workflow; CI green on the branch.
