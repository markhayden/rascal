//! The contract every operation shares: one trait, one manifest, one envelope per invocation.
//!
//! Task 1.1 has the minimal manifest and envelope the walking slice needs. REPLACED BY 1.4
//! (the full manifest with schemas and bounds, measurements, regions, images, transforms,
//! provenance, registry validation).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::artifact::{Artifact, ArtifactType};
use crate::error::{Error, Result};
use crate::frame::FrameTable;

/// A typed input or output of an op.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Port {
    pub name: String,
    pub artifact_type: ArtifactType,
    pub required: bool,
}

/// A parameter an op accepts in a recipe step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Param {
    pub name: String,
    pub purpose: String,
    pub required: bool,
}

/// What an op is and takes. REPLACED BY 1.4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub purpose: String,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    #[serde(default)]
    pub params: Vec<Param>,
}

impl Manifest {
    /// The declared input port.
    pub fn input(&self, name: &str) -> Option<&Port> {
        self.inputs.iter().find(|p| p.name == name)
    }

    /// The declared output port.
    pub fn output(&self, name: &str) -> Option<&Port> {
        self.outputs.iter().find(|p| p.name == name)
    }
}

/// An invocation's outcome class. Ops never return a verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    MeasuredNothing,
    NotApplicable,
    InvalidArgs,
}

/// A typed output by id and identity: rascal's extension to the envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub artifact_type: ArtifactType,
    pub id: String,
    pub identity: String,
}

/// What an op returns. REPLACED BY 1.4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub status: Status,
    pub summary: String,
    /// Why, when the status is not `ok`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The typed outputs by output name.
    #[serde(default)]
    pub artifacts: BTreeMap<String, ArtifactRef>,
}

impl Envelope {
    /// An `ok` envelope with no artifacts yet.
    pub fn ok(summary: impl Into<String>) -> Envelope {
        Envelope {
            status: Status::Ok,
            summary: summary.into(),
            reason: None,
            artifacts: BTreeMap::new(),
        }
    }

    /// A `not_applicable` envelope with its reason.
    pub fn not_applicable(reason: impl Into<String>) -> Envelope {
        let reason = reason.into();
        Envelope {
            status: Status::NotApplicable,
            summary: format!("not applicable: {reason}"),
            reason: Some(reason),
            artifacts: BTreeMap::new(),
        }
    }
}

/// The inputs an op is given, by port name.
pub type Inputs = BTreeMap<String, Artifact>;

/// The resolved parameters of a step.
pub type Params = toml::Table;

/// What the op runs against. REPLACED BY 1.7a (the workspace).
pub struct Context<'a> {
    pub frames: &'a FrameTable,
}

/// An op's result: the envelope and the outputs by output name. Outputs are empty unless the
/// status is `ok`; an op that cannot act publishes nothing.
#[derive(Debug, Clone)]
pub struct Output {
    pub envelope: Envelope,
    pub outputs: BTreeMap<String, Artifact>,
}

impl Output {
    /// Builds the envelope's artifact table from the outputs.
    pub fn ok(
        summary: impl Into<String>,
        outputs: BTreeMap<String, Artifact>,
        ids: &BTreeMap<String, String>,
    ) -> Output {
        let mut envelope = Envelope::ok(summary);
        for (name, artifact) in &outputs {
            envelope.artifacts.insert(
                name.clone(),
                ArtifactRef {
                    artifact_type: artifact.artifact_type(),
                    id: ids.get(name).cloned().unwrap_or_else(|| name.clone()),
                    identity: artifact.identity().as_str().to_string(),
                },
            );
        }
        Output { envelope, outputs }
    }

    /// A `not_applicable` result.
    pub fn not_applicable(reason: impl Into<String>) -> Output {
        Output {
            envelope: Envelope::not_applicable(reason),
            outputs: BTreeMap::new(),
        }
    }
}

/// One operation.
pub trait Op: Send + Sync {
    /// The manifest.
    fn manifest(&self) -> Manifest;

    /// Runs with validated inputs (every required port bound to an artifact of the declared
    /// type; `validate_inputs` has run) and the step's parameters.
    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output>;
}

/// Checks the inputs against the manifest: every required port bound, every bound port of the
/// declared type, no unknown port, every parameter declared.
pub fn validate_inputs(manifest: &Manifest, inputs: &Inputs, params: &Params) -> Result<()> {
    for port in &manifest.inputs {
        match inputs.get(&port.name) {
            Some(artifact) if artifact.artifact_type() != port.artifact_type => {
                return Err(Error::op(
                    &manifest.name,
                    format!(
                        "input {} must be a {}, got a {}",
                        port.name,
                        port.artifact_type,
                        artifact.artifact_type()
                    ),
                ));
            }
            None if port.required => {
                return Err(Error::op(
                    &manifest.name,
                    format!("input {} is required", port.name),
                ));
            }
            _ => {}
        }
    }
    for name in inputs.keys() {
        if manifest.input(name).is_none() {
            return Err(Error::op(&manifest.name, format!("unknown input {name}")));
        }
    }
    for name in params.keys() {
        if !manifest.params.iter().any(|p| &p.name == name) {
            return Err(Error::op(
                &manifest.name,
                format!("unknown parameter {name}"),
            ));
        }
    }
    for param in manifest.params.iter().filter(|p| p.required) {
        if !params.contains_key(&param.name) {
            return Err(Error::op(
                &manifest.name,
                format!("parameter {} is required", param.name),
            ));
        }
    }
    Ok(())
}
