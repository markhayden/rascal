# Spec: rascal

The find-and-mask step of an image pipeline. Draft 8, 2026-10-28, from the owner's interview and
five external reviews (`docs/reviews/`); drafts 7 and 8 add what plan drafts 2 to 4 needed: the `rect`
utility recipe, `inspect --json`, `workspace list`, `ops list` and `ops describe`, `tools call`,
the `--out` rule, the `fan-thin-v1` set, the tighten tangent and cap rules, and the Diagnostics
section. The spec is the source of truth; `PLAN.md` orders the
work; `docs/adr/` records each decision.

## Assumptions

Stated so they can be struck.

1. Single owner, open source, Apache-2.0. No backwards compatibility, no shims, no deprecation
   paths: a breaking change is a version bump and an ADR.
2. Rust is the core and the only language in the binary. Python exists only as an optional tool
   pack over MCP for models that are Python-first. The pure-Rust, no-system-library rule applies
   to the binary; the pack documents its own runtime.
3. Clean room. The only material taken from another repository is the schema data listed in
   `schemas/SOURCES.toml`, with its provenance and licence notice. Every method is written from
   first principles and described in this document's own words; fixtures and page templates are
   authored here; no other project is cited as the origin of a rule or a constant.
4. rascal contains no model-driven controller and no in-process inference. Models run only in
   explicitly configured finder packs and filler plugins, out of process, and their outputs are
   recorded as data.
5. Files are the state. Every op reads and writes files, and every other surface (the MCP
   server, a future binding) is a thin wrapper over the same functions and the same files.
6. The contracts for tool manifests, result envelopes and locations are the shapes an evaluation
   harness in the same family already uses. rascal copies them as data, produces a
   **compatibility export** that validates against them, and uses its own versioned schemas for
   everything native. rascal is fully standalone; the shared shape only lets the two exchange
   documents without translation where the export suffices. Mounting rascal as a tool of that
   harness is that harness's adapter, not rascal's concern.

## Objective

Image models paint well inside a region and badly at its edge. The quality of an edit, a heal, a
removal or an inject is decided as much by **which pixels the model may touch, how the edge is
softened, what is done to the pixels first, and how the result is put back** as by the model
itself. Today that knowledge lives in hand-made masks and one-off scripts.

rascal turns a need into a mask a model can paint inside and blends the result back:

- a **need** is a use case plus a location: the wrap seam of a 360, the chairs in a room, a
  scratch across an old print, a stain, a user's rough stroke;
- a **mask** is a record, not a picture: zones (what may be painted, what only blends, what must
  never change), a soft edge, optional prescriptions on the pixels (a tone ramp across a line, a
  transform for an inject), the frame it lives in, and the provenance of every step that made it;
- a **rendering** is what one consumer wants to see: a feathered greyscale, a binary at a
  threshold, the photo with the region tinted, an alpha cut-out;
- the **composite** puts a filler's pixels back through the mask's own weights, exactly inside
  the core, blended across the blend zone, with a gradient-domain option for a tone step.

Users:

- a **pipeline author** who wires rascal between a generator and an evaluator, by CLI or by MCP;
- an **agent** (any MCP client) that drives rascal's operations one by one to find, refine and
  check a mask;
- the **owner**, running a lab: a recipe matrix over a private corpus, judged blind.

Success is a mask that blends better than the fixed hand-made mask it replaces, shown on the
owner's corpus by blind pairs under a protocol frozen before any result is inspected, with every
step reproducible from files.

### Use cases

| state | use case | the location | the mask's shape |
|---|---|---|---|
| v1 | **seam**: the wrap edge of a 360, or any vertical line | a column (the wrap edge after `cut`) | a band whose half-width follows the row: wide where the surface is smooth, thin where it is detailed; a tone ramp across it |
| v1 | **remove**: objects, with their shadow and reflection | a segmenter prompt, a click, a stroke, a polygon or a rectangle | a blob set: instances, grown, grown further downward for a shadow, minus protect zones |
| v1 | **repair**: scratches and tears (a stroke), stains and spots (an area) | a user stroke; a rectangle or polygon (a learned detector later) | a stroke tightened to the defect's edges, feathered; an area rasterized and feathered |
| v1 | **protect**: the inverse of any mask, for a second pass | any mask | a new mask whose core is everything the first left untouched |
| later | **replace**: screens, signs, labels, frames | a quad | a mask plus a homography the inject is warped by |
| later | **retouch**: wrinkles, blemishes, glare, stray hair | an evaluator's finding or a stroke | tiny soft masks with a change budget |
| later | **extend**: outpainting, stitch holes, aspect change | the empty region | the hole plus its blend band |
| later | **isolate**: cut-outs for compositing | the subject | an alpha matte; hair and glass decide quality |
| later | **fan along a path**: a scratch through a smooth surround | a `stroke` | the seam band's rule applied along a path instead of a column |
| later | **scaled windows**: a filler whose `max_side` is smaller than the mask's support | | a downscaled request with a pixel-centre transform |
| never | grades, upscales, denoise, anything with no region | | |

### Where it sits

```
   generate / capture  ──►  FIND + MASK + COMPOSITE  ──►  paint (any model)  ──►  evaluate
      (Pixel, a camera)          (rascal)                 (filler plugin)        (Deja View, a person)
                                     ▲                                                │
                                     └──────── a location, when something is found ───┘
```

## Decisions

Each has an ADR. The interview of 2026-10-28 settled them; the reviews refined 5, 8, 10 and 12.

1. **rascal finds and masks.** It accepts a location from anywhere (a user, a file, an
   evaluator) and ships its own finders as tools returning locations.
2. **rascal owns the act step up to and including the composite.** Pre-passes that belong with
   the mask (cut and put for a 360, the tone ramp), per-consumer rendering, and the paste back.
   The filler is a plugin; rascal bundles no weights for painting.
3. **v1 use cases: seam, remove, repair, protect.** Replace, retouch, extend, isolate, the path
   fan and scaled windows are named milestones. Whole-frame work is out for good.
4. **Surfaces in v1: a library, a CLI and an MCP server.** Node (napi), Swift (uniffi) and WASM
   are later milestones, each a thin crate over the core.
5. **A mask is a record**: `mask.toml` plus lossless planes beside it. Core and protect are
   binary; blend holds the fractional weights outside the core. Renderings are `Image` artifacts
   and are never accepted where a `Mask` is expected.
6. **Operations are tools; recipes are TOML step lists with typed bindings.** One manifest per
   op, callable as a subcommand and over MCP. Every step has an id, typed inputs bound to earlier
   steps' outputs, and typed outputs. rascal ships one recipe per v1 use case and the literal
   shipped recipes are acceptance fixtures.
7. **No model-driven loop inside rascal** in v1. Agents drive its tools. The door stays open:
   if a loop is ever needed it is an ADR, and nothing in the workspace model forbids it.
