# table-manners: rascal's side of the shared contract

rascal conforms to [table-manners](https://github.com/markhayden/table-manners) v0.1.0, the shared
contract of the owner's tools (ADR 0020, decision 13): what an agent, a script or the playground
sees from outside is the same for squint, rascal and the tools after them. The contract's text is
the specification; this page is what rascal does under it, task by task as milestone M8 lands
(PLAN.md).

## The pin

`table-manners = "=0.1.0"` in the workspace `Cargo.toml`: the crate's serde types and embedded
schemas, no logic. `scripts/check-contract.sh` warns in CI when crates.io has a newer release;
moving the pin is a deliberate commit that also refreshes `fixtures/contract/`.

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
`result`; `remove_prompt` fills `locations.found`), the cost, the duration and the `dash` render.
With `--config`, the configured fillers are the `filler` enum, and any configured filler makes an
action `may_spend` with its key's variable named (never its value).
`fixtures/contract/describe.json` pins the document without a configuration. The `describe` MCP
tool returns the same document (8.5).

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
rascal serve --workspace ws/ [--config rascal.toml] [--concurrency n] [--runs dir]
```

Beside the op tools and the workspace tools (ADR 0015, unchanged), `serve` lists one MCP tool per
action in the description and `describe`; an action never shadows an op's name. A call runs the
recipe, through `heal` when `filler` is given, in its own run directory under `--runs`
(`$XDG_DATA_HOME/rascal/runs`, else `~/.local/share/rascal/runs`), and its result is that run's
`run.json` as `structuredContent`, with `isError` true for `invalid` and `error`. JSON-RPC errors
are left for failures before a run exists (an unknown tool, malformed arguments).

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
  `progress` is a counter that only increases, the words are in `message`, and the first message
  names the run's resource.
- **Cancellation.** `notifications/cancelled` sets the run's flag: the run stops before its next
  step (each op checks the flag before it commits, and `heal` before it calls the filler), writes
  `run.json` as `cancelled` with `stopped_before`, and the call gets no result. A filler call
  already in flight runs to its end (its timeout bounds it).
- **Resources.** `rascal://runs` (the contract's runs list, newest first, a run still going is
  `running`), `rascal://run/{id}/run.json`, `rascal://run/{id}/journal`,
  `rascal://run/{id}/images/{role}`. An id is exactly `run-<ms>-<pid>`; nothing else under the
  runs root, and nothing outside it, can be read.

## The fill cache, prices and limits (8.6)

A filler entry's `spend_usd_per_call` is the run's price; `[cache] dir` keeps every fill by its
request, filler and revision, so an identical request (a retry, a second arm with the same mask)
is answered without calling the filler: `run.json` says `cache: hit` and spend 0, a call says
`miss` with the price (table-manners 4.4). The lab always caches, in `fills/` under its directory.
`[limits]` holds `serve` to `call_cap_usd` (a call's `max_spend_usd` may lower it, never raise
it) and `session_budget_usd` across the process's calls; a call whose price would pass either is
refused before it runs, an `error` result with code `budget` (4.5). The `estimate` action returns
the worst case for an action and filler, the cap, the budget, what the session has spent and
whether the call would be allowed. An action `may_spend` in the description when a configured
filler has a price.

```toml
[cache]
dir = ".rascal/fills"
[limits]
call_cap_usd = 0.50
session_budget_usd = 5.00
[fillers.hosted]
spend_usd_per_call = 0.04
```

## `rascal playground` (8.7)

```
rascal playground [args…]
```

The playground is one program shared by the waffle tools, in its own repository; rascal carries
no UI and no server for it. The command finds the `playground` binary (`PLAYGROUND` when set,
else `PATH`, else `$XDG_DATA_HOME/waffle/bin`, else `~/.local/share/waffle/bin`) and runs it
with `--tool rascal` and the arguments given, returning its exit code. When none is installed it
says where to put it and exits 69.

## Conformance (8.8)

- **Fixtures.** `fixtures/contract/results/` holds one masked `run.json` per outcome from the
  command line; `fixtures/contract/describe.json` the description; both are validated against
  the contract's schemas and compared in the test suite. They are what the contract repository's
  `fixtures/rascal/` takes.
- **The cross-tool acceptance test, rascal's half.** A squint-shaped finding (a table-manners
  location with squint's `source`, `disposition` and `confidence`) masks exactly what rascal's own
  location does, and a finding on another image is refused
  (`crates/rascal-mask/tests/contract_locations.rs`), in CI against the pinned contract. The other
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
| cancelling a call | the run stops before its next step | a filler call already in flight runs to its end, bounded by its timeout |
| `view` | accepted, never emitted | as the contract says |

The `rect` recipe's action is `mask_rect`, because `serve` already has a `rect` op tool.

