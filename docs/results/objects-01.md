# objects-01: the object cap (fan-v2) against fan-v1 under Bria

The first round on the mask builder after the plan (the band swallowing solid objects,
`tasks/todo.md`), run on 2026-10-29 and judged on 2026-10-30 on the seam-03 corpus: the ramp
band from `fan-v2`, whose opening stops short of a solid object and goes thin where one
straddles the line (SPEC.md "fan", step 6b), against `fan-v1`, both filled by Bria Eraser.
**On the structured group the owner preferred fan-v2 ten to three on the first reading
(`preferred`) and six to two on a second, more conservative reading (`inconclusive` by
count), with most pairs `same`; on the sky group fan-v2 was `worse`, three to seven with
eleven `same`.** The sky losses are the cap's own doing: it took cloud edges and horizon
silhouettes for objects and chopped the band into hard notches and stripes. The structured
page showed the other two faults: an object the seam runs through should be removed whole,
not striped, and the cap's edges step where they should taper.

## The round

| | |
|---|---|
| corpus | `labs/objects-01/corpus.toml`, the seam-03 corpus (47 panoramas) |
| protocol | `labs/objects-01/protocol.toml`, digest in `lab.json`; arms `ramp-v2-bria` (`fan.constants = "fan-v2"`) and `ramp-bria` (`fan-v1`), both Bria Eraser; `min_decided = 10`; seed 20261102 |
| run | 94 cells, 94 ran, 0 stopped, 0 failed; 94 Bria calls, spend $3.76 |
| pages | one per group; the structured page judged twice (picks `(11)` and `(12)`), the sky page once (`(13)`) |
| the cap's reach | it engaged in every cell: a median of 850 rows capped on structured cells and 552 on sky cells; no pair's two masks were identical |

## The decode

```
structured, first reading
group          pairs decided  wins losses  same neither  outcome
structured        26      13    10      3    11       2  preferred

structured, second reading
structured        26       8     6      2    16       2  inconclusive

sky
sky               21      10     3      7    11       0  worse
```

Wins are `fan-v2`'s. The two structured readings agree on every decided pair; the second
calls more of them `same`. On sky, fan-v2 won the winter sky, the first sky and the mountain
lake and lost the coastal headland, the volcanic coast, the snowy field, the desert basin and
three of the plain skies. Wins: the workshop, the greenhouse, the conference room, the modern library, the
living room, the studio, the kitchen, the dining room, the lobby, the attic (first reading).
Losses: the parking garage, the sunroom, and the office break room. `neither`: the museum
gallery and the harbour promenade.

## What the cells show

- **On sky, the cap fires on clouds.** Cloud edges and the horizon's silhouettes clear the
  step threshold (a median strong step of 12 to 25 levels against a sky whose median step is
  1 to 2), so the cap notched and striped the band through the sky, where fan-v1's wide ramp
  was right. What separates them from a fixture or a shelf is not strength but persistence:
  measured on the fan-v1 cut frames, a cloud edge chains down the frame for a handful of rows
  (the 90th percentile 7 to 23 rows) while the living room's mirror and the studio's shelves
  run for 200 to 550 rows; the break-room light runs 19.
- **Beside the line, the cap does its job.** The living room's mirror, the studio's shelves
  and plants, the dining room, the attic: objects next to the seam that the fan-v1 band
  opened over are now left alone, and those are the wins.
- **On the line, a stripe is the worst of both.** The break-room light (cells `32bd4e6f…`
  fan-v2, `1d7ff896…` fan-v1) and the harbour chimney (`10c8387e…`, `70cef177…`) straddle the
  wrap edge. fan-v2 narrowed the band to the minimum through them; the filler cannot rebuild a
  fixture from a ten-pixel stripe, the light came out split and the chimney doubled, and the
  owner preferred fan-v1's clean removal of the light. An object the seam runs through is
  either removed whole or left with the seam inside it; the stripe should go.
- **The cap's edges step.** The mask shows hard rectangular notches where the cap starts and
  stops (the running minimum); they should taper over a few rows.
- **The filler invents.** In `str-10` (`312de4d6…`, `6f871b5d…`) Bria painted content into
  the band that is not in the scene, under both arms; a hallucination no mask rule prevents,
  and its own note.
- **Most pairs differ only in the fill.** The cap changes a handful of rows per image, and
  Bria is not deterministic between calls, so pairs whose masks differ little still look
  different, in the fill, not the mask. A mask comparison under a stochastic filler should
  show the judge the pairs whose masks differ most, and when two arms produce the same mask
  the lab should share the fill rather than buy it twice.

## What follows

1. **fan-v3** (built from these findings, SPEC.md "fan" 6b): an edge counts only when it
   runs down the frame for `object_run` of the height, so cloud and foliage texture no
   longer caps the band (on the plain sky `sky-04` fan-v3's mask is fan-v1's exactly; on the
   volcanic coast the notches are gone); an object on the line is removed whole, ends
   included, instead of striped at thin (the break-room light, the harbour chimney); and the
   cap tapers over `object_taper` of the height instead of stepping. fan-v2 with its own
   constants still reproduces this round's masks byte for byte. `labs/objects-02/`, a
   structured-only round against fan-v1, says whether fan-v3 keeps the wins and loses the
   regressions; the sky group is left out because fan-v3 differs from fan-v1 there only where
   a real object stands at the horizon.
2. **The lab**: order a page's pairs by how much the two masks differ, and reuse a fill across
   arms when the request is identical.
3. The owner's five flagged cells stay the examples for the mask-builder work; the filler's
   hallucination is a filler question.

The run directory (`lab/objects-01/` on the owner's machine) holds every cell's journal, the
measures, the two pages and the two picks files beside them.
