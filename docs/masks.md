# Masks

A mask is a record, not a picture: a directory `<identity hex>.mask/` holding `mask.toml`, the
three planes, and any profile and prescription files it references. The record is the truth; a
rendering is an `Image` projected from it for one consumer and is never read back as a mask.

## The record

```toml
version = 1
id = "rasterize.mask"                   # local; not part of identity
frame = "raw:320x200:flat"              # the frame id the planes are in
frame_identity = "sha256:…"
width = 320
height = 200
use_case = "custom"                     # seam | remove | repair | protect | custom
identity = "sha256:…"

[zones]
core = "core.png"                       # binary: 255 where a filler may paint; weight 1
blend = "blend.png"                     # 8-bit weights outside the core; 0 inside it and under protect
protect = "protect.png"                 # binary: 255 where nothing may change; weight 0

[profile]                               # present when the mask is a band along a line
line = "sha256:…"                       # the line location's identity
band = "band.profile.json"              # the BandProfile: per-row half-width, centre column, windows
sigma_px = "sigma.series.json"          # the per-row feather sigma

[[prescriptions]]                       # pixel changes that belong with the mask; `apply` performs them
kind = "tone_ramp"
version = 1
step = "prescription-0.series.json"     # per-row colour step, 3 channels, levels
share = 0.5

[source]
locations = ["sha256:…"]                # identities of the locations it was made from
instances = []

[[provenance]]                          # a copy of the journal's nodes; not part of identity
step = "rasterize"
op = "rasterize"
version = "0.1.0"
params = {}
inputs = { image = "sha256:…", location = "sha256:…" }
outputs = { mask = "sha256:…" }
```

## The effective weight

One rule, used by every rendering, prescription, refinement and paste:

```
w = 0            where protect is set
w = 1            else where core is set
w = blend / 255  otherwise
```

## Invariants

Checked whenever a mask is read, and named in the refusal: the planes are the record's size and
each other's; `core` and `protect` are binary; `blend` is zero inside the core and under protect;
a profile is in the mask's frame with one entry per row; a tone-ramp series has three channels
and one entry per row in the mask's frame; the stored identity matches the content; every file
`mask.toml` names is a plain name inside the mask directory (no separator, not `..`), checked
before anything is read or written, so a hand-edited record cannot reach outside the mask. A
hand-edited plane is therefore refused on load, and `inspect --replay` reports a hand-edited
record (`record_differs`) even when its planes are intact.

## The feather

`profile` and `feather` write the blend the same way. For a pixel outside the core, `D` is the
distance from its centre to the nearest core pixel's edge (the exact Euclidean distance
transform, less the half pixel to the boundary) and `σ` is the row's sigma (`profile`:
`feather_ratio · half[y]`, stored as the `sigma_px` series) or the op's one sigma (`feather`):

```
blend = round(255 · exp(−D² / (2σ²)))      for 0 < D ≤ support · σ,   else 0
```

The weight tends to 1 at the core's boundary, the 50% contour is `1.1774 σ` outside the core,
and the support (`feather-v1`: 3 sigmas) is finite. Protected pixels get `blend = 0`.

## Prescriptions

A prescription describes a change `apply` will make to the canvas; the mask stays a
description until then. `ramp` adds a `tone_ramp`: a 3-channel series `Δ[y]` (right minus
left on the band profile's step windows, row-smoothed) and a `share`. `apply` performs every
prescription once, in order: a pixel with effective weight `w > 0` at distance `d = |x + 0.5 −
c|` from the line moves by `sign · share · Δ[y] · max(0, 1 − d / half[y])`, the left side toward
the right and the right toward the left, rounded and clamped; a pixel with `w = 0` (protected,
or outside the mask's support) is byte-identical. The ramp is zero at and beyond the band's
half-width.

## Paste

`paste` composites a fill onto a base through the mask. The output starts as a copy of the
base and only pixels with `w > 0` are written, so `out = base` wherever `w = 0`, protected
pixels included: the preservation guarantee of a standalone paste. `feather` writes
`(1 − w) · base + w · fill`; `hard` writes the fill where the core is set and not protected;
`poisson`, the gradient-domain blend, arrives with M7 and is not applicable until then.
Arithmetic is per RGB channel in `f32`, rounded and clamped; a base's alpha is kept, a fill's
alpha is ignored and the summary says so.

## Identity

`sha256:` over the three planes' pixel digests, the use case, the frame identity, the profile's
line, band and sigma identities, every prescription with its series replaced by the series'
identity, and the source identities. Ids, file names, timestamps and provenance are not part of
it (ADR 0016).

## Refinement postconditions

- An op that replaces the core (`grow`, `shrink`, `close`, `select`, `tighten`, `rasterize`)
  clears `blend` everywhere, removes the profile and every prescription, keeps `protect`, and
  reports `blend_cleared` and `profile_cleared`; the recipe re-feathers afterwards.
- `subtract` adds to `protect` and clears `core` and `blend` under it; the profile and the
  prescriptions stay, since `apply` honours the new protect.
- Setting `blend` forces zero inside the core and under protect.
- Prescriptions never touch a pixel with `w = 0`.

## Reading one by hand

`rascal inspect <dir> --json` prints the record, the zone counts (core, blend, protect, painted,
the support bbox, the weight sum) and the identity. `--out <dir>` writes `overlay.jpg` (the
image in the mask's frame tinted by `w`, the core's edge in white), `chart.svg` (the core's
width per row; a band mask's rulers join it in M2) and `inspect.json`. `--replay` rebuilds the
mask from its operation record and the stored inputs and diffs the planes: an untouched run
reports an empty diff, a tampered plane is named, and a run directory copied elsewhere replays
the same because every input it used is inside it.

## Renderings

A rendering is an `Image` projected from the record for one consumer; it is never read back as
a mask. `rascal render <dir> --for <profile> [--image <path>] --out <file>`:

| profile | role | output | polarity |
|---|---|---|---|
| `grey` | `mask_input` | 8-bit `round(255 · w)` | 255 = paint |
| `binary` | `mask_input` | 8-bit, 255 where `w ≥ threshold / 255` (`--threshold` 1 to 255, default 127); protected pixels are 0 whatever the threshold | 255 = paint |
| `alpha` | `image_input` | RGBA of the image, alpha 0 where the core is set and not protected | transparent = paint |
| `tinted` | `image_input` | RGB of the image with the unprotected core blended toward `--colour` (default `#ff00ff`) by `--strength` (default 0.7) | tint = paint |
| `overlay` | inspection | the image, the tint by `w` at half strength, the core's edge as a white line; a `.jpg` destination gets a JPEG | for the eye |

The image must be in the mask's frame. There is no `protect` rendering: a second pass uses the
`invert` op, which produces a real mask that governs both rendering and paste.
