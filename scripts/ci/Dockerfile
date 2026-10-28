# The Linux leg of `just ci-local`: the pinned toolchain from rust-toolchain.toml plus the pinned
# helper tools, and nothing else. Built once per toolchain by scripts/ci-local.sh.
FROM rust:1.99.1-slim-bookworm
ARG JUST_VERSION=1.59.0
ARG DENY_VERSION=0.20.2
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl git python3 \
 && rm -rf /var/lib/apt/lists/*
RUN rustup component add clippy rustfmt
RUN set -eu; \
    case "$(uname -m)" in \
      x86_64) target=x86_64-unknown-linux-musl ;; \
      aarch64) target=aarch64-unknown-linux-musl ;; \
      *) echo "unsupported architecture $(uname -m)" >&2; exit 1 ;; \
    esac; \
    curl -sSfL "https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-${target}.tar.gz" \
      | tar xz -C /usr/local/bin just; \
    curl -sSfL "https://github.com/EmbarkStudios/cargo-deny/releases/download/${DENY_VERSION}/cargo-deny-${DENY_VERSION}-${target}.tar.gz" \
      | tar xz -C /usr/local/bin --strip-components=1 "cargo-deny-${DENY_VERSION}-${target}/cargo-deny"
# The checkout is mounted and owned by the host user; git must trust it.
RUN git config --global --add safe.directory /work
WORKDIR /work
