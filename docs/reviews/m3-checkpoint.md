# M3 checkpoint review

Branch `m3-mcp`, 2026-10-29. Two reviews ran before the owner's checkpoint review, as the plan
requires: a five-axis code review of commits `m2..b1a59de` against SPEC "The MCP server and
client", ADRs 0015 and 0019 and `docs/review-checklist.md`, and a test-coverage review mapping
every M3 acceptance bullet and CLI scenario to its test, both by Claude Code agents, read-only.

Verdicts: the code review requested changes with two must-fix items (the SDK's 16 MiB line
limit made the declared image ceilings unreachable and could leave a large import unanswered; a
byte-index slice of a pack-controlled message could panic) and twelve should-fix items, and
found the pack lifecycle, the result validation and the error-class mapping sound; the test
review found 184 workspace tests and 22 M3 process tests green and stable across three runs and
three oversubscribed runs, named four tests whose names promised more than they proved, and
listed eight tests worth adding. Every must-fix and should-fix item and most proposed tests
landed the same day in one commit; the rest are recorded with owners.

**Benchmark.** `cargo test --workspace`: 47 s cold, 184 tests before the checkpoint (the
process suites compile only under `--features rascal-mask/testkit`, which `just check` and CI
run). `cargo test -p rascal-mask --features testkit --test packs --test serve --test isolation`:
about 15 s warm, 34 tests after the checkpoint. Not a gate.

**A note for anyone running a test binary by hand.** The feature and non-feature builds share
`target/debug/rascal`; whichever built last decides whether `rascal fake-pack` exists. Under
`cargo test` this never matters; a stale `packs-*` binary run directly fails every test at once.

## Findings and dispositions

| # | Finding | Disposition |
|---|---|---|
| 1 | must-fix: the SDK's transport drops any line over 16 MiB and sends nothing back, so a pack answer under the declared ceilings (64 MiB per image) surfaced as a timeout and a `workspace_import` of base64 over about 12 MiB hung the client | fixed: both transports are built explicitly with a 256 MiB line (`rascal_core::pack::MAX_ANSWER_BYTES`); a tool's declared images as base64 plus its envelope must fit it (`check_bounds`); documented in tools.md and SPEC; tests: a 17 MB plane is refused as a validation error, not a timeout, and a 23 MB base64 import through the independent client is answered |
| 2 | must-fix: `&text[..300]` on an initialize error whose text the peer wrote; a multibyte character at the cut panicked before the child was killed | fixed: the cut is at a character boundary |
| 3 | the request identity hashed the arguments with the image's workspace id, so a recording from a shared workspace missed in a fresh run | fixed: the identity is over the pack, the tool, the parameters and the inputs' identities, never a port's id; test: a run whose image was `img-3` replays into a run where it is `img-1` |
| 4 | `pack stored:` is journaled before the commit, so a run that failed after the call left a recording whose outputs do not exist | fixed on the reading side: a replay passes over a stored call whose outputs the recorded run does not hold, and says so; documented; test: a recording with a deleted plane is unrecorded |
| 5 | the restart check compared tool names only, so a restarted pack listing the same names with other ports or bounds passed | fixed: the whole manifests must match; test through the `mutate` fake |
| 6 | a cancelled `serve` call still committed its op while the client was told "cancelled" | fixed: the cancellation sets the op's `Cancel` flag (nothing is published after it) and fails only the pack of the cancelled tool, not every pack; `Server::new` takes the packs and maps tool to pack; tests: the mapping and a sibling pack's in-flight call surviving, and a cancellation through the server with the independent client (`notifications/cancelled`; the journal records the call as `cancelled`, the next call restarts the pack, nothing of the cancelled call is published) |
| 7 | redaction split on spaces and examined the first `=` or `:` only, so a credential in a URL's query string passed | fixed: words split at whitespace and `? & ; ,`; test with a URL and a comma list |
| 8 | `std::env::vars()` panics on a non-Unicode variable in the parent | fixed: `vars_os`, skipping what is not Unicode |
| 9 | two concurrent failers: the second returned at once and its caller could restart the pack while the first was still SIGTERMing the old process | fixed: the slot is held for the whole escalation, so a second failer and any restart wait for it |
| 10 | a dying pack's stderr was lost: the buffer lived with the process and was cleared before the call journaled it | fixed: the buffer lives on the pack; test: `die` writes a line first and the journal has it, redacted |
| 11 | `expect` in production (`Arc::get_mut` in the server builders, the pack's runtime accessor) and an `unreachable!` in validation | fixed: `Server::new` takes everything up front; the accessor returns an error; `filter_map` |
| 12 | `startup_timeout_s` applied to initialize and to the listing separately | fixed: one deadline for both; documented |
| 13 | nits: a non-string `constants` became `""`; `start + PAGE` could overflow on a hostile cursor; `Port::index_of` accepted `plane_01`; a malformed stored note was skipped silently; `tools::call` indexed `packs[0]`; the error-class list in tools.md lacked `cancelled` and `operational`; the fake's `late` behaviour duplicated `exact` with a delay; `Semaphore::new(64)` unnamed; `Vec::remove(0)` as a ring | all fixed (the malformed note is an error naming the line; the fake's `late` is gone) |
| 14 | test review: `a_late_answer_for_a_tombstoned_call_is_ignored` produced no late answer (the fake exited on stdin close before answering) | fixed: the signal-ignoring fake outlives stdin closing, so its answer at 1.5 s is really written into the closed transport; the same test now covers the SIGKILL step, and a second test times the escalation for a pack that exits on stdin close (never signalled) and one that ignores everything |
| 15 | test review: the cancellation test called the hook directly; the `ServiceError::Cancelled` arm and the server's cancellation branch were untested | fixed: the server test in finding 6; the client arm is reached by the same path |
| 16 | test review: the `segment` export test trusted `export --compat`'s own validation | fixed: validated in the test against the copied schema |
| 17 | test review: value-based redaction was never reached through a process | fixed: the `stderr` fake echoes the prompt; a prompt carrying a filler's key value from the environment is redacted in the journal |
| 18 | test review: `concurrent_when_both_agree` had 350 ms of headroom; the timeout and cancellation tests bounded the whole escalation with about 1 s to spare | fixed: 1.5 s calls bounded at 2.6 s; the sequence bounds use the timeout plus two graces |
| 19 | test review: no unit test of the `many` port rules; no replay test for another revision; no startup-timeout test; validator branches for the three size bounds untested; the stale name `invalid_args_is_journaled_and_is_a_usage_error` | fixed: `many_output_ports_take_numbered_artifacts_and_require_at_least_one` in `op_contracts.rs`; replay under `revision = "fake 2"` is unrecorded; the `mute` fake and its test; the three bounds in `client.rs`; renamed |
| 20 | `serve --concurrency n` with two overlapping calls, and the restart path from a `Pack` dropped inside a tokio worker, have no test | recorded: the independent client is one request at a time by design; an overlap test needs the fake to stamp its intervals, which goes with the lab's timing work (M6); the worker-thread drop is exercised by `serve` itself and is covered by review, not by a test |
| 21 | `workspace_import` by `path` reads any file the server process can | recorded, as the spec says; tools.md now says to give a host only the workspace it should touch |
| 22 | the effective stdin-close grace is about 3 s, not 5, because the SDK drains for up to 2 s before closing the transport | recorded: the SIGTERM and SIGKILL steps keep their full grace; the fake that ignores everything is killed on schedule |

## Verification

`just check` green after the fixes (both feature sets, `cargo deny`, the crate graph, packaging,
the clean-room scan, the docs coverage, the hook and check self-tests); CI green on the branch.
