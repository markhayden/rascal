//! rascal over MCP (SPEC.md "The MCP server and client", ADR 0015): the shared wire shapes
//! both sides use, the pinned protocol revision, and later the server behind `rascal serve`
//! (task 3.2) and the client that drives tool packs (task 3.3). The transport is `rmcp`.

pub mod client;
pub mod jsonrpc;
pub mod server;
pub mod transport;
