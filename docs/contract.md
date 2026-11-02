# table-manners: rascal's side of the shared contract

rascal conforms to [table-manners](https://github.com/markhayden/table-manners) v0.3 on `main`
(milestones M8 and M9 for v0.1.0, M11 for v0.2.0, M12 for v0.3.0), pinned `=0.3.0` from crates.io
(PLAN.md 12.10), and to the v0.4 draft on the M10 branch (PLAN.md M10); the shared contract of
the owner's tools (ADR 0020, decisions 13 and 14): what an agent, a script or the playground sees
from outside is the same for squint, rascal and the tools after them. The contract's text is the
specification; this page is what rascal does under it (PLAN.md M8 to M12), as built on the M10
branch.

## The pin

`table-manners = "=0.3.0"` in the workspace `Cargo.toml` of `main`: the crate's serde types and
embedded schemas, no logic. On the M10 branch the pin is
`{ path = "../table-manners", version = "=0.4.0" }`, a sibling checkout of the contract's `v0.4`
branch (built from `d9e6058`, the commit with the v0.4 schemas, types and examples), until v0.4.0
is published (PLAN.md 10.15). `scripts/check-contract.sh` warns in CI when
crates.io has a newer release, and when the pin is a path (a build against a draft); moving the
pin is a deliberate commit that also refreshes `fixtures/contract/`.

## Image references (8.1)

Every import records the SHA-256 of the caller's file (`file_sha256` on the `import` record), so
`rascal_core::contract::image_reference` can give the contract's reference for any image in a
workspace: an imported image names the caller's file, an image rascal made names the PNG it
wrote. Projection and topology come from the image's frame; `pixel_sha256` is rascal's own pixel
digest, which is the contract's recipe byte for byte (the twelve vectors in
`fixtures/contract/pixels/` reproduce).

```
rascal workspace import ws/ photo.jpg     # img-1 raw:4032x3024:flat file_sha256:3f9a…
```

## Locations (8.2)

`--location` and `--stroke` take a table-manners location (recognised by its `image` field) as
well as rascal's own. Its image reference must name the file the run was given (the digest and
the dimensions); a location on another image exits 64. rascal holds one shape per location, so in
v0.1 it refuses, with the reason: a shape in several parts (split at the wrap edge), a polygon over
64 vertices (rascal's location schema is the copied envelope's, ADR 0009), and a `mask` in a
location slot. Outbound, `contract::location_out` turns rascal's locations into one contract
location: a split location leaves in parts with `straddles_wrap`, a `view` never leaves, and an
`absent` always carries a reason.

## The description (8.3)

```
rascal describe                          # one line per action: name, kind (and group), title
rascal describe --json [--config rascal.toml]
```

The contract's description document, an operations tool's (`profiles: ["images",
"operations"]`, task 10.4): one `task` per shipped recipe, then one `operation` per built-in op
(below), with the top-level `artifact_types` and the operations' groups in `vocabulary.group`.

The tasks: one action per shipped recipe (`seam`, `remove`,
`remove_prompt`, `repair`, `repair_area`, `protect`, and `mask_rect` for the `rect` recipe, so
it does not take the `rect` op's name), each `kind: "task"` (a whole job) with no
`group` and no `ports`, with the image slots (role
`source`), the location slots and the kinds each takes, the text and mask inputs and the common
arguments (`filler`, `set`, `projection`, `strict`) as `params_schema`, each with the default a
call that leaves it out runs with where there is one (`strict: false`; `seam`'s projection
`equirectangular`, the projection its recipe declares; without one the image's shape decides,
2:1 being equirectangular), the outputs (`mask`,
`result`; `remove_prompt` fills `locations.found`), the cost, the readiness, the duration and the
two renders (3.2): `dash` (`kind: "server"`, `ready_timeout_s: 10`, `rascal dash {run.dir}
--serve --port 0 --frame-ancestor {frame_ancestor}`) and `page` (`kind: "document"`, `rascal
dash {run.dir} --document`), described in `docs/diagnostics.md`. Every action's `params_schema` declares
`max_spend_usd` as `{"type": "number", "exclusiveMinimum": 0}`, since every task takes a filler
and so may be paid. With `--config`, the configured fillers are the `filler` enum and a
configured filler with a price makes a task `may_spend`.

The operations (3, 4.7; `describe::operation`): each of the 24 ops of `rascal_ops::registry()`
(SPEC "Operations", but `segment`, which a pack serves: a description is a local inspection and
a pack's ops are known only by starting it, so pack operations are out of v0.4 for rascal). No op
shares a name with a task or a reserved name. Each has:

- `name` the op's, `title` the name capitalized, `purpose` the manifest's `purpose` followed by
  "When to use: …" and "When not to use: …" from the manifest, `kind: "operation"`;
- `group` the op's (`Op::group`, a required trait method, so no op can leave it out): `find`
  (`line`, `rect`, `stroke`), `refine` (`rasterize`, `tighten`, `grow`, `shrink`, `close`,
  `subtract`, `select`, `invert`, `fan`, `profile`, `feather`), `prep` (`cut`, `put`, `ramp`,
  `apply`, `window`, `unwindow`), `out` (`render`, `paste`, `measure`) or `inspect` (`inspect`);
- `ports` the manifest's inputs (`in`) and outputs (`out`), each with `name`, `type` (the
  contract's: `image`, `location`, `rascal:Mask`, `rascal:BandProfile`, `rascal:Series`,
  `rascal:Plan`), `required`, `many` (only `select`'s `plane` takes a list) and `purpose`. Port
  names are lowercase with underscores and never a parameter's name (a test holds every op to
  it);
- `params_schema` the manifest's parameter schema (`Manifest::params_schema`) with, for an op
  that has constant sets, `constants` as an `enum` of them whose
  `default` is the first; no `max_spend_usd`;
- `images: []` and `locations: []` (its image and location inputs are ports only); `outputs`
  the roles of the images its out-ports make (the port's name), `preview` when it makes a
  location or a Mask, and `locations: ["found"]` when it makes a location;
- `cost: {"may_spend": false}`; `readiness` `ready` with no requirements in every configuration
  state (a built-in op reads no configuration); `duration` `typical_s` the manifest's
  `cost.seconds` and `max_s` a hundred times that, at least 60; `renders: []` (the dash reads a
  task's run workspace; an operation's picture is its preview).

`artifact_types` lists every type a port names but `image`, which the contract defines whole
(3.3), each at `schema_versions: [1]` with a purpose: `location` (`portable: "json"`, `preview:
true`), `rascal:Mask` (`bundle`, `true`), and `rascal:BandProfile`, `rascal:Series` and
`rascal:Plan` (`none`, `false`: they cannot leave their workspace yet, so a host labels them
workspace-dependent). `serve`'s `tools/list` is exactly the description's actions plus
`describe` and `estimate` (task 10.6; "Operations as runs" below).

`vocabulary` is the core's object of word lists: `group` (`find`, `refine`, `prep`, `out`,
`inspect`) and the images profile's four (`disposition`, `confidence` and `evidence` empty,
`score_kind` `["owlv2-detection"]`). `describe`,
`estimate`, `export` and `import` are the contract's reserved names, tools and never actions
(`describe::RESERVED`; `estimate` below; `export` and `import` arrive with task 10.10). The
description does not use MCP roots: caller paths are governed by `--allow-dir` (8.5).
`fixtures/contract/describe.json` pins the document without a configuration. The
`describe` MCP tool returns the same document (8.5).

