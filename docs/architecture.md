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
| the MCP transport and the independent client | rascal-mcp, rascal-testkit | done: ADR 0015 (`rmcp` 3.8.0, revision 2025-11-25 only), the shared wire shapes, the independent client proven against a fake server | M3 task 3.1 |
| `rascal serve` | rascal-mcp, rascal-mask | done: one tool per op with ordinary arguments, the five workspace tools, the envelope as structured content with image blocks in correspondence, the error mapping, serial by default with `--concurrency` | M3 task 3.2 |
| the pack client and the isolation suite | rascal-core, rascal-mcp, rascal-mask, rascal-testkit | done: `[packs.*]`, the environment allowlist, startup and manifest validation from the tool's `_meta`, `_harness`, result validation before commit, `many` output ports, the AND rule, timeout and cancellation failing every in-flight call with the shutdown escalation, stderr redaction, `pack call:` / `pack stored:` / `pack manifest:` journal notes, `--replay` without the pack, `rascal tools list|call`, the fake pack, the isolation suite, ADR 0019 | M3 task 3.3 |
| `fan` and the band profile | rascal-ops, rascal-mask | done: the two rulers and the half-width rule, `fan-v1` and `fan-thin-v1` embedded, constant sets as parameter defaults, the step-sky fixture and golden profile, the profile panel; `fan` 1.1.0 adds the object cap (`fan-v2`, 2026-10-29): a row's opening stops short of a solid object's edge and stays thin where the object straddles the line | M2 task 2.1; the object cap after M7 |
| `profile` and `feather` | rascal-ops | done: the band core at `half`, the per-row and the one-sigma feather by distance transform with finite support, the `sigma_px` series, protect wins | M2 task 2.2 |
| `ramp` and `apply` | rascal-ops | done: the step on the profile's windows as a prescription, apply with the `w > 0` rule, `ramp-v1` | M2 task 2.3 |
| `window` and `unwindow` | rascal-ops | done: the window contract (support, context, square, caps, slide), the `crop:` frame reused by identity, the cropped planes, exact unwindow | M2 task 2.4 |
| `paste` (feather and hard) | rascal-ops, rascal-mask | done: the paste contract on pixels, both modes, alpha rules, `rascal paste` on files | M2 task 2.5 |
| `measure` v1 | rascal-ops, rascal-mask | done: the five measures with `measures-v1`, series with row indices and units, summaries, skipped rows, `measured_nothing`, circular hue, `rascal measure` on files, the measures panel, the docs check | M2 task 2.6 |
| the fillers and the fake fillers | rascal-core, rascal-mask, rascal-testkit | done: `[fillers.*]` with its checks, the command filler (placeholders, timeout, exit handling), the HTTP filler (multipart, bearer from `key_env`, timeout), the request-field matrix, the dimension assertion, the journal record without the key, six fake behaviours as a command and a local server | M2 task 2.7 |
| `heal` and the shipped `seam` recipe | rascal-mask | done: the typed graph executed literally over the recipe's outputs, the preservation check in raw coordinates, the measures in the mask's frame, the stored fill and `--replay`, the run page by default, the step-pano fixture and the golden seam mask | M2 task 2.8 |
| `grow`, `shrink`, `close` | rascal-ops, rascal-testkit | done: disc morphology with the shadow extension, the refinement postconditions and report, the chair-blob fixture | M4 task 4.1 |
| `subtract`, `select`, `invert`; many input ports; list parameters | rascal-core, rascal-ops, rascal-mask, rascal-mcp | done: protect from a mask or a location, the kept instances' union minus exclude boxes, the inversion through the frame chain; a many input bound by prefix in a recipe, repeated on the CLI, an array over MCP; integer-list and box-list parameters | M4 task 4.2 |
| `tighten` | rascal-ops, rascal-testkit | done: the signed-derivative boundary pairs, the tie-breaks, the direction rules, smoothing by runs with the crossing guard, the tube with round joins and caps, `tighten-v1`, the two scratch fixtures | M4 task 4.3 |
| the `remove`, `repair`, `repair-area`, `protect` recipes and the acceptance script | rascal-core, rascal-mask, rascal-testkit | done: optional recipe inputs, the four recipes with goldens, `--mask` in a derived frame the workspace knows, the two-pass test, `scripts/acceptance.sh` installing from the packaged crates and wired into the package workflow, the `independent-walk` binary | M4 task 4.4 |
| the segmenter decision | tools/python | done: branch A, OWLv2 base + a SAM decoder exported to ONNX (ADR 0018, decision 5 signed 2026-10-29; the export took the ADR's drop-in, SAM 2.1 Hiera-Tiny) | M4 task 4.5 |
| the Python pack: skeleton, `echo_region`, `fetch`, the harness | tools/python, rascal-mask | done: a `uv` project on the SDK's 1.x line, each tool listed from one `ToolSpec` as both `_meta` documents with bounds checked at listing, `_harness` parsed, envelopes with image blocks, `echo_region` and the debug `echo_env`, `fetch` with digest verification, the parity harness with `segmenter` skips, ruff and pytest gates in `just check` and CI, the real pack driven through rascal's client | M5 task 5.1 |
| `segment` | tools/python, rascal-core, rascal-mask | done: OWLv2 base and SAM 2.1 Hiera-Tiny exported to ONNX (`tools/python/export/`, `MODEL.md`, `NOTICE`), `rascal_tools.models` on onnxruntime with numpy preprocessing mirroring the processors, the `segment` tool with parity on the chair fixture, `artifacts.toml` with the digests, recipe `Text` inputs bound by `--prompt`, the `remove-prompt` recipe, the models in the bundle and the bundle acceptance calling `segment` | M5 task 5.2 |
| the bundle and offline setup | tools/python, rascal-mcp, rascal-mask | done: `scripts/build-bundle.sh` (the provisioning project with the wheel, the platform-only lock against the wheelhouse, `bundle.toml` with every digest), `rascal tools setup` (`rascal_mcp::provision`: platform check, digest verification before any write, the listed files copied, `uv sync` offline with no index and no cache, the launcher smoked), the disconnected acceptance step on Linux and macOS, the pytest bundle test; the model artifacts' bytes join the bundle with 5.2 | M5 task 5.3 |
| the lab: corpus, protocol and matrix | rascal-core, rascal-mask, rascal-ops | done: the three boundary types with generated schemas and fixtures (`rascal_core::lab`), `rascal corpus verify`, matrix validation against the protocol, the corpus and the recipes with overrides applied and the dependent-finder rejection naming the binding, `fan-thin-v1` thin at three widths, `examples/lab/`; the shipped recipes moved to `rascal_mask::recipes` | M6 task 6.1 |
| the lab: `lab run` | rascal-core, rascal-mask | done: the lab, cell, attempt and ledger records with generated schemas, the cell identity over every component with finder invocation descriptors, the lab directory with attempts and `latest`, resume by identity with no external call, `--redo`, the protocol digest recorded, every cell healed through `heal` with the arm's filler, the ledger with spend | M6 task 6.2 |
| the lab: `lab pairs` and `lab decode` | rascal-core, rascal-mask | done: the assignment and picks records with generated schemas, the seed-drawn order and sides (splitmix64), the static page with results embedded and no arm named, each pair at its focus (the region both results changed, `rascal_core::lab::focus`), whole and, for an equirectangular frame, from inside the sphere, the decode with its rejections and the outcome rules, the protocol kept beside the lab | M6 task 6.3 |
| the lab: the owner's round | owner | done 2026-10-29: three rounds on the generated corpus (`labs/seam-0{1,2,3}/`); `seam-03` under Bria met criterion 6 (sky `preferred` 18 of 19, structured `preferred` 14 of 16) after `seam-01` failed it under LaMa and `seam-02` showed the filler decided it (`docs/results/`) | M6 task 6.4 |
| the scaled window | rascal-core, rascal-mask | done 2026-10-29: a filler entry's `fill_max_side` sends the request downscaled and scales the fill back (decision 10); `lama-scale-01` judged it, sky 20 of 21 for the window, structured 12 to 14 (`docs/results/lama-scale-01.md`) | after M6 |
| the Poisson paste | rascal-ops, rascal-mask | done: the sparse 4-neighbour system over the unprotected `w > 0` pixels, the fill's gradients with the base's values at the edge, conjugate gradients in `f32` from the fill with the true residual confirming convergence, `poisson-v1`, the whole-image domain not applicable, a wrap frame refused, the clamped solution feathered; `paste --mode poisson` and `paste = "poisson"` through `heal`; the 1000×1000 benchmark in `docs/results/poisson-bench.md` | M7 task 7.1 |

## The success criteria (SPEC.md), their outcome and the command that proves each

Recorded for the v0.1.0 release (task 7.2), 2026-10-29.

| # | criterion | outcome | proved by |
|---|---|---|---|
| 1 | `cargo install rascal-mask` in a clean environment gives a `rascal` that runs the shipped recipes by name on a JPEG with no configuration file | met: the six location recipes from the packaged crates in a clean install; the seventh, `remove-prompt`, through the pack in the bundle step | `scripts/acceptance.sh` (the package workflow, every push) |
| 2 | `rascal heal seam` with a local filler heals a 360 whose `w = 0` and protected pixels equal the base, with the fan, the ramp, the filler call and the measures journaled; the mis-sized filler refused | met | `cargo test -p rascal-mask --features testkit --test heal` (`heal_through_every_fake_with_and_without_max_side`, `the_key_appears_in_no_file_under_out_and_in_no_span`) |
| 3 | `cargo test --workspace` passes offline in under three minutes of test execution on the owner's Apple silicon machine | met: 30.8 s for the default feature set (250 tests) and 129.9 s with the testkit (314 tests), 2 min 41 s in all, Apple M4 Max, 2026-10-29 | `docs/results/suite-timing.md` |
| 4 | the independent minimal client drives `rascal serve` through import, find, refine, render and paste, and the CLI reads the workspace afterwards | met | `cargo test -p rascal-mask --features testkit --test serve`; `scripts/acceptance.sh` step 3 |
| 5 | branch A: the pack installs from a local bundle with the network disabled, launches offline, and `segment` returns instances for a text prompt on a fixture with recorded parity, on macOS arm64 and Linux x86_64 | met on macOS arm64 (the bundle with the models, installed disconnected, `segment` finds the chair, parity `fixtures/parity/segment/chair`); on Linux x86_64 the bundle installs and `echo_region` answers in CI, and `segment` runs there once the model files are published as the `models-2026-10` release (the bundles workflow fetches them) | `scripts/bundle-acceptance.sh`; `tools/python/tests/test_segment.py::test_the_chair_is_found_with_parity` |
| 6 | on the owner's corpus, under a committed protocol, `lab decode` reports `preferred` on sky and `not_worse` on structured for `fan-ramp` against `thin`, from blind picks | met 2026-10-29 by `seam-03` under Bria: sky `preferred` 18 of 19, structured `preferred` 14 of 16 (which satisfies `not_worse`, decision 9) | `rascal lab decode lab/seam-03 <picks>`; `docs/results/seam-03.md` |
| 7 | `docs/` names every op, profile, recipe field, config key, measure and run-page panel in the registry and nothing else | met | `scripts/check-docs.sh` (`just check`, every commit; `scripts/acceptance.sh` step 4) |
