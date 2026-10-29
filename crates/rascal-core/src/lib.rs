//! The rascal core: frames, images, locations, artifacts and identity, the mask record and its
//! effective weight, renderings, recipes, the workspace and journal, configuration, and the
//! contracts every operation shares (the `Op` trait, manifests, envelopes, the registry).
//!
//! Each module arrives with the task that owns it (PLAN.md, M1). Modules the walking slice
//! (task 1.1) brought in a minimal form say which later task replaces them.

pub mod artifact;
pub mod canonical;
pub mod config;
pub mod error;
pub mod frame;
pub mod identity;
pub mod image;
pub mod journal;
pub mod location;
pub mod mapping;
pub mod mask;
pub mod op;
pub mod outdir;
pub mod profile;
pub mod recipe;
pub mod registry;
pub mod rendering;
pub mod schema_drift;
pub mod schemas;
pub mod series;

pub use error::Error;
