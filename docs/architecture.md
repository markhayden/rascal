# Architecture

The crate layout from SPEC.md "Architecture", kept current with the code. The status table says
which parts exist; it is updated in the same commit as the code it describes.

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

## Principles

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

## Status

| Part | Crate | State | Arrives |
|---|---|---|---|
| schema drift harness and boundary-type inventory | rascal-core | done | M0 task 0.4 |
| frames, images, locations, artifacts, identity | rascal-core | done: the three frame kinds and the cut plan, canonical serialization and committed vectors (ADR 0016), the seven location kinds with their rules, mapping toward the root with parts and partial | M1 task 1.2 |
| the mask record and effective weight | rascal-core | done: the full record with profile, prescriptions, source and provenance; invariants named on load; refinement helpers; BandProfile and Series; `inspect --json` | M1 task 1.3 |
| Op trait, manifest, envelope, registry | rascal-core | done: the full manifest with derived schemas and bounds, parameter resolution, the native envelope, registry checks at registration, shared input validation, `ops list` and `ops describe` | M1 task 1.4 |
| renderings | rascal-core, rascal-ops | done: the five profiles with roles, polarity and parameters; `render` with `--image`, `--threshold`, `--colour`, `--strength` | M1 task 1.5 |
| recipes with typed bindings | rascal-core | done: every validation error with its step id, constant sets, defaults resolved into the journal, the recipe and config schemas, the binding flags | M1 task 1.6 |
| workspace, journal, repair | rascal-core | done: generations, staging locks, the commit protocol with the spec's sync order (ADR 0017), operation records, one journal line per invocation, cancellation before the lock, `workspace list`, `import` and `repair` (orphans, missing payloads, the journal rebuilt from the records), per-op subcommands | M1 tasks 1.7a, 1.7b |
| primitives, `cut`, `put`, `line`, `rect`, `stroke`, `rasterize` | rascal-ops | done: the six primitives, `cut` and `put` with the Plan artifact, `rect`, `line`, `stroke`, and `rasterize` for every kind | M1 tasks 1.8a, 1.8b |
| `inspect`, compatibility export | rascal-core, rascal-mask | done: the inspect op and pages, `--replay`, the export projection with its two refusals, self-contained runs, the docs-coverage check | M1 task 1.9 |
| the run page and tracing | rascal-core, rascal-mask | done: `rascal dash` (`--out`, `--serve` on loopback, `--panels`), the panel registry with the envelope and artifacts panels, `RASCAL_LOG` spans per op | M1 task 1.10 |
| MCP client and server | rascal-mcp | not started | M3 |
| `fan` and the band profile | rascal-ops, rascal-mask | done: the two rulers and the half-width rule, `fan-v1` and `fan-thin-v1` embedded, constant sets as parameter defaults, the step-sky fixture and golden profile, the profile panel | M2 task 2.1 |
| `profile` and `feather` | rascal-ops | done: the band core at `half`, the per-row and the one-sigma feather by distance transform with finite support, the `sigma_px` series, protect wins | M2 task 2.2 |
| `ramp` and `apply` | rascal-ops | done: the step on the profile's windows as a prescription, apply with the `w > 0` rule, `ramp-v1` | M2 task 2.3 |
| `window` and `unwindow` | rascal-ops | done: the window contract (support, context, square, caps, slide), the `crop:` frame reused by identity, the cropped planes, exact unwindow | M2 task 2.4 |
| `paste` (feather and hard) | rascal-ops, rascal-mask | done: the paste contract on pixels, both modes, alpha rules, poisson registered as not applicable until M7, `rascal paste` on files | M2 task 2.5 |
| `measure` v1 | rascal-ops, rascal-mask | done: the five measures with `measures-v1`, series with row indices and units, summaries, skipped rows, `measured_nothing`, circular hue, `rascal measure` on files, the measures panel, the docs check | M2 task 2.6 |
| the fillers and the fake fillers | rascal-core, rascal-mask, rascal-testkit | done: `[fillers.*]` with its checks, the command filler (placeholders, timeout, exit handling), the HTTP filler (multipart, bearer from `key_env`, timeout), the request-field matrix, the dimension assertion, the journal record without the key, six fake behaviours as a command and a local server | M2 task 2.7 |
| `heal` and the shipped `seam` recipe | rascal-mask | done: the typed graph executed literally over the recipe's outputs, the preservation check in raw coordinates, the measures in the mask's frame, the stored fill and `--replay`, the run page by default, the step-pano fixture and the golden seam mask | M2 task 2.8 |
| the refine ops and the remove, repair, protect recipes | rascal-ops, rascal-mask | not started | M4 |
| the Python pack | tools/python | not started | M5 |
| the lab | rascal-mask | not started | M6 |
| the Poisson paste | rascal-ops | not started | M7 |
