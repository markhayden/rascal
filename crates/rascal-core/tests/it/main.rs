//! rascal-core's integration tests in one binary: each file here is a module, so the dependency
//! graph is linked once, not once per file. `cargo nextest run` runs each test in its own
//! process.

mod config;
mod contract;
mod export_replay;
mod frames_identity;
mod images;
mod lab_decode;
mod lab_focus;
mod locations_mapping;
mod many_bindings;
mod mask;
mod mask_bundle;
mod op_contracts;
mod recipe;
mod recovery;
mod schemas;
mod text_inputs;
mod workspace;
