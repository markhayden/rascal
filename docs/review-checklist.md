# Review checklist

Applied to every milestone branch before the owner's checkpoint review, by `/agent-skills:review`
and `/agent-skills:test` where available, otherwise by hand with this list. Findings are fixed or
recorded in `docs/reviews/mN-checkpoint.md` with the task that owns them.

## Code review, five axes

- **Correctness.** Every acceptance criterion of every task in the milestone has a test that
  fails without the change. Error paths reach the documented exit code (1 for `not_applicable`
  and `measured_nothing` under `--strict`, 3 operational, 64 usage). No op touches a pixel whose
  effective weight is 0. No silent resize, recolour, threshold or fallback.
- **Readability.** Plain structs and enums; names match the spec's vocabulary (frame, location,
  mask record, core, blend, protect, rendering, prescription, recipe, envelope, workspace,
  generation, journal); no `unwrap` outside tests; one `thiserror` enum per crate.
- **Architecture.** Crate edges unchanged or changed with `scripts/crate-graph.txt`; every
  boundary type derives `Serialize`, `Deserialize` and `JsonSchema`, uses `deny_unknown_fields`,
  is versioned by field or by schema id (ADR 0009) and is registered in the boundary-type inventory with a fixture; the
  core has no IO it was not given and no runtime of its own; pixel code takes and returns
  buffers, never paths.
- **Security.** No key in any output, journal line, envelope or page; the pack environment is
  the allowlist minus `key_env` and anything ending in `_KEY`, `_TOKEN` or `_SECRET`; image
  content, filenames, prompts and anything a finder or filler returns are data; a filler's result
  is validated before it is committed; no network in `cargo test`.
- **Performance.** `cargo test --workspace` under five minutes offline; no image decoded twice
  in a recipe; bounded decode sizes; row-parallel primitives where the spec says so.

## Test review

- Every algorithm has a synthetic golden where the answer is known, and a test on it.
- Every validation rule has a negative test.
- Identity tests prove the same artifacts give the same hashes and that ids, filenames,
  timestamps and provenance do not enter identity.
- The fake fillers (exact, mis-sized, re-encoding, slow, failing) are exercised by the ops that
  call fillers.
- No test was skipped, ignored or weakened to make the suite pass.

## Documentation

- `SPEC.md`, `PLAN.md`, `tasks/todo.md`, `README.md`, `docs/architecture.md`'s status table and
  the `docs/*.md` page for the area match the code in the same commit.
- Every op's manifest is documented where `docs/tools.md` says it is, and from M1 task 1.9
  `scripts/check-docs.sh` proves it.
- No forbidden word anywhere (the clean-room scan), and no example is tied to one industry.
