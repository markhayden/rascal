# rascal

The find-and-mask step of an image pipeline. rascal turns a need (a seam to heal, an object to
remove, a scratch to repair, a region to protect) into a high-quality mask and the plumbing a
visual model needs to blend its result back: zones, soft edges, a tone ramp, per-consumer
renderings, and cut/put geometry for wrapped images such as 360 panoramas.

Pure Rust library, CLI and MCP server. Fillers (the models that paint) are plugins; ML finders
are a Python tool pack over MCP.

Status: pre-release. The design is in [SPEC.md](SPEC.md) and the build order in
[PLAN.md](PLAN.md). Nothing is built yet.

Licence: Apache-2.0.
