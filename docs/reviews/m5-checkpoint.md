# M5 checkpoint review

Branch `m5-pack`, 2026-10-29. Tasks 5.1 (the Python pack skeleton, `echo_region`, `fetch`,
the parity harness) and 5.3 (the bundle and offline setup) are in; task 5.2 (`segment`) waits
on owner decision 5 and the branch is reviewed without it. Two reviews ran before the owner's
checkpoint review, as the plan requires: a five-axis code review of commits `c592a07` and
`6c559a3` against SPEC "The Python pack and its bundle", the MCP sections, PLAN tasks 5.1 and
5.3 and `docs/review-checklist.md`, and a test-coverage review of the same commits together
with M6's, both by Claude Code agents, read-only.

Verdicts: the code review requested changes with one must-fix (`setup` writing into the
current directory for a pack command with no directory before `.venv`), nine should-fix items
and seven nits, and found the digest-before-write ordering, the path containment, the uv flag
set, the wheelhouse-only lock, the platform check, the `_meta` construction and the ceilings
sound; the test review found both suites green (Rust 283 tests, pytest 11 passed and 2 skipped)
and named the install path as invisible to `cargo test` without a bundle. The must-fix, the
should-fix items and the test additions landed in commit `befa895` the same day; the rest are
recorded with owners below.

**What the disconnected step proves.** On Linux the install and the call run in a network
namespace with no interfaces, and CI now fails if the runner refuses the namespace. On macOS
there is no such namespace: the proof is uv's `--offline --no-index --no-cache` with
interpreter downloads forbidden, plus the launcher never calling uv. Recorded here as
flag-level on macOS.

**Scope.** A bundle given as a URL (the spec's one network step) is not implemented; `--bundle`
names a directory until the release task 7.2, and `docs/tools.md` says so. `UV_OFFLINE=1` at
launch is not set by the client because the launcher (`<dir>/.venv/bin/rascal-tools`) never
invokes uv; the spec's sentence describes the install, where it is set.

## Findings and dispositions

| # | Finding | Disposition |
|---|---|---|
| 1 | must-fix: `install_dir_of` returned an empty directory for a bare `.venv/bin/rascal-tools` command, so `setup --config` copied the bundle into the current directory before uv failed | fixed: a bare `.venv` names no directory; a relative directory is taken from the configuration file's directory; an empty destination is refused; test: both commands refused with exit 64 and the current directory untouched |
| 2 | any non-empty directory holding a `bundle.toml` was replaced whole with `remove_dir_all`, so a second downloaded bundle or model weights beside an install could be lost; the destination was checked after the prerequisites, so the refusal needed uv to test | fixed: the destination is checked before the prerequisites and before any write; an earlier install must be this pack's and hold nothing beyond its listed files, its environment and its manifest, else refused naming the extra entries; replacement removes only those; tests without uv: the refusals, exactly the listed files copied (a stray file in the bundle is not), the old wheel and environment gone on replacement |
| 3 | `uv sync` and the launcher smoke inherited the whole shell environment (keys, `UV_PROJECT_ENVIRONMENT`, index settings) | fixed: both run with the baseline (`PATH`, `HOME`, `LANG`, `LC_ALL`, `TMPDIR`) plus the offline settings, `UV_PROJECT_ENVIRONMENT` pinned to `<dir>/.venv`, `--no-config` |
| 4 | the Rust offline-install test never ran in CI and passed silently without `RASCAL_BUNDLE`; the real-pack test passed silently without the venv | fixed: both fail under `CI`; `scripts/bundle-acceptance.sh` runs the install test against the bundle it built |
| 5 | on Linux the namespace was silently degraded when refused | fixed: in CI on Linux the step fails without it; the macOS proof is recorded above |
| 6 | `sha256_file` read whole files into memory (model weights join the bundle later) | fixed: hashed in chunks |
| 7 | `BundleManifest` was a boundary type outside the inventory | fixed: in `rascal_core::pack`, schema `bundle-v1.json`, fixture, registered |
| 8 | the server's docstring claimed an unexpected exception becomes a JSON-RPC error with its class; the SDK makes it an `isError` result; missing harness keys raised `KeyError` | fixed: the docstring says what happens; missing keys, malformed base64 and decode failures are harness errors (`invalid_args`) |
| 9 | an image was decoded before its declared size was checked | fixed: the header's size is checked first; `DecompressionBombError` is a harness error |
| 10 | SPEC said the pytest bundle test installs; it checks the build's output; `docs/tools.md` did not say `--bundle` is the only form | fixed: SPEC's sentence names the Rust install test; `docs/tools.md` says the URL form arrives with the release |
| 11 | nits: an interrupted fetch left `.partial`; `urlopen` accepted `file://`; a float plane was truncated silently; `UPDATE_PARITY` kept stale planes; "already points here" compared uncanonicalised paths; `prerequisites` negatives untested | fixed: the partial file is removed on any failure and the URL must be http(s); a plane is bool or uint8; stale planes are removed; canonical comparison; no uv on PATH is refused after the bundle verified and before any write (test) |
| 12 | `install` mixes policy, copy, sync and smoke; `Prerequisites.python` is found but uv re-finds it; `pip download` evaluates markers on the host, so a cross-platform build could miss a marker-gated wheel (the lock would then fail loudly) | recorded: the function is three steps in one place by design; a cross build is checked by the lock step; the macOS job builds its own bundle |
| 13 | test review: `test_fetch.py` binds a loopback HTTP server; `examples/rascal.toml` is read by no test; the `segmenter` skip is proven for an unknown name only | recorded: loopback is not the network; the example is prose; the known-name branch becomes reachable with 5.2 |

## Verification

`just check` green after the fixes (both feature sets, `cargo deny`, the crate graph, packaging,
the clean-room scan, the docs coverage, the hook and check self-tests; the Python gates run
against the pack's environment); the real install test green against a bundle built on the
owner's machine; CI green on the branch, the bundle step on Linux and macOS included.
