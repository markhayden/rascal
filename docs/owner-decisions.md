# Owner decisions

Decisions only the owner can make, each dated when made and tied to the task that needs it.
The plan's "Owner decisions" table is the index; this file holds the answers.

| # | Decision | Needed before | The gate it must clear | Status |
|---|---|---|---|---|
| 1 | Confirmation of no conflicting obligation | the bootstrap commit (pre-M0) | no commit exists on `main` until the dated statement is in this file | made 2026-10-28 |
| 2 | The forbidden-word list location (`RASCAL_FORBIDDEN_WORDS`) | task 0.1's first commit | the clean-room scan runs locally on every commit from 0.1 on; the list never enters the tree | made 2026-10-28 |
| 3 | A real local filler and a real wrapped image on the owner's machine | task 2.8's by-hand run | M2's checkpoint needs one real heal through a real filler, viewed by the owner | made 2026-10-29 |
| 4 | The MCP transport (an MCP crate or an in-house JSON-RPC implementation) | task 3.1, by ADR 0015 | no MCP code is written until the ADR is accepted | made 2026-10-29 (ADR 0015) |
| 5 | The segmenter, its branch (A or B) and the export's redistribution terms | task 5.2, decided in task 4.5 by ADR 0018 | M5's `segment` task does not start, and success criterion 5 or 5b is not fixed, until the ADR is accepted | made 2026-10-29 (ADR 0018) |
| 6 | The private corpus: location, first contents, the sky and structured groups with at least `min_decided` images each | task 6.1 | no lab matrix is written until the corpus manifest verifies | made 2026-10-29: a generated corpus |
| 7 | The path fan before or after v1 | after M7, unless task 4.3 shows the need | v0.1.0 ships without it unless the owner decides otherwise during M4 | made 2026-10-29: after v1 |
| 10 | Scaled windows in v1, as a filler option (`fill_max_side`), rather than the later milestone the spec named: the lab showed LaMa cannot carry a wide band at full resolution | the LaMa scale option, after seam-03 | the spec's "later" list and the fillers' contract change; the option is journaled and in the cell identity | made 2026-10-29 |
| 11 | When `v0.1.0` ships: not at the end of the plan but when the owner calls the work ready, after the improvement rounds (the band around solid objects first); the model files are published now | the `v0.1.0` tag (task 7.2's last step) | the release gate's criteria are met and recorded; the tag waits on this decision | made 2026-10-29 |
| 9 | The `not_worse` rule: under the spec's default fractions (`preferred` at wins ≥ 2/3, `not_worse` at losses < 1/3) a group can never be `not_worse` without being `preferred`, since wins and losses sum to the decided pairs; the implementation reports `preferred`, `not_worse`, `worse` or `inconclusive` with the strongest first. Options: (a) read criterion 6's "not_worse on the structured group" as "preferred or not_worse" (what `docs/lab.md` says now); (b) count `same` as not-a-loss and define `not_worse` over all judged pairs; (c) give `seam-01`'s protocol fractions with a gap (for example `preferred = [3, 4]`) | task 6.4 | the structured group's gate (G16) reads the outcome literally, so the owner's reading must be recorded before the first round is decoded | made 2026-10-29: option (a) |

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

## 8. Merging and pushing

2026-10-28, at the M0 checkpoint. After the owner has reviewed and approved a milestone, the
agent merges it into `main`, runs `just check` on the merge commit, pushes `main` and the tags,
waits for CI on both platforms, and tags the checkpoint. `main` is never force pushed.

Widened 2026-10-29: the agent merges at will during development, without the owner's approval,
as long as every check and CI run is green. A merge is a merge commit that has passed `just
check` (with the clean-room scan) locally and CI on both platforms; milestone tags still mark a
complete milestone. The owner reviews the checkpoint documents at their own pace.

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
2026-10-29. The owner signs branch A with OWLv2 base and MobileSAM, both exported to ONNX, as
the survey recommends; ADR 0018 records it and task 5.2 starts. The redistribution terms are
the Apache-2.0 tags read on the day of the export, recorded in the bundle's notice by 5.2.

## 6. The corpus

2026-10-29. The owner chose to generate the corpus rather than use their photographs: twenty
equirectangular panoramas made with `gemini-3.1-flash-image` from the prompt list the agent
wrote (nine sky, eleven structured; two of the sky images are restyles of one composition),
reviewed and kept by the owner, each with its prompt and provenance in the manifest's licence
note. The files live outside the repository in the owner's generator's output directory; the
manifest, the protocol and the matrix are committed under `labs/seam-01/`. The images were
requested as 21:9 and resized to 2:1, so their spherical geometry is approximate; the wrap
seam is real and unhealed, which is what the round judges. The first protocol sets
`min_decided = 8`, under the spec's default of ten, because the sky group holds nine images.
More scenes can be generated as the rounds need them.

## 7. The path fan

2026-10-29. After v1. `fan` keeps its line form through v0.1.0; a path form is a later
milestone if the lab or a use case asks for it.

## 9. The `not_worse` rule

2026-10-29. Option (a), left to the agent by the owner: criterion 6's "not_worse on the
structured group" reads as "preferred or not_worse", which `preferred` satisfies. The decode
reports the strongest outcome first, and the protocol's default fractions stay as the spec
wrote them; a round that wants a distinct `not_worse` band gives its protocol fractions with a
gap (option c) and says so in its results document.

## 10. Scaled windows in v1

2026-10-29. After seam-02 showed LaMa smears a wide band at the panoramas' resolution and the
owner asked for tools that clean up a filler's shortcoming, the owner agreed to the plan that
put a LaMa scale option before the release ("sounds like a plan. keep trucking"). The spec had
scaled windows as a later milestone; it is now a filler entry's `fill_max_side`: the request
is sent downscaled to that longest side and the fill scaled back to the support, said in the
journal and carried in the lab's cell identity. Whether it helps is a lab question.

## 11. The release waits for readiness

2026-10-29. With every plan task built and the seven criteria recorded, the owner chose to
publish the model files (the `models-2026-10` release) but to hold the `v0.1.0` tag: "maybe
we dont need to release till we are super ready", and to circle back first to the improvements
noted during the lab rounds, starting with the fan's band swallowing solid objects on smooth
surfaces (`tasks/todo.md`). The release machinery is complete and proven (`docs/results/suite-timing.md`);
pushing the tag is the one step left, on the owner's word.
