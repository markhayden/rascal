//! The rascal core: frames, images, locations, artifacts and identity, the mask record and its
//! effective weight, renderings, recipes, the workspace and journal, configuration, and the
//! contracts every operation shares (the `Op` trait, manifests, envelopes, the registry).
//!
//! Each module arrives with the task that owns it (PLAN.md, M1). At M0 the crate holds the
//! schema directory and the drift harness every boundary type is checked by.

pub mod error;
pub mod schema_drift;
pub mod schemas;

pub use error::Error;