### Readiness (3.1, task 12.2)

`describe` never fails on the configuration and always exits 0. The configuration is read once
into a state: `missing` (no `--config`, or a path naming no file), `invalid` (it does not read,
parse or check; the reason names the file, line and column, never the offending text) or
`loaded`, with its identity, the SHA-256 of its canonical serialization (ADR 0016); with no file
the identity is the built-in defaults' (no filler, no pack, no limits), which are a
configuration too. There is no default lookup: `--config <path>` is the only way to choose one.

Each action's readiness is its default use: unconfigured, the mask alone, which needs nothing
(every action but `remove_prompt` is `ready` with no requirements); with fillers configured,
healing with any of them, so every variable a configured filler reads is named. It is
`needs_setup` when a requirement is `missing` or `invalid`, else `ready` (rascal never says
`unavailable`). Requirements come from a local inspection (`rascal_mask::readiness::for_call`):
it reads variable names, the configuration and the file system, never a credential's value, and
never starts a pack, sends a request or writes anything.

| kind | name | state |
|---|---|---|
| `config` | `rascal.toml` | `present`, `invalid` (with the reason in `setup`) or `missing`; listed on every action once a file is given, and on `remove_prompt` always |
| `model` | `fillers.<name>` | `present` for each configured filler (configured, not checked) |
| `credential` | a filler's `key_env` | `missing` when the variable does not exist, else `valid` or `invalid` from this session's provider answers, else `present` |
| `dependency` | a command filler's `command[0]` | `present` when it is an executable file (by path, or found on `PATH`), else `missing` |
| `dependency` | `packs.<name>` (`remove_prompt`) | the entry whose command is `<dir>/.venv/bin/rascal-tools`: `missing` with no such entry or no launcher there, `invalid` when `serve` could not start it, else `present` |
| `file` | each model file the install's `artifacts.toml` names | `present` when it is in the directory the pack reads and not empty, else `missing` naming that directory; digests are not hashed here (the pack checks them before use), so a file is never `valid` |
| `env` | each non-secret variable the segmenter entry passes (`[packs.*] env`) | `present` when set; unset, `RASCAL_TOOLS_MODELS` is `missing` only when the files are not where the pack falls back to, and anything else is omitted |

The directory the pack reads its models from is resolved exactly as the pack does:
`RASCAL_TOOLS_MODELS` when the entry passes it and it is set and not empty, else `models/` in the
install directory when it exists, else `~/.cache/rascal-tools`. `fixtures/readiness/models-dir.json`
is the table both the Rust and the Python suites check. The install's `artifacts.toml` is the one
at its root (a bundle install) or under `src/rascal_tools/` (a checkout's `tools/python`). A
command filler passes no variables of its own, so it adds no `env` requirement.

The session keeps what providers said about each credential (task 12.3): when a run ends,
its filler call records update `serve`'s answers, a 2xx making the filler's key `valid` and a 401
or 403 `invalid` (any other answer, or a cache hit, which makes no call, leaves it as it was). A
fresh session, and the command line, show `present` until a real call settles it; a key a
provider rejected stays `invalid` for the session, since its environment cannot change.

A call is refused only for what it needs, before any run: its arguments are read for the
filler, its own readiness is computed (`MaskOnly`, or the one filler it names), and when a
requirement it needs is `missing` or `invalid` (no configuration or an invalid one for a filler
call, a filler the configuration does not have, its key not set or already rejected, its program
not found, `remove_prompt` without its pack, a model file or the models variable), `serve`
answers JSON-RPC `-32602` with `data: {reason: "not_ready", run: null, requirements}` naming
exactly those requirements (`refusal.json`). It allocates nothing, writes no run, spends nothing
and releases its `client_run_id`. On the command line `rascal run` and `rascal heal` make the
same check before the run directory exists and exit 64 naming the requirements (under
`--replay` they do not: a replay answers from the record, so whether it needs the pack or the
filler is known only as it runs). A mask-only call needs nothing and runs in every
configuration state.

## `run.json` and the header (8.4)

Every `run` and `heal` writes `run.json` into its run directory, last and atomically (a
temporary name, then a rename), whatever the outcome, when the directory was new or empty at the
start; an `--out` that held something is refused untouched. It is the contract's result: the
header (`status`, `error`, `run` with `rascal://run/<id>/run.json` and the absolute directory,
`started`, `finished`, `duration_ms`, `input_identity`, `spend`, `cache`, `notes` from the
journal) and rascal's body (`recipe`, `outcome` in rascal's own statuses, `stopped_at`,
`reason`, `mask_dir`, `filler`).

