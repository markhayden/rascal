//! Replay: a mask rebuilt from its operation record and the stored inputs, then compared with
//! the stored planes. An untouched run replays to an empty diff; a tampered plane is named. A
//! run directory copied elsewhere replays the same, since every input it used is inside it.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::artifact::Artifact;
use crate::error::{Error, Result};
use crate::mask::Mask;
use crate::registry::OpRegistry;
use crate::workspace::Workspace;
use crate::workspace::record::OperationRecord;

/// What a replay found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReplayReport {
    /// The report's version.
    pub replay_version: u32,
    /// The record that produced the mask.
    pub record: String,
    pub op: String,
    pub step: String,
    /// The stored mask's identity and the rebuilt one.
    pub stored_identity: String,
    pub rebuilt_identity: String,
    /// Planes that differ, with the count of differing pixels: empty when identical.
    pub differing_planes: BTreeMap<String, u64>,
    /// The stored record's identity, recomputed from its content, is not the identity it
    /// carries: the record was edited.
    pub record_differs: bool,
}

impl ReplayReport {
    /// True when the rebuilt mask equals the stored one.
    pub fn is_clean(&self) -> bool {
        self.differing_planes.is_empty()
            && !self.record_differs
            && self.stored_identity == self.rebuilt_identity
    }
}

/// Finds the record whose output is this identity.
pub fn record_for(ws: &Workspace, identity: &str) -> Result<OperationRecord> {
    ws.records()?
        .into_iter()
        .find(|r| r.outputs.values().any(|o| o == identity))
        .ok_or_else(|| {
            Error::usage(format!(
                "{identity}: no committed record produced it in {}",
                ws.root.display()
            ))
        })
}

/// Re-runs the record's op on the stored inputs and compares the planes with the stored mask.
pub fn replay_mask(ws: &Workspace, registry: &OpRegistry, stored: &Mask) -> Result<ReplayReport> {
    let record = record_for(ws, &stored.record.identity)?;
    let op = registry.require(&record.op)?;
    let mut inputs = BTreeMap::new();
    for (port, identity) in &record.inputs {
        inputs.insert(port.clone(), ws.load(identity)?);
    }
    let params: crate::op::Params = serde_json::from_value(record.params.clone()).map_err(|e| {
        Error::usage(format!(
            "record {}: parameters are not a table: {e}",
            record.identity
        ))
    })?;
    let frames = ws.frames()?;
    let context = crate::op::Context { frames: &frames };
    let output = op.run(&context, &inputs, &params)?;
    let port = record
        .outputs
        .iter()
        .find(|(_, o)| *o == &stored.record.identity)
        .map(|(p, _)| p.clone())
        .unwrap_or_else(|| "mask".to_string());
    let Some(Artifact::Mask(rebuilt)) = output.outputs.get(&port) else {
        return Err(Error::usage(format!(
            "record {}: replaying {} produced no mask on port {port}",
            record.identity, record.op
        )));
    };
    let mut differing_planes = BTreeMap::new();
    for (name, a, b) in [
        ("core", &stored.core, &rebuilt.core),
        ("blend", &stored.blend, &rebuilt.blend),
        ("protect", &stored.protect, &rebuilt.protect),
    ] {
        if a.dimensions() != b.dimensions() {
            differing_planes.insert(
                name.to_string(),
                u64::from(a.width()) * u64::from(a.height()),
            );
            continue;
        }
        let n = a
            .pixels()
            .zip(b.pixels())
            .filter(|(p, q)| p[0] != q[0])
            .count() as u64;
        if n > 0 {
            differing_planes.insert(name.to_string(), n);
        }
    }
    Ok(ReplayReport {
        replay_version: 1,
        record: record.identity.clone(),
        op: record.op.clone(),
        step: record.step.clone(),
        stored_identity: stored.record.identity.clone(),
        rebuilt_identity: rebuilt.record.identity.clone(),
        differing_planes,
        record_differs: stored.compute_identity().as_str() != stored.record.identity,
    })
}

/// Reads a stored mask without the identity check, for replaying a tampered one; the planes
/// and the record are compared, not trusted.
pub fn read_untrusted(dir: &std::path::Path) -> Result<Mask> {
    Mask::read_unchecked(dir)
}
