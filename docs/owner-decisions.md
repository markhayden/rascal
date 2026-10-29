# Owner decisions

Decisions only the owner can make, each dated when made and tied to the task that needs it.
The plan's "Owner decisions" table is the index; this file holds the answers.

| # | Decision | Needed before | The gate it must clear | Status |
|---|---|---|---|---|
| 1 | Confirmation of no conflicting obligation | the bootstrap commit (pre-M0) | no commit exists on `main` until the dated statement is in this file | made 2026-10-28 |
| 2 | The forbidden-word list location (`RASCAL_FORBIDDEN_WORDS`) | task 0.1's first commit | the clean-room scan runs locally on every commit from 0.1 on; the list never enters the tree | made 2026-10-28 |
| 3 | A real local filler and a real wrapped image on the owner's machine | task 2.8's by-hand run | M2's checkpoint needs one real heal through a real filler, viewed by the owner | made 2026-10-29 |
| 4 | The MCP transport (an MCP crate or an in-house JSON-RPC implementation) | task 3.1, by ADR 0015 | no MCP code is written until the ADR is accepted | made 2026-10-29 (ADR 0015) |
| 5 | The segmenter, its branch (A or B) and the export's redistribution terms | task 5.2, decided in task 4.5 by ADR 0018 | M5's `segment` task does not start, and success criterion 5 or 5b is not fixed, until the ADR is accepted | survey drafted 2026-10-29 (`tools/python/MODEL.md`); awaiting the owner's signature |
| 6 | The private corpus: location, first contents, the sky and structured groups with at least `min_decided` images each | task 6.1 | no lab matrix is written until the corpus manifest verifies | pending |
| 7 | The path fan before or after v1 | after M7, unless task 4.3 shows the need | v0.1.0 ships without it unless the owner decides otherwise during M4 | pending |
| 9 | The `not_worse` rule: under the spec's default fractions (`preferred` at wins ≥ 2/3, `not_worse` at losses < 1/3) a group can never be `not_worse` without being `preferred`, since wins and losses sum to the decided pairs; the implementation reports `preferred`, `not_worse`, `worse` or `inconclusive` with the strongest first. Options: (a) read criterion 6's "not_worse on the structured group" as "preferred or not_worse" (what `docs/lab.md` says now); (b) count `same` as not-a-loss and define `not_worse` over all judged pairs; (c) give `seam-01`'s protocol fractions with a gap (for example `preferred = [3, 4]`) | task 6.4 | the structured group's gate (G16) reads the outcome literally, so the owner's reading must be recorded before the first round is decoded | pending (raised by the M6 review, 2026-10-29) |

## 1. No conflicting obligation

2026-10-28. The owner confirms that no conflicting obligation applies: all of this work is the
owner's own, from the owner's own ideas and from extensions of earlier work the owner also owns
and built from scratch.

## 2. The forbidden-word list

2026-10-28. The list lives outside the tree at `~/rascal/forbidden-words.txt`, named by
`RASCAL_FORBIDDEN_WORDS` in the owner's shell. It is short and literal: a handful of names that
must never appear in the repo. The clean-room scan in `just check` reads it when set, matches
case-insensitively, and passes trivially when unset (as in CI).

The owner also asks for breadth of framing. Wrapped images and 360 panoramas are one supported
case among many, not the project's identity. Examples, fixtures and docs draw on varied subjects
(stitched scans, product shots, archival prints, interiors, landscapes, screens and signs) rather
than any single industry.

## 8. Merging and pushing at checkpoints

2026-10-28, at the M0 checkpoint. After the owner has reviewed and approved a milestone, the
agent merges it into `main`, runs `just check` on the merge commit, pushes `main` and the tags,
waits for CI on both platforms, and tags the checkpoint. `main` is never force pushed, and no
milestone is merged without the owner's approval.

## 3. A real filler and a real image

2026-10-29. Three of the owner's own 360 panoramas (5760×2880) went through `heal seam` with
Bria Eraser on fal as a command filler and with LaMa run locally on Metal; the owner judged the
results by eye ("10 out of 10"). The setup lives outside the repository. Two observations were
recorded in `tasks/todo.md`: output writing dominates a run's time, and LaMa wants a narrower
band in smooth sky (`wide_max = 0.05`), a per-filler constant set for the lab.

## 4. The MCP transport

2026-10-29. ADR 0015: the official `rmcp` crate pinned at 3.8.0, revision 2025-11-25 only, with
the independent minimal client as the in-house check on it.

## 5. The segmenter (awaiting signature)

The survey is `tools/python/MODEL.md`. Its recommendation is branch A with OWLv2 base
(Apache-2.0) for text to boxes and MobileSAM (Apache-2.0) for boxes to planes, both exported to
ONNX for the bundle, with SAM 2.1 Hiera-Tiny as the drop-in if MobileSAM's masks prove loose.
The owner signs here with the branch, the model pair and the date; ADR 0018 follows.