8. **Shared contracts by copied schema, exported on request.** `schemas/copied/` holds the
   manifest and envelope schemas unchanged (the location definition is embedded in the envelope
   schema and is referenced from rascal's location schema by `$ref`). Native output follows
   `schemas/rascal/`. `rascal export --compat` projects a native envelope onto the copied
   envelope's fields (native bookkeeping such as `artifacts` is dropped by the projection) and
   fails only on content the copied schema cannot express, naming it. No code dependency.
9. **Crate names**: the binary crate is `rascal-mask` (command `rascal`); `rascal-core`,
   `rascal-ops`, `rascal-mcp`, `rascal-testkit`; the Python pack is `rascal-tools`; the Node
   binding later is `@markhayden/rascal`. `rascal` itself is taken on every registry.
10. **Pure Rust pixels.** `image`, `imageproc`, `rayon`; the Poisson paste is an in-house solver
    specified below. No OpenCV, no system libraries.
11. **The Python tool pack ships in v1**, last milestone. The segmenter is an owner decision
    recorded during M4. Branch A (a permissively licensed public segmenter with an exportable
    checkpoint **and text-prompt input** exists): `segment` ships and success criterion 5
    applies. Branch B (none does):
    M5 ships the pack with provisioning, `echo_region` and the parity harness, `segment` is a
    later milestone, and criterion 5 is replaced by 5b. Either way, the bundle and offline
    install contract below hold.
12. **Fillers are a command or an HTTP endpoint declared in TOML**, PNG in and out, the key from
    a named environment variable, no vendor adapters. A filler's output must match the request's
    dimensions exactly; anything else is an operational error. In v1 a filler's `max_side` is
    met by an integer crop only; a mask whose support does not fit is `not_applicable`.
13. **Measures are evidence, never verdicts.** Synthetic fixtures in git; real photos in a
    private corpus referenced by SHA-256; `rascal lab` runs matrices and writes blind pairs pages;
    pass or fail is a person's or an evaluator's call under a protocol frozen before the run.
14. **Apache-2.0 and the family's repository conventions**: Rust 2024 workspace, `just check`,
    `cargo-deny`, `cargo-dist`, dated ADRs, `docs/owner-decisions.md`, a pre-commit credential
    scanner.

## Architecture

```
            recipe.toml      rascal.toml (fillers, packs, corpus)      image(s), location(s)
                 │                      │                                      │
                 ▼                      ▼                                      ▼
┌───────────────────────────────────────────────────────────────────────────────────────┐
│ rascal-core                                                                           │
│  canonical images · frames + topology · locations · typed artifacts · the mask        │
│  record · effective weight · renderings · recipes (parse, bind, validate, run) ·      │
│  workspace (generations, staging, commit) · journal · identity hashing · config ·     │
│  Op trait, manifest, envelope, registry · compatibility export                        │
└──────────────┬─────────────────────────────┬──────────────────────────────────────────┘
               │ Op trait                    │ Finder/Filler ports
               ▼                             ▼
┌──────────────────────────────┐   ┌────────────────────────────────────────────────────┐
│ rascal-ops                   │   │ rascal-mcp                                          │
│  primitives: smooth_rows,    │   │  client: MCP over stdio to tool packs (segment …)   │
│   sobel, disc morphology,    │   │  server: `rascal serve`, every op as a tool over a  │
│   distance transform, Lab    │   │   workspace; ordinary arguments, no injected object │
│  find: line, rect, stroke    │   └────────────────────────────────────────────────────┘
│  refine: rasterize, tighten, │   ┌────────────────────────────────────────────────────┐
│   grow, shrink, close,       │   │ fillers (out of process)                            │
│   subtract, select, invert,  │   │  command = [...]  or  url = …   PNG in, PNG out     │
│   fan, profile, feather      │   └────────────────────────────────────────────────────┘
│  prep: cut, put, ramp, apply,│
│   window, unwindow           │
│  out: render, paste, measure │
│  inspect                     │
└──────────────────────────────┘
┌──────────────────────────────┐   ┌────────────────────────────────────────────────────┐
│ rascal-mask (the binary)     │   │ tools/python  (rascal-tools, uv, MCP server)        │
│  run · <op> · heal · render  │   │  echo_region, segment (ONNX), later ocr, detectors  │
│  serve · lab · tools ·       │   └────────────────────────────────────────────────────┘
│  corpus · workspace · export │   ┌────────────────────────────────────────────────────┐
└──────────────────────────────┘   │ rascal-testkit: fake fillers, fake pack, independent│
                                   │  minimal MCP client, synthetic image builders       │
                                   └────────────────────────────────────────────────────┘
```

Principles:

- **Files are the state.** Every op takes paths or workspace ids and writes artifacts plus one
  JSON envelope. The MCP server and the lab runner call the same functions on the same files. A
  run is reproducible from its directory alone, which therefore holds every input it used.
- **Deterministic by default.** Every op in `rascal-ops` is a pure function of its inputs and
  resolved parameters: the same artifacts give the same identity hashes. Only finder packs and
  fillers may be otherwise, and their outputs are stored so a replay reuses them.
- **Zones, not pixels.** The mask record says what each pixel's role is. Renderings are
  projections of the record for one consumer, typed as images, never read back as a mask.
- **Nothing silent.** No silent resize, recolour, threshold or fallback. Every transformation is
  a provenance node with resolved parameters and input and output identities.
- **Trust boundary.** Recipes, config and manifests are trusted. Image content, filenames,
  prompts, and anything a finder or filler returns are data.
- **The core is a library** with no IO assumptions beyond paths it is given and no runtime of
  its own; the binary adds `tokio` for process and HTTP plumbing only.

## Domain model

### Frames and images

A **frame** is a coordinate space: a width and height, a projection, a **topology**, and for
a derived frame its parent frame and the mapping to it. A frame has no pixels of its own and
knows nothing of pixel content: two images of the same size and projection are in the same raw
frame. An **`Image`** is pixels in a frame: a lossless PNG plus a record naming its `frame`, its
`role` and its identity hash. Many images share one frame (the canonical pixels, the ramped
working image, a filler's output, a composite, a later pass's input); every op that takes pixels
takes an `Image`, and the frame comes with it; a mask made in a frame applies to any image in it.

| frame id | created by | mapping to parent | topology |
|---|---|---|---|
| `raw:<W>x<H>:<projection>` | import | none (a root) | `wrap` if equirectangular, else `bounded` |
| `cut:<image-id>:<plan-id>` | `cut` | two column spans of the parent, recorded in the plan | `bounded`; its centre column is the parent's wrap edge |
| `crop:<n>` | `window` | an integer bbox of the parent | `bounded` |
| `warp:…` | reserved for the replace milestone | | |

Topology belongs to the frame, not its root: only a complete raw equirectangular frame wraps
horizontally. The workspace keeps a short local alias per frame for the command line; identities
and records use the semantic form.

A **canonical image** is the lossless decode of a job image (JPEG or PNG in; RGB8 or RGBA8),
`role = canonical`, in its `raw:` frame, stored once and never modified. The **baseline `B`** of
a `heal` invocation is the decoded samples of the image that invocation was given, captured
before any prescription is applied: the canonical image on a first pass, or an earlier
composite on a later pass. The root canonical image stays in provenance either way. A **projection** is `flat` or `equirectangular`, declared by the user or
assumed when the image is exactly 2:1 and `--projection` is not given; the assumption is a
journal entry.

Image roles: `canonical`, `working` (after `apply`), `fill` (a filler's output), `composite`,
`rendering` with a `rendering_role` of `mask_input` or `image_input`, `plane` (a binary instance
plane from a finder), `inspection`. No op accepts an `Image` where a `Mask` is declared, whatever
its role.

Pixel coordinates are integer sample indices; normalised coordinates are `x / width`,
`y / height` in `[0, 1]`; radii and widths are in pixels of their frame. "Byte-identical" means
identical decoded 8-bit channel samples, never encoded file bytes.

**`cut`** takes a canonical or working `Image` in a `raw:` frame of width `W ≥ 4` and a strip
fraction `f ∈ (0, 0.5]` (default 0.07). The strip width is

```
s = min( max(2, 2 · round_half_up(f · W / 2)),  2 · floor(W / 4) )
```

so `s` is even, at least 2, and `2s ≤ W` (the strips never overlap). Examples: `W = 4 → 2`,
`W = 5 → 2`, `W = 10, f = 0.07 → 2`, `W = 10, f = 0.3 → 4`, `W = 5760, f = 0.07 → 404`. The canvas
is `2s × H`: the rightmost `s` columns followed by the leftmost `s`, so the wrap edge is the
centre column. No resampling, cropping or padding. The plan records `W`, `H`, `s` and both source
spans. **`put`** takes the parent `Image` and a canvas `Image` in the `cut:` frame, copies the
canvas's left half onto the parent's rightmost `s` columns and its right half onto the leftmost
`s`, and touches no other pixel. A `cut` then `put` with an unchanged canvas is byte-identical.

### Locations

Tagged records, each in one frame, normalised coordinates:

| kind | fields | rules |
|---|---|---|
| `bbox` | `[x0, y0, x1, y1]` | `x0 < x1`, `y0 < y1`; on a `wrap` frame it may not straddle `x = 0` (emit two) |
| `polygon` | 3 to 64 vertices, simple | same seam rule |
| `view` | `yaw`, `pitch`, `fov` | `wrap` frames only |
| `whole_image` | | |
| `absent` | | something missing from this frame |
| `line` (rascal) | `column: x` **or** `path: [[x, y], …]`, plus `width_px` (full width) | a path on a `wrap` frame is split at the seam into parts sharing one `group` |
| `stroke` (rascal) | `path: [[x, y], …]`, `radius_px` | same split rule; the rough mark `tighten` refines |

The first five are the copied kinds, referenced by `$ref` into the copied envelope schema; `line`
and `stroke` are defined in `schemas/rascal/location-v1.json`.

**Mapping.** Every location maps toward its root raw frame through the registry. The
result is `{ parts: [Location], partial: bool, source: location-id }`: a location that crosses a
discontinuity (the centre of a `cut:` frame, the seam of a `wrap` frame) is split into parts, one
per contiguous region in the target; geometry partly outside the parent is clipped and flagged
`partial`; geometry wholly outside is a validation error. Only unclipped, invertible mappings
round-trip exactly, and the tests say so; a clipped mapping keeps the original geometry on the
source side and reports `partial`.

### Artifacts and identity

Every op input and output is a typed artifact with a versioned schema:

| type | on disk | notes |
|---|---|---|
| `Image` | PNG + record | pixels in a frame, with a role |
| `Location` | an entry in the registry | one kind, one frame |
| `Mask` | `<id>.mask/` | the mask record |
| `BandProfile` | `<id>.profile.json` | per-row half-width, the line and the ruler geometry |
| `Series` | `<id>.series.json` | a per-row measurement with row indices, units and the frame |
| `Plan` | an entry in the registry | the cut plan |

The **identity** of an artifact is the SHA-256 of its canonical payload, in which every
reference to another artifact or frame is replaced by that thing's own identity, never by a
workspace-local id, path or filename:

- a **pixel digest** is the hash of an image's decoded samples alone (width, height, channel
  count and the sample bytes in row-major order); it depends on nothing else;
- a **frame**'s identity is the hash of its kind, width, height, projection and topology, and
  for a derived frame its parent frame's identity and mapping parameters (a `cut:` frame: the
  parent identity and the plan's `W, H, s`; a `crop:` frame: the parent identity and the integer
  bbox). A raw frame's identity involves no image, so the dependency order is pixel digest, then
  frame, then image, and there is no cycle;
- a `Plan`'s identity is the hash of the parent frame identity and `W, H, s` and the spans;
- an `Image`: its pixel digest, its frame identity, `role` and `rendering_role`;
- a `Location`: its kind, frame identity and geometry;
- a `BandProfile` or `Series`: its values and metadata, with its frame identity;
- a `Mask`: the three planes' pixels, `use_case`, its frame identity, the identities of the
  profile's line location, band and sigma series, and every prescription's fields with its series
  replaced by that series' identity.

Identity **excludes** ids, filenames, timestamps, durations and provenance. Provenance nodes
reference artifacts by identity; the copy inside `mask.toml` is a convenience and is not hashed.
Two workspaces with different allocation histories running the same recipe on the same inputs
produce the same identities; changing a referenced series under an unchanged local name changes
the referencing mask's identity; import computes the pixel digest, the frame and then the image
identity in that order; tests assert all three.

### The mask record

A directory, `<id>.mask/`, holding `mask.toml` and planes the size of the frame:

