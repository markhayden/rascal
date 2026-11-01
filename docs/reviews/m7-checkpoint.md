# M7 checkpoint review

Branch `m7-checkpoint` from `41e09c2`, 2026-11-01. This is a post-release checkpoint: `v0.1.0`
was tagged from `41e09c2` the same day on the owner's word (decision 11), so the M7 tag waits
on this record. Task 7.1 (the Poisson solver and `paste --mode poisson`, commits `3a9408d`
and `26779ee`, with its own review fixes), 7.1b (the scaled window, `fill_max_side`,
`eb14a84`) and 7.2 (the timing run and the release machinery: `setup` from a bundle archive
or URL by digest, the `bundles` workflow, the `dist` release configuration, the README install
section, `f71d5a7` and `9735f97`) are in. Two passes ran against SPEC "Operations" (the paste
contract), the filler section, "The Python pack and its bundle", PLAN Milestone M7 and
`docs/review-checklist.md`: a five-axis code review (correctness, readability, architecture,
security, performance) of those commits, and a test-coverage pass over the same paths. Real
bugs were fixed on this branch, test first, one commit each; the rest are recorded below.

Verdicts: the code review found the Poisson solve sound (the domain, the right-hand side, the
Dirichlet holes at protected pixels, the whole-image test as the one unanchored case on a
4-connected grid, the true residual before every test and on the recurrence's pass, the clamp
and the single rounding after the feather, the wrap refusal), the scaled window sound (the
request identity over the bytes sent, the replay at either size, the cell identity), the
release configuration matching the published asset names (`dist plan` lists
`rascal-mask-<target>.tar.xz` for the three targets with `sha256.sum`; the bundles are
`rascal-tools-bundle-<platform>.tar.gz` with a `.sha256`; the models are `models-2026-10`), and
the digest-before-unpack order right for both a URL and a local archive. It found the archive
handling under-hardened (links followed, escaping entries skipped in silence, no bound while
streaming or unpacking, a reusable scratch directory), two silent paths in the scaled window
and a release bundle that could ship without the models. The test pass found the URL form's
digest refusal, hostile archives, the scaled window through the cache and `fill_max_side` in
the cell identity untested.

