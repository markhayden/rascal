#!/usr/bin/env bash
# `just check` inside a Linux container with the pinned toolchain, run through Docker. Builds the
# image from scripts/ci/Dockerfile once per toolchain version, mounts the checkout at /work with
# its own target directory (target/ci-linux) and a named volume for the cargo registry so later
# runs are fast. The container's architecture is the host's. RASCAL_FORBIDDEN_WORDS is not passed
# in, so the clean-room scan is skipped there exactly as it is in CI.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v docker >/dev/null || { echo "ci-local: docker is not installed" >&2; exit 1; }
docker info >/dev/null 2>&1 || { echo "ci-local: docker is not running" >&2; exit 1; }
toolchain="$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)"
image="rascal-ci:$toolchain"
if ! docker image inspect "$image" >/dev/null 2>&1; then
  echo "ci-local: building $image"
  docker build -q -t "$image" scripts/ci
fi
mkdir -p target/ci-linux
exec docker run --rm \
  -v "$(pwd):/work" \
  -v rascal-ci-cargo-registry:/usr/local/cargo/registry \
  -e CARGO_TARGET_DIR=/work/target/ci-linux \
  -e CARGO_TERM_COLOR=always \
  "$image" just check