| rascal | `status` | `error.code` |
|---|---|---|
| the recipe ran, whatever its steps reported (`ok`, `measured_nothing`, `not_applicable`) | `completed` | |
| a usage or recipe error, a bad parameter | `invalid` | `usage` |
| a location on another image, one rascal cannot hold, an image it cannot decode | `invalid` | `input` |
| reading or writing its files | `error` | `io` (retryable) |
| a filler endpoint | `error` | `provider` (retryable) |
| a filler endpoint that rejects the key (HTTP 401 or 403) | `error` | `credential` (never retryable; exit 3) |
| a `serve` call whose named estimate no longer matches it (task 12.6) | `invalid` | `stale_estimate` (never retryable) |
| a worst case over the effective per-call cap (task 12.4) | `error` | `cap` (never retryable) |
| a session budget that cannot hold the worst case (task 12.4) | `error` | `budget` (never retryable) |
| a filler command or a pack that died | `error` | `tool_process` (retryable) |
| a timeout | `error` | `timeout` (retryable) |
| anything else | `error` | `internal` |
| SIGINT or SIGTERM | `cancelled`, exit 130 or 143 | |

`images.in` is the input image's reference (slot `image`, role `source`); `images.out` is the
mask (the `grey` rendering, role `mask`) and, for `heal`, the result (role `result`), each with
its file's digest and dimensions and an absolute `path`. A heal whose filler fails is an `error`
that still lists the mask. `locations.used` holds the location the run took in;
`locations.found` the segmenter's boxes, with `score` and `score_kind: owlv2-detection`.
`input_identity` is a SHA-256 over what was asked (the action, the recipe, the image's file
digest, the location files, `--set`, the prompt, the filler), so equal requests have equal
identities. `spend` and `cache` come from the fill (8.6). `artifacts` (4.7), which every result
of an operations tool carries, is `{"in": [], "out": []}` on a task's result until tasks list
the Masks they make (task 10.8); an operation's run fills it ("Operations as runs" below).

### The invocation (4.8, task 10.2)

Every result records `invocation`, what the call resolved to, so it can be repeated from another
directory, by another client or on another machine (`crates/rascal-mask/src/invocation.rs`):

- `action`: the action's name (a recipe file run by `rascal run` has no action and is `custom`).
- `arguments`: the call's, resolved. Every default the action's `params_schema` declares is
  applied (`strict: false`), and `projection` names the projection the image ran in (given,
  declared by the recipe, or chosen by the image's shape). Each image is `{"expect": <image
  reference>}` with no `path` or `inline`: the reference of the file, the same one `images.in`
  names, so one image given by path or inline records one invocation. Each location is as given;
  `max_spend_usd` is kept; `return_images` and `image_blocks` (delivery) are left out. `mask` and
  `protect` are Mask references (3.4, task 10.3): the store's reference of the Mask `serve`
  bound, so nothing in an invocation is a path.
- `effective`: the estimate's object, from the one function both use (`estimate::effective`):
  the model, the configuration's identity, and `x-rascal` with the filler and the constant sets
  in force by step (see "Estimates"), so an estimate and the call it priced say the same.
- `requirements`: the name of each readiness requirement the call used (its own readiness, 3.1),
  never a value; a filler program given by path is named by its file name (`rascal`), so no path
  is recorded.

It is null only on an `invalid` run whose arguments did not resolve (a bad argument, an image
that does not match its `expect` or cannot be read, a location on another image). A run that
resolved and then failed, was cancelled or was refused by the limits (`cap`, `budget`,
`stale_estimate`) records it. On the command line, `rascal run` and `heal` record their flags as
the matching action's arguments: the image by its reference, `--bbox`, `--location` and
`--stroke` as the contract locations the run used (a contract location file as given), `--set`
as `set` with its values typed, `--prompt`, `--projection`, `--strict`, `--mask` and `--protect`
as the Mask references the stored directories have (the reference the workspace around the
directory, or `--workspace`, gives; built from the mask alone for a raw frame no workspace
holds), and `heal`'s filler; so a command-line run is callable through `serve` with a locator
added to its image. The flags resolve before the run starts and before the signal watcher is
armed: flags that do not resolve end the command `invalid` at once (exit 64, `run.json` with a
null invocation and nothing else in the run directory), so a SIGINT or SIGTERM can only cancel a
run that records its invocation.

## `serve` (8.5)

```
rascal serve [--workspace ws/] [--config rascal.toml] [--concurrency n] [--runs dir]
             [--allow-dir /abs/dir]...