```toml
version = 1
id = "seam-01"
frame = "cut:pano:plan-1"
use_case = "seam"                   # seam | remove | repair | protect | custom
identity = "sha256:…"

[zones]
core = "core.png"                   # binary: 255 where a filler may paint; paste weight 1
blend = "blend.png"                 # 8-bit: fractional paste weights outside the core; 0 inside the core; 0 beyond the feather's support
protect = "protect.png"             # binary: 255 where nothing may change; effective weight 0 wherever it is set

[profile]                           # present when the mask is a band along a line
line = "loc-2"
band = "seam-01.profile.json"       # BandProfile
sigma_px = "seam-01.sigma.series.json"   # per-row feather sigma, written by `profile`

[[prescriptions]]                   # pixel changes that belong with this mask; `apply` performs them; `heal` runs `apply` exactly once
kind = "tone_ramp"
version = 1
step = "seam-01.ramp.series.json"   # per-row colour step, 3 channels, smoothed
share = 0.5

[source]
locations = ["loc-2"]
instances = []

[[provenance]]                      # a copy of the journal's nodes for this mask; not part of identity
step = "fan"
op = "fan"
version = "1.0.0"
params = { tex_low = 0.5, tex_high = 2.0, wide_max = 0.15, gate = true, thin_px = 15.0, ref_width = 1620, row_sigma = 0.012, width_sigma = 0.02 }
inputs = { frame = "sha256:…", line = "sha256:…" }
outputs = { band = "sha256:…" }
constants = "fan-v1"
```

Rules:

- **Effective weight** `w` of a pixel is `0` where `protect` is set, else `1` where `core` is
  set, else `blend / 255`. Every rendering, prescription, refinement and paste uses `w`.
- `core` and `protect` are binary; `blend` is zero inside the core and zero beyond the feather's
  support. The feather is defined below; its 50% contour lies outside the core's edge.
- **Refinement postconditions.** An op that changes `core` (`grow`, `shrink`, `close`,
  `select`, `tighten`, `rasterize`) writes `blend = 0` everywhere, removes the `[profile]`
  section and every prescription (they were derived from the old support), and reports
  `blend_cleared = true` and `profile_cleared = true`; the recipe re-feathers afterwards, and
  band measures are unavailable on such a mask until a new profile is made. `subtract` sets
  `protect` and clears `core` and `blend` under it, and keeps the profile and prescriptions
  (`apply` honours the new protect). Every refinement preserves an existing `protect` unless it
  is `subtract`; `invert` is the one op that builds a new protect from an old mask.
- **Prescriptions never touch a pixel with `w = 0`.** `apply` multiplies every prescribed
  change by `[w > 0]` after the prescription's own falloff, so a protected pixel or one outside
  the current support is copied unchanged whatever the prescription says.
- The record is the truth. A rendering is an `Image`.
- Every op that changes a zone appends a provenance node (journal and copy) with resolved
  parameters and input and output identities. `rascal inspect --replay` rebuilds the record from
  the stored inputs and the journal and diffs the planes.

### Renderings

`rascal render <mask> [--image <working image>] --for <profile> --out <path>`. Each profile
declares its role (which filler field it fills), channel format and polarity; `image_input`
profiles take the `Image` to draw on, which must be in the mask's frame:

| profile | role | output | polarity |
|---|---|---|---|
| `grey` | `mask_input` | 8-bit, `round(255 · w)` | 255 = paint |
| `binary` | `mask_input` | 8-bit, 255 where `w ≥ t / 255`, `t ∈ [1, 255]` (default 127); protected pixels are 0 whatever `t` | 255 = paint |
| `alpha` | `image_input` | RGBA of the image, alpha 0 where `core` is set and not protected, 255 elsewhere | transparent = paint |
| `tinted` | `image_input` | RGB of the image with `core` (not protected) blended toward `--colour` (default a magenta no photograph has) by `--strength` (default 0.7) | tint = paint |
| `overlay` | inspection | JPEG: the image, the tint by `w`, the core's edge as a white line | for the eye only |

For a `mask_input` profile `heal` sends the working image as `image` and the rendering as `mask`;
for an `image_input` profile it sends the rendering as `image` and no `mask` unless the filler
entry sets `send_mask = true`, in which case `grey` goes with it. There is no `protect`
rendering: a second pass uses the `invert` op, which produces a real `Mask` that governs both
rendering and paste.

### Operations

