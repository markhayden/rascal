# table-manners: rascal's side of the shared contract

rascal conforms to [table-manners](https://github.com/markhayden/table-manners) v0.2.0 (milestones
M8 and M9 for v0.1.0, M11 for v0.2.0) and is moving to v0.3.0 in milestone M12, built against the
v0.3 draft until it is published; the shared contract of the owner's tools (ADR 0020, decisions
13 and 14): what an agent, a script or the playground sees from outside is the same for squint,
rascal and the tools after them. The contract's text is the specification; this page is what
rascal does under it (PLAN.md M8, M9 and M11).

## The pin

`table-manners = "=0.2.0"` in the workspace `Cargo.toml` of `main`: the crate's serde types and
embedded schemas, no logic. On the M12 branch the pin is
`{ path = "../table-manners", version = "=0.3.0" }`, a sibling checkout of the contract's `v0.3`
branch, until v0.3.0 is published (PLAN.md 12.10). `scripts/check-contract.sh` warns in CI when
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
rascal describe                          # one line per action
rascal describe --json [--config rascal.toml]
```

The contract's description document: one action per shipped recipe (`seam`, `remove`,
`remove_prompt`, `repair`, `repair_area`, `protect`, and `mask_rect` for the `rect` recipe, so
it does not take the `rect` op's name in `serve`), with the image slots (role
`source`), the location slots and the kinds each takes, the text and mask inputs and the common
arguments (`filler`, `set`, `projection`, `strict`) as `params_schema`, the outputs (`mask`,
`result`; `remove_prompt` fills `locations.found`), the cost, the readiness, the duration and the
two renders (3.2): `dash` (`kind: "server"`, `ready_timeout_s: 10`, `rascal dash {run.dir}
--serve --port 0 --frame-ancestor {frame_ancestor}`) and `page` (`kind: "document"`, `rascal
dash {run.dir} --document`), described in `docs/diagnostics.md`. Every action's `params_schema` declares
`max_spend_usd` as `{"type": "number", "exclusiveMinimum": 0}`, since every action takes a filler
and so may be paid. With `--config`, the configured fillers are the `filler` enum and a
configured filler with a price makes an action `may_spend`. `estimate` is a reserved tool, never
an action (see "Estimates" below). `fixtures/contract/describe.json` pins the document without a configuration. The
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
identities. `spend` and `cache` come from the fill (8.6).

## `serve` (8.5)

```
rascal serve [--workspace ws/] [--config rascal.toml] [--concurrency n] [--runs dir]
```

`serve` starts in every configuration state (3.1, 5): with no `--config`, a file that does not
exist or one that does not load (said once on stderr), its actions report what is missing and a
call that needs it is refused `not_ready`. `--workspace` (the op tools' workspace) defaults to
the configuration's `[workspace] dir`, relative to the file, else
`$XDG_DATA_HOME/rascal/workspace` beside the runs root, so a host needs nothing but `serve` and,
if it likes, `--config`. A pack that does not start no longer stops the server: its tools are
absent, stderr says why, and its requirement reads `invalid`; a run started with such a pack
notes it in its journal, and a recipe whose op nothing serves then fails with the pack's own
error.

Beside the op tools and the workspace tools (ADR 0015, unchanged), `serve` lists one MCP tool per
action in the description, `describe` and `estimate`; an action never shadows an op's name. A call runs the
recipe, through `heal` when `filler` is given, in its own run directory under `--runs`
(`$XDG_DATA_HOME/rascal/runs`, else `~/.local/share/rascal/runs`), and its result is that run's
`run.json` as `structuredContent`, with `isError` true for `invalid` and `error`. JSON-RPC errors
are left for failures before a run exists (an unknown tool, malformed arguments, a call that is
not ready).

- **Images in.** `image` is `{ "path": "/abs/…" }` or `{ "inline": { "media_type", "data" } }`,
  each with an optional `expect` reference checked against the file (digest and dimensions; a
  mismatch is `invalid` with code `input`). A path must be absolute; inline bytes are written to
  the run's input directory (`<runs>/<id>.in/`) and imported like a file, journaled; the limit is
  the description's `limits.inline_bytes` (64 MiB).
- **Images out.** `return_images` chooses the locator: `path` (the default), `inline`, or
  `resource` (`rascal://run/<id>/images/<role>`). `image_blocks: true` also sends each as an MCP
  image block.
- **Locations in.** `location` and `stroke` take a table-manners location object (8.2).
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
before its request left rascal: no key, the connection refused, the command not started, or 401
and 403), *known* (the filler answered: a 2xx, or exit 0) or *unresolved* (sent and not settled:
a timeout, a failure status, a non-zero exit, or the run killed mid-call). Known and unresolved
calls count at the price; `heal` journals `filler call started` before each call, so a run stopped
mid-call counts it too. `spend.usd` is the total and `spend.unresolved_usd` the part no answer
settled, with a note (table-manners v0.2, 4.5). The contract files an authentication refusal as
*known* at zero; rascal records it as `not_sent`, which also counts zero, so the spend is the
same.
`[limits]` holds `serve` to `call_cap_usd` per call and `session_budget_usd` across the process's
calls (4.5). A call's `max_spend_usd` lowers the cap: the effective cap is the configured one
lowered by it, or the caller's value when no cap is configured. It is an argument like any other,
so 0 or less (a cap of zero is not a cap) and a value above the configured cap are an `invalid` run
with code `usage` naming the amounts, never silently lowered (task 12.4). The limits check runs
after allocation and before any paid step, in this order:

1. the arguments, `max_spend_usd` among them (`invalid`, `usage`);
2. from task 12.6, the estimate the call names (`stale_estimate`);
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
six decimals. rascal reads its configuration once, so a session lasts the process, and every run
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
  kind: "http" | "command", revision, paste, fill_max_side}}` or `{"filler": null}`.
- `readiness` (the call's own), `session` (one observation of the ledger, the document
  `rascal://session` returns, taken once with the budget check) and `session_id`, and `expires`,
  one hour after the estimate (prices are configuration, fixed for the session).
- `binding`: the record identity (ADR 0016, kind `estimate`) of `{action, arguments,
  tool_version, config_identity, session_id}`, where `arguments` leaves out `return_images` and
  `image_blocks` (they only choose delivery), each image slot's value is `{"file_sha256": …}`
  (a path read, inline bytes decoded; an `expect` verified, then dropped), each stored-mask input
  (`mask`, `protect`) is `{"mask": <its identity>}`, and every other argument, `max_spend_usd`
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
  contract repository's `fixtures/rascal/` takes (task 11.6): from `rascal serve` through the
  independent client, the description, a run per status (an error with an unresolved charge
  among them), two runs pages, the session and both reuse refusals, with machine values replaced
  by stand-ins that stay valid; `tests/it/table_manners_fixtures.rs` regenerates and compares them.
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

The `rect` recipe's action is `mask_rect`, because `serve` already has a `rect` op tool.

