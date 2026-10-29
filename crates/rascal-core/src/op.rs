//! The contract every operation shares: one trait, one manifest, one envelope per invocation
//! (SPEC.md "Operations"). The native manifest and envelope share their shape with the copied
//! schemas where the fields coincide, so the compatibility export is a projection; the native
//! additions are the typed ports, the parameter table, `artifacts` and `refinement`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::artifact::{Artifact, ArtifactType};
use crate::error::{Error, Result};
use crate::frame::FrameTable;
use crate::location::Location;
use crate::mask::Refinement;

/// A typed input or output of an op.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Port {
    pub name: String,
    pub artifact_type: ArtifactType,
    pub required: bool,
    /// What the port is for.
    pub purpose: String,
}

/// The value kinds a parameter can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    Number,
    Integer,
    Boolean,
    String,
    /// A fixed-length array of numbers; `length` says how many.
    NumberArray,
    /// One of `choices`.
    Choice,
}

/// A parameter an op accepts in a recipe step or on the command line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Param {
    pub name: String,
    pub purpose: String,
    pub kind: ParamKind,
    pub required: bool,
    /// The default when not required, as JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// For a number or integer: the inclusive lower bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// For a number or integer: the inclusive upper bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// For a number array: its length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
    /// For a choice: the allowed strings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
}

impl Param {
    /// A required parameter of a kind.
    pub fn required(name: &str, purpose: &str, kind: ParamKind) -> Param {
        Param {
            name: name.to_string(),
            purpose: purpose.to_string(),
            kind,
            required: true,
            default: None,
            min: None,
            max: None,
            length: None,
            choices: Vec::new(),
        }
    }

    /// An optional parameter with a default.
    pub fn optional(
        name: &str,
        purpose: &str,
        kind: ParamKind,
        default: serde_json::Value,
    ) -> Param {
        Param {
            required: false,
            default: Some(default),
            ..Param::required(name, purpose, kind)
        }
    }

    /// With inclusive bounds.
    pub fn bounded(mut self, min: f64, max: f64) -> Param {
        self.min = Some(min);
        self.max = Some(max);
        self
    }

    /// With an array length.
    pub fn length(mut self, length: u32) -> Param {
        self.length = Some(length);
        self
    }

    /// With choices.
    pub fn choices(mut self, choices: &[&str]) -> Param {
        self.choices = choices.iter().map(|c| c.to_string()).collect();
        self
    }

    /// The JSON Schema of one value.
    fn schema(&self) -> serde_json::Value {
        let mut schema = serde_json::Map::new();
        schema.insert("description".into(), self.purpose.clone().into());
        match self.kind {
            ParamKind::Number | ParamKind::Integer => {
                schema.insert(
                    "type".into(),
                    if self.kind == ParamKind::Integer {
                        "integer"
                    } else {
                        "number"
                    }
                    .into(),
                );
                if let Some(min) = self.min {
                    schema.insert("minimum".into(), min.into());
                }
                if let Some(max) = self.max {
                    schema.insert("maximum".into(), max.into());
                }
            }
            ParamKind::Boolean => {
                schema.insert("type".into(), "boolean".into());
            }
            ParamKind::String => {
                schema.insert("type".into(), "string".into());
            }
            ParamKind::NumberArray => {
                schema.insert("type".into(), "array".into());
                schema.insert("items".into(), serde_json::json!({ "type": "number" }));
                if let Some(n) = self.length {
                    schema.insert("minItems".into(), n.into());
                    schema.insert("maxItems".into(), n.into());
                }
            }
            ParamKind::Choice => {
                schema.insert("type".into(), "string".into());
                schema.insert("enum".into(), self.choices.clone().into());
            }
        }
        if let Some(default) = &self.default {
            schema.insert("default".into(), default.clone());
        }
        serde_json::Value::Object(schema)
    }

