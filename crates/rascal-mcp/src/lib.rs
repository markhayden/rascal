//! rascal over MCP (SPEC.md "The MCP server and client", ADR 0015): the shared wire shapes
//! both sides use, the pinned protocol revision, the server behind `rascal serve`, the client
//! that drives tool packs, and `provision`, which installs the Python pack from its bundle
//! with no network. The transport is `rmcp`.

pub mod client;
pub mod jsonrpc;
pub mod provision;
pub mod server;
pub mod transport;