Every op implements one trait, carries one manifest (name, version, purpose, when to use, when
not to use, output guidance, cost tier, its inputs with artifact types and which are required,
its outputs with types, input and output JSON Schemas, bounds on what it returns), and returns
one **envelope**: `status` (`ok | measured_nothing | not_applicable | invalid_args`), `summary`,
`measurements` (named scalars with units and the frame), `regions`, `images`, `transforms`,
`provenance`, `artifacts` (rascal's extension: the typed outputs by id and identity). Ops never
return a verdict.

Shared rules: an op validates its inputs' artifact types and frames before running (a mask and
an image must share a frame id and size); a requirement names an input binding and is satisfied
only by an artifact that exists and was produced by a successful step; an op that cannot act on
its input returns `not_applicable` with a reason and **publishes nothing**: no artifact, no
registry change, no operation record. Every invocation, publishing or not, successful or not,
appends one **journal line** (an observation: op, resolved parameters, input identities, the
envelope, duration, and for a committed invocation the record's identity), and every filler and
pack call appends one too (target, request size, duration, status, never a key). The journal is
the diagnostics source; the operation records are the durable subset, and a journal rebuilt by
`workspace repair` holds only committed invocations and says it was rebuilt.

v1 catalogue (`rascal-ops`), with primitives built and tested first (`smooth_rows`, 3×3 Sobel
with edge replication, disc dilate and erode, Euclidean distance transform, sRGB to CIE Lab D65,
the standard normal CDF):

| group | op | in | out |
|---|---|---|---|
| find | `line` | Image; `column` or `path` + `width_px` | Location(`line`) |
| find | `rect` | Image; bbox | Location(`bbox`) |
| find | `stroke` | Image; path, `radius_px` | Location(`stroke`) |
| find | `segment` (pack) | Image; text prompt or points; `max_instances` | Location(`polygon`)[] with scores, plus one `plane` Image per instance |
| refine | `rasterize` | Location, Image (for the frame) | Mask: core = the interior; `bbox`, `polygon` as drawn; `whole_image` all; `line` column form = a band `width_px` wide centred on the column, path form = the path dilated by `width_px / 2`; `stroke` = the path dilated by `radius_px`; `view` and `absent` → `not_applicable` |
| refine | `tighten` | Location(`stroke`), Image | Mask (algorithm below) with `tightened` per sample in the envelope |
| refine | `grow` / `shrink` | Mask; `px`; `grow` also `down_px` | Mask: core dilated or eroded by a disc of radius `px` (outside the image unset, so `shrink` erodes from the border), `grow` then also shifted-union downward by `down_px` (a shadow), minus protect; blend cleared |
| refine | `close` | Mask; `radius_px` | Mask: core closed (dilate then erode, outside the image counting as set so a shape against the border keeps its edge); blend cleared |
| refine | `subtract` | Mask; Mask or Location; `grow_px` | Mask: protect ∪= the second input (rasterized if a location) dilated by `grow_px`; core and blend cleared there |
| refine | `select` | Image[] (`plane` role); keep or drop lists; exclude boxes | Mask: the union of the kept instances as core |
| refine | `invert` | Mask; optional target frame (default the mask's root raw frame) | Mask in the target frame: the old mask's nonzero support and protect are mapped into the target through the frame chain (a `cut:` mask lands on the panorama's two edge spans; a `crop:` mask on its bbox); pixels outside the mapped extent count as untouched; core = untouched and unprotected pixels; blend = 0; protect = mapped support ∪ mapped protect |
| refine | `fan` | Image, Location(`line`, column form) | BandProfile (algorithm below) |
| refine | `profile` | BandProfile; `feather_ratio` | Mask: core = the band at each row's half-width; blend = the feather (algorithm below); `sigma_px` series |
| refine | `feather` | Mask; `sigma_px` | Mask: blend = the feather of the core's boundary outward (algorithm below); existing blend replaced; protect preserved |
| prep | `cut` | Image (`raw:`, equirectangular); `strip` | Image (`cut:` frame), Plan |
| prep | `put` | Image (`raw:`), Image (`cut:`), Plan | Image (`raw:`) |
| prep | `ramp` | Image, Mask with a profile; `share`, `ramp_sigma` | Mask: the same record with a `tone_ramp` prescription added (estimation only; no pixels change) |
| prep | `apply` | Image, Mask | Image (`working`): the image with every prescription applied once, in order; protected pixels untouched |
| prep | `window` | Image, Mask; `max_side`, `context` | Image (`crop:` frame), Mask (`crop:` frame): an integer crop around the mask's nonzero support (contract below) |
| prep | `unwindow` | Image (parent), Image (`crop:`) | Image (parent frame): the crop's pixels written back into their bbox |
| out | `render` | Mask [, Image]; profile | Image (`rendering`) |
| out | `paste` | Image (base), Image (fill), Mask; `mode` | Image (`composite`) (contract below) |
| out | `measure` | Image (`before`), Image (`after`), Mask [, Location(`line`)]; `set` | Series[] and scalar summaries (below) |
| inspect | `inspect` | Mask [, Image] | overlay JPEG, the per-row chart, with `--json` the record and zone statistics, and with `--replay` the provenance diff |

### Algorithms

Normative. Constants are named sets with a version, shipped as recipe defaults and expected to
be refitted on the owner's corpus; a change to a value is a new set version. Initial values:

| set | values |
|---|---|
| `fan-v1` | `ref_width 1620`, `thin_px 15`, `gap_px 4`, `step_cols 10`, `texture_cols 160` (the three window sizes scaled by `w / ref_width` and rounded to integers ≥ 1; `thin_px` scaled unrounded), `tex_low 0.5`, `tex_high 2.0` (contrast, percent per pixel), `lum_floor 40`, `step_min 0.5`, `step_ref 4.0` (levels of 255), `wide_max 0.15` (fraction of `w`), `row_sigma 0.012`, `width_sigma 0.02` (fractions of `h`), `gate true` |
| `profile-v1` | `feather_ratio 0.55` |
| `ramp-v1` | `share 0.5`, `ramp_sigma 0.02` |
| `feather-v1` | `support 3.0` (sigmas) |
| `tighten-v1` | `min_contrast 12` (levels), `min_defect_px 1`, `max_defect_px 2 · radius`, `path_sigma 4` (samples), `sample_spacing_px 1` |
| `window-v1` | `context 0.5` |
| `poisson-v1` | `tol 1e-4`, `max_iter 2000` |
| `fan-thin-v1` | `fan-v1` with `wide_max 0.0`: the effective upper width is then `thin`, so every row is the minimum width at any image width; the lab's fixed-width arm |

**Row smoothing** (`smooth_rows(v, σ)`): a Gaussian kernel truncated at `3σ`, renormalised over
the samples it covers at each position, applied along the row axis; `σ < 0.5` px is the identity.

**Feather** (used by `profile` and `feather`): with `D(x, y)` the Euclidean distance from a pixel
outside the core to the nearest core pixel's edge (distance transform of the complement, in
pixels, measured from pixel centres with the core's boundary at half a pixel), and `σ` the row's
or the op's sigma,

```
blend = round(255 · exp(−D² / (2σ²)))      for 0 < D ≤ support · σ,   else 0
```

The weight tends to 1 at the core's boundary, the 50% contour is at `D = σ · √(2 ln 2) ≈ 1.1774 σ`
outside the core, and the support is finite. Protected pixels get `blend = 0`.

**`fan`** on a `bounded` frame of width `w`, height `h`, centre column `c = column · w` rounded,
with `fan-v1`; `thin = thin_px · w / ref_width`:

1. The core's reference edge is `c ± thin`. The step windows are the `step_cols` columns ending
   `gap_px` before `c − thin` (left) and starting `gap_px` after `c + thin` (right); the texture
   windows are the `texture_cols` columns ending at `c − thin` and starting at `c + thin`. All
   windows are clipped to `[0, w)`; a window the frame clips to fewer than 2 columns (or to
   fewer than it asked for, when the scaled geometry asked for one) is `not_applicable`.
2. Luminance `L = 0.299 R + 0.587 G + 0.114 B`.
3. *Step* per row: the mean over channels of `|mean(left) − mean(right)|` on the step windows.
4. *Texture* per row: the mean 3×3 Sobel gradient magnitude of `L` (divided by 8; edge
   replication at the frame border) over the texture windows, divided by
   `max(mean L over the same columns, lum_floor)`, times 100.
5. Both smoothed with `smooth_rows(·, row_sigma · h)`.
6. `smooth = clamp((tex_high − texture) / (tex_high − tex_low), 0, 1)`;
   `open = gate ? clamp((step − step_min) / (step_ref − step_min), 0, 1) : 1`;
   `wide = max(thin, wide_max · w)` (the effective upper width, so a set with `wide_max = 0`
   fixes every row at `thin`);
   `raw_half = thin + (wide − thin) · smooth · open`;
   `half = clamp(smooth_rows(raw_half, width_sigma · h), thin, wide)`.
   A closed gate yields `raw_half = thin` on that row; after row smoothing a row within a few
   `width_sigma · h` of an open row may be wider than `thin`, by design.
7. Output: the `BandProfile`: `half[y]`, `c`, `thin`, and the resolved window geometry (`gap`,
   `step_cols`, `texture_cols` in pixels), so `ramp` and the measures reuse exactly these columns.

**`profile`**: `core` is set where `|x + 0.5 − c| ≤ half[y]`; `σ[y] = feather_ratio · half[y]`
is written as the `sigma_px` series; `blend` is the feather above with the per-row σ (the distance
transform is 2-D, the σ is the row's). The band is symmetric about `c` by construction.

**`ramp`** estimates, per row and channel, `Δ[y] = mean(right) − mean(left)` on the step windows
recorded in the `BandProfile`, smoothed with `smooth_rows(·, ramp_sigma · h)`, and stores it as a
Series prescription. **`apply`** of a `tone_ramp` moves each pixel with `w > 0` at distance
`d = |x + 0.5 − c|` by `sign · share · Δ[y] · max(0, 1 − d / half[y])`, the left side toward the
right (`+`) and the right toward the left (`−`), rounded and clamped to 8 bits; pixels with
`w = 0` are untouched. The ramp is zero at and beyond the band's half-width.

**`tighten`** with `tighten-v1`: the stroke's path is resampled at `sample_spacing_px` by linear
interpolation; the local direction at a sample is the central difference of its neighbours
(one-sided at the ends); a sample whose central difference is zero (the path turns back on
itself) takes the previous sample's direction, the first such sample takes the next nonzero one,
and a path with no nonzero direction is `not_applicable`. For each sample, `L` is read by bilinear interpolation along the
perpendicular over `[−r, r]` at `⌈2r⌉` equal steps (about 1 px, symmetric about the path), and
its **signed** first derivative `g(t)` per pixel is taken by central differences. A boundary pair is an index pair `(a, b)` with `a < b`, `g(a)` and
`g(b)` of opposite sign, `|g(a)|, |g(b)| ≥ min_contrast`, and `min_defect_px ≤ b − a ≤
max_defect_px`; its score is `|g(a)| + |g(b)|`; ties break toward the pair whose midpoint is
nearest `t = 0`, then toward the narrower pair. A sample with no admissible pair is
**untightened**: its extent is `[−r, r]` and it is excluded from smoothing. Over each maximal run
of tightened samples, `a` and `b` are smoothed separately with `smooth_rows(·, path_sigma)`; if
after smoothing `a ≥ b` at a sample, that sample reverts to untightened (with one kernel on
both, proper extents cannot cross by smoothing alone; the reversion is a guard, kept). The core is the union of
the quadrilaterals between consecutive samples' `[a, b]` extents, with round joins between them
and round caps of the local half-extent at both ends; untightened samples contribute their full
`[−r, r]`, so an entirely untightened stroke gives exactly the mask `rasterize` gives the same
stroke.

**`window`** with `window-v1`: the **support** is the integer bbox of pixels with `w > 0`
(`x0, y0` floored, `x1, y1` ceiled to pixel edges); an empty support is `not_applicable`, and a
support with either side larger than `max_side` is `not_applicable` with the sizes in the reason
(scaled windows are a later milestone). The support is mandatory; **context** is optional: the
window is the support expanded by `context` times its width and height on each side, then made
square at the larger side, then capped to `max_side` on each side, then capped to the image's
width and height, then slid inside the image (toward the smaller offset when two placements
tie). Capping shrinks context only; the support is always inside the window. The crop is that
integer bbox; the mask's planes are cropped by the same bbox into the new `crop:` frame; nothing
is resampled. **`unwindow`** writes the crop back into exactly that bbox. Tests: full-height
support (the window is the full height, context only sideways), support touching each border,
support that fits while its requested context does not.

**`paste`** contract. Inputs: `base`, `fill`, `mask`, all in one frame and size; `fill` RGB or
RGBA (its alpha is ignored and the journal says so); `base` RGB or RGBA (its alpha is kept
unchanged in `out`). All arithmetic is per RGB channel on 8-bit sRGB sample values as `f32`,
rounded and clamped at the end. `out` starts as a copy of `base`; only pixels with `w > 0` are
written; pixels with `w = 0`, protected pixels included, are copied without arithmetic, which is
the preservation guarantee of a standalone paste: `out = base` wherever `w = 0`. Modes:

- `feather`: `out = (1 − w) · base + w · fill`.
- `hard`: `out = fill` where `core` is set and not protected; nothing else changes.
- `poisson` (M7): `Ω` is every pixel with `w > 0` and not protected. Per channel, solve the
  discrete Poisson equation on the 4-neighbour grid over `Ω` with the **source gradients of
  `fill`** (not mixed gradients) inside `Ω` and **Dirichlet values from `base`** at every
  neighbour outside `Ω`, protected holes included. An image-border pixel with a missing neighbour
  has one fewer term in its equation. `feather` and `hard` work in any topology (they need no
  adjacency); `poisson` refuses a `wrap` frame in v1 (the seam canvas is `bounded`), so there is
  no wrap adjacency in the solve. A connected component of `Ω`
  with no neighbour outside `Ω` is `not_applicable` (nothing to anchor to). Conjugate gradients
  in `f32` with the initial guess `fill`; the residual of the current iterate is evaluated
  before every convergence test, including the first, and the solve is converged when
  `‖r‖₂ ≤ tol · max(‖b‖₂, 1)`; `max_iter` reached is an operational error, never a fallback. The solution is
  clamped to `[0, 255]`, then `out = (1 − w) · base + w · solution`, so the blend zone still
  fades. Tests: a `fill` equal to `base` returns `base` for every mode; a constant `fill` on a
  constant `base` of another value returns `base` exactly under `poisson` (zero source gradients,
  boundary from `base`), including the single-editable-pixel case where `b = 0` and the initial
  guess is wrong; a linear-gradient `fill` on a matching linear `base` is reproduced to one level.

### Recipes

```toml
version = 1
name = "seam"
use_case = "seam"
projection = "equirectangular"

[inputs]
image = { type = "Image" }              # the CLI imports a path as a canonical Image in its raw: frame

[[step]]
id = "cut"
op = "cut"
inputs = { image = "inputs.image" }
strip = 0.07

[[step]]
id = "line"
op = "line"
inputs = { image = "cut.image" }
column = 0.5

[[step]]
id = "fan"
op = "fan"
inputs = { image = "cut.image", line = "line.location" }
constants = "fan-v1"

[[step]]
id = "band"
op = "profile"
inputs = { band = "fan.band" }
constants = "profile-v1"

[[step]]
id = "ramp"
op = "ramp"
inputs = { image = "cut.image", mask = "band.mask" }
constants = "ramp-v1"

[outputs]                               # the contract `heal` consumes, by output name, never by step name
mask = "ramp.mask"                      # Mask, required
image = "cut.image"                     # Image in the mask's frame, required
plan = "cut.plan"                       # Plan, required when the mask's frame is a cut:
render = [{ for = "grey", out = "seam-grey.png" }]
```

Bindings are `<step id>.<output name>` or `inputs.<name>`, typed; validation fails on a type
mismatch, an unknown op or parameter, an unbound input, or a requirement pointing at a missing or
failed step, with the step id in the message. Defaults are resolved at validation and the
resolved values are what provenance records. `rascal run` writes, under `--out`, a workspace (`images/` with the
canonical images, `artifacts/`, `ops/`, `registry/`, `CURRENT`, `journal.jsonl` with one line
per invocation: step id, op, version, resolved params, input identities, output identities,
envelope, duration) plus the projections `frames.json` and `locations.json` and the outputs
named; with `--workspace` the artifacts and the journal go to that workspace instead. A recipe's `[inputs]` may declare any artifact type;
the CLI binds them from `--location`, `--stroke`, `--protect`, `--mask` and `--prompt` (the last
through a configured pack), and `heal` passes the same typed input map through unchanged; `--image` may name a path or a
workspace `Image` id, so a composite can feed the next pass without re-import. The six shipped recipes run, literally as shipped, as acceptance fixtures.

**`heal`** is the fixed typed graph below, with `F` the recipe's mask frame:

```
R      = run(recipe, inputs)                        → the recipe's declared outputs: mask M in F, image I_F in F, plan (when F is a cut:)
W      = apply(I_F, M)                              → working Image in F
(Wc, Mc) = window(W, M, filler.max_side)            → crop frame C   (identity when max_side is unset: Wc = W, Mc = M, C = F)
Rend   = render(Mc [, Wc], filler.profile)          → rendering
Fill   = filler(Wc or Rend, Rend?)                  → fill Image in C, dimensions asserted
P      = paste(Wc, Fill, Mc, filler.paste)          → composite in C
W'     = unwindow(W, P)                             → composite in F   (identity when C = F)
Out    = F is cut: ? put(inputs.image, W', R.plan) : W'   → the result in raw:
check:   for every raw pixel whose mapped w = 0, and every protected pixel, Out = B (this invocation's input image)
measure: band set with before = I_F, after = W' in F (the mask's frame); drift with before = I_F, after = W'
```

The journal gains the filler call: command or URL, request dimensions, duration, bytes, exit
status; never a key.

Shipped recipes, embedded and addressable by name as well as by path:

- `seam`: as above. Input: an equirectangular image.
- `remove`: `inputs.image`, `inputs.location` (a `bbox`, `polygon` or `stroke`, or the planes of
  `segment` via `--prompt` when a pack is configured) → `rasterize` or `select` → `grow` (`px`,
  `down_px`) → `close` → `subtract` (optional `inputs.protect`) → `feather`. Requires a location
  or a prompt.
- `repair`: `inputs.stroke` → `tighten` → `feather`. For an area, `repair-area`: `inputs.location`
  (`bbox` or `polygon`) → `rasterize` → `feather`. The path fan is a later milestone.
- `rect`: `inputs.image`, `inputs.location` (a `bbox`) → `rasterize`, one step. The utility
  recipe: a rectangle as a mask, used by the walking slice, the tests and anyone who wants a
  plain box. `--bbox x0,y0,x1,y1` on the command line is sugar that binds `inputs.location` to
  a new `bbox` location in the image's raw frame; it adds no step.
- `protect`: `inputs.image` (the earlier composite, a workspace `Image` or a path, in a raw
  frame) and `inputs.mask` (the earlier mask, in that raw frame or any frame derived from it) →
  `invert` into the image's raw frame. Its outputs are the inverted mask and the image, with no
  plan: the second pass runs in the raw frame and may change everything the first pass left
  alone, the panorama's middle included, while the first pass's edited pixels are protected.
  `heal protect` takes the composite as its input image, so its preservation check is against
  that composite. `heal seam` followed by `heal protect` in one workspace is an end-to-end test:
  the first edit is preserved and a pixel in the middle may change.

### Fillers

```toml
# rascal.toml
[fillers.local-inpaint]
command = ["python", "fill.py", "{image}", "{mask}", "{out}"]   # placeholders substituted; exit 0 and a PNG at {out}
profile = "grey"
paste = "feather"
timeout_s = 120
revision = "fill.py 2026-10-28"                 # free text the lab hashes into cell identity; required for lab use

[fillers.hosted]
url = "https://example.invalid/inpaint"        # multipart POST: image, mask (when the profile has one), prompt?; PNG back
profile = "binary"
paste = "hard"
key_env = "HOSTED_FILLER_KEY"                  # Authorization: Bearer; never stored, logged or journaled
timeout_s = 180
max_side = 2048
revision = "vendor model 2026-10"
```

A filler receives an image (and a mask for `mask_input` profiles) of exactly the dimensions
rascal chose and must return a PNG of those dimensions; any other size, a non-zero exit, a
timeout or a non-PNG body is an operational error. There is no automatic resize of a filler's
output, and no scaled request in v1.

### Workspace

A **workspace** is a directory:

```
images/                 canonical PNGs by identity
artifacts/<identity>    every other artifact file or directory, named by identity (collisions impossible; orphans harmless)
registry/<n>.json       one registry generation: frames, locations, artifacts, plans, the id counter
CURRENT                 the generation number, written by atomic rename
staging/<token>/        one directory per in-flight op, holding a `lock` the owning process keeps locked for its lifetime
journal.jsonl           observations, appended after commit
lock                    the writer lock, held only across a commit
```

**Commit protocol.** An op writes its outputs into its staging directory together with an
immutable **operation record** `ops/<identity>.json` (op, version, resolved parameters, input
and output identities, the envelope, the provenance node, and two observations outside its
identity: `duration_ms` and the workspace sequence number it committed at; its identity is the
hash of the rest) and validates everything. To commit it syncs every staged file and every directory of the staged
trees (a mask is a directory), takes the writer lock, moves artifact files and directories into
`artifacts/` and `images/` and the record into `ops/` by rename (an existing identical identity
is left in place), syncs those three directories so the new entries are durable, reads
`CURRENT`, writes `registry/<n+1>.json` with the new entries, the advanced counter and the
record's identity, syncs that file and then `registry/`, writes the new pointer to a temporary
file, syncs it, renames it over `CURRENT`, syncs the workspace directory, releases the lock, then
appends to the journal. Visibility is the rename; durability is acknowledged only after the
final sync, and an op reports `committed` only then. Everything `CURRENT` names is durable before
`CURRENT` is; a crash before it publishes nothing (orphaned files are unreachable and
are collected by `workspace repair`); a crash after it leaves a committed generation whose
operation record is on disk but not yet journaled, and `rascal workspace repair` rebuilds the
journal as a projection of the committed records. The journal is therefore never the only copy
of anything. Readers read `CURRENT` once and then the named generation, so they see a consistent
snapshot. Concurrent ops stage in parallel and commit in
turn. **Cancellation** is checked before the writer lock is taken; once taken, the commit runs to
completion and the op reports `committed = true` with `cancelled = true`; cancelled before,
nothing is published and the staging directory is removed by its owner. On open, a staging
directory whose `lock` can be acquired has no owner and is removed; one that cannot be acquired
is active and is left alone. A registry generation that names an artifact missing on disk
refuses the workspace until `rascal workspace repair` has run.

The CLI takes `--workspace <dir>` on every op; without it an op uses a private throwaway
workspace under `--out`.

### The MCP server and client

**`rascal serve --workspace <dir>`** is an MCP server over stdio at protocol revision
`2025-11-25`. It advertises one tool per op with the op's manifest and its input schema as the
tool schema, with **ordinary arguments only**: artifact ids and parameters. No argument is
injected; the server resolves ids against its own workspace. Workspace tools: `workspace_import`
(a path or base64 image in; a canonical `Image` and its `raw:` frame out), `workspace_list`
(artifacts by type), `workspace_describe` (any artifact's record as JSON), `workspace_read` (an
`Image` artifact's bytes as one image content block, bounded by `max_image_bytes`; any other type
is `invalid_args`). Results are the envelope as `structuredContent` plus image content blocks in
exactly the count and order of `images`. Errors map: validation to `invalid_args` in the
envelope; operational failure to a JSON-RPC error carrying the error class. Calls are serial per
workspace by default; `--concurrency n` allows parallel calls, each still committing in turn. A
standard MCP client with no knowledge of rascal can import an image, list tools, run a recipe's
steps one by one and read the result; the testkit's independently written minimal client proves
it.

As a **client**, `rascal-mcp` starts configured packs:

```toml
[packs.rascal-tools]
command = ["…/.venv/bin/rascal-tools", "serve"]
env = ["RASCAL_TOOLS_MODELS"]                  # declared additions to the baseline environment
accepts = "path"                               # or "base64"
concurrent = false                             # effective concurrency = this AND the manifest's
revision = "segmenter export 2026-10"          # free text the lab hashes into cell identity; required for lab use
startup_timeout_s = 60
call_timeout_s = 120
```

The pack's environment is `PATH`, `HOME`, `LANG`, `LC_ALL`, `TMPDIR` plus the declared `env`
names, **minus** every variable any filler names as `key_env` and minus any name ending in
`_KEY`, `_TOKEN` or `_SECRET`; removal wins over declaration. Startup: spawn, `initialize` (the
server must answer `2025-11-25` and advertise `tools`), `tools/list` to exhaustion, manifest
validation: each tool carries in its MCP `_meta` the copied-shape `manifest`
(`schemas/copied/manifest-v1.json`) and rascal's annotation block `rascal`
(`schemas/rascal/pack-annotation-v1.json`: typed ports, parameters, constant sets); the native
manifest is built from the two, checked as every op's is, and its bounds must stay under
rascal's ceilings, the whole answer under the 256 MiB one line of the wire carries (ADR 0019). A finder's variable outputs are **many ports**: an output port
marked `many` takes any number of artifacts named `<port>_<n>` from 0, required meaning at
least one; a tool's regions become `Location` artifacts and its images `Image` artifacts with
the `plane` role, in order, on the many ports of those types. A call carries the op's arguments
(parameters by name, an `Image` port's id, a `Location` port's location) plus `_harness`
(rascal's client-side context: `run_id`, `call_id`, the referenced images by path or base64
with their records, and every frame they are in up to the root;
`schemas/rascal/pack-context-v1.json`); this object exists only between rascal and its packs
and never appears in the public server's tool schemas. **Result validation before anything is
committed**: the envelope as rascal's closed type and within the manifest's bounds, `isError`
exactly for `invalid_args`; image blocks in exactly the count, order and media types of
`images`, each decoding to the size its entry and frame say; every returned region's and
image's frame among the frames passed in the call, with valid geometry; failure is an
operational error and nothing from that call is stored. Concurrency is allowed only when both
the manifest and the config permit it; a timeout, a cancellation or the death of the process
makes every in-flight call on that pack an operational error and shuts the pack down before any
reuse: stdin close, 5 s, SIGTERM, 5 s, SIGKILL; the next call starts afresh and the restarted
pack must list the same tools. Captured stderr goes to the journal with credential redaction
applied. The journal records each call (`pack call:`, never a key), the pack's listing at
startup (`pack manifest:`) and each successful call's request identity, envelope and outputs
(`pack stored:`), so `run --replay <recorded run>` and `heal --replay` answer pack steps from
the recording without starting the pack (G4); a request the recording lacks is an error, never
a call. `rascal tools list --config` starts every pack and prints its tools; `rascal tools call
<pack> <tool> --workspace … --<port> … --<param> …` runs one as its op subcommand.

### Measures

`measure` takes `before` and `after` images in **one frame** and a `Mask` in that frame, with an
optional `line` location, and a `set` naming the measures; a measure whose inputs are absent
returns `not_applicable` for itself and the rest proceed. Band measures require a mask with a
profile and use its recorded window geometry. Each measure has a versioned definition
(`measures-v1`). Per-row results are `Series` with row indices and units; summaries (mean,
median, p95, max, and `rows_skipped`) go into the envelope. Sampling windows are clipped to the
frame; a row whose numerator or denominator window is empty after clipping, or whose denominator
is `0`, is skipped and counted; a hue difference is omitted where either side's chroma is below
1 and is otherwise the signed circular difference in degrees in `(−180, 180]`. When every row is
skipped the measure returns `measured_nothing`, writes no series and no summaries, and reports
`rows_skipped = rows`. Every serialised value is finite.

| measure | needs | definition |
|---|---|---|
| `continuity` | `after`, a line | per row, the mean over channels of the absolute difference between the two columns adjacent to the line, divided by the row's mean absolute difference between neighbouring columns |
| `tone_step` | `after`, a profile | per row, the mean colour of the profile's step windows on each side in CIE Lab (D65, sRGB), reported as ΔL, Δchroma, Δhue |
| `transition` | `after`, a profile | per row, the mean Sobel magnitude of L over `[c − half, c + half]` divided by the row's mean outside `[c − 2 · half, c + 2 · half]` |
| `texture_match` | `after`, a profile | per row, the mean Sobel magnitude of L inside the core divided by that over `[half, 2 · half]` beyond it on each side |
| `drift` | `before`, `after`, a mask | mean absolute 8-bit difference per channel inside `core`, inside `blend`, and where `w = 0` (the last must be `0`) |

In `heal` the measures run in the mask's frame on `I_F` and `W'`; the preservation check runs in
raw coordinates on `B` and `Out`.

### The lab

```toml
# matrix.toml
version = 1
name = "seam-01"
corpus = "corpus.toml"
protocol = "protocol.toml"                       # required; its digest is recorded; lab run refuses to start without it
seed = 20261028

[[image]]
id = "sky-02"
inputs = { location = "locations/sky-02-stroke.json" }   # per-image recipe bindings, when a recipe needs them

[[arm]]
name = "thin"
recipe = "seam"
overrides = { "fan.constants" = "fan-thin-v1" }
filler = "local-inpaint"
spend_usd_per_call = 0.0

[[arm]]
name = "fan-ramp"
recipe = "seam"
filler = "local-inpaint"
```

A **cell** is one arm on one image. Its identity is the SHA-256 over: the image's identity, the
identities of every bound input artifact and the values of every scalar input (a prompt, points,
a column), the recipe text, the resolved overrides, the constant set versions, the op versions
involved, the filler entry (command or URL, profile, paste, `revision`; never the key), and, for
every finder the arm would invoke, its **invocation descriptor**, computed before any finder
runs: the pack name, the bundle digest, the pack entry's `revision` (a required free-text field
on a pack used by the lab, bumped by the owner when a model or its configuration changes), the
declared `env` names, the tool name, the resolved arguments and the semantic identities of its
inputs. The cell key is therefore known before execution; a finder or filler runs only on a key
miss or `--redo`. To keep that true, `rascal lab run` validates every arm's recipe and rejects
one in which a finder's input derives, through any chain of bindings, from another finder's
output (a dependent-finder recipe runs under `rascal run`, just not in the lab in v1); every
finder input in a lab recipe comes from the initial inputs or from deterministic ops over them,
so every descriptor is computable from the initial inputs and the resolved invocation graph.
Concrete intermediate identities are recorded inside the attempt. Changing only a prompt, a finder parameter or a pack `revision` is a new cell;
a key hit makes no external call; a dependent-finder recipe is rejected by `lab run` with the
offending binding named; tests say so. `rascal lab run` writes
`lab/<name>/cells/<identity>/attempts/<n>/` with the run directory, the result image (and the
source it started from, named by the attempt record) and `cell.json`; `latest` points at the attempt in use; a ledger records every external call. Resume
reuses a cell only when its identity matches and `latest` is complete; `--redo <arm>` adds an
attempt and never overwrites one. `rascal lab pairs --arms a,b` writes `pairs/assignment.json`
(version, pair ids, the two cell identities and attempts per pair, the side assignment drawn
from `seed`, the protocol digest) and `pairs/index.html` (static, images embedded, order drawn
from `seed`; each pair shown at its focus, the region both results changed in the source, at
full size with the sides scrolling together, whole, and for an equirectangular frame from
inside the sphere with one orientation for both sides); the page writes `picks.json` (version,
assignment digest, one of `left | right | same | neither` per pair id). `rascal lab decode` rejects picks whose assignment digest does not
match or that are missing, duplicated or foreign, and reports per group: decided pairs (`left`
or `right`), wins, losses, `same`, `neither`, and the outcome under the protocol.

`protocol.toml` names the groups (corpus ids), the arms compared, the seed, and the rules:
`preferred` when wins over decided pairs `≥ 2/3`; `not_worse` when losses over decided pairs
`< 1/3`; `inconclusive` when a group has fewer than `min_decided` (default 10) decided pairs;
`same` and `neither` count toward neither side. The protocol is committed before `lab run` is
invoked and the run records its digest; a protocol change after a run is a new lab name.

A corpus manifest lists images by id, SHA-256, group and licence note, with a location outside
the repository (`RASCAL_CORPUS`, default `~/rascal/corpus`); `rascal corpus verify` checks
hashes. No real photograph is ever committed.

### Compatibility export

`rascal export --compat` takes a native envelope (from a journal line or an op's output) and
**projects** it: it keeps `status`, `summary`, `measurements`, `regions`, `images`, `transforms`
and `provenance`, and drops `artifacts` and every other native field, since that is rascal's
bookkeeping and not content. It then fails, naming the item, on any content the copied schema
cannot express: a `line` or `stroke` region, an image whose mapping to its parent is not a plain
bbox; nothing is approximated. A standalone location exports as the location object alone and
validates against the copied envelope's embedded `Location` definition (by `$ref`), not as an
envelope. The schema tests validate exports of real native outputs (a `rect` envelope, a
`segment` envelope, a `bbox` location) against the copied schema, assert the two failure cases,
and validate native documents against `schemas/rascal/`.

### The Python pack and its bundle

`tools/python` is a uv project. Its release bundle is one directory per supported platform containing:
`bundle.toml` (bundle version, the pack version, the required Python version, the platform, the
SHA-256 of every file below), the pack's wheel, a **wheelhouse** with every transitive dependency
as a wheel for that platform and Python, a provisioning `pyproject.toml` that pins the pack
wheel by relative path, its `uv.lock`, `MODEL.md`, and under branch A the model artifact bytes
themselves with their digests in `artifacts.toml`. Prerequisites on the machine: `uv` and an
interpreter of the required version; both are named in the README and checked by `setup`.
`rascal tools setup` takes a bundle (downloaded by `setup` when given a URL, which is the only
network step, or local with `--bundle`), verifies every digest, and installs with downloads
prohibited: `uv sync --frozen --no-dev --offline --no-index --find-links <wheelhouse>` with
`UV_PYTHON_DOWNLOADS=never` and empty caches. Launch is always offline:
`<dir>/.venv/bin/rascal-tools serve` with `UV_OFFLINE=1`. The disconnected acceptance test
starts from a local bundle, a pre-installed interpreter and `uv`, with the network disabled and
the uv and pip caches empty, installs, launches and calls `echo_region`, and `segment` under
branch A.

### Diagnostics

Every invocation appends a journal line and every committed invocation an operation record,
whether or not anyone looks. The diagnostic surface reads the journal (and, after a repair, the
rebuilt journal from the records) and nothing else, so it is optional and costs nothing when
unused. Two forms:

- **The run page.** `rascal dash <run directory or workspace> [--out page.html | --serve]`
  writes a static page (images embedded, from a template authored here) or serves it on
  loopback: a timing waterfall with one bar per step in journal order and the filler and pack
  calls marked; every artifact as a thumbnail that opens at full size; for a band mask the
  rulers' per-row charts beside the strip they measured; the measures with their series; the
  envelope of every step; the resolved parameters and constant set versions. The page names every journal line (steps, repeated steps, `not_applicable` and failed
  invocations marked as such, filler and pack calls) and nothing else, and embeds every artifact
  the journal names as a `data:` URI; a test asserts both. `heal` and `lab run` write it beside
  their journal by default (`--no-dash` skips it); `serve` exposes a `workspace_dash` tool that
  writes the page into the workspace and returns its path.
- **Tracing.** `RASCAL_LOG=<level>` turns on `tracing` spans to stderr: one span per op with its
  duration and identities, one per filler or pack call with request size and status, never a
  key or a pixel. Off by default. Inside `serve`, the same spans go to stderr only (never to the MCP stream),
  so an agent's operator can watch a session live.

Every op that produces something a person would want to see (a chart, a profile, a series)
registers a **panel** with the run page in the task that adds the op, so the view is never
behind the tool.

## Tech stack

- Rust stable, edition 2024, pinned by `rust-toolchain.toml`. Crates: `image`, `imageproc`,
  `rayon`, `serde`, `serde_json`, `toml`, `schemars`, `jsonschema`, `clap`, `thiserror`,
  `tracing` with `tracing-subscriber` (binary only: the `RASCAL_LOG` sink), `sha2`, `fs4` (file
  locks), `tokio` (binary only: process and HTTP), `reqwest` with
  `rustls` (binary only, HTTP fillers), the `rmcp` MCP crate pinned by ADR 0015 (the
  in-house alternative was declined there) with `tokio-util` (its cancellation token) and
  `nix` (SIGTERM in the pack shutdown escalation; ADR 0019). Anything beyond this list needs
  an ADR.
- Python 3.12, `uv`, `pytest`, `ruff`, the MCP Python SDK, `onnxruntime`, `numpy`, `pillow`.
  Supported in v1: macOS arm64 and Linux x86_64.
- GitHub Actions; `cargo-dist` for release archives; `cargo-deny` for licences and sources.
- No system libraries for the binary. No OpenCV. No GPU requirement.

## Commands

```
# everything a commit must pass
just check                                     # fmt, clippy -D warnings (both feature sets), cargo test, deny, crate graph, packaging

# Rust
cargo build --workspace
cargo test --workspace                         # unit, golden, schema, workspace, fake MCP both sides, offline e2e; no network
cargo test --workspace --features rascal-mask/testkit
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
scripts/acceptance.sh                          # release acceptance: install, recipes by name, serve, bundle offline, docs coverage

# Python tool pack
cd tools/python && uv sync --frozen                        # development only; may download
uv run --frozen --offline --no-sync rascal-tools fetch     # model artifacts; network, explicit
uv run --frozen --offline --no-sync pytest                 # offline; segmenter tests skip until fetched
uv run --frozen --offline --no-sync ruff check . && uv run --frozen --offline --no-sync ruff format --check .

# the binary
rascal run seam pano.jpg --out run/
rascal run remove room.jpg --location chairs.json --out run/
rascal run repair print.jpg --stroke scratch.json --out run/
rascal run ./my-recipe.toml image.jpg --out run/
rascal heal seam pano.jpg --filler local-inpaint --config rascal.toml --out run/
rascal fan --workspace ws/ --image img-3 --line loc-2
rascal render run/artifacts/<id> --for tinted --image img-3 --out seam-tinted.png
rascal paste base.png fill.png run/artifacts/<id> --mode poisson --out composite.png
rascal measure --before a.png --after b.png --mask run/artifacts/<id> --json
rascal inspect run/artifacts/<id> --out inspect/ [--replay] [--json]   # --json: the record, zone statistics, identity
rascal ops list [--json]                                   # every op's manifest as a client sees it
rascal ops describe fan --json
rascal dash run/ --out run/dash.html                        # the run page (Diagnostics); --serve for loopback
rascal export --compat run/journal.jsonl --step fan --out fan-envelope.json
rascal serve --workspace ws/ --config rascal.toml
rascal workspace list ws/ [--type Mask]
rascal workspace repair ws/
rascal tools list --config rascal.toml
rascal tools call rascal-tools segment --image img-3 --prompt "chair"   # one pack tool from the shell
rascal tools setup --config rascal.toml [--bundle <dir>]   # network unless --bundle and artifacts are local
rascal lab run matrix.toml --out lab/seam-01/
rascal lab pairs lab/seam-01/ --arms thin,fan-ramp
rascal lab decode lab/seam-01/ picks.json
rascal corpus verify corpus.toml
```

Exit codes: 0 ok; 1 the op reported `not_applicable` or `measured_nothing` under `--strict`; 3
operational error (a filler failed, a pack died, IO, non-convergence, a preservation check, a
result that failed validation); 64 usage error. A bad parameter (`invalid_args`) is a usage
error, 64, with or without `--strict`. Three kinds of `--out`: a **run directory** (`run`,
`heal`, `inspect --out`, `dash --out <dir>`) must be new or empty, else usage error 64; a
**file destination** (`render`, `put`, `paste`, `export`, `dash --out <file>`) is written over an
existing file, as `cp` would; a **lab directory** (`lab run --out`) is resumable: it must be new
or hold a lab of the same matrix name, and anything else is usage error 64.

## Project structure

```
Cargo.toml                       workspace
rust-toolchain.toml  clippy.toml  deny.toml  dist-workspace.toml  justfile
LICENSE  README.md  SPEC.md  PLAN.md
crates/
  rascal-core/                   frames, images, locations, artifacts and identity, the mask record, effective weight, renderings, recipes, workspace, journal, config, Op trait, manifest, envelope, registry, compatibility export
  rascal-ops/                    primitives, every op in the catalogue (pure functions plus a thin Op impl each), the solvers
    constants/                   fan-v1.toml, profile-v1.toml, ramp-v1.toml, feather-v1.toml, tighten-v1.toml, window-v1.toml, poisson-v1.toml, measures-v1.toml, embedded
  rascal-mcp/                    MCP client (packs) and server (`serve`)
  rascal-testkit/                fake fillers (exact, mis-sized, re-encoding, slow, failing), fake pack, the independent minimal client, synthetic image builders
  rascal-mask/                   the binary: clap commands, heal, lab, inspect pages
    recipes/                     rect.toml, seam.toml, remove.toml, repair.toml, repair-area.toml, protect.toml, embedded
    pages/                       static HTML templates for the pairs, inspect and dash pages, authored here, embedded
tools/python/                    rascal-tools: uv project, MCP server, echo_region, segment, MODEL.md, uv.lock, bundle build, tests
schemas/
  copied/                        manifest-v1.json, envelope-v1.json, unchanged; SOURCES.toml (origin, commit, version, licence)
  rascal/                        location-v1, frame-v1, image-v1, mask-v1, band-profile-v1, series-v1, recipe-v1, config-v1, journal-v1, matrix-v1, protocol-v1, assignment-v1, picks-v1, pack-context-v1, envelope-v1 (native)
fixtures/
  images/                        synthetic goldens: a 2:1 step sky, a textured floor, a chair blob with a shadow, a scratch on a smooth and on a textured ground
  masks/                         expected mask records for the golden images
  recipes/                       recipes the tests run
examples/                        rascal.toml with a local and a hosted filler, a corpus manifest, a lab matrix, a protocol
scripts/                         repository checks, credential scanner, hooks, publish, archive smoke, acceptance.sh
docs/
  architecture.md                the diagram above, kept current
  adr/                           one file per decision, dated
  owner-decisions.md             decisions only the owner can make, before the task that needs them
  reviews/                       verbatim external reviews, one file per round
  masks.md  recipes.md  fillers.md  measures.md  lab.md  tools.md  workspace.md  export.md  diagnostics.md
.github/workflows/               ci (just check), package, release (generated by dist; runs acceptance.sh)
```

## Code style

Plain structs and enums; boxed trait objects for ops, fillers and packs; no clever generics;
one `thiserror` enum per crate with context; no `unwrap` outside tests; `unsafe` forbidden.
Every type that crosses a boundary derives `Serialize`, `Deserialize` and `JsonSchema`, uses
`deny_unknown_fields`, and is versioned: by a version field, or by its schema's `$id` when its
shape is shared with a copied schema that has none (the location; ADR 0009). Pixel code takes and returns owned `image`
buffers or views, never paths; the Op wrapper does the IO.

```rust
/// The unsmoothed half-width of the band on one row, from the two rulers. Wide where the
/// surround is smooth, thin where it is detailed; a closed gate returns the minimum width.
pub fn raw_half_width(texture: f32, step: f32, k: &FanConstants, thin_px: f32, wide_max_px: f32) -> f32 {
    let smooth = ((k.tex_high - texture) / (k.tex_high - k.tex_low)).clamp(0.0, 1.0);
    let open = if k.gate {
        ((step - k.step_min) / (k.step_ref - k.step_min)).clamp(0.0, 1.0)
    } else {
        1.0
    };
    thin_px + (wide_max_px - thin_px) * smooth * open
}

/// Effective weight of one pixel: protect wins, then the core, then the feather.
pub fn effective_weight(core: bool, blend: u8, protect: bool) -> f32 {
    match (protect, core) {
        (true, _) => 0.0,
        (false, true) => 1.0,
        (false, false) => f32::from(blend) / 255.0,
    }
}
```

Python: `ruff` defaults at line length 100, type hints everywhere, one module per tool, pure
functions over arrays with the MCP wrapper kept thin.

Naming: crates `rascal-*`; ops lower snake case and stable once published; frame ids as above;
location ids `loc-<n>`; artifact ids as the recipe step names them or `<op>-<n>`; call ids
`call:<n>`; recipe step ids as the author writes them, unique per recipe.

## Testing strategy

Two suites. `cargo test --workspace` is the timed offline suite; `scripts/acceptance.sh` is
release acceptance, run by the release workflow and by hand.

- **Primitives**: row smoothing's normalisation at the ends and identity under half a pixel;
  Sobel on a ramp; disc morphology on a point; the distance transform against brute force; Lab
  round trips; the normal CDF against tabulated values.
- **Unit per op**: the fan's monotonicity in texture and step and `raw_half = thin` at a closed
  gate; the profile's symmetry, its core edge at the half-width and the 50% contour at
  `1.1774 σ` outside it, finite support; the ramp's zero at and beyond the half-width and
  `share · Δ` at the line; `grow`, `shrink`, `close`, `subtract`, `select`, `invert`
  postconditions on all three planes, protect preserved, blend cleared, `invert` from a `cut:`
  mask into the raw frame landing on the two edge spans with the middle counted as untouched; `rasterize` for every
  supported kind and `not_applicable` for `view` and `absent`; `tighten` on a drawn scratch
  (finds its two edges, reverts crossing samples) and on an edgeless stroke (untightened
  throughout); `cut`/`put` byte-identical round trips at `W = 4, 5, 10` and a large even width
  with the `s` values listed; `window` at the bounds, `not_applicable` when over `max_side` or
  empty, `unwindow` exactness; mapping through every frame kind with splitting at the cut centre
  and the wrap seam, clipping flagged `partial`, exact round trips asserted only for unclipped
  mappings; every rendering profile including `binary` with protected pixels zero at `t = 1`;
  every paste mode writing only `w > 0` unprotected pixels and the three Poisson cases plus the
  no-boundary and non-convergence errors; every measure on a constructed case, zero-denominator
  and empty-window rows skipped and counted, an all-skipped measure returning `measured_nothing`,
  hue wraparound; `ramp → shrink → feather → apply` changing no pixel outside the new support;
  identity equal across two workspaces with different allocation histories, and changed by a
  changed referenced series under an unchanged local name.
- **Golden**: the shipped recipes on the synthetic fixtures reproduce the stored mask records
  (planes to 1 level, profiles to 0.5 px); a lab cell's identity changes when only a prompt or a
  finder parameter changes; a constant set change is a new version and a fixture
  update in the same commit.
- **Schema**: a **boundary-type inventory** in `rascal-core` lists every type that crosses a
  boundary; a test asserts each is in the inventory, derives the three traits, uses
  `deny_unknown_fields`, carries a version field, has a committed schema, and has a serialized
  fixture that validates against it; every rascal type's schema is committed and the test fails
  on drift; native
  documents validate against `schemas/rascal/`; compatibility exports of real native
  envelopes (which all carry `artifacts`) project and validate against `schemas/copied/`; an
  export of a `line` region or a non-bbox image mapping fails naming it.
- **Recipe validation**: every error path with its step id; the six shipped recipes validate
  and run.
- **Diagnostics**: the run page for a fixture run names every journal step and no other,
  embeds every artifact, and renders offline; `RASCAL_LOG` output contains no key and no pixel
  bytes (a grep over a run with a configured key).
- **Workspace**: two processes commit concurrently without a torn generation; a crash injected at
  each boundary of the commit sequence (after staging, after artifact renames, after the
  generation file, after the pointer rename) publishes nothing or a complete generation, never a
  pointer to a missing generation or payload (tested with a filesystem shim that drops unsynced
  writes); a crash after `CURRENT` and
  before the journal is recovered by `repair` and the rebuilt journal replays the run to
  identical identities; opening removes an abandoned
  staging directory and leaves a locked one; cancellation before the lock publishes nothing and
  after it reports `committed`; a generation naming a missing artifact refuses the workspace.
- **MCP**: the client against the fake pack (initialise, listing, manifest validation,
  `_harness`, result validation failures of each kind rejected before commit, serial and
  concurrent calls under the AND rule, timeout and cancellation failing every in-flight call and
  shutting the pack down, late responses, process death, the environment allowlist removing a
  credential-named variable, shutdown): the pure parts in `rascal-mcp`'s tests, the process
  parts in `rascal-mask`'s `tests/packs.rs` where the fake runs as `rascal fake-pack`; the
  server driven by the independent minimal client through import, list, every op, read,
  describe, and the error mappings.
- **End to end offline**: `heal` through each fake filler on the synthetic panorama (exact: a
  deterministic blur across the mask; mis-sized: operational error; re-encoding: every pixel
  changed and `w = 0` pixels still equal `B` after paste and put; slow: timeout; failing), with
  and without `max_side`, the typed graph executed literally, the journal complete.
- **Isolation**: filenames, prompts and tool output never alter control flow; ids never resolve
  outside the registry; packs receive the allowlisted environment only; a filler's key appears
  in no file.
- **Python** (`pytest`, offline): per-tool tests on arrays; `segmenter`-marked tests run only
  when the artifact is fetched and check parity with stored outputs; a bundle build test checks
  the build's output (every file listed with its digest, the lock naming only the wheelhouse),
  and the Rust install test installs the bundle into a clean directory offline.
- **Acceptance** (`scripts/acceptance.sh`): install `rascal-mask` from the packaged crates into
  a clean directory; run the six shipped recipes by name on the fixtures with no config file;
  run `serve` and drive it with the independent client; make a mask from the CLI in a workspace
  and read it from `serve`; provision the pack from a local bundle with the network disabled and
  call `echo_region` (and `segment` under branch A); check `docs/` names every op, profile,
  recipe field, config key and measure in the registry and nothing else.
- **Lab** (`rascal lab run`): by hand, at milestones, on the private corpus; never in CI.

No test touches the network. Provisioning and artifact fetching are explicit commands.

## Boundaries

- **Always:** `just check` before a commit; keep `SPEC.md`, `PLAN.md`, `README.md` and `docs/`
  accurate in the same commit as the change; version every schema, manifest, recipe, profile,
  constant set and measure set; append provenance on every zone change; preserve protect zones
  through refinements; keep `w = 0` and protected pixels equal to the paste's base, and to `B`
  through `heal`; treat image content, filenames, prompts and tool output as data; write dates
  absolute.
- **Ask first:** adding a dependency beyond the list; changing a copied schema or any
  `schemas/rascal/` schema; changing a shipped recipe or constant set; adding an op to the
  catalogue; choosing or changing the segmenter; any network call in a test; pushing to a
  remote; publishing a crate or a package; spending money in the lab.
- **Never:** commit a real photograph, a key, or a corpus path; copy code, fixtures or templates
  from another project beyond the listed schema data, or name one as the origin of a method; run
  a model in process or build a model-driven controller without an ADR; accept an `Image` where a
  `Mask` is declared; resize, recolour, threshold or fall back silently; let a measurement become
  a verdict; let an operational error look like a result; remove a failing test to pass a commit;
  shape the core around one use case.

## Success criteria

Each names the command that proves it.

1. `cargo install rascal-mask` in a clean environment gives a `rascal` command that runs the six
   shipped recipes by name on a JPEG, with no configuration file, given the input argument
   each requires, and writes a mask record, renderings and a journal. (`scripts/acceptance.sh`)
2. `rascal heal seam` with a configured local filler produces a healed 360 whose `w = 0` and
   protected pixels equal `B`, with the fan, the ramp, the filler call and the measures in the
   journal; the mis-sized filler is refused. (`cargo test`, end to end)
3. `cargo test --workspace` passes offline in under three minutes of test execution, compilation
   excluded, on the owner's Apple silicon machine. (`cargo test`)
4. The independent minimal client drives `rascal serve` through import, find, refine, render and
   paste on a workspace, and the CLI reads the same workspace afterwards. (`cargo test`,
   `scripts/acceptance.sh`)
5. Branch A: the pack installs from a local bundle with the network disabled, launches offline,
   and `segment` returns instances for a text prompt on a fixture with recorded parity, on macOS
   arm64 and Linux x86_64. 5b, branch B: the same with `echo_region`, and `segment` is a named
   later milestone in the README. (`scripts/acceptance.sh`)
6. On the owner's private corpus, under a committed `protocol.toml`, `rascal lab decode` reports
   `preferred` for `fan-ramp` against `thin` on the sky group and `not_worse` on the structured
   group, from the owner's blind picks. (`rascal lab`, by hand)
7. `docs/` names every op, profile, recipe field, config key, measure and run-page panel in the
   registry and nothing else. (`scripts/acceptance.sh`)

## Milestones (detail in PLAN.md)

| M | delivers |
|---|---|
| M0 | workspace, checks, CI, docs skeleton, copied schemas with sources and licence, rascal schemas, ADRs 0001 to 0014 |
| M1 | core: frames and topology, images and roles, locations and mapping, artifacts and identity, the mask record and effective weight, renderings, recipes with typed bindings, the workspace commit protocol with operation records, journal, compatibility export; primitives; ops `line`, `rect`, `stroke`, `rasterize`, `cut`, `put`, `render`, `inspect` |
| M2 | the seam recipe end to end: `fan`, `profile`, `ramp`, `apply`, `feather`, `window`, `unwindow`, `paste` (`feather`, `hard`), `measure`, `heal`'s typed graph with the testkit fillers and the command and HTTP fillers; the constant sets |
| M3 | `serve` driven by the independent client; the MCP client against the fake pack; the pack-context schema; result validation |
| M4 | `tighten`, `grow`, `shrink`, `close`, `subtract`, `select`, `invert`; the remove, repair, repair-area and protect recipes without a model; the segmenter decision (owner) |
| M5 | the Python pack: bundle, offline install test, `echo_region`, `segment` under branch A, parity tests; remove with a prompt |
| M6 | the lab: matrix, per-image bindings, cells and identity, attempts, ledger, assignment and picks, protocol, decode, corpus manifest and verify |
| M7 | the Poisson paste as specified |
| later | Node (`@markhayden/rascal`), Swift, WASM; the path fan; scaled windows; replace with homography; retouch; extend; isolate; `segment` under branch B |

## Open questions

Owner decisions, recorded in `docs/owner-decisions.md` before the task that needs them:

1. The segmenter (during M4): which permissively licensed public model is exported, its
   redistribution terms for the export, and whether it supports text prompts, points or both;
   or branch B.
2. The private corpus (before M6): its location, first contents, and the sky and structured
   groups for the protocol.
3. The MCP crate versus an in-house JSON-RPC implementation (M3, by ADR).
4. Whether `fan` gains a path form before or after v1 (the spec says after).
