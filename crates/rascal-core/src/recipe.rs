//! Recipes: TOML step lists with typed bindings, validated before anything runs, and the
//! runner that executes one against a run directory, writing artifacts and the journal.
//!
//! Task 1.1 binds `Image`, `Location` and `Mask`, resolves no defaults and writes under
//! `--out` directly. REPLACED BY 1.6 (the recipe schema, defaults resolved at validation,
//! requirements, every error path) and 1.7a (the workspace commit protocol).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde::Deserialize;

use crate::artifact::{Artifact, ArtifactType};
use crate::error::{Error, Result};
use crate::frame::FrameTable;
use crate::image::Image;
use crate::journal::{Event, Journal, JournalLine};
use crate::location::LocationEntry;
use crate::mask::{Mask, UseCase};
use crate::op::{self, Context, Envelope, Inputs, Params, Status};
use crate::registry::OpRegistry;

/// A declared recipe input.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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
    pub params: Params,
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
            params: table,
        })
    }
}

/// A rendering the recipe writes under `--out`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderOut {
    #[serde(rename = "for")]
    pub profile: String,
    pub out: String,
}

/// The contract a recipe's consumer reads, by output name.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
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

/// A recipe file. REPLACED BY 1.6 (its schema in `schemas/rascal/recipe-v1.json`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
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
            for name in step.params.keys() {
                if !manifest.params.iter().any(|p| &p.name == name) {
                    return Err(
                        self.fail(&step.id, format!("op {} has no parameter {name}", step.op))
                    );
                }
            }
            for param in manifest.params.iter().filter(|p| p.required) {
                if !step.params.contains_key(&param.name) {
                    return Err(self.fail(
                        &step.id,
                        format!("parameter {} is required by {}", param.name, step.op),
                    ));
                }
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
            for r in &self.outputs.render {
                if r.out.is_empty() || r.out.contains('/') || r.out.contains("..") {
                    return Err(self.fail(
                        "outputs",
                        format!("render out '{}' must be a plain file name", r.out),
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
    /// Every binding produced, `inputs.*` included.
    pub bindings: BTreeMap<String, Artifact>,
    /// The named outputs: `mask`, `image`, `plan`.
    pub outputs: BTreeMap<String, Artifact>,
    /// Files written for `outputs.render`, by profile.
    pub renders: Vec<(String, PathBuf)>,
}

/// A run directory under construction: `inputs/`, `frames.json`, `locations.json`,
/// `artifacts/`, the named outputs and `journal.jsonl`. REPLACED BY 1.7a (the workspace).
pub struct Run {
    pub dir: PathBuf,
    pub frames: FrameTable,
    pub locations: Vec<LocationEntry>,
    journal: Journal,
}

impl Run {
    /// Opens a prepared run directory.
    pub fn open(dir: &Path) -> Result<Run> {
        Ok(Run {
            dir: dir.to_path_buf(),
            frames: FrameTable::default(),
            locations: Vec::new(),
            journal: Journal::open(&dir.join("journal.jsonl"))?,
        })
    }

    /// Appends a note to the journal.
    pub fn note(&mut self, text: impl Into<String>) -> Result<()> {
        self.journal.note(text).map(|_| ())
    }

    /// Stores an artifact where its kind lives: canonical images under `inputs/`, other
    /// images and masks under `artifacts/`, locations in `locations.json`.
    pub fn store(&mut self, id: &str, artifact: &Artifact) -> Result<()> {
        match artifact {
            Artifact::Image(image) if image.record.role == crate::image::Role::Canonical => {
                image.write(&self.dir.join("inputs")).map(|_| ())
            }
            Artifact::Image(image) => image.write(&self.dir.join("artifacts")).map(|_| ()),
            Artifact::Mask(mask) => mask.write(&self.dir.join("artifacts").join(mask.dir_name())),
            Artifact::Location { location, identity } => {
                if !self
                    .locations
                    .iter()
                    .any(|l| l.identity == identity.as_str())
                {
                    self.locations.push(LocationEntry {
                        id: id.to_string(),
                        identity: identity.as_str().to_string(),
                        location: location.clone(),
                    });
                }
                Ok(())
            }
        }
    }

    /// Writes `frames.json` and `locations.json`.
    pub fn write_tables(&self) -> Result<()> {
        for (name, value) in [
            ("frames.json", serde_json::to_value(&self.frames)),
            ("locations.json", serde_json::to_value(&self.locations)),
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

    /// Runs one op, journals the invocation whatever happens, and returns its output.
    pub fn invoke(
        &mut self,
        step_id: &str,
        op_name: &str,
        registry: &OpRegistry,
        inputs: &Inputs,
        params: &Params,
    ) -> Result<op::Output> {
        let op = registry
            .get(op_name)
            .ok_or_else(|| Error::usage(format!("unknown op {op_name}")))?;
        let manifest = op.manifest();
        op::validate_inputs(&manifest, inputs, params)?;
        let started = Instant::now();
        let context = Context {
            frames: &self.frames,
        };
        let result = op.run(&context, inputs, params);
        let duration_ms = started.elapsed().as_millis() as u64;
        let input_identities: BTreeMap<String, String> = inputs
            .iter()
            .map(|(k, a)| (k.clone(), a.identity().as_str().to_string()))
            .collect();
        let (envelope, outputs) = match &result {
            Ok(output) => (
                Some(output.envelope.clone()),
                output
                    .outputs
                    .iter()
                    .map(|(k, a)| (k.clone(), a.identity().as_str().to_string()))
                    .collect(),
            ),
            Err(error) => (
                Some(Envelope {
                    status: Status::InvalidArgs,
                    summary: format!("failed: {error}"),
                    reason: Some(error.to_string()),
                    artifacts: BTreeMap::new(),
                }),
                BTreeMap::new(),
            ),
        };
        self.journal.append(JournalLine {
            journal_version: 1,
            seq: 0,
            event: Event::Op,
            step: Some(step_id.to_string()),
            op: Some(manifest.name.clone()),
            version: Some(manifest.version.clone()),
            params: Some(serde_json::to_value(params).unwrap_or_default()),
            inputs: input_identities,
            outputs,
            envelope,
            duration_ms: Some(duration_ms),
            note: None,
        })?;
        result
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
        run: &mut Run,
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
            let id = format!("inputs.{name}");
            run.store(&id, &artifact)?;
            bindings.insert(id, artifact);
        }
        let mut outcome = Outcome {
            status: Status::Ok,
            stopped_at: None,
            bindings: BTreeMap::new(),
            outputs: BTreeMap::new(),
            renders: Vec::new(),
        };
        for step in &self.step {
            let step_inputs: Inputs = step
                .inputs
                .iter()
                .map(|(port, binding)| {
                    bindings
                        .get(binding)
                        .cloned()
                        .map(|a| (port.clone(), a))
                        .ok_or_else(|| {
                            self.fail(&step.id, format!("binding {binding} is not available"))
                        })
                })
                .collect::<Result<_>>()?;
            let output = run.invoke(&step.id, &step.op, registry, &step_inputs, &step.params)?;
            if output.envelope.status != Status::Ok {
                outcome.status = output.envelope.status;
                outcome.stopped_at = Some(step.id.clone());
                break;
            }
            for (name, artifact) in output.outputs {
                let id = format!("{}.{name}", step.id);
                run.store(&id, &artifact)?;
                bindings.insert(id, artifact);
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
