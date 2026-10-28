# rascal developer recipes. One manual step first: `cargo install just --version 1.59.0 --locked`.

set shell := ["bash", "-euo", "pipefail", "-c"]

deny_version := "0.20.2"
dist_version := "0.33.1"

# Install the pinned helper tools (idempotent).
bootstrap:
    cargo install cargo-deny --version {{deny_version}} --locked
    cargo install cargo-dist --version {{dist_version}} --locked

# Everything a commit must pass. Exactly the list in PLAN.md "Commit strategy".
check: fmt-check clippy test deny graph packaging clean-room checks-test

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy --workspace --all-targets --features rascal-mask/testkit -- -D warnings

test:
    cargo test --workspace
    cargo test --workspace --features rascal-mask/testkit

# Licences, bans and sources work offline; advisories need the network and run in CI.
deny:
    cargo deny check licenses bans sources

deny-advisories:
    cargo deny check advisories

# Every crate-to-crate edge is listed in scripts/crate-graph.txt.
graph:
    scripts/check-crate-graph.sh

# Every embedded asset sits inside its crate and in the file list `cargo package` ships.
packaging:
    scripts/check-packaging.sh

# No forbidden word in any tracked file. Reads the list at $RASCAL_FORBIDDEN_WORDS (kept outside
# the tree); passes trivially when the variable is unset, as in CI.
clean-room:
    scripts/scan-clean-room.sh

# Prove the repository checks fail when they should.
checks-test:
    scripts/test-checks.sh

# Package and verify every crate as `cargo publish` would, without uploading (network for the index).
publish-dry-run:
    scripts/publish.sh --dry-run

# The offline smoke against any built binary, for example `just smoke target/release/rascal`.
smoke binary="target/debug/rascal":
    scripts/smoke.sh {{binary}}

# `just check` inside a Linux container with the pinned toolchain (needs Docker). The container's
# architecture is the host's, so on Apple silicon this is a Linux aarch64 check.
ci-local:
    scripts/ci-local.sh
