# seam-02: two masks by two fillers on the generated corpus

The second round (task 6.4's refit after `seam-01`), run and judged on 2026-10-29. The same
twenty images and the same two masks, `fan-ramp` (`fan-v1`) and `thin` (`fan-thin-v1`), each
healed by Bria Eraser on fal and by LaMa locally: eighty cells, four arms, four pairs pages
judged blind. **The gate comparison, `fan-ramp-bria` against `thin-bria`, is `inconclusive`
on both groups for want of decided pairs, so criterion 6 stays open**; but the four pages
together answer the question `seam-01` raised, and the answer is that the filler, not the
mask, decided `seam-01`.

## The round

| | |
|---|---|
| corpus | `labs/seam-02/corpus.toml`, the `seam-01` corpus unchanged (decision 6) |
| protocol | `labs/seam-02/protocol.toml`, digest `sha256:be1c593caea56cc5dd0cb542a308b95b6fd289d837560bd89007133b3f73d510`; arms `fan-ramp-bria`, `thin-bria`; `min_decided = 8`; seed 20261030 |
| matrix | four arms: `fan-ramp-bria`, `thin-bria`, `fan-ramp-lama`, `thin-lama`; Bria Eraser (`bria eraser via fal, 2026-10`, a hosted filler) and LaMa (`big-lama via simple-lama-inpainting, 2026-10`, local) |
| run | 80 cells, 80 ran, 0 stopped, 0 failed; 40 Bria calls at an estimated $0.04, spend $1.60 |
| pages | one per comparison under `pairs/`, `--whole-max-side 2000`, judged at the focus with `s` for the source |

## The four decodes

Wins are the first arm's.

| page | assignment | group | decided | wins | losses | same | neither | outcome |
|---|---|---|---|---|---|---|---|---|
| `fan-ramp-bria` vs `thin-bria` (the gate) | `sha256:eb9dbb…` | sky | 6 | 5 | 1 | 2 | 1 | inconclusive |
| | | structured | 5 | 1 | 4 | 6 | 0 | inconclusive |
| `fan-ramp-lama` vs `thin-lama` | `sha256:7a593e…` | sky | 8 | 2 | 6 | 1 | 0 | worse |
| | | structured | 5 | 1 | 4 | 6 | 0 | inconclusive |
| `fan-ramp-bria` vs `fan-ramp-lama` | `sha256:af8c50…` | sky | 9 | 8 | 1 | 0 | 0 | preferred |
| | | structured | 10 | 2 | 8 | 1 | 0 | worse |
| `thin-bria` vs `thin-lama` | `sha256:7b0769…` | sky | 9 | 1 | 8 | 0 | 0 | worse |
| | | structured | 11 | 2 | 9 | 0 | 0 | worse |

## Per image

Each cell names the arm the owner preferred on that page, or `same` / `neither`.

| group | image | ramp vs thin, Bria | ramp vs thin, LaMa | ramp: Bria vs LaMa | thin: Bria vs LaMa |
|---|---|---|---|---|---|
| sky | beach-dark-sunset | neither | same | LaMa | LaMa |
| sky | beach-dawn | ramp | ramp | Bria | LaMa |
| sky | sky-02-v2 | ramp | thin | Bria | LaMa |
| sky | sky-03 | same | thin | Bria | LaMa |
| sky | sky-04 | ramp | thin | Bria | LaMa |
| sky | sky-05 | thin | ramp | Bria | LaMa |
| sky | sky-06 | ramp | thin | Bria | LaMa |
| sky | sky-07 | same | thin | Bria | Bria |
| sky | sky-08 | ramp | thin | Bria | LaMa |
| structured | str-01-v2 | thin | thin | LaMa | LaMa |
| structured | str-02-v2 | same | same | LaMa | LaMa |
| structured | str-03 | same | same | LaMa | Bria |
| structured | str-04 | same | same | LaMa | LaMa |
| structured | str-06 | same | thin | same | LaMa |
| structured | str-07 | thin | same | LaMa | LaMa |
| structured | str-08 | same | same | LaMa | LaMa |
| structured | str-09 | same | same | Bria | Bria |
| structured | str-10 | thin | thin | LaMa | LaMa |
| structured | str-11 | ramp | thin | Bria | LaMa |
| structured | str-12 | thin | ramp | LaMa | LaMa |

## The measures

Means over the group of each cell's `continuity.mean` (lower is smoother across the seam)
and `tone_step.dL.p95` (levels), from the journals.

| group | arm | continuity | tone step p95 |
|---|---|---|---|
| sky | fan-ramp-bria | 0.662 | 8.2 |
| sky | thin-bria | 0.624 | 8.2 |
| sky | fan-ramp-lama | 0.956 | 10.7 |
| sky | thin-lama | 1.003 | 11.1 |
| structured | fan-ramp-bria | 1.041 | 21.8 |
| structured | thin-bria | 0.911 | 19.7 |
| structured | fan-ramp-lama | 0.995 | 22.2 |
| structured | thin-lama | 1.041 | 22.5 |

The measures separate the fillers on sky (Bria smoother by a third) and do not separate the
masks anywhere in the direction the eye went; as in `seam-01`, they are evidence of
smoothness, not of what a judge sees.

## What the round says

- **`seam-01` was decided by the filler.** Under LaMa the owner again preferred `thin` on sky
  (six of eight decided pairs, after seven of eight in `seam-01`, with a fresh draw and
  blinding), and under Bria preferred the ramp (five of six). The ramp is the better mask
  when the filler can carry a wide band across smooth sky; LaMa cannot, and a thin band hides
  what it leaves.
- **The fillers have opposite strengths, and the mask changes which shows.** With the ramp,
  Bria wins sky eight of nine and LaMa wins structure eight of ten. With the thin mask, LaMa
  wins nearly everything (eight of nine sky, nine of eleven structured). So LaMa is the
  stronger model on a narrow band and on detail, Bria on a wide band across smooth content.
- **Structure is a draw between the masks.** On detailed surfaces the fan collapses to its
  minimum width under either constant set, the cells coincide, and the owner says `same`; the
  decided structured pairs lean to `thin` four to one under both fillers.
- **The gate needs decided pairs.** Five of six on sky is `preferred` under the protocol's
  rules; the group was two decided pairs short of its eight. More sky scenes, independent
  compositions, close that gap.

## What follows

1. **A fifth page**, `fan-ramp-bria` against `thin-lama`, the best of the four on sky by each
   page against the best of the four by the other: it says which single arm wins sky
   outright. The page is written; it is the owner's to judge.
2. **The filler chooses its band.** The matrix already pairs a constant set with a filler per
   arm; `heal` should do the same outside the lab: a filler entry naming the overrides it wants
   (`[fillers.lama] overrides = { "fan.constants" = "fan-thin-v1" }`), applied when that
   filler heals, so `heal seam --filler lama` runs the band LaMa can carry and `--filler
   bria-erase` the wide ramp, with the journal recording the resolved set. A small change to
   the config and `heal`; the owner's "if the filler is LaMa, do what LaMa needs".
3. **`seam-03`** with more sky scenes to reach `min_decided` on the gate, and the structured
   group judged with the fillers rather than the masks in mind, since the masks coincide
   there.

The run directory (`lab/seam-02/` on the owner's machine) holds every cell's journal, the
measures, the five pages and the four picks files beside them.
