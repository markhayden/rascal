# seam-01: fan-ramp against thin on the generated corpus

The first lab round (task 6.4, success criterion 6), run and judged on 2026-10-29. **Criterion
6 is not met: the gate is open.** The sky group came out `worse` for `fan-ramp` against
`thin`, the structured group `inconclusive`. A refit is a new constant set, a new lab name and
a new protocol (`seam-02`); the M7 release gate waits on it, or on the owner's recorded scope
change (`docs/owner-decisions.md`).

## The round

| | |
|---|---|
| corpus | `labs/seam-01/corpus.toml`: twenty panoramas generated with `gemini-3.1-flash-image` from the agent's prompt list and kept by the owner (decision 6), nine sky and eleven structured; 6336×3168 (two at 3168×1584), requested at 21:9 and resized to 2:1, so the spherical geometry is approximate and the wrap seam is real and unhealed |
| protocol | `labs/seam-01/protocol.toml`, digest `sha256:0563537891ebc3c8b42e40124b3bfc79c4ba1a2ce66d03d46434f27242292480`; arms `fan-ramp` (the `seam` recipe as shipped, `fan-v1`) and `thin` (`fan.constants = "fan-thin-v1"`); `min_decided = 8`; seed 20261029 |
| filler | LaMa, local on Metal (`big-lama via simple-lama-inpainting, 2026-10`), the same for both arms |
| run | `rascal lab run labs/seam-01/matrix.toml`: 40 cells, 40 ran, 0 stopped, 0 failed, 40 filler calls, no spend |
| page | `rascal lab pairs --arms fan-ramp,thin --whole-max-side 2000`; judged at the focus with the source held under `s`; the draw put `fan-ramp` on the left in 8 pairs and on the right in 12 |
| picks | 20 picks on assignment `sha256:38692add5c7399b9ad076c617d4d0f6f51b7218b58ff94e8f1c5efea7fe2db18` of this lab: 10 left, 2 right, 8 same, 0 neither |

## The decode

```
group          pairs decided  wins losses  same neither  outcome
sky                9       8     1      7     1       0  worse
structured        11       4     2      2     7       0  inconclusive
```

Wins are `fan-ramp`'s over `thin`. On sky the owner preferred `thin` in seven of eight decided
pairs, on both sides of the page (five times when `thin` was on the left, twice on the right),
so the lopsided left count is the arms, not the side. On structured content seven of eleven
pairs were `same`, four were decided (two each way), under the protocol's eight.

## Per pair

`continuity` is the measure's mean over the band's rows (lower is smoother across the seam),
`tone step` its 95th percentile of the lightness step (levels); both from each cell's journal.

| group | image | fan-ramp on | pick | outcome | continuity fan-ramp / thin | tone step p95 fan-ramp / thin |
|---|---|---|---|---|---|---|
| sky | beach-dark-sunset | right | same | same | 0.904 / 0.931 | 9.3 / 9.3 |
| sky | beach-dawn | right | left | thin | 0.901 / 0.946 | 16.9 / 17.2 |
| sky | sky-02-v2 | left | right | thin | 1.040 / 1.105 | 18.3 / 18.5 |
| sky | sky-03 | right | left | thin | 0.921 / 0.955 | 8.0 / 8.2 |
| sky | sky-04 | right | left | thin | 0.973 / 0.954 | 3.5 / 6.2 |
| sky | sky-05 | left | right | thin | 1.014 / 1.064 | 7.2 / 7.4 |
| sky | sky-06 | left | left | fan-ramp | 1.052 / 1.134 | 16.1 / 16.4 |
| sky | sky-07 | right | left | thin | 0.878 / 0.991 | 5.8 / 5.8 |
| sky | sky-08 | right | left | thin | 0.922 / 0.944 | 10.8 / 11.0 |
| structured | str-01-v2 | left | same | same | 0.951 / 1.051 | 10.9 / 10.8 |
| structured | str-02-v2 | right | same | same | 1.028 / 1.057 | 26.0 / 26.0 |
| structured | str-03 | left | same | same | 0.958 / 0.958 | 16.8 / 16.8 |
| structured | str-04 | left | same | same | 1.017 / 1.017 | 40.4 / 40.4 |
| structured | str-06 | right | left | thin | 0.764 / 0.834 | 26.5 / 27.7 |
| structured | str-07 | right | same | same | 1.119 / 1.096 | 57.6 / 57.6 |
| structured | str-08 | right | same | same | 0.872 / 0.872 | 21.4 / 21.4 |
| structured | str-09 | right | same | same | 1.461 / 1.461 | 20.8 / 20.8 |
| structured | str-10 | left | left | fan-ramp | 1.103 / 1.195 | 8.2 / 9.9 |
| structured | str-11 | right | left | thin | 0.825 / 0.957 | 9.0 / 9.1 |
| structured | str-12 | left | left | fan-ramp | 0.843 / 0.947 | 6.2 / 7.4 |

## What the round says

- **The eye and the continuity measure disagree on sky.** `fan-ramp` has the lower
  continuity mean on eight of nine sky images, yet the owner preferred `thin` on seven. The
  measures are evidence, not the verdict (ADR 0013); the verdict is that a wide band with a
  tone ramp, filled by LaMa across smooth sky, reads worse than a thin one. The likely reason
  is the one noted after the real-filler check (decision 3): LaMa over a wide band in smooth
  sky leaves a visible patch, and the ramp spreads it; a thin band leaves less to see. The
  continuity measure rewards the smoother gradient the ramp makes and does not see the patch.
- **On structured content the two arms converge.** Four structured cells (`str-03`, `str-04`,
  `str-08`, `str-09`) have identical measures under both arms: on detailed surfaces the fan
  collapses to its minimum width whichever constant set it runs under, so the masks, the
  fills and the results are the same, and the owner judged them `same`. The structured
  comparison is largely vacuous as designed, which is why it is `inconclusive` rather than a
  finding either way.
- **The corpus did its job.** Every cell healed, every pair was judged, the wrap seams were
  real. Two caveats on the material: the two beach images are one composition restyled, and
  the images' geometry is a resize of a 21:9 frame, not a true projection, which affects the
  look-around view and nothing the round judged.

## What seam-02 should change

1. A sky constant set with a narrower `wide_max` for the fan (the `0.05` the real-filler check
   suggested, against `fan-v1`'s wider band), as a new versioned set, so the comparison stays
   `fan-ramp` against `thin` with the band the filler can carry.
2. Keep the structured group, but expect convergence; if it stays `same`, the structured gate
   reads as `not_worse` under decision 9's option (a) only once enough pairs are decided, so
   either more structured images or a lower `min_decided` for that group is needed to decide it.
3. More sky images, independent compositions, so `min_decided` can return toward the default.

The run directory (`lab/seam-01/` on the owner's machine) holds every cell's journal, the
measures and the pairs page; the assignment and the picks are beside it.