    /// Checks one value; the message names the parameter.
    pub fn check(&self, value: &toml::Value) -> std::result::Result<(), String> {
        let name = &self.name;
        let number = || {
            value
                .as_float()
                .or_else(|| value.as_integer().map(|i| i as f64))
        };
        match self.kind {
            ParamKind::Number | ParamKind::Integer => {
                if self.kind == ParamKind::Integer && value.as_integer().is_none() {
                    return Err(format!("{name} must be an integer"));
                }
                let n = number().ok_or_else(|| format!("{name} must be a number"))?;
                if !n.is_finite() {
                    return Err(format!("{name} must be finite"));
                }
                if self.min.is_some_and(|min| n < min) || self.max.is_some_and(|max| n > max) {
                    return Err(format!(
                        "{name} must be in [{}, {}], got {n}",
                        self.min.unwrap_or(f64::NEG_INFINITY),
                        self.max.unwrap_or(f64::INFINITY)
                    ));
                }
            }
            ParamKind::Boolean => {
                value
                    .as_bool()
                    .ok_or_else(|| format!("{name} must be true or false"))?;
            }
            ParamKind::String => {
                value
                    .as_str()
                    .ok_or_else(|| format!("{name} must be a string"))?;
            }
            ParamKind::NumberArray => {
                let items = value
                    .as_array()
                    .ok_or_else(|| format!("{name} must be an array of numbers"))?;
                if self.length.is_some_and(|n| items.len() != n as usize) {
                    return Err(format!(
                        "{name} must have exactly {} numbers, got {}",
                        self.length.unwrap_or(0),
                        items.len()
                    ));
                }
                for item in items {
                    let n = item
                        .as_float()
                        .or_else(|| item.as_integer().map(|i| i as f64))
                        .ok_or_else(|| format!("{name} must be an array of numbers"))?;
                    if !n.is_finite() {
                        return Err(format!("{name} must hold finite numbers"));
                    }
                }
            }
            ParamKind::Choice => {
                let s = value
                    .as_str()
                    .ok_or_else(|| format!("{name} must be one of {}", self.choices.join(", ")))?;
                if !self.choices.iter().any(|c| c == s) {
                    return Err(format!(
                        "{name} must be one of {}, got {s}",
                        self.choices.join(", ")
                    ));
                }
            }
        }
        Ok(())
    }
}

/// How expensive one call is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CostTier {
    Cheap,
    Moderate,
    Expensive,
}

/// The cost of one call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Cost {
    pub tier: CostTier,
    /// Rough duration of one call, in seconds.
    pub seconds: f64,
}

/// Bounds on what an op returns in its envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub max_images: u32,
    pub max_long_edge: u32,
    pub max_image_bytes: u64,
    pub max_summary_bytes: u32,
    pub max_regions: u32,
    pub max_region_label_bytes: u32,
    pub max_measurements: u32,
    pub max_transforms: u32,
    pub max_envelope_bytes: u64,
}

impl Bounds {
    /// The bounds every pure op has: no inline images, a short summary, few regions.
    pub fn standard() -> Bounds {
        Bounds {
            max_images: 0,
            max_long_edge: 0,
            max_image_bytes: 0,
            max_summary_bytes: 1024,
            max_regions: 64,
            max_region_label_bytes: 64,
            max_measurements: 64,
            max_transforms: 8,
            max_envelope_bytes: 1 << 20,
        }
    }
}

/// What an op is and takes. Every field is required to be meaningful: the registry checks
/// them at start and a test checks them for every shipped op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// The manifest schema version.
    pub manifest_version: u32,
    pub name: String,
    pub version: String,
    pub purpose: String,
    pub when_to_use: String,
    pub when_not_to_use: String,
    pub output_guidance: String,
    pub cost: Cost,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    #[serde(default)]
    pub params: Vec<Param>,
    /// The named constant sets a step may select with `constants = "<name>"`; the first is the
    /// default. Empty for an op that has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constants: Vec<String>,
    /// Bounds on what the envelope holds.
    pub returns: Bounds,
    /// The JSON Schema of the parameter table, derived from `params`.
    pub input_schema: serde_json::Value,
    /// The JSON Schema of the `artifacts` table, derived from `outputs`.
    pub output_schema: serde_json::Value,
}

