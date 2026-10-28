//! The op registry: every op by name. The ops crate registers the catalogue; the binary and
//! the server look ops up here. REPLACED BY 1.4 (manifest and envelope validation against the
//! schemas at start and in tests).

use std::collections::BTreeMap;

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

    /// Adds an op under its manifest's name, replacing any earlier op of that name.
    pub fn register(&mut self, op: Box<dyn Op>) {
        self.ops.insert(op.manifest().name.clone(), op);
    }

    /// The op with this name.
    pub fn get(&self, name: &str) -> Option<&dyn Op> {
        self.ops.get(name).map(|b| b.as_ref())
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