```

`serve` starts in every configuration state (3.1, 5): with no `--config`, a file that does not
exist or one that does not load (said once on stderr), its actions report what is missing and a
call that needs it is refused `not_ready`. `--workspace` (the session's store, where references
resolve and operations commit) defaults to
the configuration's `[workspace] dir`, relative to the file, else
`$XDG_DATA_HOME/rascal/workspace` beside the runs root, so a host needs nothing but `serve` and,
if it likes, `--config`. `serve` starts each configured pack once to learn whether it starts and
then stops it (it serves no pack tool): a pack that does not start does not stop the server,
stderr says why, and its requirement reads `invalid`; a run started with such a pack notes it in
its journal, and a recipe whose op nothing serves then fails with the pack's own error.

`serve` lists one MCP tool per action in the description, tasks and operations, then `estimate`
and `describe`, and nothing else (5; task 10.6): the op tools, the pack tools and the
`workspace_*` tools of ADR 0015, with rascal's own envelope as their result, are gone, with no
shim. A task's call runs the
recipe, through `heal` when `filler` is given, in its own run directory under `--runs`
(`$XDG_DATA_HOME/rascal/runs`, else `~/.local/share/rascal/runs`), and its result is that run's
`run.json` as `structuredContent`, with `isError` true for `invalid` and `error`. JSON-RPC errors
are left for failures before a run exists (an unknown tool, malformed arguments, a call that is
not ready, references that do not bind).

- **Images in.** `image` is `{ "path": "/abs/…" }` or `{ "inline": { "media_type", "data" } }`,
  each with an optional `expect` reference checked against the file (digest and dimensions; a
  mismatch is `invalid` with code `input`). A path is held to the path rule (below); inline bytes are written to
  the run's input directory (`<runs>/<id>.in/`) and imported like a file, journaled; the limit is
  the description's `limits.inline_bytes` (64 MiB).
- **The path rule (5.2, task 10.1).** `serve` reads a caller's path only inside the directories
  the session allows. Each `--allow-dir` (repeatable) must be absolute and name an existing
  directory, else `serve` exits 64 naming the flag before it answers anything; each is resolved
  once at start (`.`, `..`, every symlink). The configuration's `[serve] allow_dirs` (absolute)
  narrows them to the overlap and never widens them: where a configured directory and a flag
  directory overlap, the deeper one is allowed; a configured directory under no flag allows
  nothing; `allow_dirs = []` allows nothing; no `[serve] allow_dirs` leaves the flags as given; a
  configured directory that does not resolve allows nothing (stderr says so); a configuration
  that does not load allows no caller path, failing closed since what it would narrow is unknown
  (stderr says why; `serve` still starts, and `describe`, `estimate` and inline images work). With
  no `--allow-dir` every caller path is refused, closed by default; inline images keep working. The set is fixed for the session, printed on stderr at
  start (`rascal serve: callers' paths are read only inside: …`) and listed by `rascal://session`
  as `allowed_dirs` (4.6, task 10.2: absolute, resolved, `[]` with none). MCP roots are not used:
  `serve` never sends `roots/list`.
  `rascal_core::paths::PathRule::admit` is the one check, for every caller path `serve` takes:
  the image slots of tasks and the image ports of operations (`mask` and `protect` are
  references, task 10.3, never paths). The path must be absolute and its canonical form (every symlink resolved,
  the final component included) must exist, be a regular file and lie under an allowed directory
  component by component (`/a/bc` is not under `/a/b`); the file is then read through the canonical path. So a link inside
  pointing out is refused, and a link outside whose target is inside is read (the target is
  what is read). Every failure is one refusal with one text, `<slot>: this path is not readable
  here (only files inside the directories rascal serve allows; CONTRACT 5.2)`, which says
  nothing about whether the file exists or what it is: a run ends `invalid` with code `input`;
  `estimate` is the `-32602` `invalid_arguments` refusal with code `input`. `serve` never writes at a caller's path: only under its
  runs root, its workspace and its cache. A check-then-read race (a symlink
  swapped between the check and the read) is outside what the rule guards: the caller is on the
  host's own stdio, and the rule exists to keep an agent from naming files such as a key. The
  rule governs `serve` only: `rascal run`, `heal`, `estimate` and every other command take the
  person's own paths.
- **Images out.** `return_images` chooses the locator: `path` (the default), `inline`, or
  `resource` (`rascal://run/<id>/images/<role>`). `image_blocks: true` also sends each as an MCP
  image block.
- **Locations in.** `location` and `stroke` take a table-manners location object (8.2).
- **Artifact references and binding (3.4, 4.7, task 10.3).** `mask` (of `protect`) and
  `protect` (of `remove` and `remove_prompt`) take a `rascal:Mask` artifact reference and
  nothing else: `{type, schema_version, tool, identity, frame, on?, resource}`
  (`rascal_core::contract::artifact_ref`). rascal's types are `image`, `location`,
  `rascal:Mask`, `rascal:BandProfile`, `rascal:Series` and `rascal:Plan`, each at schema version
  1; `identity` is the artifact's identity (ADR 0016), an image's being `sha256:` and its file
  digest; `frame` is the frame's identity, never its id; `on` is the image at the root of the
  frame chain it was made on, as its file digest with the root frame's size, projection and
  topology and no pixel digest, so every store gives an artifact the same reference; `resource`
  is `rascal://artifact/<identity>` (served from task 10.7). No workspace id appears. The
  session's store is `serve`'s workspace (`--workspace`): a reference resolves there by identity
  (`Workspace::resolve`, from an index built once per generation; an image by the file digest
  its `import` recorded, or, for an image rascal made, the digest of the PNG it wrote, read once
  per image), so a reopened or moved workspace resolves the same identities. After
  readiness and before any run, each reference is bound (`rascal_core::binding`): the type
  (another tool's type, an unknown one, one the argument does not take, or a `rascal:` type a
  tool other than rascal made is `wrong_type`), the version (`wrong_version`), then resolution
  (`unresolved`, with a remedy: for a Mask "Import its bundle with the import tool (rascal
  import), then call again."). References that do not bind are refused with `-32602` and
  `data: {reason: "binding", run: null, bindings}`, one entry per reference in argument order,
  `port` the argument's name, `expected` `rascal:Mask v1`, `got` the type and version or the
  identity; like `not_ready` it writes no run, spends nothing and releases the call's
  `client_run_id`. A value that is no reference (a path, a string, a missing field) is plain
  invalid params (`-32602`, class `usage`). A bound Mask goes to the run as its bundle (10.5,
  exported from the store in memory) and is imported into the run's workspace after the image,
  with its frame chain, so a mask in a `cut:` or `crop:` frame needs no `--workspace` from the run
  that made it; the recipe then runs as before, and a Mask on another image is the run's
  `invalid`. Until the import tool (task 10.10), `rascal workspace import-mask <store>
  <bundle>` puts a Mask into the store.
