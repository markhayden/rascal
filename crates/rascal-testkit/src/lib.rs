//! Test support shared by every crate's tests and by the binary's `testkit` feature: fake
//! fillers (exact, mis-sized, re-encoding, slow, failing), the fake pack, the independent
//! minimal MCP client, a filesystem shim and synthetic image builders.
//!
//! Builders arrive with the tasks that need them: the synthetic images (1.1), the filesystem
//! shim (1.7b), the fake fillers (2.7), the independent MCP client and the fake MCP server (3.1).

pub mod fake_filler;
pub mod fs_shim;
pub mod images;
pub mod mcp_client;
