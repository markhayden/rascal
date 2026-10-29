# Tools

Every operation is a tool: one manifest, one envelope per invocation, callable as a subcommand,
from a recipe step, and (from M3) over MCP with the same arguments. `rascal ops list` prints the
catalogue; `rascal ops describe <op> [--json]` prints one manifest as a client sees it. From task
1.9 a check proves every op is documented on this page.

## The manifest

| field | meaning |
|---|---|
| `name`, `version` | the op and its semantic version; a behaviour change bumps it |
| `purpose`, `when_to_use`, `when_not_to_use`, `output_guidance` | the four sentences an agent reads before calling |
| `cost` | `tier` (cheap, moderate, expensive) and rough `seconds` per call |
| `inputs`, `outputs` | typed ports: name, artifact type (`Image`, `Location`, `Mask`, `BandProfile`, `Series`, `Plan`), required, purpose; an output port may be `many`: it takes any number of artifacts named `<port>_0`, `<port>_1`, … (a finder's instances), and required then means at least one |
| `params` | each with a kind (number, integer, boolean, string, number array, point list, integer list, box list, choice), required or a default, bounds, length or choices |
| `returns` | bounds on the envelope: images, regions, measurements, transforms, summary bytes |
| `input_schema`, `output_schema` | JSON Schemas derived from `params` and `outputs` |

The registry checks every manifest when it registers it (`Manifest::check`): empty texts, a
version that is not `major.minor.patch`, duplicate ports, an optional parameter without a
default, a choice without choices, an array without a length, or schemas out of step with the
fields are refused at start and in tests.

## The envelope

`status` (`ok`, `measured_nothing`, `not_applicable`, `invalid_args`), `summary`, `reason` (when
not `ok`), `measurements`, `regions`, `images`, `transforms`, `provenance` (tool, version,
duration), `artifacts` (rascal's extension: every typed output by name, id and identity) and
`refinement` (what a refinement cleared). An envelope that is not `ok` publishes nothing and
needs a reason; an `ok` envelope has every required output of the declared type and no
undeclared one; counts stay within the manifest's bounds. The runner checks each envelope
before accepting it.

## Shared rules

- Inputs are validated against the manifest before the op runs: every required port bound,
  every bound port of the declared type (a rendering is an `Image` and is refused where a
  `Mask` is declared, naming its role), no unknown port, and a mask and an image in one
  invocation in one frame.
- Parameters are resolved before the op runs: defaults filled, kinds and bounds checked. A bad
  value is an `invalid_args` envelope in the journal and a usage error (exit 64), with or
  without `--strict`.
- An op that cannot act on its input returns `not_applicable` and publishes nothing: no
  artifact, no journal output, one journal line. `rascal run` exits 1 under `--strict` and 0
  without. A later step that binds the refused step's output does not run; the run stops
  naming the requirement.
- Every invocation appends one journal line, publishing or not, successful or not.
- A step port bound to a recipe input the caller left out (an `optional` input) is unbound;
  the op sees no artifact there and must say what it does without one.

## The catalogue

| op | group | in | out | since |
|---|---|---|---|---|
| `rect` | find | Image; `bbox` | Location (`bbox`) | 1.1 |
| `line` | find | Image; `column` or `path`, `width_px` | Location (`line`) | 1.8b |
| `stroke` | find | Image; `path`, `radius_px` | Location (`stroke`) | 1.8b |
| `rasterize` | refine | Location, Image; `use_case` | Mask: `bbox` and `polygon` as drawn, `whole_image` all, a column `line` as a band `width_px` wide, a path `line` dilated by `width_px / 2`, a `stroke` dilated by `radius_px`; `view` and `absent` are `not_applicable` | 1.1, 1.8b |
| `render` | out | Mask [, Image]; `for`, `threshold`, `colour`, `strength` | Image: `grey`, `binary` (`mask_input`), `alpha`, `tinted` (`image_input`), `overlay` (inspection) | 1.1, 1.5 |
| `cut` | prep | Image (wrapped raw); `strip` | Image in a new `cut:` frame (the two edge strips side by side, the wrap edge at the centre) and the Plan | 1.8a |
| `put` | prep | Image (parent), Image (canvas), Plan | Image in the parent frame with the canvas's halves on the edge strips; cut then put is byte-identical; a canvas not in the cut frame of this image and plan is `invalid_args` | 1.8a |
| `fan` | refine | Image, Location | a BandProfile: the half-width per row along a column line, wide where the surface is smooth and the sides differ in tone, `thin` where it is detailed, with the sampling windows it resolved; constant sets `fan-v1` and `fan-thin-v1` (`--constants`) | 2.1 |
| `profile` | refine | BandProfile | a seam Mask: the core at each row's half-width about the line, the blend a feather whose sigma is `feather_ratio` of the row's half-width (`profile-v1`), the profile and its `sigma_px` series stored | 2.2 |
| `feather` | refine | Mask | the same mask with its blend replaced by the feather of the core's edge outward with one `sigma_px`, zero beyond `support` sigmas (`feather-v1`); protect, the profile and the prescriptions kept | 2.2 |
| `ramp` | refine | Image, Mask | the same mask with a `tone_ramp` prescription: the per-row colour step `Δ[y]` (right minus left on the profile's step windows, row-smoothed by `ramp_sigma`) and the `share` each side moves (`ramp-v1`); nothing is painted | 2.3 |
| `apply` | prep | Image, Mask | an Image in the mask's frame with every prescription performed once in order: a tone ramp moves each pixel with `w > 0` toward the other side by `share · Δ[y] · max(0, 1 − d / half[y])`; `w = 0` pixels are byte-identical; not applicable without a prescription | 2.3 |
| `window` | prep | Image, Mask | an Image and a Mask in a new `crop:` frame: the integer bbox of the mask's support (`w > 0`) expanded by `context` (`window-v1`: 0.5) on each side, made square, capped to `max_side` and the image, slid inside the image; the support is always inside; nothing resampled; not applicable for an empty support or one over `max_side`, naming the sizes | 2.4 |
| `unwindow` | prep | Image (parent), Image (crop) | an Image in the parent frame with the crop written back into exactly its bbox; `unwindow(window(x)) = x` | 2.4 |
| `paste` | out | Image (base), Image (fill), Mask | an Image with the composite role: a copy of the base with only `w > 0` pixels written (`feather`: `(1 − w) · base + w · fill`; `hard`: the fill where the core is set and not protected); `w = 0` pixels, protected ones included, are copied without arithmetic; a base's alpha kept, a fill's ignored; `poisson` is not applicable until M7; `rascal paste` runs it on files | 2.5 |
| `measure` | out | Image (after), Image (before, optional), Mask, Location (line, optional) | a Series per measure in the set (`continuity`, `tone_step`, `transition`, `texture_match`; `drift` is scalars) with the summaries in the envelope; a measure without its inputs is not applicable for itself; one whose every row is skipped measured nothing; `measures-v1`; see `docs/measures.md` | 2.6 |
| `inspect` | inspect | Mask, Image | the overlay (an inspection Image) and a Series of the core's width per row; `rascal inspect` adds `--json`, `--out` and `--replay` | 1.9 |
| `grow` | refine | Mask; `px`, `down_px` | the same mask with its core dilated by a disc of radius `px` and, with `down_px`, unioned with itself shifted down by every row up to `down_px` (a shadow); cleared under protect; blend cleared, profile and prescriptions removed (`refinement` in the envelope) | 4.1 |
| `shrink` | refine | Mask; `px` | the same mask with its core eroded by a disc of radius `px`, cleared under protect; outside the image counts as unset, so a core touching the border shrinks from the border too; not applicable when nothing survives; blend cleared, profile and prescriptions removed | 4.1 |
| `subtract` | refine | Mask, Mask (`subtract`) or Location; `grow_px` | the same mask with protect enlarged by the second mask's support (core or blend) or the location's interior, dilated by `grow_px`; core and blend cleared there; the profile and prescriptions kept, since `apply` honours protect; at most one of the two second inputs, and with neither the mask passes through unchanged (a recipe's optional protect left out) | 4.2 |
| `select` | refine | Image[] (`plane`, the instance planes of a finder, a `many` port); `keep`, `drop`, `exclude` | a new Mask in the planes' frame: the union of the kept instances (`keep` names them, else all but `drop`), cleared inside every `exclude` box; blend and protect empty; not applicable when nothing is left | 4.2 |
| `invert` | refine | Mask; `to` | a new Mask in the target frame (`to`: a frame id on the mask's chain, or `raw` for its root, the default): protect = the old support and protect mapped through the chain (a `cut:` mask lands on the two edge spans, a `crop:` mask on its bbox), core = every other pixel, blend empty | 4.2 |
| `tighten` | refine | Location (`stroke`), Image | a Mask whose core follows the defect's edges: at each path sample (`sample_spacing_px`) the best pair of opposite-signed luminance edges inside `[−r, r]`, read at `⌈2r⌉` equal steps (`min_contrast`, `min_defect_px`, `max_defect_factor · r`), extents smoothed along the path (`path_sigma`), a crossing reverting; samples without a pair keep the full radius, so an untightened stroke equals `rasterize`'s; plus a Series `tightened` (per sample: tightened, a, b); `tighten-v1`; a radius is bounded by the frame's diagonal | 4.3 |
| `close` | refine | Mask; `radius_px` | the same mask with its core closed (dilated then eroded) by a disc of `radius_px`, so gaps and notches narrower than the disc fill; the outer extent is unchanged, at the image border too (the closing's erosion counts outside as set); cleared under protect; blend cleared, profile and prescriptions removed | 4.1 |

The pixel primitives every op is built from live in `rascal_ops::primitives`: row smoothing
(a Gaussian truncated at 3σ, renormalised at the edges), the 3×3 Sobel magnitude with edge
replication, disc dilation and erosion (through the distance transform, so linear in the plane
whatever the radius; erosion in two conventions, outside the image unset for `shrink` and set
for `close`), the exact Euclidean distance transform, sRGB to CIE Lab under D65, and the
standard normal CDF; each is tested against brute force or tabulated values.

An op subcommand's image port takes a workspace id or a path (a path is imported first), a mask
port takes an id or a mask directory, and `--out <file>` also writes a lone image output as a
PNG.

A point-list parameter is written `[[x, y], …]` in a recipe and `x,y;x,y` on the command
line; an integer list `[0, 2]` and `0,2`; a box list `[[x0, y0, x1, y1], …]` and
`x0,y0,x1,y1;…`. A `many` input port (`select`'s `plane`) takes `plane_0`, `plane_1`, … in a
recipe through one binding, `plane = "<step>.plane"`, which expands to every instance the step
produced; on the command line `--plane` is repeated, in order; over MCP it is an array of ids.

`scripts/check-docs.sh` (part of `just check`) reads the catalogue, the render profiles and the run-page panels from
the binary and fails when an op or profile is missing from these pages, or when a documented
name no longer exists.

The remaining op arrives with its task (PLAN.md): `segment` from the Python pack in M5 (the client that runs
it, and a fake pack that serves it, are below).

## The server

`rascal serve --workspace ws/ [--config rascal.toml] [--concurrency n]` is an MCP server over
stdio at protocol revision `2025-11-25` (ADR 0015). It advertises one tool per op in the table
above, named as the op, with ordinary arguments only: one string property per input port (an
artifact id or identity in the workspace) plus the op's parameters by name, and `constants`
when the op has sets. No argument is injected; ids resolve against the served workspace and
nowhere else. Five workspace tools go with them: `workspace_import` (a path or base64 bytes in;
the canonical Image and its `raw:` frame out), `workspace_list` (by type), `workspace_describe`
(a record as JSON), `workspace_read` (an Image's PNG as one image content block, bounded at
64 MiB; any other type is `invalid_args`) and `workspace_dash` (writes the run page into the
workspace and returns its path).

A result is the envelope as `structuredContent`, a text block with the summary, and image
content blocks in exactly the count and order of the envelope's `images` (none for a pure op;
one for `workspace_read`). Validation stays in the envelope as `invalid_args` with `isError`;
a caller's mistake outside any envelope (an unknown tool or id, a malformed argument) is a
JSON-RPC invalid-params error; an operational failure is a JSON-RPC internal error whose
`data.class` names rascal's error class (`io`, `op`, `image`, `format`, `mask_invariant`,
`usage`, `operational` for anything else, and `cancelled` when the client cancelled the call:
nothing is published after a cancellation reaches the op). Calls are serial per workspace by
default; `--concurrency n` lets `n` run at once, each still committing in turn. One request is
one line of at most 256 MiB (a longer line is dropped by the transport and gets no answer), so
a `workspace_import` of base64 bytes fits any image rascal decodes; a `path` import reads any
file the server process can, as the spec says, so give a host only the workspace it should
touch. Tracing goes to stderr only, never to the protocol stream. The
testkit's independent client (`crates/rascal-testkit/src/mcp_client.rs`), written against the
protocol document and nothing of rascal's, drives the server in `tests/serve.rs`.

## The packs

A **pack** is a tool server rascal starts: a process speaking MCP over its stdio, declared in
`rascal.toml` and holding the learned finders (`segment` from the Python pack in M5; the
testkit's fake pack now). Every tool a pack lists becomes an op in the registry of the run,
callable from a recipe step, from `rascal tools call`, and from `rascal serve` when it is given
the same configuration.

```toml
[packs.rascal-tools]
command = ["/opt/rascal-tools/.venv/bin/rascal-tools", "serve"]   # the process; no placeholders
env = ["RASCAL_TOOLS_MODELS"]            # passed through in addition to the baseline
accepts = "path"                         # how it takes images: path (default) or base64
concurrent = false                       # calls may overlap only when the tool's manifest agrees
revision = "segmenter export 2026-10"    # the lab hashes it into a cell's identity
startup_timeout_s = 60                   # initialize and tools/list
call_timeout_s = 120                     # one call; then the pack is shut down
```

**Environment.** The process sees `PATH`, `HOME`, `LANG`, `LC_ALL`, `TMPDIR` plus the declared
`env` names, minus every variable any filler names as `key_env` and minus every name ending in
`_KEY`, `_TOKEN` or `_SECRET`; removal wins over declaration. Nothing else of the parent's
environment reaches it.

**Startup.** Spawn; `initialize` must answer revision `2025-11-25` and advertise `tools`;
`tools/list` to exhaustion; every tool must carry, in its MCP `_meta`, `manifest` (the copied
manifest shape, `schemas/copied/manifest-v1.json`) and `rascal` (the annotation block,
`schemas/rascal/pack-annotation-v1.json`: the typed ports, the parameters, the constant sets).
rascal builds the native manifest from the two, checks it as it checks every op's, and refuses
bounds over its ceilings (64 images, 64 MiB per image, 16 MiB per envelope, the image side
limit, and the whole answer, images as base64 plus the envelope, under 256 MiB, which is the
one line the transport carries). A refusal names the tool and the reason; the pack is stopped.
`startup_timeout_s` covers initialize and the listing together. The listing is journaled
(`pack manifest:` notes), so a replay knows the tools without the pack; a restart after a
failure must list the same tools with the same manifests, or it is refused.

**A call** carries the step's parameters by name, each `Image` port's workspace id, each
`Location` port's location object, and `_harness` (`schemas/rascal/pack-context-v1.json`): the
run and call ids, every referenced image with its record and its pixels by absolute path or as
base64 PNG, and every frame those images are in up to the root. `_harness` exists only between
rascal and its packs; `rascal serve` never shows it.

**The answer** is validated before anything is stored: the envelope as rascal's closed type and
within the manifest's bounds; `isError` exactly for `invalid_args`; image content blocks in
exactly the count, order and media type (`image/png`) of `images`, each decoding to the size
its entry and its frame say; every region and image in a frame the call passed, with valid
geometry. A failure is an operational error, the call is journaled, nothing is published. On
success the envelope's regions become `Location` artifacts and its images `Image` artifacts
with the `plane` role, on the tool's `many` output ports in order (`region_0`, `plane_0`, …);
a single port of the type takes exactly one. The envelope's `images[].identity` is filled with
the stored artifact's identity.

**Concurrency and failure.** Calls to a tool overlap only when the config's `concurrent` and
the manifest's both say so; otherwise one at a time. A timeout, a cancellation (a `serve`
client's, or `Pack::fail_in_flight`) or the process dying fails every in-flight call on the
pack and shuts it down before any reuse: stdin closed, five seconds, SIGTERM, five seconds,
SIGKILL. The next call starts a fresh process, which must list the same tools. A late answer
for a call that already failed goes nowhere.

**The journal** gets one `pack call:` note per call (pack, tool, command, request and response
bytes, duration, status `ok | timeout | cancelled | died | error`, outcome; never a key), the
pack's stderr as `pack stderr:` notes with credential redaction (every credential-shaped value
of the parent's environment, anything after `Bearer`, anything after a `key=`, `token:` or
similar; a dying pack's last lines are kept for the call that saw it die), and one `pack
stored:` note per successful call: the request identity (over the pack, the tool, the
parameters and the inputs' identities, never a workspace id), the pack and its revision, the
tool, the envelope and the outputs by identity. The note precedes the commit; a replay passes
over a stored call whose outputs the recorded run does not hold.

**Replay** (`rascal run … --replay <run or workspace>`, and `heal --replay`): no pack is
started; the tools come from the recorded run's `pack manifest:` notes and each call is answered
from a `pack stored:` note with the same request identity, pack name and revision, its outputs
loaded from the recorded run by identity. A request the recording does not hold is an error
naming it; the pack is never called under `--replay`.

**From the shell.**

```
rascal tools list --config rascal.toml [--json]                     # start every pack, print its tools
rascal tools call fake segment --config rascal.toml --workspace ws/ \
    --image photo.png --prompt "the chair" [--max_instances 3] [--json]   # one tool, as its op subcommand
```

`tools call` takes the tool's own arguments: `--<port>` an id (or a path, imported first) and
`--<param>` a value, plus `--json`, `--out` and `--strict` as every op subcommand does.

**The Python pack** is `tools/python`, a `uv` project (`rascal-tools`): an MCP server over stdio
on the SDK's 1.x line (revision 2025-11-25), listing each tool with both `_meta` documents built
from one `ToolSpec` and holding its bounds under rascal's ceilings at listing time. It serves
`echo_region` (a box in, one region and one plane out: the parity tool) and, with
`serve --debug-tools`, `echo_env` (the variable names it can see). `rascal-tools fetch` is the
one network step: it downloads the artifacts `artifacts.toml` names into `RASCAL_TOOLS_MODELS`
(default `~/.cache/rascal-tools`) and verifies each SHA-256, discarding a mismatch. Its tests
are offline (`pytest`, `ruff`; `scripts/check-python.sh`, part of `just check` when the
environment exists), with a parity harness that compares a tool's answer to stored outputs
within a stated tolerance and skips `segmenter`-marked cases until the artifact is fetched.
Launch is `tools/python/.venv/bin/rascal-tools serve` after `uv sync --frozen` there;
`crates/rascal-mask/tests/packs_real.rs` drives it through rascal's client. `segment` arrives
with the owner's segmenter decision (`tools/python/MODEL.md`).

**The bundle and offline setup.** A release ships one bundle per platform (`macos-arm64`,
`linux-x86_64`), built by `scripts/build-bundle.sh <platform> <dir>` on a connected machine:
the directory is the provisioning project itself (`pyproject.toml` pinning the pack wheel by
relative path for that platform only, and its `uv.lock`, resolved against the wheelhouse and
nothing else), `wheel/` (the pack from `uv build`), `wheelhouse/` (every locked dependency as
a wheel for the platform and Python 3.12), `MODEL.md`, `artifacts.toml`, and `bundle.toml`
naming the pack, its version, the Python version, the platform and the SHA-256 of every file
(the model artifacts' bytes join it with `segment`). On the target machine the prerequisites
are `uv` and a Python 3.12 it can find without downloading one (`uv python install 3.12` on a
connected machine). `rascal tools setup --bundle <dir> --into <dir>` (or `--config rascal.toml`,
installing where the pack entry's `command` points, `<dir>/.venv/bin/rascal-tools`) checks the
platform, verifies every digest (a mismatch or a missing file is refused with exit 3 naming the
file, before anything is written), copies exactly the listed files, runs `uv sync --frozen
--no-dev --offline --no-index --find-links wheelhouse --no-cache` with interpreter downloads
forbidden, smokes the launcher's `--version`, and prints the `command` line for the pack
entry; an earlier install in the same directory (one holding a `bundle.toml`) is replaced
whole, any other non-empty directory is refused. The SHA-256 of `bundle.toml` is the bundle
digest the lab records. Launch never touches the network. `scripts/bundle-acceptance.sh`
(part of `scripts/acceptance.sh`, and a macOS job in the package workflow) builds this
machine's bundle, installs it with the network disabled where the kernel allows (`unshare
-rn`) and the caches empty, calls `echo_region` through the install, refuses a tampered copy,
runs the pytest bundle test (`tools/python/tests/test_bundle.py`, which checks the build's
output when `RASCAL_BUNDLE` names one and skips otherwise) and the Rust install test against
the built bundle. On Linux the install and the call run in a network namespace with no
interfaces; macOS has no such namespace, so there the proof is uv's `--offline --no-index
--no-cache` with interpreter downloads forbidden, and the fact that the launcher never calls
uv. `setup` runs uv and the launcher with the baseline environment only (`PATH`, `HOME`,
`LANG`, `LC_ALL`, `TMPDIR`), so no key and no uv setting of the shell's reaches them, and the
environment is always `<dir>/.venv`. A bundle given as a URL (the spec's one network step)
arrives with the release, task 7.2; until then `--bundle` names a directory.

**The fake pack** (`rascal fake-pack <behaviour>` with the `testkit` feature, or the testkit's
`fake-pack` binary) serves `segment` with one selectable behaviour: `exact`, `bad-schema`,
`over-bounds`, `block-mismatch`, `foreign-frame`, `wrong-size`, `non-png`, `slow`,
`big-plane`, `die`, `ignore-signals`, `mutate <marker>`, `stderr`, `mute`, `env`,
`no-annotation`, `old-version`; `--concurrent` marks its manifest concurrent and `--name=<tool>` renames the
tool.
`crates/rascal-mask/tests/packs.rs` drives each through the client; `tests/isolation.rs` is the
isolation suite (G5): instruction-like filenames, prompts and tool output never alter control
flow, ids never resolve outside the registry, a pack sees the allowlisted environment only.
