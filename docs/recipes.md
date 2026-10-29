# Recipes

A recipe is a TOML list of typed steps. Every op is a tool; a recipe is the fixed, reviewable
order in which a use case calls them, with data flowing between steps by name. rascal ships one
recipe per v1 use case; the shipped files are the acceptance fixtures and are addressable by
name (`rascal run rect …`) as well as by path.

## The file

```toml
version = 1
name = "seam"
use_case = "seam"                       # seam | remove | repair | protect | custom
projection = "equirectangular"          # optional: how an input image is read when --projection is not given

[inputs]                                # what the caller binds; any artifact type
image = { type = "Image" }

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
  Location, but render.image is a Image`); every required port is bound;
- every parameter is declared by the op and passes its kind and bounds; a `constants` names
  one of the op's sets; defaults are resolved by the manifest, and the resolved values are what
  the journal records;
- `outputs.mask`, `outputs.image` and `outputs.plan` name bindings of the right type; a render
  needs `outputs.mask`, a plain file name, a real profile, and `outputs.image` for the profiles
  that draw on the image.

## Running one

```
rascal run <recipe> <image> --out run/ [--bbox x0,y0,x1,y1 | --location l.json]
                                       [--stroke s.json] [--protect <mask dir>] [--mask <mask dir>]
                                       [--projection flat|equirectangular] [--config rascal.toml] [--strict]
```

The positional image binds `inputs.image`. `--bbox` is sugar that binds `inputs.location` to a
new bbox location in the image's frame; `--location` binds any location kind from a JSON file
(the `frame` may be omitted; if named it must be the image's frame); `--stroke` binds a stroke
location to `inputs.stroke`; `--protect` and `--mask` bind stored mask directories in the
image's frame. A flag without its declared input, or a declared input without its flag, is a
usage error. `--prompt` arrives with the Python pack (M5).

A step that returns `not_applicable` publishes nothing; a later step that binds its output does
not run and the run stops naming the requirement; `rascal run` exits 1 under `--strict` and 0
without. A bad parameter value is `invalid_args` in the journal and exit 64.

## The shipped recipes

| recipe | since | steps |
|---|---|---|
| `rect` | 1.1 | `inputs.location` → `rasterize`; renders `grey` |
| `seam` | M2 | `cut` → `line` → `fan` → `profile` → `ramp` |
| `remove`, `repair`, `repair-area`, `protect` | M4 | see SPEC.md "Recipes" |

## The configuration file

`--config rascal.toml` names the file a pipeline gives rascal. Its general sections are here;
`[fillers]` arrives with M2 and `[packs]` with M3. The schema is `schemas/rascal/config-v1.json`.

```toml
config_version = 1

[workspace]                     # where runs keep their workspace when --workspace is not given
dir = "ws"

[corpus]                        # the private corpus (M6)
manifest = "corpus.toml"
```
