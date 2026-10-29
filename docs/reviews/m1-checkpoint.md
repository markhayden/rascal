# M1 checkpoint review

Branch `m1-core`, 2026-10-29. Two reviews ran before the owner's checkpoint review, as the plan
requires: a five-axis code review of commits `m0..m1.2` against `docs/review-checklist.md`,
SPEC "Workspace", ADR 0016 and ADR 0017, and a test-coverage review mapping every M1
acceptance bullet and CLI scenario to its test, both by Claude Code agents, read-only.

Verdicts: the code review requested changes, with one must-fix (repair raced a concurrent
commit) and twelve should-fix items, and found nothing that needed the architecture to change;
the test review found 134 tests green and stable across two runs and listed eight tests worth
adding, the first being crash injection on a commit with a mask directory in flight. Every
must-fix and should-fix item and every proposed test landed the same day in two commits
(`fix(core,ops): M1 checkpoint fixes` and the dash follow-up); four nits are recorded with
their owning tasks.

**Benchmark.** `cargo test --workspace`, warm build, Apple silicon: 18 s and 19 s on two runs,
134 tests before the checkpoint, 144 after. Not a gate.

## Findings and dispositions

| # | Finding | Disposition |
|---|---|---|
| 1 | `repair` never took the writer lock: it could remove a mask directory a concurrent commit had just renamed in, or race it for the next generation number | fixed: `repair_in_place` holds `lock` throughout; test `repair_waits_for_the_writer_lock` |
| 2 | Journal appends were two writes and `seq` was counted before the write, so two shells could interleave a line or share a number; the two-shells test never read the journal | fixed: one write under the journal file's lock, `seq` from the file; the two-shells test asserts 1..=101 unique; a torn last line is skipped on read and rebuilt by repair |
| 3 | An invocation identical to an earlier one dropped its record file but pushed the identity again, so its `seq` and duration were never persisted | fixed: `Generation.records` entries carry `identity`, `seq`, `duration_ms`; `records()` overlays them; dedupe test extended |
| 4 | File names in `mask.toml` were joined unvalidated, so a hand-edited record could read, and a later refinement write, outside the mask | fixed: plain names required before any read or write, by both readers and the writer; test with four escapes |
| 5 | Decoding had no explicit limits; a grey image was converted to RGB by `cut`/`put`'s catch-all arms | fixed: `decode_limited` (32768 a side, 2 GiB) for every file rascal did not write; `Luma8` arms in `cut` and `put`; tests |
| 6 | `bbox_pixels` used floor/ceil of `v * size`, one column too many at `0.55 × 100` and a different rule from the polygon filler | fixed: pixel-centre rule; test against the same box as a polygon |
| 7 | `put` accepted any same-size canvas, from another panorama too | fixed: the canvas frame must be the cut frame of this image and plan; test with a twin panorama |
| 8 | Replay compared the identity string copied from `mask.toml`, so an edited record with intact planes replayed clean | fixed: `record_differs` recomputed from content; `is_clean` requires it false; test |
| 9 | The repair report, the replay report and the inspect JSON derived only `Serialize`, outside the drift harness | fixed: `Report`, `ReplayReport`, `InspectReport` (with `ZoneStatistics`, `InspectProfile`) registered with schemas and fixtures |
| 10 | `dash --serve` had no read timeout, embedded every PNG twice per figure and once per line, byte-sliced identities, and showed any I/O error as an empty journal | fixed: 5 s read and 30 s write timeouts; one asset table keyed by identity; `chars().take`; only not-found is empty |
| 11 | A TOML integer for a `Number` parameter was stored as-is, so `width = 1` and `width = 1.0` hashed differently | fixed: numbers coerce to floats at resolution, defaults included; test |
| 12 | A one-sided luminance assertion | fixed |
| 13 | Profiles hard-coded in recipe validation; the run doc comment contradicted the loop | fixed: `Profile::needs_image`; the comment states the real rule (dependents skipped with a note, independent steps run) |
| 14 | A render could be named `journal.jsonl` or `CURRENT` | fixed: reserved run file names refused; test |
| 15 | `validate_inputs` type errors from the op CLI exited 3 | fixed: usage errors, 64; test |
| 16 | `max_envelope_bytes` was never checked | fixed: an envelope over the bound fails the op |
| 17 | `#[serde(default)] plans` was a pre-1.8a shim | fixed: removed |
| 18 | The cancellation test was sleep-timed; the "second repair finds nothing" assertion was vacuous; no crash test had a directory in flight; the shim modelled a crashed append as total loss | fixed: the flag is set from an `Fs` hook at the first write under the lock; orphan and leftover-staging counts asserted per boundary; `a_crash_at_each_boundary_of_a_mask_commit_leaves_the_mask_whole_or_absent`; the shim truncates |
| 19 | Allocation histories, the run-directory form of `dash`, a torn journal | tests added |
| 20 | nit: `--protect` and `--mask` are read from disk, not committed, so such a run is not self-contained | recorded; M4 (the refinement tasks that take stored masks as inputs) |
| 21 | nit: `Identity::parse_or_hash` is a silent fallback on every `identity()` | recorded; revisit when a second identity scheme or a validation path exists (M3 export) |
| 22 | nit: `Mask::check` walks pixels with `get_pixel` three times; `put` clones the parent and canvas | recorded; 2.1 touches the mask hot path and measures |
| 23 | nit: a `Serialize`-only derive is not flagged by the drift grep | recorded; the three types that mattered are now registered; the grep extension goes with the next boundary type (2.6 measures) |

## What the reviews praised

The commit sequence matches ADR 0017 exactly; the `LIVE` set plus the young-dir guard closes
the create-then-lock window; no `unwrap` outside tests; invariants named on load for every
plane, the profile and the prescriptions; the drift harness is hard to sidestep; the dash has
one route, constant headers, loopback only and one escape choke point; every primitive is
tested against brute force; the rendering profile table is exhaustive; by-name and by-path
recipe runs are compared whole-line.
