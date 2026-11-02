# Diagnostics

Every invocation appends a journal line and every committed invocation an operation record,
whether or not anyone looks. The diagnostic surface reads the journal (after a repair, the
journal rebuilt from the records) and the artifacts it names, and nothing else, so it is
optional and costs nothing when unused.

## The run page

```
rascal dash run/ --out run/dash.html     # a static page, every image embedded; opens offline
rascal dash run/ --document              # the same page as one file in the cache; prints its path
rascal dash run/ --serve                 # serve it on loopback, rebuilt on every request
rascal dash run/ --serve --port 8765     # a fixed port; 0 (the default) picks a free one
rascal dash run/ --serve --frame-ancestor http://127.0.0.1:4801   # framable by that origin only
rascal dash --panels                     # the registered panels, one per line
```

A run directory is a workspace (`docs/workspace.md`), so either kind of path works. The page is
built from `journal.jsonl` and the operation records, from the template in
`crates/rascal-mask/pages/dash.html`, and holds:

- **Timing**: one row per journal line in journal order, with the step, the op, the status, the
  duration and a waterfall bar. A step that ran more than once is marked `repeat n`; a
  `not_applicable` or `measured_nothing` step is amber, a failed (`invalid_args`) one red; a
  journal note is grey.
- **Filler and pack calls**: one row per `filler call:` or `pack call:` note in the journal
  (the kind, the entry or the pack and tool, its command or URL, the request's size, the
  bytes, the duration, the exit code, HTTP status or call status, and the outcome).
- **Steps**: a section per journal line with every panel's rendering for it.

The page names every journal line and no other, and embeds every artifact the journal names as
a `data:` URI (PNG for images and mask planes, JSON for locations, plans, profiles and series),
shown in place by the thumbnails; a link that opens an artifact in a new tab turns the same
bytes into a `blob:` URL on its first click, since browsers refuse to navigate a tab to a
`data:` URI,
so the file stands alone. Each artifact is embedded once, in an asset table keyed by identity
(a mask's planes under `<identity>/core`, `/blend`, `/protect`), however many journal lines name
it; thumbnails and links take theirs from the table by key. After `rascal workspace repair` the
journal holds only committed invocations and the page carries a `journal rebuilt by repair`
badge.

Building the page only reads: the run directory or workspace is opened read-only (no layout
created, no staging swept), so `rascal dash` never changes a byte of what it shows, and a path
that does not exist is an error rather than a new workspace.

`--serve` binds `127.0.0.1` only and prints the URL, `http://127.0.0.1:<port>/`, as the one
line of stdout once it is listening, and nothing else ever (tracing goes to stderr). It is a
one-connection-at-a-time page server for a person watching a run, not a service: no other
routes, no TLS, no authentication; a client that sends nothing for five seconds or reads nothing
for thirty is dropped so it cannot hold the page. It serves until its stdin reaches its end or
it receives SIGTERM, then exits 0. At a terminal stdin stays open, so it runs until Ctrl-D (or
Ctrl-C); `< /dev/null` makes it print its URL and exit at once. Every response sends
`Content-Security-Policy: frame-ancestors <origin>` for the origin given with
`--frame-ancestor` (`http://` or `https://`, a host and an optional port, no path; anything else
exits 64 before listening), and `frame-ancestors 'none'` without it, so no page but the one host
may frame it.

`--document` writes the page as one self-contained file, every image a `data:` URI and the
styles and script inline, to `$XDG_CACHE_HOME/rascal/dash/<run id>.html` (`~/.cache/` when
`XDG_CACHE_HOME` is unset or relative; the run id from `run.json`, else a digest of the path for
a workspace), written under a temporary name and renamed. It prints the file's absolute path as
its only stdout line and exits 0 once the file exists; a cache directory inside the run is
refused (64), since a render never writes into the run.

### The renders a host runs

The description gives every action two renders (table-manners 3.2), each running `rascal`
itself (`tool.name`) with `{run.dir}` and `{frame_ancestor}` as whole arguments:

| Render | Kind | Command |
|---|---|---|
| `dash` | `server`, `ready_timeout_s: 10` | `rascal dash {run.dir} --serve --port 0 --frame-ancestor {frame_ancestor}` |
| `page` | `document` | `rascal dash {run.dir} --document` |

The host owns the server's process: it keeps stdin open while the page is shown and closes it
(or sends SIGTERM) to stop it. Neither render modifies the run directory.

## Panels

A panel renders one view of one journal line. An op registers its panel in the task that adds
the op, so the view is never behind the tool. The registry is `rascal_core::panel::PanelRegistry`;
the binary assembles it in `crates/rascal-mask/src/dash.rs`. `scripts/check-docs.sh` fails when a
registered panel is missing from this table or a listed panel is not registered.

| Panel | Shows | Since |
|---|---|---|
| `envelope` | the native envelope (status, summary, measurements, requirements) and the resolved parameters with the constant set version | 1.10 |
| `artifacts` | every output of the line: images and the three mask planes as thumbnails that open at full size, other artifacts as JSON links | 1.10 |
| `profile` | a band profile's half-width per row as a chart, beside the strip of the input image the band can reach | 2.1 |
| `measures` | every series a `measure` line produced as a chart, with the envelope's summaries in a table | 2.6 |


## Tracing

```
RASCAL_LOG=debug rascal run rect photo.jpg --bbox 0.2,0.2,0.6,0.6 --out run/
```

`RASCAL_LOG` takes a level (`error`, `warn`, `info`, `debug`, `trace`) or a `tracing`
filter (`rascal_core=debug`). Off when unset. Spans go to stderr only, never to stdout and never
to the MCP stream, so an operator can watch a session live. One span per op carries the op name,
the step id and the input identities; the closing event carries the duration and whether the op
returned. A filler call adds a `filler` span (the entry, request bytes and size) and a pack call
a `pack` span (the pack, the tool, the call id, request bytes) whose closing event carries the
status, duration and response bytes. Nothing in a span is a key or a pixel; `tests/it/dash.rs`,
`tests/it/heal.rs` and `tests/it/packs.rs` check runs' output.