- **Operations as runs (4.7, task 10.6).** Each built-in op is an MCP tool of its name whose
  input is its `params_schema` plus one property per in-port: an artifact reference, a non-empty
  array of them for a `many` port (`select`'s `plane`), and for an `image` port also an image in
  (`path` under the path rule or `inline`, optional `expect`, whose projection the import takes;
  without it an exactly 2:1 image is equirectangular), for a `location` port also a location
  (a table-manners location on the call's image or rascal's own in its frame, validated against
  the call's first image, else against the root image an input in a raw frame was made on, and
  registered in the store as `workspace import-location` does); `return_images` and
  `image_blocks` as for a task. A missing required port, an empty `many` list and a value that is
  no reference where only a reference goes are plain invalid params. Before allocation every
  reference is bound as above, and the frames of the bound inputs are checked together with the
  op's `Op::check_frames` (by default every input in the first's frame, by frame identity; `put`:
  the canvas in the `cut:` frame of the image's frame and the plan made for that frame;
  `unwindow`: the crop in a `crop:` frame of the image's; `measure`: the images and the mask in
  one frame, the line in any; `invert`: one input; `window`: the default). Every problem goes in
  one `binding` refusal, in port order and within a `many` port in the order given, with `index`
  for an item of a `many` port, `wrong_frame`'s `expected` naming the frame wanted and `got` the
  input's frame identity; no run, nothing spent, the `client_run_id` released. Readiness is
  always `ready`. Then the run is allocated like a task's (the claim, the permit, progress with
  the run's reference, cancellation, the association, `run.json` atomically and last); the limits
  check only checks the arguments (an operation's worst case is 0; an estimate named in `_meta`
  is `stale_estimate`, since this session issues none for an operation until task 10.9). Images
  and locations given by value are registered in the store, and the frame check runs again over
  every input (a mismatch is `invalid`/`usage`). The op runs with `Workspace::invoke_with` on the
  store, step id the run's id, so its outputs commit there as one operation record and bind by
  reference at once; nothing upstream reruns. The run directory holds the run's journal line
  (`rascal://run/{id}/journal`), a copy of each image out and each preview, never a workspace.
  The result: `ok` and `measured_nothing` are `completed`; `not_applicable` is `completed` with
  the reason in `notes` and nothing out; `invalid_args` is `invalid`/`usage`; an operational
  failure is `error` with its code; whatever stops the run before its invocation resolves is
  `invalid`. `artifacts.in` lists each input (`preview: null`) and `artifacts.out` each output,
  a `many` port once per item, each with its reference and, for `location` and `rascal:Mask`, a
  preview: an image out with role `preview`, `preview-<output>.png` in the run directory, located
  as `return_images` asks (`crate::preview::draw`: a Mask is `inspect`'s overlay on the image of
  its frame when the store holds it, found through the records that made it, else its grey
  rendering; a location is its rasterized outline drawn the same way on its image, or on a
  plain canvas of its frame when the store holds none, or that image alone for a location with
  no interior; deterministic). Images taken are in `images.in` (slot and role the port's name),
  images made and every preview in `images.out` (an image out's role is its port's name, and its
  identity is the file digest), locations taken in `locations.used` and made in
  `locations.found`; `body` is `{"envelope": <the op's envelope>}`; `spend` is a measured zero.
  The invocation's `arguments` are the parameters as the op ran them (the constant set's values
  and the manifest's defaults applied, `constants` named), each reference as the store's, each
  image as `{"expect": <reference>}`, each location as given; `effective` is `{model: null,
  config_identity, x-rascal: {constants}}`; `requirements` is empty. `input_identity` digests the
  op, its version, the resolved parameters and the inputs' identities.
- **Progress.** With a progress token, every op the run journals is a notification
  (`step fan (fan): ok`), and a heartbeat goes at least every five seconds while it waits;
  `progress` is a counter that only increases and the words are in `message`, for people. From
  the run's allocation, every notification, heartbeats included, carries the run's reference
  (`id`, `resource`, `session_id`, `client_run_id`) in `_meta["table-manners/run"]`; a call still
  waiting for the permit sends none, and no message names the run (table-manners 4.1, task 11.4).
- **Correlation ids.** A call may carry `_meta["table-manners/client_run_id"]` (1 to 128 of
  `[A-Za-z0-9._:-]`; anything else is refused with `-32602` before a run). `serve` claims it when
  it accepts the call, before the permit wait, so duplicates execute at most once; a claimed id
  is refused with `-32602` and `data: {reason: "client_run_id_reused", run}`, `run` being the
  earlier call's reference, or null while it has none. Once a run is allocated its claim lasts
  the session, whatever the run's outcome, and so does the claim of a call cancelled while queued
  or before allocation (with `run: null`): a retry uses a new id (table-manners 4.4, task 11.3). A
  call that ends in a JSON-RPC error while it has no run (the action's error or a panic before
  allocation, any refusal before a run) releases its id, so the corrected call retries under the
  same id; the release and the claim are one step under the same lock, and a run reference the
  action sent just before it ended reaches the claim first (v0.3, task 12.7). `describe` and
  `estimate` are not runs and take no id.
- **Cancellation.** A call cancelled while it waits for the permit, or before its run is
  allocated (checked again once the permit is held), leaves no run. After allocation,
  `notifications/cancelled` sets the run's flag: the run stops before its next
  step (each op checks the flag before it commits, and `heal` before it calls the filler), writes
  `run.json` as `cancelled` with `stopped_before`, and the call gets no result. A filler call
  already in flight runs to its end (its timeout bounds it).
- **Reads never wait on a run.** A run holds the call permit (serial by default,
  `--concurrency n` for more); `describe`, `estimate` and every resource read do not, so they
  answer while a run is under way (task 9.3).
- **Resources.** `rascal://runs{?cursor}` (the contract's runs list, newest first by start and id,
  50 a page; `next_cursor` is the last entry's run id, null on the last page, and
  `rascal://runs?cursor=<it>` reads the next; a run started after page one was read is on a fresh
  page one; task 11.5; a run still going is `running` from its allocation: its directory and its
  association, the action and both ids in `<runs>/<id>.in/association.json`, are made before the
  first progress line and any paid work, tasks 9.4 and 11.3), `rascal://session` (4.6),
  `rascal://run/{id}/run.json`, `rascal://run/{id}/journal`, `rascal://run/{id}/images/{role}`. An
  id is exactly `run-<ms>-<pid>`; nothing else under the runs root, and nothing outside it, can be
  read.

## The fill cache, prices and limits (8.6)

A filler entry's `spend_usd_per_call` is the run's price; `[cache] dir` keeps every fill by its
request, filler and revision, so an identical request (a retry, a second arm with the same mask)
is answered without calling the filler: `run.json` says `cache: hit` and spend 0, a call says
`miss` with the price (table-manners 4.4). A fill is kept at the size of the request it
answered (the scaled size under `fill_max_side`), and a cached fill of any other size is refused
with exit 3 rather than resized. The lab always caches, in `fills/` under its directory.

A run's spend is its journal's, whatever its status (task 9.1): a call that may have been charged
is never reported as zero, and cancelling does not erase a cost. Each call is *not sent* (refused
before its request left rascal: no key, the connection refused, the command not started),
*known* (the filler answered: a 2xx, exit 0, or the authentication rejected with 401 or 403) or
*unresolved* (sent and not settled: a timeout, any other failure status, a non-zero exit, or the
run killed mid-call). Known and unresolved calls count at the price, except an answer that
refused before any work, a 401 or 403, which is known and counts zero (4.5); `heal` journals
`filler call started` before each call, so a run stopped mid-call counts it too. `spend.usd` is
the total and `spend.unresolved_usd` the part no answer settled, with a note (table-manners v0.2,
4.5). `spend.basis` is `estimated` when any amount comes from a price (a known or unresolved
call, at its configured price) and `measured` when the spend is a known zero: a call answered
with 401 or 403 and none counted at its price (`run-credential-rejected.json`). A run with no
call, or only calls not sent, stays `estimated` at zero. The call's reservation is settled at
what it spent, so a rejected key holds nothing back from the budget.
`[limits]` holds `serve` to `call_cap_usd` per call and `session_budget_usd` across the process's
calls (4.5). A call's `max_spend_usd` lowers the cap: the effective cap is the configured one
lowered by it, or the caller's value when no cap is configured. It is an argument like any other,
so 0 or less (a cap of zero is not a cap) and a value above the configured cap are an `invalid` run
with code `usage` naming the amounts, never silently lowered (task 12.4). The limits check runs
after allocation and before any paid step, in this order:

1. the arguments, `max_spend_usd` among them (`invalid`, `usage`);
2. the estimate the call names, when it names one (`stale_estimate`, task 12.6; see
   "Estimates");
3. the cap: a worst case (the filler's price) over the effective cap is an `error` with code `cap`;
4. the budget: a worst case the session's budget left cannot hold is an `error` with code
   `budget`, and otherwise the worst case is reserved in the same step.

`cap` and `budget` are never retryable, and a refused call spends nothing and never reaches the
filler. From task 12.2, a requirement the call needs that is missing or invalid is refused before
allocation, with no run. The budget check and the reservation are one step under the session ledger's
lock (task 9.2), so concurrent calls
(`--concurrency`) never both pass against the same spend; the run's end settles the reservation
into what it spent (unresolved charges included), and a run that ends without settling counts its
reservation as unresolved spend. The `estimate` tool (below) prices a call with the same function
as the limits check; the session is read
from `rascal://session` (table-manners 4.6, task 11.2): one atomic observation of the ledger
under a random UUID made when `serve` starts, with `budget_usd` and `remaining_usd` (both null
when uncapped), `spent_usd` (unresolved included), `unresolved_usd` and `reserved_usd`, amounts at
six decimals, and `allowed_dirs`, the directories caller paths may name (8.5). rascal reads its configuration once, so a session lasts the process, and every run
`serve` makes carries its id. An action `may_spend` in the description when a
configured filler has a price. Each run's start time is strictly increasing within the process, so
calls allocated in the same millisecond still get distinct ids and directories.

```toml
[cache]
dir = ".rascal/fills"
[limits]
call_cap_usd = 0.50
session_budget_usd = 5.00
[fillers.hosted]
spend_usd_per_call = 0.04
```

## Estimates (4.5, task 12.5)

```
estimate                                  # the reserved MCP tool: {"action": …, "arguments": {…}}
rascal estimate [--json] [--config rascal.toml] <request.json | ->
```

`estimate` takes the action's name and exactly the arguments the paid `tools/call` would send
(images, locations, `filler`, `set`, `max_spend_usd`, `return_images`, …) and answers with the
contract's `estimate.json` as `structuredContent`. It is never a run: it never waits for the
permit, never allocates, imports, writes a run or touches the workspace, never starts a pack and
never calls a filler; it reads the call's images only to hash them. It checks the arguments as
the run does and prices the call with the function the limits check uses (`estimate::price` in
`crates/rascal-mask/src/estimate.rs`), so the two cannot disagree:

- `worst_case_usd`: the filler's `spend_usd_per_call` for the one call a heal makes, 0 for a
  mask-only call (or an unpriced filler). A call that cannot be priced (it needs a
  configuration that is missing or invalid, names a filler that is not configured, or is
  `remove_prompt` without its pack) has `worst_case_usd` null and is refused for `readiness`,
  never priced at a made-up zero.
- `cap_usd`: the effective per-call cap (the configured `call_cap_usd` lowered by
  `max_spend_usd`), null when uncapped.
- `allowed` and `refused`: `readiness` when the call's own readiness (its filler, 3.1) is not
  `ready`, else `cap`, else `budget`, else allowed. A call that is not ready is an estimate
  refused for `readiness`, not an error.
- `effective`: `model` is `fillers.<name>` for a configured filler, null for a mask-only call;
  `config_identity` is the loaded file's identity, or the built-in defaults' when the call reads
  no configuration (with no file, or with one that is missing or invalid), null only when the
  call needs the configuration that is missing or invalid; `x-rascal` is `{"filler": {name,
  kind: "http" | "command", revision, paste, fill_max_side} | null, "constants": {<step>:
  <set>}}`, the constant set in force at each recipe step whose op has sets: the recipe's, then
  the named filler's `"<step>.constants"` override, then the call's `set`, a step naming none
  running its op's first set (task 10.2; `estimate::constants`). The same
  function (`estimate::effective`) gives the run's `invocation.effective`, so an estimate and the
  call it priced name the same choices.
- `readiness` (the call's own), `session` (one observation of the ledger, the document
  `rascal://session` returns, taken once with the budget check) and `session_id`, and `expires`,
  one hour after the estimate (prices are configuration, fixed for the session).
- `binding`: the record identity (ADR 0016, kind `estimate`) of `{action, arguments,
  tool_version, config_identity, session_id}`, where `arguments` leaves out `return_images` and
  `image_blocks` (they only choose delivery), each image slot's value is `{"file_sha256": …}`
  (a path read, inline bytes decoded; an `expect` verified, then dropped), each Mask reference
  (`mask`, `protect`) is `{"mask": <its identity>}` (the estimate checks the reference's shape
  and never reads the store, so it does not bind it), and every other argument, `max_spend_usd`
  included, enters as sent. One call by path or by the same bytes inline binds once; any other
  argument, a byte of an image, the configuration, the session or the version binds anew.

A wrong request is never an estimate. What the input schema rejects (no `action`, `arguments`
not an object, another key) is a plain JSON-RPC `-32602`. Anything else (an unknown action, an
argument the action does not take or a malformed one, a missing required input, `max_spend_usd`
not above zero or above the configured cap, an `expect` that does not match its image) is
`-32602` with refusal data `{"reason": "invalid_arguments", "run": null, "error": {"code",
"retryable": false, "reason"}}`, the code as the run would give it (`input` for an image rascal
cannot take, `usage` otherwise) and the reason naming the argument. A run checks its arguments
the same way first, so an argument an action does not take is now an `invalid` run too.

**A call bound to its estimate (task 12.6).** A call may send its estimate's binding as
`_meta["table-manners/estimate"]`; `serve` hands it to the action with the correlation id
(`CallMeta` in `rascal-mcp`). At the limits check, after the arguments and before the cap, the
binding is recomputed for the call as it arrived (the same `estimate::bind`, over the same
images) and compared with what this server issued: it remembers every binding it issues, with
its expiry and the digest of each argument (images by slot), the newest 1024. The run fails
`invalid` with code `stale_estimate` (never retryable, spend 0, before any step's paid work, any
reservation and any filler call) when the value is not a binding (`sha256:<64 hex>`, or not a
string at all: a run exists by then, so it is no JSON-RPC error), when this session never issued
it (another session's, which for rascal also means any other configuration's, since a session
reads its configuration once; or the command line's), when the call differs (the reason names
each image whose bytes changed and each argument added, removed or changed), or when it is past
its `expires`. A matching estimate is no reservation: the cap and the budget are checked again,
so an allowed estimate followed by a call the budget can no longer hold ends `budget`. A call
that names no estimate is checked against the limits alone.

`rascal estimate --json` prints the same document with no session: `session_id` and `session`
are null and the binding is made over a null session id, so no server accepts it; the budget is
never the refusal (there is no ledger), the cap and readiness still are. It exits 0, or 64 on a
bad request with the reason on stderr; without `--json` it prints one line.

## `rascal playground` (8.7, 9.5)

```
rascal playground
```

Wafflery (formerly Wafflehouse) is the one local program shared by the waffle tools, in its own
repository; rascal
carries no UI and no server for it, and follows Wafflery's launcher agreement (H1), as squint
does. The command finds `wafflery`: `WAFFLERY_BIN` when set (an absolute path to an
executable file; anything else is 69 naming the variable, never a fall-through to another
program), else `PATH`, else `~/.local/bin`. It runs it with exactly `--tool rascal`, without a
shell, and returns its exit code (128 + N when a signal ended it). The subcommand takes no
arguments (64). Wafflery owns everything after the hand-off: the host, its locks, the browser.

When it is missing, rascal says where to get it and exits 69. The installer waits for an
approved Wafflery release, so a release build makes no offer; test builds reach the offer
through a seam (`--installer <program>`, run as `<program> --prefix ~/.local/bin`, and
`--terminal` to answer from stdin). On a terminal (stdin and stderr) rascal asks on stderr: no,
or no answer, exits 0 and changes nothing; yes installs, then runs `~/.local/bin/wafflery`; an
installation that fails, or leaves no program, exits 74 and runs nothing. Off a terminal: 69.

## Conformance (8.8)

- **Fixtures.** `fixtures/contract/results/` holds one masked `run.json` per outcome from the
  command line and `fixtures/contract/describe.json` the description, both validated against the
  contract's schemas and compared in the test suite. `fixtures/table-manners/` is what the
  contract repository's `fixtures/rascal/` takes (tasks 11.6 and 12.9): from `rascal serve` and
  `rascal estimate` through the independent client, never written by hand.
  `tests/it/table_manners_fixtures.rs` regenerates them under `UPDATE_FIXTURES=1` and otherwise
  compares them file for file, holding each to the contract as the contract's own
  `tests/fixtures.rs` does (the schema its name chooses, the contract version, the tool's name,
  the crate's types, at most six decimals in every dollar amount). Machine values are replaced by
  stand-ins that stay valid: paths under `/runs`, `/data`, `/work` and `/tmp` (the temporary
  directory the suite's servers allow), one time, session ids, run ids, estimate bindings and
  configuration identities in their order, the estimates' and then the invocations' (the
  built-in defaults' identity is kept, being the same everywhere), the fake filler's address.
  At v0.4 (task 10.2) every action has `kind`, the session has `allowed_dirs`, and every result
  has its `invocation`, null only in `run-invalid.json`, whose image is not the one it expected.

  | file | what it shows |
  |---|---|
  | `describe.json` | configured (command fillers, an HTTP filler with its key set, a cap and a budget): every action but `remove_prompt` `ready` |
  | `describe-unconfigured.json` | no `--config`: the mask-only actions `ready` with no requirements, `remove_prompt` `needs_setup` |
  | `describe-credential-missing.json`, `-present.json`, `-valid.json`, `-invalid.json` | the HTTP filler's key unset; set (every action `ready`); after a 2xx; after a 401 |
  | `estimate-allowed.json`, `estimate-refused-cap.json`, `estimate-refused-budget.json` | a paid `seam` call allowed; under its price (`max_spend_usd`); after a paid call left too little budget |
  | `estimate-refused-readiness.json` | `remove_prompt` without its pack: `worst_case_usd` null |
  | `estimate-unconfigured.json` | a mask-only call with no file: allowed at 0 under the built-in defaults' identity |
  | `estimate-cli.json` | `rascal estimate --json`: `session_id` and `session` null |
  | `run-completed.json`, `run-error-unresolved.json`, `run-invalid.json`, `run-cancelled.json` | a run per status, an unresolved charge among them |
  | `run-stale-estimate-argument.json`, `-image.json`, `-session.json`, `-expired.json` | `stale_estimate`: another argument, a changed byte of the image, a server restarted under another configuration (for rascal one answer stands for another session and another configuration: a session reads its configuration once), a binding past its `expires` |
  | `run-cap.json`, `run-budget.json`, `run-credential-rejected.json` | the per-call cap, the session budget, a 401 during the run |
  | `run-claim-released.json` | the accepted retry under the id a `not_ready` refusal released |
  | `refusal-not-ready.json` | a call naming the filler whose key is unset, refused before any run |
  | `refusal-earlier-run.json`, `refusal-no-run.json`, `refusal-cancelled-before-allocation.json` | an id reused after its run ended; ids whose calls were cancelled before allocation, still refused with `run: null` |
  | `runs.json`, `runs-page-2.json`, `session.json` | the two pages of the runs list and the session |
- **No provider call.** `tests/it/no_provider_call.rs` proves `describe` and `estimate` make
  none (table-manners 7): across every configuration state (none, a path naming no file, a file
  that does not load, a loaded one) and credential state (missing, present, valid, invalid),
  `describe` (the command line and the tool), `estimate` (the command line and the tool, allowed
  and refused for the cap, the budget and readiness) and every `not_ready` refusal (the tool's,
  and `rascal heal`'s exit 64) leave the recording fake HTTP filler with no request, the runs root
  and the workspace unchanged, and, on the command line, the fake segmenter pack never started
  (`serve` starts its configured packs when it starts, so its servers are configured without one).
- **The cross-tool acceptance test, rascal's half.** A squint-shaped finding (a table-manners
  location with squint's `source`, `disposition` and `confidence`) masks exactly what rascal's own
  location does, and a finding on another image is refused
  (`crates/rascal-mask/tests/it/contract_locations.rs`), in CI against the pinned contract. The other
  half, rascal's mask back to squint as the region where change was allowed, lands with the mask
  handoff milestone.

## rascal's v0.1 limits, for the contract's MAPPING

What rascal refuses at its boundary in v0.1, each with the reason in the error, so the
contract's `MAPPING.md` can say so:

| the contract allows | rascal v0.1 | why |
|---|---|---|
| a location in several parts (`straddles_wrap`) | refused inbound (`input`); emitted outbound | rascal holds one shape per location; send each part as its own location |
| a polygon of 65 to 256 vertices | refused inbound | rascal's location schema is the copied envelope's (64, ADR 0009) |
| `absent` with a `reason` | taken; the reason is not kept in rascal's own record | the copied schema's `absent` has no reason; `run.json` carries it |
| a `mask` location in a location slot | refused | a mask goes to `--mask`; the handoff milestone adds the rendering both ways |
| cancelling a call | the run stops before its next step | a filler call already in flight runs to its end, bounded by its timeout, and its price stays spent |
| `view` | accepted, never emitted | as the contract says |

The `rect` recipe's action is `mask_rect`, because the `rect` op is an operation of that name.

