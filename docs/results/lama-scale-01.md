# lama-scale-01: the scaled window for LaMa

The scaled window's lab question (owner decision 10, `docs/fillers.md`), run and judged on
2026-10-29: the ramp band (`fan-v1`) healed by LaMa handed the support at full size against
the same LaMa handed it through a scaled window of 1024 px, over the seam-03 corpus. **On sky
the scaled window wins twenty of twenty-one; on structure it loses fourteen to twelve.** The
smear seam-01 found was LaMa's resolution, not its model.

## The round

| | |
|---|---|
| corpus | `labs/lama-scale-01/corpus.toml`, the seam-03 corpus (47 panoramas, 21 sky, 26 structured) |
| protocol | `labs/lama-scale-01/protocol.toml`, digest `sha256:9cf56e6bb8187fbd1e0045d6da56ec2eceb5b6fd8a8ac522ed7ea65fa3de3694`; arms `ramp-lama-scaled` (`fill_max_side = 1024`) and `ramp-lama`; `min_decided = 10`; seed 20261101 |
| filler | LaMa, local (`big-lama via simple-lama-inpainting, 2026-10`), the one entry twice, the scaled one with `fill_max_side = 1024`; the configuration lives outside the repository |
| run | 94 cells, 94 ran, 0 stopped, 0 failed, no spend; the scaled request is 287×1024 where the full one is 888×3168, and the median filler call 1.7 s against 3.9 s |
| pages | one per group; picks on assignments `sha256:e7d683…` (sky) and `sha256:4fd89f…` (structured) |

## The decode

```
group          pairs decided  wins losses  same neither  outcome
sky               21      21    20      1     0       0  preferred
structured        26      26    12     14     0       0  worse
```

Wins are the scaled arm's. Sky: every pair decided, the one loss the dark-sunset beach that
has failed under every arm of every round. Structured: every pair decided and nearly even,
split by source: the full-size fill won nine of the eleven resized Gemini scenes, the scaled
one ten of the fifteen native 2:1 Sunburst scenes.

## The measures

Means over the cells of `continuity.mean` and `tone_step.dL.p95`, from the journals.

| group | arm | continuity | tone step p95 | n |
|---|---|---|---|---|
| sky | ramp-lama | 1.002 | 13.8 | 21 |
| sky | ramp-lama-scaled | 0.584 | 11.9 | 21 |
| structured | ramp-lama | 0.950 | 16.5 | 26 |
| structured | ramp-lama-scaled | 0.677 | 15.2 | 26 |

For once the continuity measure and the eye agree on sky, and the measure keeps preferring
the scaled fill on structure where the eye does not: the scaled fill is smoother across the
seam everywhere, and on detail that smoothness is the lost texture.

## What the round says

- **LaMa's weakness was resolution.** Handed the band near the size it was trained at, LaMa
  carries the wide ramp across smooth sky (twenty of twenty-one) where at full size it smeared
  it (seam-01, seam-02). The scaled window is the "clean up the filler's shortcoming" tool the
  owner asked for, one line in the filler's entry, and it is faster by more than half.
- **On detail the downscale costs texture.** The structured group is a near draw with the
  full-size fill slightly ahead, most clearly on the resized Gemini scenes whose seams are
  larger. The right setting for LaMa therefore depends on what the seam crosses, which is the
  same observation the mask rounds made about the band: content should choose, and the fan's
  own smoothness is the signal.
- **The band swallows solid objects on a smooth surface.** The owner found, judging this
  round, that where a ceiling light or a track spot sits on an otherwise smooth ceiling the
  fan opens the band for the smooth surface, the object falls inside it and the filler removes
  it, under both arms. Two cells keep the examples: `sunburst-training-17-office-break-room`
  (`56af200d…` scaled, `683d99ad…` full) and `str-10` (`262d246e…` scaled, `2d0edb13…` full).
  That is a mask-builder improvement for after the initial development (`tasks/todo.md`): a
  fan that recognises a solid object inside the smooth region and goes around it.

## What follows

1. The example configuration recommends `fill_max_side` for a filler trained small, with
   this round as the evidence; the choice stays the entry's.
2. A later round can pit the scaled ramp against the thin band under LaMa, the two ways LaMa
   can be made to work, and the two fillers' best arms against each other; none of it is on a
   gate.
3. The mask-builder work above, as a `fan-v2` with its own round.

The run directory (`lab/lama-scale-01/` on the owner's machine) holds every cell's journal,
the measures, the two pages and the two picks files beside them.