impl Manifest {
    /// A manifest with the schemas derived from its params and outputs.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: &str,
        version: &str,
        purpose: &str,
        when_to_use: &str,
        when_not_to_use: &str,
        output_guidance: &str,
        cost: Cost,
        inputs: Vec<Port>,
        outputs: Vec<Port>,
        params: Vec<Param>,
    ) -> Manifest {
        let mut manifest = Manifest {
            manifest_version: 1,
            name: name.to_string(),
            version: version.to_string(),
            purpose: purpose.to_string(),
            when_to_use: when_to_use.to_string(),
            when_not_to_use: when_not_to_use.to_string(),
            output_guidance: output_guidance.to_string(),
            cost,
            inputs,
            outputs,
            params,
            constants: Vec::new(),
            returns: Bounds::standard(),
            input_schema: serde_json::Value::Null,
            output_schema: serde_json::Value::Null,
        };
        manifest.input_schema = manifest.derive_input_schema();
        manifest.output_schema = manifest.derive_output_schema();
        manifest
    }

    /// With the constant sets the op accepts.
    pub fn with_constants(mut self, constants: &[&str]) -> Manifest {
        self.constants = constants.iter().map(|c| c.to_string()).collect();
        self
    }

    fn derive_input_schema(&self) -> serde_json::Value {
        let properties: serde_json::Map<String, serde_json::Value> = self
            .params
            .iter()
            .map(|p| (p.name.clone(), p.schema()))
            .collect();
        let required: Vec<&str> = self
            .params
            .iter()
            .filter(|p| p.required)
            .map(|p| p.name.as_str())
            .collect();
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": format!("{} parameters", self.name),
            "type": "object",
            "additionalProperties": false,
            "properties": properties,
            "required": required,
        })
    }

    fn derive_output_schema(&self) -> serde_json::Value {
        let properties: serde_json::Map<String, serde_json::Value> = self
            .outputs
            .iter()
            .map(|p| {
                (
                    p.name.clone(),
                    serde_json::json!({
                        "type": "object", "additionalProperties": false,
                        "description": p.purpose,
                        "properties": {
                            "artifact_type": { "const": p.artifact_type.to_string() },
                            "id": { "type": "string" },
                            "identity": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" }
                        },
                        "required": ["artifact_type", "id", "identity"]
                    }),
                )
            })
            .collect();
        let required: Vec<&str> = self
            .outputs
            .iter()
            .filter(|p| p.required)
            .map(|p| p.name.as_str())
            .collect();
        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": format!("{} artifacts", self.name),
            "type": "object",
            "additionalProperties": false,
            "properties": properties,
            "required": required,
        })
    }

    /// The declared input port.
    pub fn input(&self, name: &str) -> Option<&Port> {
        self.inputs.iter().find(|p| p.name == name)
    }

    /// The declared output port.
    pub fn output(&self, name: &str) -> Option<&Port> {
        self.outputs.iter().find(|p| p.name == name)
    }

    /// The declared parameter.
    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.name == name)
    }

    /// Every field meaningful: non-empty texts, a semantic version, unique port and parameter
    /// names, defaults on optional parameters, choices on choice parameters, a length on
    /// arrays, schemas derived.
    pub fn check(&self) -> std::result::Result<(), String> {
        let name = &self.name;
        for (field, text) in [
            ("name", &self.name),
            ("version", &self.version),
            ("purpose", &self.purpose),
            ("when_to_use", &self.when_to_use),
            ("when_not_to_use", &self.when_not_to_use),
            ("output_guidance", &self.output_guidance),
        ] {
            if text.trim().is_empty() {
                return Err(format!("{name}: {field} is empty"));
            }
        }
        if self.manifest_version != 1 {
            return Err(format!("{name}: manifest_version must be 1"));
        }
        let parts: Vec<&str> = self.version.split('.').collect();
        if parts.len() != 3 || parts.iter().any(|p| p.parse::<u32>().is_err()) {
            return Err(format!(
                "{name}: version must be major.minor.patch, got {}",
                self.version
            ));
        }
        if !(self.cost.seconds.is_finite() && self.cost.seconds >= 0.0) {
            return Err(format!(
                "{name}: cost.seconds must be a non-negative number"
            ));
        }
        if self.outputs.is_empty() {
            return Err(format!("{name}: an op has at least one output"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for port in self.inputs.iter().chain(&self.outputs) {
            if port.name.trim().is_empty() || port.purpose.trim().is_empty() {
                return Err(format!("{name}: a port needs a name and a purpose"));
            }
        }
        for port in &self.inputs {
            if !seen.insert(port.name.as_str()) {
                return Err(format!("{name}: input {} is declared twice", port.name));
            }
        }
        seen.clear();
        for port in &self.outputs {
            if !seen.insert(port.name.as_str()) {
                return Err(format!("{name}: output {} is declared twice", port.name));
            }
        }
        seen.clear();
        for param in &self.params {
            if param.name.trim().is_empty() || param.purpose.trim().is_empty() {
                return Err(format!("{name}: a parameter needs a name and a purpose"));
            }
            if !seen.insert(param.name.as_str()) {
                return Err(format!(
                    "{name}: parameter {} is declared twice",
                    param.name
                ));
            }
            if !param.required && param.default.is_none() {
                return Err(format!(
                    "{name}: optional parameter {} needs a default",
                    param.name
                ));
            }
            if param.kind == ParamKind::Choice && param.choices.is_empty() {
                return Err(format!(
                    "{name}: choice parameter {} needs choices",
                    param.name
                ));
            }
            if param.kind == ParamKind::NumberArray && param.length.is_none() {
                return Err(format!(
                    "{name}: array parameter {} needs a length",
                    param.name
                ));
            }
            if let (Some(min), Some(max)) = (param.min, param.max)
                && min > max
            {
                return Err(format!("{name}: parameter {} has min > max", param.name));
            }
        }
        if self.input_schema != self.derive_input_schema()
            || self.output_schema != self.derive_output_schema()
        {
            return Err(format!(
                "{name}: the schemas do not match the params and outputs"
            ));
        }
        Ok(())
    }

    /// The parameters with defaults filled in, checked against their kinds and bounds. The
    /// message names the parameter and is what `invalid_args` carries.
    pub fn resolve_params(&self, given: &Params) -> std::result::Result<Params, String> {
        let mut resolved = Params::new();
        for name in given.keys() {
            if self.param(name).is_none() {
                return Err(format!("unknown parameter {name}"));
            }
        }
        for param in &self.params {
            match given.get(&param.name) {
                Some(value) => {
                    param.check(value)?;
                    resolved.insert(param.name.clone(), value.clone());
                }
                None if param.required => {
                    return Err(format!("parameter {} is required", param.name));
                }
                None => {
                    let default = param.default.clone().unwrap_or(serde_json::Value::Null);
                    let value: toml::Value = serde_json::from_value::<toml::Value>(default)
                        .map_err(|e| {
                            format!("parameter {}: default is not a TOML value: {e}", param.name)
                        })?;
                    resolved.insert(param.name.clone(), value);
                }
            }
        }
        Ok(resolved)
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

/// A named scalar with a unit, in a frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    pub name: String,
    pub value: f64,
    pub unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<String>,
}

/// A located finding with a label and a score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub id: String,
    pub location: Location,
    pub label: String,
    pub score: f64,
}

/// An image the op produced, by reference to its artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeImage {
    pub frame: String,
    pub parent: String,
    pub width: u32,
    pub height: u32,
    pub media_type: String,
    /// The image artifact's identity.
    pub identity: String,
}

/// A transform between two frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    pub id: String,
    pub from: String,
    pub to: String,
    pub kind: String,
    pub params: Vec<f64>,
}

