# objects-02: fan-v3 against fan-v1 under Bria, structured group

The round after objects-01 (`docs/results/objects-01.md`), run and judged on 2026-10-30: the
ramp band from `fan-v3` (`fan` 1.2.0: an edge must run down the frame, an object on the line
is removed whole, the cap tapers) against `fan-v1`, both filled by Bria Eraser, on the 26
structured panoramas of the seam-03 corpus. **Four wins, one loss, eighteen `same`, three
`neither`: `inconclusive` by count (five decided against a `min_decided` of ten), and fan-v3
lost none of objects-01's wins while its regressions are gone.** Seventeen of the pairs had
byte-identical masks, so the round could decide at most nine; of those, fan-v3 won four, lost
one and tied four. The owner's reading: the pairs are close, most winners are the fill's doing,
one pair showed a hard line, and the chimney came out wrong under both arms.

## The round

| | |
|---|---|
| corpus | `labs/objects-02/corpus.toml`, the structured group of seam-03 (26 panoramas) |
| protocol | `labs/objects-02/protocol.toml`; arms `ramp-v3-bria` (`fan.constants = "fan-v3"`) and `ramp-bria` (`fan-v1`), both Bria Eraser; `min_decided = 10`; seed 20261103 |
| run | 52 cells, 52 ran, 0 stopped, 0 failed; 52 Bria calls, spend $2.08 |
| masks | 17 of 26 pairs identical; the nine that differ: the workshop, the living room, the sunroom, the break room, the dining room, the lobby, `str-10`, the studio, `str-11` |
| page | one, `pairs/ramp-v3-bria-vs-ramp-bria-structured/`, judged once (`picks (14)`) |

## The decode

```
group          pairs decided  wins losses  same neither  outcome
structured        26       5     4      1    18       3  inconclusive
```

Wins are `fan-v3`'s: the dining room, the lobby, `str-10` and the studio. The loss is the
living room. `neither`: the workshop, the harbour promenade and `str-12`. Every pair whose masks
were identical was called `same` except the harbour and `str-12` (`neither`): on identical
masks a difference is the filler's randomness, and the judge read it as such.

## What the cells show

- **The regressions are gone.** The break-room light and the harbour chimney, striped at thin
  by fan-v2, are now left to the rulers: the break room's masks differ only in the taper and
  the pair was `same`; the harbour's masks are identical to fan-v1's.
- **The cap's boundary still stepped.** In the living room (the loss) the band narrows beside
  the mirror and the door frame, and the taper as built eased only the rows outside the cap
  while a capped row kept its cap, so a step of half the original size remained at the
  boundary, and two short caps on the light switch left dark notches inside the band. That is
  the hard line the owner saw. Fixed after the round (`fan` 1.2.1): the cap is carried three
  sigmas further out as a running minimum and then smoothed, so the eased profile is
  continuous at the boundary; `object_taper` set to 0.005 so the eased stretch stays short.
  fan-v3's objects-02 masks are from 1.2.0 and stay in the run directory; fan-v3 under 1.2.1
  differs from them in the taper rows of the nine differing pairs only.
- **An object on the line is removed, and the filler invents one.** The harbour chimney sits
  on the wrap edge; both arms' bands cover it whole, and Bria painted two stubs where it
  stood. Removing it whole is better than fan-v2's stripe (which doubled it), but the right
  answer keeps the chimney and heals the seam through it, which no band rule does: it needs
  the mask to recognise an object on or near the line and treat it as a thing to keep
  (`tasks/todo.md`, the mask-builder follow-up).
- **Identical masks, different fills.** Seventeen pairs differed only by Bria's randomness.
  The judge called them `same`; a lab that reused a fill across arms when the request is
  identical would have spent $0.68 less and shown nothing misleading.

## What follows

1. The object cap rests here: fan-v3 is the default candidate for the seam recipe once the
   taper fix has a round of its own, or is folded into the next mask-builder round.
2. The mask-builder follow-up the owner keeps returning to: objects on or near the line are
   recognised and kept (the chimney, the light), not striped and not removed; the band heals
   the seam through them.
3. The lab: share a fill across arms whose requests are identical; order a page's pairs by how
   much the masks differ, so the judge's attention goes where the arms actually differ.

The run directory (`../rascal-lab/objects-02/` on the owner's machine) holds every cell's
journal, the page and the picks file beside it.
