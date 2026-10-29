# Diagnostics

Every invocation appends a journal line and every committed invocation an operation record,
whether or not anyone looks. The diagnostic surface reads the journal (after a repair, the
journal rebuilt from the records) and the artifacts it names, and nothing else, so it is
optional and costs nothing when unused.

## The run page

```
rascal dash run/ --out run/dash.html     # a static page, every image embedded; opens offline
rascal dash run/ --serve                 # serve it on loopback, rebuilt on every request
rascal dash run/ --serve --port 8765     # a fixed port; 0 (the default) picks a free one
rascal dash --panels                     # the registered panels, one per line
```

A run directory is a workspace (`docs/workspace.md`), so either kind of path works. The page is
built from `journal.jsonl` and the operation records, from the template in
`crates/rascal-mask/pages/dash.html`, and holds:

- **Timing**: one row per journal line in journal order, with the step, the op, the status, the
  duration and a waterfall bar. A step that ran more than once is marked `repeat n`; a
  `not_applicable` or `measured_nothing` step is amber, a failed (`invalid_args`) one red; a
  journal note is grey.
- **Filler and pack calls**: one row per `filler call:` note in the journal (the entry, its
  command or URL, the request's size, the bytes, the duration, the exit code or HTTP status
  and the outcome); pack calls arrive with M3.
- **Steps**: a section per journal line with every panel's rendering for it.

The page names every journal line and no other, and embeds every artifact the journal names as
a `data:` URI (PNG for images and mask planes, JSON for locations, plans, profiles and series),
so the file stands alone. Each artifact is embedded once, in an asset table keyed by identity
(a mask's planes under `<identity>/core`, `/blend`, `/protect`), however many journal lines name
it; thumbnails and links take theirs from the table by key. After `rascal workspace repair` the
journal holds only committed invocations and the page carries a `journal rebuilt by repair`
badge.

`--serve` binds `127.0.0.1` only and prints the URL on stdout. It is a one-connection-at-a-time
page server for a person watching a run, not a service: no other routes, no TLS, no
authentication; a client that sends nothing for five seconds or reads nothing for thirty is
dropped so it cannot hold the page.

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

Arriving later: filler calls (2.8), pack calls (3.3).

## Tracing

```
RASCAL_LOG=debug rascal run rect photo.jpg --bbox 0.2,0.2,0.6,0.6 --out run/
```

`RASCAL_LOG` takes a level (`error`, `warn`, `info`, `debug`, `trace`) or a `tracing`
filter (`rascal_core=debug`). Off when unset. Spans go to stderr only, never to stdout and never
to the MCP stream, so an operator can watch a session live. One span per op carries the op name,
the step id and the input identities; the closing event carries the duration and whether the op
returned. Filler and pack calls add their own spans with request size and status when they
arrive. Nothing in a span is a key or a pixel; `tests/dash.rs` checks a fixture run's output.