/// Who produced the envelope and how long it took.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub tool: String,
    pub version: String,
    pub duration_ms: u64,
}

/// A typed output by id and identity: rascal's extension to the envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub artifact_type: ArtifactType,
    pub id: String,
    pub identity: String,
}

/// What an op returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    /// The envelope schema version.
    pub envelope_version: u32,
    pub status: Status,
    pub summary: String,
    /// Why, when the status is not `ok`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub measurements: Vec<Measurement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<Region>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<EnvelopeImage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transforms: Vec<Transform>,
    pub provenance: Provenance,
    /// The typed outputs by output name.
    #[serde(default)]
    pub artifacts: BTreeMap<String, ArtifactRef>,
    /// What a refinement cleared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refinement: Option<Refinement>,
}

impl Envelope {
    fn blank(status: Status, summary: String, reason: Option<String>) -> Envelope {
        Envelope {
            envelope_version: 1,
            status,
            summary,
            reason,
            measurements: Vec::new(),
            regions: Vec::new(),
            images: Vec::new(),
            transforms: Vec::new(),
            provenance: Provenance {
                tool: String::new(),
                version: String::new(),
                duration_ms: 0,
            },
            artifacts: BTreeMap::new(),
            refinement: None,
        }
    }

    /// An `ok` envelope with no artifacts yet.
    pub fn ok(summary: impl Into<String>) -> Envelope {
        Envelope::blank(Status::Ok, summary.into(), None)
    }

