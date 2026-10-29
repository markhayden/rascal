# Measures

`measure` describes what a seam or a composite looks like at the line and what changed away
from it. Every measure is a number with a definition (`measures-v1`), reported per row as a
`Series` with row indices and a unit, and summarised in the envelope as `mean`, `median`,
`p95` (nearest rank), `max` and `rows_skipped`. None of them is a verdict: the lab (M6)
compares them across arms, and a person decides.

```
rascal measure --before a.png --after b.png --mask run/artifacts/<id> --set continuity,drift --json
rascal measure --workspace ws/ --after img-3 --before img-1 --mask mask-5 --json   # as an op
rascal measure --list                                                             # the names
```

The inputs are `after` (required), `before` (for drift), the mask (with its profile for the
band measures) and an optional `line` location in column form; all in one frame and size. A
measure whose inputs are absent is `not_applicable` for itself and the rest proceed; the
envelope's summary names each measure's outcome. Sampling windows are clipped to the frame; a
row whose numerator or denominator window is empty after clipping, or whose denominator is
zero, is skipped and counted in `<measure>.rows_skipped`. When every row of a measure is
skipped it has `measured_nothing`: no series, no summaries, `rows_skipped = rows`. When no
measure in the set produced anything the envelope is `measured_nothing` (or `not_applicable`
when none could run), and nothing is published. Every serialised value is finite.

`scripts/check-docs.sh` fails when a measure `rascal measure --list` names is missing from this
table or a listed measure is not registered.

| Measure | Needs | Definition | Series (unit) |
|---|---|---|---|
| `continuity` | `after`, a line (or the profile's column) | per row, the mean over channels of the absolute difference between the two columns adjacent to the line, divided by the row's mean absolute difference between neighbouring columns | `continuity` (ratio) |
| `tone_step` | `after`, a profile | per row, the mean colour of the profile's step windows on each side in CIE Lab (D65, sRGB), right minus left, as ΔL, Δchroma and Δhue; the hue is the signed circular difference in degrees in (−180, 180] and is omitted where either side's chroma is below `chroma_floor` (1) | `tone_step.dL` (L), `tone_step.dchroma` (chroma), `tone_step.dhue` (degrees) |
| `transition` | `after`, a profile | per row, the mean Sobel magnitude of L over `[c − half, c + half]` divided by the row's mean outside `[c − 2 · half, c + 2 · half]` | `transition` (ratio) |
| `texture_match` | `after`, a profile | per row, the mean Sobel magnitude of L inside the core divided by that over `[half, 2 · half]` beyond the core on each side | `texture_match` (ratio) |
| `drift` | `before`, `after`, a mask | the mean absolute 8-bit difference per channel inside the core, inside the blend and where `w = 0`; the last must be zero after a correct paste | none: `drift.<zone>.<channel>` in the envelope (levels) |

The Sobel magnitude is the 3×3 operator divided by 8 with edge replication at the frame border;
L is `0.299 R + 0.587 G + 0.114 B`; the step windows are the ones `fan` recorded in the profile,
so `tone_step` samples exactly the columns the band was measured on.
