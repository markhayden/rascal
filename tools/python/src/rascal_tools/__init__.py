"""rascal's Python tool pack: an MCP server with the learned finders, and `fetch` for their
model artifacts. The server lists each tool with the copied-shape manifest and rascal's
annotation block in its `_meta`, takes `_harness` with every call, and answers with an
envelope as structured content plus one image block per plane (docs/tools.md "The packs")."""

__version__ = "0.1.0"

PROTOCOL = "2025-11-25"
