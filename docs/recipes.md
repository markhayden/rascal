# Recipes

A recipe is a TOML list of typed steps. Every op is a tool; a recipe is the fixed, reviewable
order in which a use case calls them, with data flowing between steps by name. rascal ships one
recipe per v1 use case (two for remove: by location and by prompt); the shipped files are the
acceptance fixtures and are addressable by name (`rascal run rect …`) as well as by path.

## The file

```toml
version = 1
name = "seam"
use_case = "seam"                       # seam | remove | repair | protect | custom
projection = "equirectangular"          # optional: how an input image is read when --projection is not given

[inputs]                                # what the caller binds; any artifact type, or Text
image = { type = "Image" }
protect = { type = "Mask", optional = true }   # optional: the caller may leave it out; only an optional port may take it
prompt = { type = "Text" }              # text (a finder's prompt): a step's string parameter takes it as prompt = "inputs.prompt"

[[step]]
id = "cut"                              # unique, no '.', not "inputs"
op = "cut"                              # an op in the catalogue (rascal ops list)
inputs = { image = "inputs.image" }     # port = binding; a binding is inputs.<name> or <step>.<output>
strip = 0.07                            # every other key is one of the op's parameters

[[step]]
id = "fan"
op = "fan"
inputs = { image = "cut.image", line = "line.location" }
constants = "fan-v1"                    # a constant set the op declares; the first is the default

[outputs]                               # the contract a consumer reads, by output name
mask = "ramp.mask"                      # Mask
image = "cut.image"                     # Image in the mask's frame
plan = "cut.plan"                       # Plan, when the mask's frame is a cut:
render = [{ for = "grey", out = "seam-grey.png" }]
```

The schema is `schemas/rascal/recipe-v1.json`. A step is open (its parameters are the op's own
keys); everything else is closed.

## Validation

Everything is checked before anything runs, and every error names the recipe and the step:

- the version is 1 and there is at least one step; step ids are unique, non-empty, contain no
  `.` and are not `inputs`; input names likewise;
- every op is in the catalogue; every bound port exists on the op; every binding names a
  declared input or an earlier step's output of the declared type (`input location must be a
  Location, but render.image is a Image`); every required port is bound; a required port never
  binds an optional input (when the caller leaves one out, the port stays unbound and the op
  runs without it: `subtract` with no `inputs.protect` passes its mask through);
- every parameter is declared by the op and passes its kind and bounds; a `constants` names
  one of the op's sets; defaults are resolved by the manifest, and the resolved values are what
  the journal records;
- `outputs.mask`, `outputs.image` and `outputs.plan` name bindings of the right type; a render
  needs `outputs.mask`, a plain file name that is not one the run directory writes itself
  (`journal.jsonl`, `CURRENT`, `lock`, `frames.json`, `locations.json`, `dash.html`), a real
  profile, and `outputs.image` for the profiles that draw on the image.

A number parameter may be written as an integer (`width = 1`) or a float (`width = 1.0`); it
resolves as a float either way, so the two spellings record and hash the same. An integer
parameter must be written as an integer.

## Running one

```
rascal run <recipe> <image> --out run/ [--bbox x0,y0,x1,y1 | --location l.json]
                                       [--stroke s.json] [--protect <mask dir>] [--mask <mask dir>]
                                       [--projection flat|equirectangular] [--config rascal.toml] [--strict]
                                       [--replay <run or workspace>]
```

The positional image binds `inputs.image`. `--bbox` is sugar that binds `inputs.location` to a
new bbox location in the image's frame; `--location` binds any location kind from a JSON file
(the `frame` may be omitted; if named it must be the image's frame); `--stroke` binds a stroke
location to `inputs.stroke`; `--protect` and `--mask` bind stored mask directories in the
image's frame, or (for `protect`) in a frame derived from it that the workspace knows (frames
carry sizes and projections, not images, so the check is on the frame chain: give the mask the
run that made it). `--prompt` binds `inputs.prompt`, a `Text` input: not an artifact but a
string a step's parameter takes by naming it as its value (`prompt = "inputs.prompt"`); the
run substitutes the text and the journal records the resolved value, and an optional text
left out leaves the parameter to the op's default. Text binds string parameters only, never a
port. A flag without its declared input, or a declared input without its flag, is a usage
error, except an input the recipe marks `optional`; `heal` passes `--prompt` to the filler
only when the recipe declares no `inputs.prompt`, since a finder's prompt is not a filler's.

A step that returns `not_applicable` publishes nothing; a later step that binds its output does
not run and the run stops naming the requirement; `rascal run` exits 1 under `--strict` and 0
without. A bad parameter value is `invalid_args` in the journal and exit 64.

A recipe may call a configured pack's tool (`[packs.*]` in `--config`; `docs/tools.md` "The
packs") as a step like any other; its `many` outputs bind one at a time as `<step>.<port>_<n>`,
or all at once to a `many` input (`select`'s `plane = "segment.plane"`). `--replay`
names a recorded run: no pack is started and each pack step is answered from that run's journal
when the request is the same, or fails naming the request.

## The shipped recipes

| recipe | since | steps |
|---|---|---|
| `rect` | 1.1 | `inputs.location` → `rasterize`; renders `grey` |
| `seam` | 2.8 | `cut` → `line` → `fan` → `profile` → `ramp`; outputs the band mask, the cut canvas and the plan; `heal seam` consumes them (`docs/fillers.md`) |
| `remove` | 4.4 | `inputs.location` (a box, a polygon or a stroke) → `rasterize` → `grow` (4 px, 12 px down for the shadow) → `close` (3 px) → `subtract` (optional `inputs.protect`, grown 2 px) → `feather` (4 px) |
| `remove-prompt` | 5.2 | `inputs.prompt` (text, `--prompt`) → `segment` (a configured pack's, `max_instances` 8) → `select` (every plane) → the same `grow`, `close`, `subtract` and `feather` as `remove`; needs `--config` with a `[packs.*]` entry that serves `segment` (`docs/tools.md`) |
| `repair` | 4.4 | `inputs.stroke` → `tighten` (`tighten-v1`) → `feather` (2 px) |
| `repair-area` | 4.4 | `inputs.location` (a box or a polygon) → `rasterize` → `feather` (3 px) |
| `protect` | 4.4 | `inputs.image` (the earlier composite, in its raw frame) and `inputs.mask` (the earlier mask, in that frame or one derived from it that the workspace knows) → `invert` into the raw frame; outputs the inverted mask and the image, no plan: the second pass edits everything the first left alone and keeps what it painted |

Goldens for the four M4 recipes live under `fixtures/masks/` and `crates/rascal-mask/tests/recipes.rs`
reproduces them; `heal seam` followed by `heal protect` in one workspace is the two-pass test.
`scripts/acceptance.sh` (`just acceptance`, and the package workflow) installs `rascal` from
the packaged crates and runs the six by name on the fixtures with no configuration file.

## The configuration file

`--config rascal.toml` names the file a pipeline gives rascal. Its general sections are here;
`[fillers.*]` (`fillers`) is on `docs/fillers.md` and `[packs.*]` (`packs`) on `docs/tools.md`.
The schema is `schemas/rascal/config-v1.json`; `examples/rascal.toml` is a complete file.

```toml
config_version = 1

[workspace]                     # where runs keep their workspace when --workspace is not given
dir = "ws"

[corpus]                        # the private corpus (M6)
manifest = "corpus.toml"
```
