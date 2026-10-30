# The test suite's timing (success criterion 3)

`cargo test --workspace` must pass offline in under three minutes of test execution,
compilation excluded, on the owner's Apple silicon machine. The test binaries were built first
(`cargo test --no-run`) and the two runs timed with `/usr/bin/time`; `real` includes cargo's
few seconds of up-to-date checks, so each figure is an upper bound on the execution.

| date | machine | feature set | tests | passed | failed | real |
|---|---|---|---|---|---|---|
| 2026-10-29 | Apple M4 Max, 64 GiB, macOS 27.0.1, Rust 1.99.1 | default | 250 | 250 | 0 | 30.8 s |
| 2026-10-29 | the same | `--features rascal-mask/testkit` | 314 | 314 | 0 | 129.9 s |

Both under three minutes, and 2 min 41 s together. The testkit set carries the end-to-end
heals, the fake pack's timeouts (which sleep by design) and the lab scenarios; the plain set
is the library and the CLI. Rerun with:

```
cargo test --workspace --no-run && cargo test --workspace --features rascal-mask/testkit --no-run
/usr/bin/time -p cargo test --workspace
/usr/bin/time -p cargo test --workspace --features rascal-mask/testkit
```

## The release archive and the bundle archive (task 7.2)

2026-10-29, the same machine. `dist build --artifacts=local --target aarch64-apple-darwin`
produced `rascal-mask-aarch64-apple-darwin.tar.xz` with its `.sha256`; unpacked, `rascal
--version` answered `rascal 0.1.0` and `rascal run seam fixtures/images/step-pano.png` wrote
the mask, the grey rendering and the journal from the archive's binary with no configuration
file. `scripts/bundle-acceptance.sh target/debug/rascal --archive <file>` built this machine's
bundle with the model files, packed it, installed it from the archive by its digest with empty
caches, answered `echo_region` and `segment` (the chair found), refused a tampered copy, and
passed the pytest and Rust install tests against it.