**What a digest proves here.** For a URL the digest is required and checked on the downloaded
bytes before anything is unpacked; for a local archive it is checked when given. Installing a
bundle runs its code (the launcher's smoke, then every call), so the digest, or the person's
trust in the file, is the trust anchor; the per-file digests in `bundle.toml` prove only that
the bundle is whole. The hardening below keeps an archive inside its scratch directory and
bounded; it does not make an untrusted archive safe to install, and nothing claims it does.

## The seven criteria at 41e09c2

All seven met; `docs/architecture.md` carries each with its command, checked in this pass.
Run here: criteria 2 and 4 (`cargo nextest run -p rascal-mask --features testkit --test it --
heal:: serve::`, green), 3 (`just test` inside `just check`: 375 and 71 tests at `41e09c2`,
99.5 s and 10.9 s, Apple M4 Max; `docs/results/suite-timing.md` now holds these figures) and 7
(`scripts/check-docs.sh`, green). Cited: 1 (`scripts/acceptance.sh` in the package workflow;
the six location recipes and `remove-prompt` are unchanged since), 6 (`seam-03`, a recorded
round) and 5. Criterion 5's Linux half was stale ("once the model files are published"): the
models release came before the tag (decision 11), and the bundles workflow on `v0.1.0` built
the Linux bundle with them and ran the acceptance from the packed archive before the upload.
That acceptance calls `segment` only when the bundle holds the models and passed silently
without them, so it is fixed (finding 6) so that a tag requires them. The v0.1.0 `bundles`
run (41348361987) logs `bundle: the installed pack segments the chair from the bundled models`
for both `linux-x86_64` and `macos-arm64`, checked 2026-11-01: criterion 5 is met on both.

## Findings and dispositions

| # | Finding | Severity | Disposition |
|---|---|---|---|
| 1 | security: `setup` unpacked an archive with tar's `unpack`, which creates symlink and hard-link entries (then followed by `verify` and the copy, out of the scratch directory), skips an entry with `..` in silence, and has no bound on the unpacked size | should-fix | fixed `ffca7a9`: each entry checked before it is written: files and directories only, a plain relative path, 16 GiB unpacked in all, each refusal naming the entry; unit tests build hostile archives with raw names (symlink, hard link, fifo, `..`, absolute, over the ceiling) |
| 2 | security, performance: the URL's 4 GiB ceiling was checked against `Content-Length` and then against the whole body after reqwest had buffered it, so a server sending no length could fill memory first; the doc comment claimed redirects were restricted | should-fix | fixed `83a99eb`: the body read chunk by chunk and refused past the ceiling, with or without a length (test on the loopback); the comment says redirects are followed and the digest is what is trusted |
| 3 | security: the scratch directory was named from the pid and the millisecond and made with `create_dir_all`, which accepts an existing directory: two scratches in one millisecond shared one, and on a shared `/tmp` another user could create it first and change the unpacked files between the digest check and the install | should-fix | fixed `6194730`: `create_dir`, mode 0700, the next numbered name when one exists; test |
| 4 | correctness: `fill_max_side = 0` passed the configuration check and the request builder treated it as unset, so a scaled window asked for was silently not applied (`max_side = 0` was refused; untested) | should-fix | fixed `93cf8ff`: refused when the configuration is read; both zeros in the table test; `docs/fillers.md` |
| 5 | correctness: under a scaled window `heal` resizes whatever fill it holds to the support, so a cached fill of the wrong size (a damaged or replaced cache file) was resampled into the result in silence; without scaling it reached paste | nit | fixed `2fe6966`: a cached fill must be the request's size, as a replayed one already had to be, exit 3 naming the key; `docs/contract.md` |
| 6 | release: the bundles workflow fetched the models only when their release existed and the acceptance calls `segment` only when they are in the bundle, so a tag built with the models unreachable would upload an `echo_region`-only bundle with every step green | should-fix | fixed `f9c2a3c`: `BUNDLE_MODELS=required` on a version tag; a manual run keeps the optional form; `docs/tools.md` |
| 7 | docs drift: README's status said pre-release and "three ways" for two; the Poisson benchmark named `tests/poisson.rs` and `--test poisson` from before one binary per crate; `docs/tools.md` said an earlier install is "replaced whole" (M5's review narrowed it); criterion 3's nextest figures were in no results file; criterion 5's Linux half was stale; PLAN's 7.1 acceptance box was never ticked | should-fix | fixed `50567f6` |
| 8 | security: a local archive without `--sha256`, like a directory, installs on its `bundle.toml` alone | recorded | by design: a local file is the person's to trust, and installing runs the bundle's code either way; README and `docs/tools.md` give the digest beside every release asset |
| 9 | performance: the download and a local archive are held whole in memory (a full bundle is about a gigabyte) before hashing and unpacking | recorded | bounded by the ceiling; streaming to the scratch directory while hashing is the step if bundles grow |
| 10 | readability: `poisson::Settings::v1` falls back to `1e-4` and `2000` if the embedded `poisson-v1.toml` fails to parse, so a malformed edit to the set would be ignored rather than refused | nit | recorded: the fallbacks equal the set's values and the file is embedded at build time; a change to the set is a new version (SPEC "Constant sets"), which is the time to make the parse an error |
| 11 | performance: conjugate gradients without a preconditioner take iterations in proportion to the domain's side (1421 for the 1000×1000 square, 5.3 s); a larger square would reach `max_iter` and stop as an operational error; memory is about a hundred bytes per domain pixel and forty per image pixel across the three channel threads | recorded | as `docs/results/poisson-bench.md` says: a seam band is narrow and converges in few iterations, `feather` stays the default, a preconditioner is the lab's question when a corpus needs it |
| 12 | correctness: a grey (`Luma8`) base under `feather` or `poisson` takes the fill's red channel, the contract's "per RGB channel on the grey value, written back as grey" | recorded | inherited from M2's composite and documented; whether a grey base should take the fill's luminance is an owner question for when a grey base meets a colour filler |
| 13 | security, readability: the workflows pin third-party actions by tag, not commit; the bundles job waits six minutes for dist's release and then fails loudly at the upload; `--sha256` takes `<hex>` or `sha256:<hex>`, not the `.sha256` file's whole line | nit | recorded: the same pinning as the other workflows; the wait fails loudly; the README says to pass the digest |
| 14 | test review: the URL's digest refusal, a hostile archive through the CLI, an archive with no bundle, the scaled window through the cache and `fill_max_side` in the cell identity were untested | should-fix | fixed: `03f144f` (the URL refusal with nothing unpacked, a linked `uv.lock` and an empty archive refused with exit 64 and no install directory, two `fill_max_side` values each a new cell) and the tests in `ffca7a9` to `2fe6966` |
| 15 | test review: the workflows run only on a tag or by hand; the URL form's HTTP error and redirect are untested; the heal-level Poisson test converges at the initial guess (the fake fill already meets the base), so iteration is proven only in `rascal-ops` | recorded | `dist generate --check` and `dist plan` run in the package workflow on every push and passed here; the published assets are the workflows' proof; the solver's iteration, non-convergence and breakdown paths are in `poisson.rs`'s tests |

## Verification

`just check` green after every commit (both feature sets, `cargo deny`, the crate graph,
packaging, the docs coverage, the hook and check self-tests; the clean-room scan and the
Python gates skip on this machine as they say); at the branch head 382 tests with the testkit
and 77 without, the ignored benchmark the one skip. `dist generate --check` and `dist plan`
green at `41e09c2`.
