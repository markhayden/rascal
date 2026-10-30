# seam-03: the gate round, fan-ramp against thin under Bria on forty-seven panoramas

The third round (task 6.4), run and judged on 2026-10-29. **Criterion 6 is met: sky
`preferred` and structured `preferred` for `fan-ramp` against `thin`, under Bria Eraser, at
the spec's default of ten decided pairs per group.** The gate (G16) closes; M6 is complete and
the M7 release gate no longer waits on it.

## The round

| | |
|---|---|
| corpus | `labs/seam-03/corpus.toml`: forty-seven generated panoramas kept by the owner (decision 6), twenty-five from `gemini-3.1-flash-image` (requested at 21:9, resized to 2:1) and twenty-two native 2:1 from `gpt-image-2.5-sunburst` at 3840×1920; 21 sky, 26 structured; each prompt and its provenance in the manifest's licence note |
| protocol | `labs/seam-03/protocol.toml`, digest `sha256:1937e23615e9cc3e54f7151d4414cac90cbb9587a2b954b6ec847ff7a983702e`; arms `fan-ramp-bria` (`fan-v1`) and `thin-bria` (`fan-thin-v1`), both healed by Bria Eraser on fal; `min_decided = 10`; seed 20261031 |
| matrix | the two gate arms and the two LaMa arms (`fan-ramp-lama`, `thin-lama`), 188 cells, 188 ran, 0 stopped, 0 failed; 94 Bria calls at an estimated $0.04, spend $3.76 |
| pages | one per comparison and group (`--group`, `--whole-max-side 2000`); the two gate pages judged, the LaMa and cross-filler pages written for later |
| picks | sky on assignment `sha256:8b8890…`, structured on `sha256:bf37c2…`; the draw put `fan-ramp-bria` on the left in 11 of 21 and 12 of 26 |

## The decode

```
group          pairs decided  wins losses  same neither  outcome
sky               21      19    18      1     1       1  preferred
structured        26      16    14      2     7       3  preferred
```

Wins are `fan-ramp-bria`'s. Sky: eighteen of nineteen decided, the loss the mountain lake,
`same` the lakeside dock at night, `neither` the dark-sunset beach restyle. Structured:
fourteen of sixteen decided, the losses the workshop and the greenhouse, seven `same`, three
`neither` (the museum gallery, the conference room, the harbour promenade). Under decision 9's
reading, `preferred` satisfies the structured group's `not_worse`; here it is `preferred`
outright, with losses at an eighth of the decided pairs.

## Per image

| group | image | source | outcome |
|---|---|---|---|
| sky | beach-dark-sunset | gemini | neither |
| sky | beach-dawn | gemini | ramp |
| sky | sky-01-new | gemini | ramp |
| sky | sky-02-v2 | gemini | ramp |
| sky | sky-03 | gemini | ramp |
| sky | sky-04 | gemini | ramp |
| sky | sky-05 | gemini | ramp |
| sky | sky-06 | gemini | ramp |
| sky | sky-07 | gemini | ramp |
| sky | sky-08 | gemini | ramp |
| sky | sky-09 | gemini | ramp |
| sky | sky-10 | gemini | ramp |
| sky | sky-11 | gemini | ramp |
| sky | sky-12 | gemini | same |
| sky | sunburst-sky-winter | sunburst | ramp |
| sky | sunburst-training-02-coastal-headland | sunburst | ramp |
| sky | sunburst-training-04-desert-basin | sunburst | ramp |
| sky | sunburst-training-08-mountain-lake | sunburst | thin |
| sky | sunburst-training-12-prairie-dusk | sunburst | ramp |
| sky | sunburst-training-16-snowy-field | sunburst | ramp |
| sky | sunburst-training-20-volcanic-coast | sunburst | ramp |
| structured | str-01-v2 | gemini | thin |
| structured | str-02-v2 | gemini | same |
| structured | str-03 | gemini | same |
| structured | str-04 | gemini | same |
| structured | str-06 | gemini | ramp |
| structured | str-07 | gemini | thin |
| structured | str-08 | gemini | same |
| structured | str-09 | gemini | same |
| structured | str-10 | gemini | neither |
| structured | str-11 | gemini | ramp |
| structured | str-12 | gemini | neither |
| structured | sunburst-str-library-modern | sunburst | ramp |
| structured | sunburst-str-library-v2 | sunburst | same |
| structured | sunburst-training-01-home-living-room | sunburst | ramp |
| structured | sunburst-training-03-office-studio | sunburst | ramp |
| structured | sunburst-training-05-home-kitchen | sunburst | ramp |
| structured | sunburst-training-06-forest-clearing | sunburst | same |
| structured | sunburst-training-07-office-meeting-room | sunburst | ramp |
| structured | sunburst-training-09-home-bedroom | sunburst | ramp |
| structured | sunburst-training-10-city-courtyard | sunburst | ramp |
| structured | sunburst-training-11-home-dining-room-v2 | sunburst | ramp |
| structured | sunburst-training-13-office-lobby | sunburst | ramp |
| structured | sunburst-training-15-home-attic | sunburst | ramp |
| structured | sunburst-training-17-office-break-room | sunburst | ramp |
| structured | sunburst-training-18-harbor-promenade | sunburst | neither |
| structured | sunburst-training-19-home-sunroom | sunburst | ramp |

