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
| frames, images, locations, artifacts, identity | rascal-core | not started | M1 task 1.2 |
| the mask record and effective weight | rascal-core | not started | M1 task 1.3 |
| Op trait, manifest, envelope, registry | rascal-core | not started | M1 task 1.4 |
| renderings | rascal-core, rascal-ops | not started | M1 task 1.5 |
| recipes with typed bindings | rascal-core | not started | M1 task 1.6 |
| workspace, journal, repair | rascal-core | not started | M1 tasks 1.7a, 1.7b |
| primitives, `cut`, `put`, `line`, `rect`, `stroke`, `rasterize` | rascal-ops | not started | M1 tasks 1.8a, 1.8b |
| `inspect`, compatibility export | rascal-core, rascal-mask | not started | M1 task 1.9 |
| the run page and tracing | rascal-mask | not started | M1 task 1.10 |
| the seam ops and `heal` | rascal-ops, rascal-mask | not started | M2 |
| MCP client and server | rascal-mcp | not started | M3 |
| the refine ops and the remove, repair, protect recipes | rascal-ops, rascal-mask | not started | M4 |
| the Python pack | tools/python | not started | M5 |
| the lab | rascal-mask | not started | M6 |
| the Poisson paste | rascal-ops | not started | M7 |
| schema drift harness and boundary-type inventory | rascal-core | done | M0 task 0.4 |