    /// A `not_applicable` envelope with its reason.
    pub fn not_applicable(reason: impl Into<String>) -> Envelope {
        let reason = reason.into();
        Envelope::blank(
            Status::NotApplicable,
            format!("not applicable: {reason}"),
            Some(reason),
        )
    }

    /// An `invalid_args` envelope with its reason.
    pub fn invalid_args(reason: impl Into<String>) -> Envelope {
        let reason = reason.into();
        Envelope::blank(
            Status::InvalidArgs,
            format!("invalid arguments: {reason}"),
            Some(reason),
        )
    }

    /// A `measured_nothing` envelope with its reason.
    pub fn measured_nothing(reason: impl Into<String>) -> Envelope {
        let reason = reason.into();
        Envelope::blank(
            Status::MeasuredNothing,
            format!("measured nothing: {reason}"),
            Some(reason),
        )
    }

    /// Checks the envelope against the manifest's bounds and ports: every required output
    /// present with the declared type, no undeclared output, counts within bounds, and nothing
    /// published unless the status is `ok`.
    pub fn check(&self, manifest: &Manifest) -> std::result::Result<(), String> {
        let name = &manifest.name;
        if self.envelope_version != 1 {
            return Err(format!("{name}: envelope_version must be 1"));
        }
        if self.status != Status::Ok && !self.artifacts.is_empty() {
            return Err(format!(
                "{name}: a {:?} envelope publishes nothing",
                self.status
            ));
        }
        if self.status == Status::Ok {
            for port in manifest.outputs.iter().filter(|p| p.required) {
                match self.artifacts.get(&port.name) {
                    None => return Err(format!("{name}: output {} is missing", port.name)),
                    Some(a) if a.artifact_type != port.artifact_type => {
                        return Err(format!(
                            "{name}: output {} must be a {}, got a {}",
                            port.name, port.artifact_type, a.artifact_type
                        ));
                    }
                    _ => {}
                }
            }
            for (output, a) in &self.artifacts {
                match manifest.output(output) {
                    None => return Err(format!("{name}: undeclared output {output}")),
                    Some(port) if port.artifact_type != a.artifact_type => {
                        return Err(format!(
                            "{name}: output {output} must be a {}, got a {}",
                            port.artifact_type, a.artifact_type
                        ));
                    }
                    _ => {}
                }
                if crate::identity::Identity::parse(&a.identity).is_none() {
                    return Err(format!("{name}: output {output} has a malformed identity"));
                }
            }
        } else if self.reason.as_deref().unwrap_or("").is_empty() {
            return Err(format!(
                "{name}: a {:?} envelope needs a reason",
                self.status
            ));
        }
        let b = &manifest.returns;
        if self.summary.len() > b.max_summary_bytes as usize {
            return Err(format!(
                "{name}: summary longer than {} bytes",
                b.max_summary_bytes
            ));
        }
        if self.regions.len() > b.max_regions as usize {
            return Err(format!("{name}: more than {} regions", b.max_regions));
        }
        if self
            .regions
            .iter()
            .any(|r| r.label.len() > b.max_region_label_bytes as usize)
        {
            return Err(format!(
                "{name}: a region label longer than {} bytes",
                b.max_region_label_bytes
            ));
        }
        if self.measurements.len() > b.max_measurements as usize {
            return Err(format!(
                "{name}: more than {} measurements",
                b.max_measurements
            ));
        }
        if self.transforms.len() > b.max_transforms as usize {
            return Err(format!("{name}: more than {} transforms", b.max_transforms));
        }
        if self.images.len() > b.max_images as usize {
            return Err(format!("{name}: more than {} images", b.max_images));
        }
        if self.measurements.iter().any(|m| !m.value.is_finite()) {
            return Err(format!("{name}: a measurement is not finite"));
        }
        Ok(())
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
    pub fn ok(summary: impl Into<String>, outputs: BTreeMap<String, Artifact>) -> Output {
        let mut envelope = Envelope::ok(summary);
        for (name, artifact) in &outputs {
            envelope.artifacts.insert(
                name.clone(),
                ArtifactRef {
                    artifact_type: artifact.artifact_type(),
                    id: name.clone(),
                    identity: artifact.identity().as_str().to_string(),
                },
            );
        }
        Output { envelope, outputs }
    }

    /// With a refinement report.
    pub fn with_refinement(mut self, refinement: Refinement) -> Output {
        self.envelope.refinement = Some(refinement);
        self
    }

    /// With a measurement.
    pub fn with_measurement(
        mut self,
        name: &str,
        value: f64,
        unit: &str,
        frame: Option<&str>,
    ) -> Output {
        self.envelope.measurements.push(Measurement {
            name: name.to_string(),
            value,
            unit: unit.to_string(),
            frame: frame.map(str::to_string),
        });
        self
    }

    /// A `not_applicable` result.
    pub fn not_applicable(reason: impl Into<String>) -> Output {
        Output {
            envelope: Envelope::not_applicable(reason),
            outputs: BTreeMap::new(),
        }
    }

    /// An `invalid_args` result.
    pub fn invalid_args(reason: impl Into<String>) -> Output {
        Output {
            envelope: Envelope::invalid_args(reason),
            outputs: BTreeMap::new(),
        }
    }
}

/// One operation.
pub trait Op: Send + Sync {
    /// The manifest.
    fn manifest(&self) -> Manifest;

