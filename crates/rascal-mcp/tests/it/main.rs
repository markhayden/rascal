//! rascal-mcp's integration tests in one binary: each file here is a module, so the dependency
//! graph is linked once, not once per file. `cargo nextest run` runs each test in its own
//! process.

mod client;
mod provision;
mod transport;
