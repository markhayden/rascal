//! A registry generation: the frames, locations, artifacts and plans a workspace knows, the id
//! counter and the record that produced the generation. `registry/<n>.json`, immutable once
//! written; `CURRENT` names the latest.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::artifact::ArtifactType;
use crate::frame::{Frame, Plan};
use crate::location::LocationEntry;

/// A stored artifact: where it is and what it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactEntry {
    /// The local id, such as `img-1` or `mask-3`.
    pub id: String,
    pub artifact_type: ArtifactType,
    pub identity: String,
    /// The frame id.
    pub frame: String,
    /// The file or directory, relative to the workspace root.
    pub path: String,
    /// The sequence number of the record that produced it.
    pub seq: u64,
}

/// A stored plan: a registry entry, not a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanEntry {
    pub id: String,
    pub identity: String,
    pub plan: Plan,
    pub seq: u64,
}

/// One generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Generation {
    /// The record's version.
    pub generation_version: u32,
    /// The generation number, from 1.
    pub n: u64,
    /// The next id counter per prefix, as one number (ids are `<prefix>-<counter>`).
    pub next_id: u64,
    pub frames: Vec<Frame>,
    pub locations: Vec<LocationEntry>,
    pub artifacts: Vec<ArtifactEntry>,
    #[serde(default)]
    pub plans: Vec<PlanEntry>,
    /// The identities of every committed operation record, in commit order.
    pub records: Vec<String>,
}

impl Generation {
    /// The empty first generation.
    pub fn first() -> Generation {
        Generation {
            generation_version: 1,
            n: 1,
            next_id: 1,
            frames: Vec::new(),
            locations: Vec::new(),
            artifacts: Vec::new(),
            plans: Vec::new(),
            records: Vec::new(),
        }
    }

    /// Allocates the next local id with a prefix.
    pub fn allocate(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}-{}", self.next_id);
        self.next_id += 1;
        id
    }

    /// The artifact entry with this local id or identity.
    pub fn artifact(&self, id_or_identity: &str) -> Option<&ArtifactEntry> {
        self.artifacts
            .iter()
            .find(|a| a.id == id_or_identity || a.identity == id_or_identity)
    }

    /// The location entry with this local id or identity.
    pub fn location(&self, id_or_identity: &str) -> Option<&LocationEntry> {
        self.locations
            .iter()
            .find(|l| l.id == id_or_identity || l.identity == id_or_identity)
    }

    /// The plan entry with this local id or identity.
    pub fn plan(&self, id_or_identity: &str) -> Option<&PlanEntry> {
        self.plans
            .iter()
            .find(|p| p.id == id_or_identity || p.identity == id_or_identity)
    }

    /// The frame with this id.
    pub fn frame(&self, id: &str) -> Option<&Frame> {
        self.frames.iter().find(|f| f.id == id)
    }
}
