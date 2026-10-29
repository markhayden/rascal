//! Recipes: TOML step lists with typed bindings, validated before anything runs, and the
//! runner that executes one against a run directory, writing artifacts and the journal.
//!
//! A recipe is validated before anything runs: every op known, every step id unique, every
//! binding typed and pointing at an earlier step or a declared input, every required port
//! bound, every parameter declared and within bounds, every constant set known to its op,
//! every output of the right type; the error names the step. Defaults are resolved by the
//! manifest and the resolved values are what the journal records. The runner writes under
//! `--out` directly. REPLACED BY 1.7a (the workspace commit protocol).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::artifact::{Artifact, ArtifactType};
use crate::error::{Error, Result};
use crate::frame::FrameTable;
use crate::image::Image;
use crate::mask::{Mask, UseCase};
use crate::op::{self, Inputs, Params, Status};
use crate::registry::OpRegistry;
use crate::workspace::Workspace;

/// A declared recipe input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputDecl {
    #[serde(rename = "type")]
    pub artifact_type: ArtifactType,
}

/// One step: its id, op, typed input bindings, and every other key as a parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub id: String,
    pub op: String,
    pub inputs: BTreeMap<String, String>,
    /// The constant set the step selects, for ops that have them.
    pub constants: Option<String>,
    pub params: Params,
}

impl JsonSchema for Step {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Step".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "One step: id, op, typed input bindings, an optional constant set, and every other key as a parameter.",
            "properties": {
                "id": { "type": "string" },
                "op": { "type": "string" },
                "inputs": { "type": "object", "additionalProperties": { "type": "string" } },
                "constants": { "type": "string" }
            },
            "required": ["id", "op"],
            "additionalProperties": true
        })
    }
}

impl serde::Serialize for Step {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut table = toml::Table::new();
        table.insert("id".into(), toml::Value::String(self.id.clone()));
        table.insert("op".into(), toml::Value::String(self.op.clone()));
        if !self.inputs.is_empty() {
            table.insert(
                "inputs".into(),
                toml::Value::Table(
                    self.inputs
                        .iter()
                        .map(|(k, v)| (k.clone(), toml::Value::String(v.clone())))
                        .collect(),
                ),
            );
        }
        if let Some(c) = &self.constants {
            table.insert("constants".into(), toml::Value::String(c.clone()));
        }
        for (k, v) in &self.params {
            table.insert(k.clone(), v.clone());
        }
        table.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Step {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        use serde::de::Error as _;
        let mut table = toml::Table::deserialize(deserializer)?;
        let id = match table.remove("id") {
            Some(toml::Value::String(s)) => s,
            _ => return Err(D::Error::custom("step: `id` must be a string")),
        };
        let op = match table.remove("op") {
            Some(toml::Value::String(s)) => s,
            _ => {
                return Err(D::Error::custom(format!(
                    "step {id}: `op` must be a string"
                )));
            }
        };
        let constants = match table.remove("constants") {
            None => None,
            Some(toml::Value::String(s)) => Some(s),
            Some(_) => {
                return Err(D::Error::custom(format!(
                    "step {id}: `constants` must be a string"
                )));
            }
        };
        let inputs = match table.remove("inputs") {
            None => BTreeMap::new(),
            Some(toml::Value::Table(t)) => t
                .into_iter()
                .map(|(k, v)| match v {
                    toml::Value::String(s) => Ok((k, s)),
                    _ => Err(D::Error::custom(format!(
                        "step {id}: input {k} must be a binding string"
                    ))),
                })
                .collect::<std::result::Result<_, _>>()?,
            Some(_) => {
                return Err(D::Error::custom(format!(
                    "step {id}: `inputs` must be a table"
                )));
            }
        };
        Ok(Step {
            id,
            op,
            inputs,
            constants,
            params: table,
        })
    }
}

/// A rendering the recipe writes under `--out`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderOut {
    #[serde(rename = "for")]
    pub profile: String,
    pub out: String,
}

/// The contract a recipe's consumer reads, by output name.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Outputs {
    #[serde(default)]
    pub mask: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub render: Vec<RenderOut>,
}

/// A recipe file; its schema is the authored `schemas/rascal/recipe-v1.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub version: u32,
    pub name: String,
    pub use_case: UseCase,
    #[serde(default)]
    pub projection: Option<crate::frame::Projection>,
    #[serde(default)]
    pub inputs: BTreeMap<String, InputDecl>,
    #[serde(default)]
    pub step: Vec<Step>,
    #[serde(default)]
    pub outputs: Outputs,
}

impl Recipe {
    /// Parses recipe text.
    pub fn parse(text: &str, origin: &str) -> Result<Recipe> {
        toml::from_str(text).map_err(|source| Error::Toml {
            path: PathBuf::from(origin),
            source: Box::new(source),
        })
    }

