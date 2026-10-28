# 0012: The Python tool pack ships in v1, with two branches for the segmenter

Date: 2026-10-28. Status: accepted.

## Context

The learned finders (a segmenter now; OCR and detectors later) live in Python. The pack must
be optional, out of process, installable offline from a bundle, and must never put model
weights in the repository. Whether a permissively licensed public segmenter with an
exportable checkpoint and text-prompt input exists is an owner decision made during M4, and the
plan must work either way.

## Decision

`tools/python` is `rascal-tools`: a `uv` project exposing an MCP server with `echo_region`
(a parity tool), `segment`, and a `fetch` command for model artifacts. It ships in v1 as the
last milestone before the lab. **Branch A** (such a segmenter exists): `segment` ships and the
remove-with-a-prompt criterion applies. **Branch B** (none does): the pack ships with
provisioning, `echo_region` and the parity harness, `segment` is a later milestone, and the
criterion is replaced. The bundle and offline install contract hold on both branches. rascal
calls the pack over MCP with an environment allowlist that strips key-shaped variables.

## Alternatives

- **ONNX inference in Rust.** Possible for one model, but every new finder would need its own
  port; Python is where the finders are.
- **No pack in v1.** Leaves `remove` without a prompt-driven finder, which is the common case.

## Consequences

- The pack is a separate runtime with its own tests and lint; `cargo test` never needs it.
- The isolation suite proves the pack sees no key and no corpus path.
