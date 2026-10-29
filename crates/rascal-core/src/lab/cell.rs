//! What `lab run` writes (SPEC.md "The lab"): the lab record at the root, a cell's record
//! with the components its identity is computed from, each attempt's record, and the ledger
//! of every external call. A cell is one arm on one image; its identity is the SHA-256 over
//! the components, all known before anything runs, so a key hit makes no external call.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::canonical;

/// `lab.json` at the lab's root: what every later invocation must agree with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LabRecord {
    pub lab_version: u32,
    /// The lab's name, from the matrix and the protocol.
    pub name: String,
    /// `sha256:<hex>` of the protocol file's bytes.
    pub protocol_digest: String,
    /// The matrix file, as given.
    pub matrix: String,
    /// The cells of the last `lab run`, by `<arm>/<image>`: the identities `lab pairs` shows.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub current: BTreeMap<String, String>,
}

/// `cells/<identity>/cell.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CellRecord {
    pub cell_version: u32,
    /// `sha256:<hex>` over `components`.
    pub identity: String,
    pub arm: String,
    pub image: String,
    pub protocol_digest: String,
    pub components: CellComponents,
}

/// Everything a cell's identity covers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CellComponents {
    /// The corpus image's digest.
    pub image: String,
    /// Every bound input artifact by recipe input name: the file's digest.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, String>,
    /// The arm's and the image's overrides, as applied (`"<step>.<param>"`: value).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, serde_json::Value>,
    /// The recipe as it runs, overrides applied, as canonical JSON.
    pub recipe: serde_json::Value,
    /// The constant set each step selects, by step id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub constants: BTreeMap<String, String>,
    /// The version of every op the recipe runs, by op name.
    pub op_versions: BTreeMap<String, String>,
    pub filler: FillerEntry,
    /// One per finder step, in recipe order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub finders: Vec<FinderDescriptor>,
}

/// The filler entry as the identity sees it: what changes the work, never its name in the
/// configuration and never the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FillerEntry {
    /// The command as configured, or the URL.
    pub target: String,
    pub profile: String,
    pub paste: String,
    /// The filler's largest side, which windows the heal graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_side: Option<u32>,
    /// Whether the grey rendering is sent with an image-input profile.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub send_mask: bool,
    pub revision: String,
}

/// A finder's invocation descriptor, computed before any finder runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FinderDescriptor {
    pub step: String,
    pub pack: String,
    /// `sha256:<hex>` of the pack's `bundle.toml`, when it was installed from a bundle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_digest: Option<String>,
    pub revision: String,
    /// The pack entry's declared environment variable names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    pub tool: String,
    /// The resolved arguments (the step's parameters over the tool's defaults).
    pub arguments: serde_json::Value,
    /// The semantic identity of each input by port: an initial input's digest, or the
    /// binding (`<step>.<port>`) of a deterministic op over the initial inputs.
    pub inputs: BTreeMap<String, String>,
}

/// `cells/<identity>/attempts/<n>/attempt.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttemptRecord {
    pub attempt_version: u32,
    pub attempt: u32,
    /// `ok`, `stopped` (a step was not applicable) or `failed`.
    pub status: String,
    /// The run directory, relative to the attempt.
    pub run: String,
    /// The result image, relative to the attempt, when the heal completed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// The source image the heal started from (its canonical PNG in the run directory),
    /// relative to the attempt, when the heal completed; the pairs page finds the focus of a
    /// pair by what each result changed in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// External calls this attempt made.
    pub calls: u32,
}

/// One line of `ledger.jsonl`: an external call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LedgerLine {
    pub ledger_version: u32,
    pub cell: String,
    pub attempt: u32,
    pub arm: String,
    pub image: String,
    /// `filler` or `pack`.
    pub kind: String,
    /// The filler or pack entry's name.
    pub target: String,
    pub outcome: String,
    pub duration_ms: u64,
    pub spend_usd: f64,
}

impl CellComponents {
    /// The cell identity: `sha256` over the canonical JSON of the components.
    pub fn identity(&self) -> String {
        canonical::identity_of("cell", self).as_str().to_string()
    }
}