    /// Reads a recipe file.
    pub fn read(path: &Path) -> Result<Recipe> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        Recipe::parse(&text, &path.display().to_string())
    }

    fn fail(&self, step: &str, message: impl Into<String>) -> Error {
        Error::Recipe {
            recipe: self.name.clone(),
            step: step.to_string(),
            message: message.into(),
        }
    }

    /// The type a binding resolves to, given the steps before `upto`.
    fn binding_type(
        &self,
        binding: &str,
        upto: usize,
        registry: &OpRegistry,
    ) -> Option<ArtifactType> {
        let (source, name) = binding.split_once('.')?;
        if source == "inputs" {
            return self.inputs.get(name).map(|d| d.artifact_type);
        }
        let index = self.step[..upto].iter().position(|s| s.id == source)?;
        let manifest = registry.get(&self.step[index].op)?.manifest();
        manifest.output(name).map(|p| p.artifact_type)
    }

    /// Checks the recipe against the registry: every op known, every step id unique, every
    /// binding typed and pointing at an earlier step or a declared input, every required port
    /// bound, every parameter declared, every output of the right type.
    pub fn validate(&self, registry: &OpRegistry) -> Result<()> {
        if self.version != 1 {
            return Err(self.fail(
                "-",
                format!("version {} is not supported (expected 1)", self.version),
            ));
        }
        if self.step.is_empty() {
            return Err(self.fail("-", "a recipe needs at least one step"));
        }
        for (index, step) in self.step.iter().enumerate() {
            if step.id == "inputs" || step.id.contains('.') || step.id.is_empty() {
                return Err(self.fail(
                    &step.id,
                    "step ids must be non-empty, contain no '.', and not be 'inputs'",
                ));
            }
            if self.step[..index].iter().any(|s| s.id == step.id) {
                return Err(self.fail(&step.id, "duplicate step id"));
            }
            let op = registry
                .get(&step.op)
                .ok_or_else(|| self.fail(&step.id, format!("unknown op {}", step.op)))?;
            let manifest = op.manifest();
            for (port, binding) in &step.inputs {
                let declared = manifest.input(port).ok_or_else(|| {
                    self.fail(&step.id, format!("op {} has no input {port}", step.op))
                })?;
                let bound = self
                    .binding_type(binding, index, registry)
                    .ok_or_else(|| self.fail(&step.id, format!("input {port} is bound to {binding}, which is not an earlier step's output or a declared input")))?;
                if bound != declared.artifact_type {
                    return Err(self.fail(
                        &step.id,
                        format!(
                            "input {port} must be a {}, but {binding} is a {bound}",
                            declared.artifact_type
                        ),
                    ));
                }
            }
            for port in manifest.inputs.iter().filter(|p| p.required) {
                if !step.inputs.contains_key(&port.name) {
                    return Err(self.fail(
                        &step.id,
                        format!("input {} is required by {}", port.name, step.op),
                    ));
                }
            }
            manifest
                .resolve_params(&step.params)
                .map_err(|m| self.fail(&step.id, format!("op {}: {m}", step.op)))?;

            if let Some(constants) = &step.constants {
                if manifest.constants.is_empty() {
                    return Err(self.fail(
                        &step.id,
                        format!(
                            "op {} has no constant sets, but constants = \"{constants}\" was given",
                            step.op
                        ),
                    ));
                }
                if !manifest.constants.iter().any(|c| c == constants) {
                    return Err(self.fail(
                        &step.id,
                        format!(
                            "op {} has no constant set {constants}; it has {}",
                            step.op,
                            manifest.constants.join(", ")
                        ),
                    ));
                }
            }
        }
        for name in self.inputs.keys() {
            if name.is_empty() || name.contains('.') {
                return Err(self.fail(
                    "inputs",
                    format!("input names must be non-empty and contain no '.': {name:?}"),
                ));
            }
        }
        let n = self.step.len();
        for (name, binding, expected) in [
            ("mask", &self.outputs.mask, ArtifactType::Mask),
            ("image", &self.outputs.image, ArtifactType::Image),
            ("plan", &self.outputs.plan, ArtifactType::Plan),
        ] {
            if let Some(binding) = binding {
                match self.binding_type(binding, n, registry) {
                    Some(t) if t == expected => {}
                    Some(t) => {
                        return Err(self.fail(
                            "outputs",
                            format!("{name} = {binding} is a {t}, expected a {expected}"),
                        ));
                    }
                    None => {
                        return Err(self.fail(
                            "outputs",
                            format!("{name} = {binding} does not name a step output or an input"),
                        ));
                    }
                }
            }
        }
        if !self.outputs.render.is_empty() {
            if self.outputs.mask.is_none() {
                return Err(self.fail("outputs", "render needs outputs.mask"));
            }
            if registry.get("render").is_none() {
                return Err(self.fail("outputs", "render needs the render op"));
            }
            let render_manifest = registry.get("render").map(|op| op.manifest());
            for r in &self.outputs.render {
                if r.out.is_empty() || r.out.contains('/') || r.out.contains("..") {
                    return Err(self.fail(
                        "outputs",
                        format!("render out '{}' must be a plain file name", r.out),
                    ));
                }
                if let Some(profiles) = render_manifest
                    .as_ref()
                    .and_then(|m| m.param("for"))
                    .map(|p| &p.choices)
                    && !profiles.iter().any(|p| p == &r.profile)
                {
                    return Err(self.fail(
                        "outputs",
                        format!(
                            "render profile '{}' is not one of {}",
                            r.profile,
                            profiles.join(", ")
                        ),
                    ));
                }
                if r.profile != "grey" && r.profile != "binary" && self.outputs.image.is_none() {
                    return Err(self.fail(
                        "outputs",
                        format!(
                            "render profile '{}' draws on the image: set outputs.image",
                            r.profile
                        ),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// How a run ended.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// `ok`, or the first step's status that was not.
    pub status: Status,
    /// The step that stopped the run, when the status is not `ok`.
    pub stopped_at: Option<String>,
    /// Why, when the status is not `ok`.
    pub reason: Option<String>,
    /// Every binding produced, `inputs.*` included.
    pub bindings: BTreeMap<String, Artifact>,
    /// The named outputs: `mask`, `image`, `plan`.
    pub outputs: BTreeMap<String, Artifact>,
    /// Files written for `outputs.render`, by profile.
    pub renders: Vec<(String, PathBuf)>,
}

/// A run: a workspace (the run directory itself, or one given with `--workspace`) plus the
/// run-level projections `frames.json` and `locations.json` and the named outputs under the
/// run directory.
pub struct Run {
    /// The run directory, where the named outputs and the projections go.
    pub dir: PathBuf,
    /// The workspace artifacts and the journal live in; the run directory unless given.
    pub workspace: Arc<Workspace>,
}

impl Run {
    /// Opens a run whose workspace is the run directory.
    pub fn open(dir: &Path) -> Result<Run> {
        Ok(Run {
            dir: dir.to_path_buf(),
            workspace: Arc::new(Workspace::open(dir)?),
        })
    }

    /// Opens a run over an existing workspace.
    pub fn open_in(dir: &Path, workspace: Arc<Workspace>) -> Result<Run> {
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        Ok(Run {
            dir: dir.to_path_buf(),
            workspace,
        })
    }

    /// The frames the workspace knows now.
    pub fn frames(&self) -> Result<FrameTable> {
        self.workspace.frames()
    }

    /// Appends a note to the journal.
    pub fn note(&self, text: impl Into<String>) -> Result<()> {
        self.workspace.note(text)
    }

    /// Writes `frames.json` and `locations.json` from the workspace's snapshot.
    pub fn write_tables(&self) -> Result<()> {
        let snapshot = self.workspace.snapshot()?;
        for (name, value) in [
            (
                "frames.json",
                serde_json::to_value(FrameTable {
                    frames: snapshot.frames.clone(),
                }),
            ),
            ("locations.json", serde_json::to_value(&snapshot.locations)),
        ] {
            let path = self.dir.join(name);
            let value = value.map_err(|source| Error::Json {
                path: path.clone(),
                source,
            })?;
            let text = serde_json::to_string_pretty(&value).map_err(|source| Error::Json {
                path: path.clone(),
                source,
            })?;
            std::fs::write(&path, text + "\n").map_err(|e| Error::io(&path, e))?;
        }
        Ok(())
    }

    /// Runs one op against the workspace; see `Workspace::invoke`.
    pub fn invoke(
        &self,
        step_id: &str,
        op_name: &str,
        registry: &OpRegistry,
        inputs: &Inputs,
        params: &Params,
    ) -> Result<op::Output> {
        Ok(self
            .workspace
            .invoke(step_id, op_name, registry, inputs, params)?
            .output)
    }
}

impl Recipe {
    /// Runs the validated recipe with the given `inputs.*` artifacts (by declared name) in a
    /// run directory. Every step's outputs are stored and journaled; the first step that is
    /// not `ok` stops the run and publishes nothing of its own.
    pub fn run(
        &self,
        registry: &OpRegistry,
        inputs: BTreeMap<String, Artifact>,
        run: &Run,
    ) -> Result<Outcome> {
        for (name, decl) in &self.inputs {
            match inputs.get(name) {
                Some(a) if a.artifact_type() == decl.artifact_type => {}
                Some(a) => {
                    return Err(Error::usage(format!(
                        "input {name} must be a {}, got a {}",
                        decl.artifact_type,
                        a.artifact_type()
                    )));
                }
                None => {
                    return Err(Error::usage(format!(
                        "input {name} ({}) was not given",
                        decl.artifact_type
                    )));
                }
            }
        }
        let mut bindings: BTreeMap<String, Artifact> = BTreeMap::new();
        for (name, artifact) in inputs {
            bindings.insert(format!("inputs.{name}"), artifact);
        }
        let mut outcome = Outcome {
            status: Status::Ok,
            stopped_at: None,
            reason: None,
            bindings: BTreeMap::new(),
            outputs: BTreeMap::new(),
            renders: Vec::new(),
        };
        let mut unmet: BTreeMap<String, String> = BTreeMap::new(); // step id -> reason
        for step in &self.step {
            let mut step_inputs: Inputs = BTreeMap::new();
            let mut blocked: Option<String> = None;
            for (port, binding) in &step.inputs {
                match bindings.get(binding) {
                    Some(a) => {
                        step_inputs.insert(port.clone(), a.clone());
                    }
                    None => {
                        let source = binding.split('.').next().unwrap_or_default();
                        let why = unmet
                            .get(source)
                            .map(|r| format!("step {source} did not produce it ({r})"))
                            .unwrap_or_else(|| "it was never produced".to_string());
                        blocked = Some(format!("step {} requires {binding}, but {why}", step.id));
                        break;
                    }
                }
            }
            if let Some(reason) = blocked {
                run.note(reason.clone())?;
                if outcome.status == Status::Ok {
                    outcome.status = Status::NotApplicable;
                    outcome.stopped_at = Some(step.id.clone());
                    outcome.reason = Some(reason);
                } else {
                    outcome.reason = Some(format!(
                        "{}; {reason}",
                        outcome.reason.take().unwrap_or_default()
                    ));
                }
                break;
            }
            let mut params = step.params.clone();
            if let Some(constants) = &step.constants {
                params.insert(
                    "constants".to_string(),
                    toml::Value::String(constants.clone()),
                );
            }
            let output = run.invoke(&step.id, &step.op, registry, &step_inputs, &params)?;
            if output.envelope.status != Status::Ok {
                let reason = output.envelope.reason.clone().unwrap_or_default();
                unmet.insert(
                    step.id.clone(),
                    format!(
                        "{}: {reason}",
                        serde_json::to_string(&output.envelope.status)
                            .unwrap_or_default()
                            .trim_matches('"')
                    ),
                );
                if outcome.status == Status::Ok {
                    outcome.status = output.envelope.status;
                    outcome.stopped_at = Some(step.id.clone());
                    outcome.reason = Some(reason);
                }
                continue;
            }
            for (name, artifact) in output.outputs {
                bindings.insert(format!("{}.{name}", step.id), artifact);
            }
        }
        if outcome.status == Status::Ok {
            for (name, binding) in [
                ("mask", &self.outputs.mask),
                ("image", &self.outputs.image),
                ("plan", &self.outputs.plan),
            ] {
                if let Some(binding) = binding {
                    let artifact = bindings.get(binding).cloned().ok_or_else(|| {
                        self.fail("outputs", format!("{name} = {binding} was never produced"))
                    })?;
                    outcome.outputs.insert(name.to_string(), artifact);
                }
            }
            for render in &self.outputs.render {
                let mut render_inputs: Inputs = BTreeMap::new();
                if let Some(mask) = outcome.outputs.get("mask") {
                    render_inputs.insert("mask".to_string(), mask.clone());
                }
                if let Some(image) = outcome.outputs.get("image") {
                    render_inputs.insert("image".to_string(), image.clone());
                }
                let mut params = Params::new();
                params.insert(
                    "for".to_string(),
                    toml::Value::String(render.profile.clone()),
                );
                let step_id = format!("outputs.render.{}", render.profile);
                let output = run.invoke(&step_id, "render", registry, &render_inputs, &params)?;
                if let Some(Artifact::Image(image)) = output.outputs.get("image") {
                    let path = run.dir.join(&render.out);
                    crate::canonical::write_png(&path, &image.pixels)?;
                    outcome.renders.push((render.profile.clone(), path));
                }
            }
        }
        run.write_tables()?;
        outcome.bindings = bindings;
        Ok(outcome)
    }
}

/// Convenience for ops: a shared image.
pub fn image_artifact(image: Image) -> Artifact {
    Artifact::Image(Arc::new(image))
}

/// Convenience for ops: a shared mask.
pub fn mask_artifact(mask: Mask) -> Artifact {
    Artifact::Mask(Arc::new(mask))
}