## The measures

Means over the cells of `continuity.mean` (lower is smoother across the seam) and
`tone_step.dL.p95` (levels), from the journals, for the two gate arms.

| group | source | arm | continuity | tone step p95 | n |
|---|---|---|---|---|---|
| sky | all | fan-ramp-bria | 0.672 | 9.2 | 21 |
| sky | all | thin-bria | 0.660 | 9.0 | 21 |
| structured | all | fan-ramp-bria | 0.752 | 15.1 | 26 |
| structured | all | thin-bria | 0.727 | 13.5 | 26 |
| structured | gemini | fan-ramp-bria | 0.994 | 21.8 | 11 |
| structured | gemini | thin-bria | 0.912 | 19.8 | 11 |
| structured | sunburst | fan-ramp-bria | 0.574 | 10.2 | 15 |
| structured | sunburst | thin-bria | 0.592 | 8.9 | 15 |

As in the earlier rounds the measures do not separate the masks the way the eye did; they
separate the sources: the native 2:1 images have far smaller raw seams (a tone step around
10 levels against 20 on the resized ones), and that is where the structured verdict was won.

## What the round says

- **The ramp is the better mask when the filler can carry it.** Under Bria, eighteen of
  nineteen on sky and fourteen of sixteen on structure. Together with `seam-02` (under LaMa
  the thin mask wins, because LaMa cannot fill a wide band at this resolution) the picture is
  complete: the band is a property of the filler, which `heal` now reads from the filler
  entry's `overrides` (`docs/fillers.md`).
- **On the resized Gemini structured scenes the masks still coincide.** Seven of eleven were
  `same` or `neither`, as in `seam-01` and `seam-02`; on the native 2:1 Sunburst scenes the
  ramp won thirteen of fifteen decided. The difference is the seam itself: a resize of a 21:9
  frame leaves a larger, less photographic mismatch at the wrap edge than a native 2:1 render,
  and on those the fan collapses to its minimum width and both arms produce the same cell.
- **The corpus is now large enough for the default protocol.** Twenty-one and twenty-six
  images gave nineteen and sixteen decided pairs against a threshold of ten.

## What follows

1. The LaMa pages of this round (`fan-ramp-lama` against `thin-lama`, and each mask across
   the fillers) are written and can be judged at leisure; they are not on the gate.
2. The owner's direction for the mask builder, once the planned work is built: a fan that
   recognises patterns and how close detail lies to the seam, so the band adapts to the scene
   rather than to a constant set. That is a `fan-v2` with its own lab round, and the three
   rounds here are its baseline.
3. The release (task 7.2) no longer waits on criterion 6.

The run directory (`lab/seam-03/` on the owner's machine) holds every cell's journal, the
measures, the ten pages and the two picks files beside them.