    /// Runs with validated inputs (every required port bound to an artifact of the declared
    /// type, frames agreeing) and resolved parameters (defaults filled, kinds and bounds
    /// checked). An op that cannot act on its input returns `not_applicable`; one that finds
    /// a parameter value wrong for its input returns `invalid_args`; an operational failure is
    /// an error.
    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output>;
}

/// The registry's shared input validation: every required port bound, every bound port of the
/// declared type (a rendering is an `Image` and is never a `Mask`), no unknown port, and a
/// mask and an image in the same invocation in the same frame.
pub fn validate_inputs(manifest: &Manifest, inputs: &Inputs) -> Result<()> {
    for port in &manifest.inputs {
        match inputs.get(&port.name) {
            Some(artifact) if artifact.artifact_type() != port.artifact_type => {
                let role = match artifact {
                    Artifact::Image(image) => {
                        format!(" (an image with role {:?})", image.record.role)
                    }
                    _ => String::new(),
                };
                return Err(Error::op(
                    &manifest.name,
                    format!(
                        "input {} must be a {}, got a {}{role}",
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
    let frames: Vec<(&str, &str)> = inputs
        .iter()
        .filter(|(_, a)| matches!(a, Artifact::Mask(_) | Artifact::Image(_)))
        .map(|(k, a)| (k.as_str(), a.frame()))
        .collect();
    if let Some(first) = frames.first()
        && let Some(other) = frames.iter().find(|(_, f)| *f != first.1)
    {
        return Err(Error::op(
            &manifest.name,
            format!(
                "inputs {} (frame {}) and {} (frame {}) must be in one frame",
                first.0, first.1, other.0, other.1
            ),
        ));
    }
    Ok(())
}
