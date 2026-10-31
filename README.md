# rascal

The find-and-mask step of an image pipeline.

Image models paint well inside a region and badly at its edge. The quality of an edit is decided
as much by **which pixels the model may touch, how the edge is softened, what is done to the
pixels first, and how the result is put back** as by the model itself. rascal turns a need (a
seam to heal, an object to remove, a scratch or stain to repair, a region to protect) into a mask
a model can paint inside, and blends the result back.

- A **need** is a use case plus a location: a vertical line, the chairs in a room, a scratch
  across an old print, a stain on a wall, a user's rough stroke.
- A **mask** is a record, not a picture: zones (what may be painted, what only blends, what must
  never change), a soft edge, prescriptions on the pixels (a tone ramp across a line, a transform
  for an inject), the frame it lives in, and the provenance of every step that made it.
- A **rendering** is what one consumer wants to see: a feathered greyscale, a binary at a
  threshold, the photo with the region tinted, an alpha cut-out.
- The **composite** puts a filler's pixels back through the mask's own weights, exactly inside
  the core, blended across the blend zone, gradient matched when there is a tone step.

Pure Rust library, CLI and MCP server. The models that paint are **fillers**, plugins declared
in a TOML file as a command or an HTTP endpoint. Learned finders (a segmenter, later OCR and
detectors) are a Python tool pack over MCP. rascal has no model loop of its own: agents and
pipelines drive its tools. It is conforming to [table-manners](https://github.com/markhayden/table-manners),
the shared contract of its sibling tools, so an agent can chain it with
[squint](https://github.com/markhayden/squint) and the tools after them without glue code
(milestone M8).

## Install

Three ways to get the `rascal` command, and one more step for the Python pack.

**From crates.io** (needs a Rust toolchain):

```
cargo install rascal-mask        # installs the `rascal` command
```

**From a release archive**: each [GitHub release](https://github.com/markhayden/rascal/releases)
carries `rascal-mask-<target>.tar.xz` for `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`
and `aarch64-unknown-linux-gnu`, with a `sha256.sum` beside them. Unpack and put `rascal` on
your `PATH`:

```
tar -xf rascal-mask-aarch64-apple-darwin.tar.xz
./rascal-mask-aarch64-apple-darwin/rascal run seam pano.jpg --out run/
```

**The Python pack** (`rascal-tools`: the segmenter behind `remove-prompt`) is optional. Each
release carries one bundle per platform, `rascal-tools-bundle-macos-arm64.tar.gz` and
`rascal-tools-bundle-linux-x86_64.tar.gz`, with its SHA-256 beside it; a bundle installs with
no network. The prerequisites are [`uv`](https://docs.astral.sh/uv/) and a Python 3.12 that uv
can find (`uv python install 3.12` once, on a connected machine). Then:

```
rascal tools setup --bundle https://github.com/markhayden/rascal/releases/download/v0.1.0/rascal-tools-bundle-macos-arm64.tar.gz \
    --sha256 <the digest beside it> --into ~/rascal-tools
```

`setup` downloads the archive (the one network step), checks its digest, verifies every file
inside against the bundle's manifest, installs offline, and prints the `[packs.rascal-tools]`
entry to put in `rascal.toml`. `--bundle` also takes a local archive or an unpacked directory.
A full bundle carries the segmenter's model files (749 MB, Apache-2.0,
`tools/python/MODEL.md`); a bundle built without them says so and serves `echo_region` only,
and `rascal-tools fetch` brings the models later. `docs/tools.md` has the whole of it.

## The recipes

A recipe is a TOML list of typed steps. rascal ships one per v1 use case (two for remove), and
the shipped files are the acceptance fixtures:

| recipe | what it masks | the location it takes |
|---|---|---|
| `seam` | a band across a vertical line, wide where the surface is smooth and thin where it is detailed, with a tone ramp across it; the wrap edge of a 360 or any line | a line, or the wrap edge |
| `remove` | an object with its shadow and reflection, minus protected zones | a click, a stroke, a polygon or a rectangle |
| `remove-prompt` | the same, found by the Python pack's segmenter (OWLv2 and SAM 2.1, `tools/python/MODEL.md`) | a text prompt |
| `repair` | a scratch or tear, tightened to the defect's edges and feathered | a stroke |
| `repair-area` | a stain or spot, rasterized and feathered | a rectangle or polygon |
| `protect` | the inverse of any mask, for a second pass | a mask |
| `rect` | a plain feathered rectangle | a rectangle |

```
rascal run seam pano.jpg --out run/
rascal run remove room.jpg --location chairs.json --out run/
rascal run remove-prompt room.jpg --prompt "chair" --config rascal.toml --out run/   # the pack's segmenter finds them
rascal heal remove-prompt room.jpg --prompt "chair" --set segment.threshold=0.4 --set grow.px=8 --filler lama --config rascal.toml --out run/
rascal run repair print.jpg --stroke scratch.json --out run/
rascal heal seam pano.jpg --filler local-inpaint --config rascal.toml --out run/
rascal render run/artifacts/<id> --for tinted --image img-3 --out seam-tinted.png
rascal paste base.png fill.png run/artifacts/<id> --mode poisson --out composite.png
rascal ops list                                           # every op's manifest, as an agent sees it
rascal inspect run/artifacts/<id> --json                  # or --out pages/ for the overlay and chart, --replay to rebuild
rascal export --compat run/journal.jsonl --step rasterize --out e.json
rascal dash run/ --out run/dash.html                     # the run page: timings, envelopes, every artifact; --serve for loopback
rascal serve --workspace ws/                              # every op as an MCP tool over stdio, plus import/list/describe/read/dash
rascal tools call rascal-tools segment --config rascal.toml --workspace ws/ --image photo.png --prompt "the chair"   # a pack's tool from the shell
rascal tools setup --bundle bundle-macos-arm64/ --into ~/rascal-tools   # the Python pack installed offline from its bundle
rascal corpus verify corpus.toml --root ~/rascal/corpus                 # every image's file against its digest
rascal lab run matrix.toml --config rascal.toml --out lab/seam-01/      # every cell healed once; resume makes no external call
rascal lab pairs lab/seam-01/ --arms fan-ramp,thin [--whole-max-side 2000]   # the blind pairs page; the judge saves picks.json
rascal lab decode lab/seam-01/ picks.json                               # wins, losses and the outcome per group
```

Replace (screens, signs), retouch, extend and isolate are later milestones.

## Where it sits

```
   generate / capture  ──►  FIND + MASK + COMPOSITE  ──►  paint (any model)  ──►  evaluate
     (a generator, a camera)        (rascal)                (filler plugin)      (an evaluator, a person)
                                       ▲                                                │
                                       └──────── a location, when something is found ───┘
```

rascal accepts a location from anywhere (a person, a file, an evaluator's finding, an agent) and
hands back a mask, a rendering for whatever model is painting, and the composite. Its manifests
and envelopes share their shape with an evaluation harness in the same family, so the two
exchange documents without translation; rascal is fully standalone and depends on no other tool.

## Status

Pre-release. M0 to M6 are merged (the scaffold, the core, the seam, MCP, the refinements, the Python pack with its segmenter and bundle, the lab) and M7's Poisson paste with them; the lab's three rounds met success criterion 6 (`docs/results/seam-03.md`). Left: publishing the segmenter's model files and the release (task 7.2). The design is in [SPEC.md](SPEC.md), the build order
in [PLAN.md](PLAN.md), the decisions in [docs/adr/](docs/adr/README.md), and the current state
of each part in [docs/architecture.md](docs/architecture.md#status).

## Developing

One manual step, then everything is a `just` recipe:

```
cargo install just --version 1.59.0 --locked    # or: brew install just
just bootstrap                                  # pinned cargo-deny and cargo-dist
just hooks                                      # the pre-commit hook
just check                                      # what every commit must pass
```

`just check` runs formatting, Clippy with warnings denied in both feature configurations,
the tests in both, `cargo deny` for licences and sources, the crate-graph and packaging checks,
the clean-room scan, and the self-tests of the hook and the checks. The pre-commit hook scans
every staged blob for credential-shaped values, refuses raster image files outside `fixtures/`
and `.github/assets/`, runs the clean-room scan on the staged blobs and paths, then formatting
and Clippy; the commit-msg hook scans the message.
`just ci-local` runs the same inside a Linux container with the pinned toolchain (Docker).
`just smoke <binary>` is the offline smoke against any built binary.

The clean-room scan reads a forbidden-word list from the file named by `RASCAL_FORBIDDEN_WORDS`.
The list lives outside the repository and is never committed; with the variable unset, as in
CI, the scan passes and says so.

Licence: Apache-2.0.
