# M2 checkpoint review

Branch `m2-seam`, 2026-10-29. Two reviews ran before the owner's checkpoint review, as the plan
requires: a five-axis code review of commits `m1..m2.1` against SPEC "Operations", "Recipes",
"Fillers" and "Measures", `docs/review-checklist.md` and the M1 review, and a test-coverage
review mapping every M2 acceptance bullet and CLI scenario to its test, both by Claude Code
agents, read-only.

Verdicts: the code review requested changes narrowly, with no must-fix, six should-fix items
and eight nits, and found the heal graph matching the spec line for line; the test review found
170 workspace tests and 51 testkit tests green and stable across three runs, and listed eight
tests worth adding, the first being a negative test of the preservation check through a cut
and an offset crop. Every should-fix item, every proposed test and most nits landed the same day
in one commit; the rest are recorded with owners.

**Benchmark.** `cargo test --workspace`: 24.6 s warm, 170 tests before the checkpoint.
`cargo test -p rascal-mask --features testkit`: 30.0 s warm, 51 tests, of which `tests/heal.rs`
is about 14 s. Not a gate.

**The owner's item.** The plan's checkpoint asks the owner to run one real panorama through a
real filler by hand and look at it in a 360 viewer. Open at the time of writing; it does not
gate the merge.

## Findings and dispositions

| # | Finding | Disposition |
|---|---|---|
| 1 | A failed filler call lost its record: heal rebuilt a stub with status −1 and 0 ms exactly when the status mattered | fixed: `Filler::call` returns a `CallOutcome` (the fill or the error, and the record either way); the stub is gone; tests read the record of a mis-sized, failing and HTTP 500 call |
| 2 | The HTTP answer was buffered unbounded before the size check, and decoded under the global limit | fixed: a declared or streamed body over eight bytes per request pixel plus 1 MiB is refused; the header's dimensions are compared before any pixel memory is allocated; the decode is bounded to the request's size |
| 3 | Replay matched on request bytes only, so another filler's pixels could be reused and relabelled | fixed: the stored fill carries the filler's name and `revision` and a replay needs all three; the replayed artifact's record names the run it came from; the recorded fill's dimensions are asserted; test |
| 4 | `--replay` could not find a run that used `--workspace` | fixed in the rule and the message: `--replay` takes the directory holding the journal; a directory without one is a usage error naming the rule; documented |
| 5 | The preservation check allocated weights as `f32` and two RGB copies, and defaulted a missing crop bbox or plan silently | fixed: one `bool` per raw pixel per plane, RGB views without a copy, errors for a crop without a bbox, a cut chain without a plan, a mask that descends from another raw frame or maps outside it; unit tests through a cut and an offset crop reject a changed unreached or protected pixel |
| 6 | `chart_svg` interpolated the title and unit into markup unescaped | fixed: both escaped |
| 7 | nit: a command with no `{out}` failed only at call time; `send_mask` on a mask-input profile was accepted | fixed: config-time checks for `{image}`, `{out}`, `{mask}` against the profile, and `send_mask`; tests |
| 8 | nit: an error body excerpt could echo the bearer key | fixed: the key's value is scrubbed from the excerpt |
| 9 | nit: `paste` promoted a grey base to RGB | fixed: a `Luma8` arm |
| 10 | nit: `fan` kept `thin` fractional and admitted a one-column window, against the spec's wording | the spec's wording was the loose end: amended to say the three window sizes are rounded and `thin_px` is not, and that a window the frame clips below two columns (or below the one it asked for) is not applicable |
| 11 | nit: the Sobel plane was computed twice when two measures needed it; `span()` used a different column rule from the core's; `expect` in production | fixed: one plane per op, pixel-centre spans, no `expect` |
| 12 | nit: `measure` and `paste` stage under a predictable temp name; the measure CLI has no `--line` | recorded; the staging dir is removed after the call and holds no secret; `--line` goes with the first recipe that produces a line the measures need (2.8's heal passes the profile's column) |
| 13 | nit: three doc lines stale (architecture row, diagnostics "arriving later", the plan's heal scenario); a stopped heal skipped the tables and the page | fixed: all three; a stopped heal writes the tables and the page |
| 14 | nit: the fake HTTP server serves one request at a time and needs `Content-Length` | recorded; test-only |
| 15 | test: preservation asserted with a fixed column bound rather than the mask's `w = 0` set | fixed: the test maps the band mask to raw coordinates itself and asserts equality on that set, in both window cases |
| 16 | test: the windowed case had a zero crop offset, so the offset arithmetic was never exercised | fixed by the unit tests in 5 (an offset crop over a cut) |
| 17 | test: no HTTP heal with an image-input profile, `send_mask` or a prompt; the filler span's fields and the too-small `max_side` path untested | fixed: `heal_over_http_with_an_image_input_profile_send_mask_and_a_prompt`, `a_max_side_under_the_support_stops_at_window_and_still_writes_the_page` |
| 18 | test: a replay with a changed request or another filler untested | fixed: `a_replay_with_a_changed_request_or_another_filler_calls_the_filler` |
| 19 | test: the measures panel had no test; the crop identity check was tautological; `{prompt}`, a missing `{out}` and a truncated PNG untested | fixed: all four tests added |
| 20 | test: `timeout_s = 1` for every behaviour was the likeliest future flake | fixed: 10 s for all but `slow` |

## What the reviews praised

Constant sets as layered defaults with the name recorded in the resolved parameters; the crop
frame reused by identity with `n` outside it; one effective weight shared by `paste`, `apply`
and `drift`; the exact Sobel; the key test grepping every file under `--out` and the trace
log; `window_bbox` keeping the support inside under every cap; the graph proved literally in
the journal.
