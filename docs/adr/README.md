# Architecture decision records

One file per decision, dated, never edited after acceptance except to change the status line,
link a superseding record, or add a dated "Refined" section when a review changed the
decision's shape without reversing it. Format: Context, Decision, Alternatives, Consequences.

| # | Decision | Status |
|---|---|---|
| 0001 | A Rust library, CLI and MCP server in one Cargo workspace | accepted 2026-10-28 |
| 0002 | rascal finds and masks; it does not judge | accepted 2026-10-28 |
| 0003 | rascal owns the act step up to the composite; the filler is a plugin | accepted 2026-10-28 |
| 0004 | v1 use cases: seam, remove, repair, protect | accepted 2026-10-28 |
| 0005 | Surfaces in v1: a library, a CLI and an MCP server | accepted 2026-10-28 |
| 0006 | A mask is a record: zones, planes and prescriptions, with one effective weight | accepted 2026-10-28 |
| 0007 | Operations are tools; recipes are TOML step lists with typed bindings | accepted 2026-10-28 |
| 0008 | No model-driven loop inside rascal | accepted 2026-10-28 |
| 0009 | Shared contracts by copied schema, exported on request | accepted 2026-10-28 |
| 0010 | Crate and package names | accepted 2026-10-28 |
| 0011 | Pure Rust pixels | accepted 2026-10-28 |
| 0012 | The Python tool pack ships in v1, with two branches for the segmenter | accepted 2026-10-28 |
| 0013 | Measures are evidence; the lab decides under a frozen protocol | accepted 2026-10-28 |
| 0014 | Apache-2.0 and the family's repository conventions | accepted 2026-10-28 |
| 0015 | The MCP transport: the official SDK pinned, revision 2025-11-25, the independent client | accepted 2026-10-29 |
| 0016 | Canonical serialization and the identity order | accepted 2026-10-28 |
| 0017 | Durability: the sync primitive per platform and what rascal promises | accepted 2026-10-28 |
| 0018 | The segmenter: branch A with OWLv2 and MobileSAM, exported to ONNX | accepted 2026-10-29 |
| 0019 | The pack protocol: manifests in `_meta`, many ports, replay never calls, the escalation | accepted 2026-10-29 |
| 0020 | rascal conforms to table-manners, the shared contract of the waffle tools | accepted 2026-10-30 |

Every decision the plan named by ADR is made.
