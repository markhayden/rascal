//! Test support shared by every crate's tests and by the binary's `testkit` feature: fake
//! fillers (exact, mis-sized, re-encoding, slow, failing), the fake pack, the independent
//! minimal MCP client, a filesystem shim and synthetic image builders.
//!
//! Builders arrive with the tasks that need them; at task 1.1 the synthetic images.

pub mod fs_shim;
pub mod images;
