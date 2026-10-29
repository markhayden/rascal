# M6 checkpoint review

Branch `m6-lab`, 2026-10-29. Tasks 6.1 to 6.3 are in (the corpus manifest, protocol and
matrix; `lab run`; `lab pairs` and `lab decode`), built on an example corpus over the fixtures;
task 6.4, the owner's round, waits on owner decision 6. Two reviews ran before the owner's
checkpoint review, as the plan requires: a five-axis code review of commits `f468e8e`,
`184f0c5` and `e5fd85b` against SPEC "The lab", success criterion 6, PLAN tasks 6.1 to 6.3 and
`docs/review-checklist.md`, and a test-coverage review of the same commits together with M5's,
both by Claude Code agents, read-only.

Verdicts: the code review approved with no must-fix, three should-fix items (one of them an
owner decision) and ten nits, and found the cell identity, the dependent-finder rule, resume
and `--redo`, the ledger, the blind page, the decode rules, the draw and the exit codes
sound; the test review found the suites green and named identity as under-proven (prompt,
pack revision, bundle digest and bound inputs never changed under test, no golden), the
resume-after-failure path untested, and the page's JavaScript untested. Every should-fix item
and the test additions landed in one commit the same day; the owner decision is recorded as
decision 9 in `docs/owner-decisions.md`.

**A design fact.** Under the spec's default fractions a group can never be `not_worse`
without being `preferred`: wins and losses sum to the decided pairs, so losses below one
third means wins above two thirds. The decode reports `preferred`, `not_worse`, `worse` or
`inconclusive`, strongest first; `worse` is the implementation's name for the fourth state
the spec's three names do not cover. The test pins the fact over every split of up to twenty
decided pairs. Criterion 6 reads "not_worse on the structured group"; the owner's reading of
it (decision 9) must be recorded before the first round is decoded.

**The private dry run.** The whole lab ran over the owner's three panoramas through the local
LaMa filler in `../rascal-lab/lab-dry/` (nine cells, about eleven minutes in a debug build,
never committed), so 6.4 starts from a working flow.

## Findings and dispositions

| # | Finding | Disposition |
|---|---|---|
| 1 | identities and the usage checks (a pack without `revision`, an unknown input name, a missing input file) ran inside the run loop, after earlier cells had paid for filler calls, and `lab.json`'s `current` map was written only at the end | fixed: two phases, every cell's record and the `current` map written before any cell runs; the input-name and required-input checks moved into `Matrix::validate`; `--redo` with an unknown arm is refused before anything exists; tests: a bogus input name and an unbound required input refused with nothing run |
| 2 | `not_worse` unreachable under the default fractions; `worse` is not in the spec's vocabulary; SPEC and PLAN not amended | recorded as owner decision 9 with three options; the fact is asserted in `lab_decode.rs`; `docs/lab.md` states the precedence |
| 3 | a matrix without `protocol` exited 3 (a serde error) instead of 64 naming the field | fixed: the field defaults to empty and the explicit refusal fires; the test expects 64 and the message |
| 4 | the identity omitted a step's default constant set | fixed: the op's first set is recorded when the step names none; test on the finder recipe's `feather` step |
| 5 | `FillerEntry` carried the configuration name (a rename was a new cell) and not `max_side` or `send_mask` (which change the work) | fixed: the name is out, both fields are in; schema and fixture regenerated |
| 6 | `decode` did not check the assignment against the lab record | fixed: an assignment of another lab or protocol is refused; test |
| 7 | `"rascal-tools"` hardcoded in the descriptor code; the page read `latest` instead of the attempt it shows; `--redo` refused late; page size unannounced | fixed: `provision::PACK_NAME`; the shown attempt's record is read; the refusal is first; the size is printed and documented |
| 8 | `expect` in `fan::Settings::embedded` (inherited from `fan_v1`) | recorded: a test and lab helper over embedded sets; no non-test caller of `fan_thin_v1` |
| 9 | every `lab run` starts every configured pack, resume included | recorded in `docs/lab.md`: a listing is not an external call |
| 10 | docs drift: README's status line said M1; no `corpus`/`lab` commands listed | fixed |
| 11 | test review: identity sensitivity proven for a parameter and the filler revision only; no golden; per-image inputs, pack revision, prompt and bundle digest never changed | fixed: `lab_identity.rs` proves a prompt, a pack revision, the bundle digest and an input file's bytes each make a new cell, a file's name and the spend ceiling do not, the committed fixture's identity recomputes, key order is immaterial |
| 12 | test review: resume after a failed attempt, a failing filler across cells and the exit-3-after-all path untested; the key's absence untested | fixed: the failed attempt runs again as attempt 2 with 1 kept; the failing fake fails every cell with exit 3 after all of them; a hosted filler's key is in the environment during the run and in no file under the lab, the pairs page included |
| 13 | test review: the page's script was not checked beyond the digest; `--arms` refusals and a non-ok current cell untested; validation negatives missing; the decode test's prose | fixed: the page's ids and digest literals and the four buttons per pair are asserted; `--arms` with one, three or equal names refused; a stopped current cell refused; negatives for the corpus, protocol and matrix rules; the decode test is a table |
| 14 | test review: the page's JavaScript never runs under test; the picks format is asserted through the Rust `Picks` type | recorded: the page is accepted by hand in 6.4 (the owner's round writes real picks); the format the script writes is the `Picks` schema |
| 15 | test review: PLAN's verification lines for 6.1 and 6.3 named no test | fixed |

## Verification

`just check` green after the fixes (both feature sets, `cargo deny`, the crate graph, packaging,
the clean-room scan, the docs coverage, the hook and check self-tests, the Python gates); CI
green on the branch.
