//! The op registry: every op by name, each manifest checked when it is registered, so a
//! malformed manifest fails at start and in tests rather than at a client. The ops crate
//! registers the catalogue; the binary and the server look ops up here.

use std::collections::BTreeMap;

use crate::error::{Error, Result};
use crate::op::{Manifest, Op};

/// The ops available to recipes and the command line.
#[derive(Default)]
pub struct OpRegistry {
    ops: BTreeMap<String, Box<dyn Op>>,
}

impl OpRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an op under its manifest's name after checking the manifest; a second op of the
    /// same name is an error.
    pub fn register(&mut self, op: Box<dyn Op>) -> Result<()> {
        let manifest = op.manifest();
        manifest
            .check()
            .map_err(|m| Error::usage(format!("manifest rejected: {m}")))?;
        if self.ops.contains_key(&manifest.name) {
            return Err(Error::usage(format!(
                "op {} is registered twice",
                manifest.name
            )));
        }
        self.ops.insert(manifest.name, op);
        Ok(())
    }

    /// The op with this name.
    pub fn get(&self, name: &str) -> Option<&dyn Op> {
        self.ops.get(name).map(|b| b.as_ref())
    }

    /// The op with this name, or a usage error naming the catalogue.
    pub fn require(&self, name: &str) -> Result<&dyn Op> {
        self.get(name).ok_or_else(|| {
            Error::usage(format!(
                "unknown op {name}; the catalogue is {}",
                self.names().join(", ")
            ))
        })
    }

    /// Every manifest, by name.
    pub fn manifests(&self) -> Vec<Manifest> {
        self.ops.values().map(|op| op.manifest()).collect()
    }

    /// The op names.
    pub fn names(&self) -> Vec<&str> {
        self.ops.keys().map(String::as_str).collect()
    }
}
