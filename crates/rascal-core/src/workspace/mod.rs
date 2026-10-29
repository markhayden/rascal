//! The workspace: a directory that holds every artifact by identity, a registry of generations
//! published by atomic rename, the operation records, and the journal (SPEC.md "Workspace").
//!
//! ```text
//! images/                 canonical PNGs by identity
//! artifacts/<identity>    every other artifact file or directory, named by identity
//! ops/<identity>.json     immutable operation records
//! registry/<n>.json       one registry generation
//! CURRENT                 the generation number, written by atomic rename
//! staging/<token>/        one directory per in-flight op, holding a `lock`
//! journal.jsonl           observations, appended after commit
//! lock                    the writer lock, held only across a commit
//! ```
//!
//! Readers read `CURRENT` once and then the named generation, so they see a consistent
//! snapshot. Ops stage in parallel and commit in turn.

pub mod commit;
pub mod record;
pub mod registry;
pub mod repair;
pub mod staging;
pub mod sync;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::artifact::{Artifact, ArtifactType};
use crate::canonical;
use crate::error::{Error, Result};
use crate::frame::{Frame, FrameTable};
use crate::image::{Image, ImageRecord, Role};
use crate::journal::{Event, Journal, JournalLine};
use crate::location::Location;
use crate::mask::Mask;
use crate::op::{self, Context, Envelope, Inputs, Params, Provenance, Status};
use crate::profile::BandProfile;
use crate::registry::OpRegistry;
use crate::series::Series;
use crate::workspace::commit::{Committed, Pending};
use crate::workspace::record::OperationRecord;
use crate::workspace::registry::{ArtifactEntry, Generation};
use crate::workspace::staging::Staging;
use crate::workspace::sync::{Fs, RealFs};

/// An open workspace.
pub struct Workspace {
    pub root: PathBuf,
    fs: Arc<dyn Fs>,
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

/// What `invoke` reports beside the op's output.
#[derive(Debug, Clone)]
pub struct Invocation {
    pub output: op::Output,
    /// Set when the op published: the record is durable.
    pub committed: Option<Committed>,
    /// The artifacts published, by output port, with their new local ids.
    pub ids: BTreeMap<String, String>,
    /// Cancellation was requested. Before the writer lock, nothing was published and
    /// `committed` is `None`; after it, the commit ran to completion and `committed` is set.
    pub cancelled: bool,
}

/// A cancellation request, checked once before the writer lock is taken.
pub type Cancel = std::sync::atomic::AtomicBool;

const DIRS: [&str; 5] = ["images", "artifacts", "ops", "registry", "staging"];

impl Workspace {
    /// Opens a workspace, creating the layout if the directory is new or empty. Abandoned
    /// staging directories (whose lock can be taken) are removed; a generation naming an
    /// artifact missing on disk refuses the workspace until `repair` has run.
    pub fn open(root: &Path) -> Result<Workspace> {
        Workspace::open_with(root, Arc::new(RealFs))
    }

    /// Opens without the missing-payload refusal, for repair.
    pub fn open_unchecked(root: &Path) -> Result<Workspace> {
        let fs: Arc<dyn Fs> = Arc::new(RealFs);
        for dir in DIRS {
            fs.create_dir_all(&root.join(dir))?;
        }
        Ok(Workspace {
            root: root.to_path_buf(),
            fs,
        })
    }

    pub(crate) fn fs_handle(&self) -> Arc<dyn Fs> {
        self.fs.clone()
    }

    /// Opens with a filesystem of the caller's choosing (tests inject a recording one).
    pub fn open_with(root: &Path, fs: Arc<dyn Fs>) -> Result<Workspace> {
        for dir in DIRS {
            fs.create_dir_all(&root.join(dir))?;
        }
        let ws = Workspace {
            root: root.to_path_buf(),
            fs,
        };
        ws.sweep_staging();
        if let Some(n) = ws.read_current()? {
            let generation = ws.read_generation(n)?;
            for entry in &generation.artifacts {
                if !ws.root.join(&entry.path).exists() {
                    return Err(Error::Op {
                        op: "workspace".to_string(),
                        message: format!(
                            "{}: generation {n} names {} ({}) but {} is missing; run `rascal workspace repair`",
                            ws.root.display(),
                            entry.id,
                            entry.identity,
                            entry.path
                        ),
                    });
                }
            }
        }
        Ok(ws)
    }

    /// Removes staging directories nobody owns.
    fn sweep_staging(&self) {
        let staging = self.root.join("staging");
        let Ok(entries) = std::fs::read_dir(&staging) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && staging::is_abandoned(&path) {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }

    /// The current generation number, if any generation has been published.
    pub fn read_current(&self) -> Result<Option<u64>> {
        let path = self.root.join("CURRENT");
        match std::fs::read_to_string(&path) {
            Ok(text) => text.trim().parse::<u64>().map(Some).map_err(|_| {
                Error::op(
                    "workspace",
                    format!("{}: CURRENT is not a number", path.display()),
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::io(&path, e)),
        }
    }

    /// Generation `n`.
    pub fn read_generation(&self, n: u64) -> Result<Generation> {
        let path = self.root.join("registry").join(format!("{n}.json"));
        let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        serde_json::from_str(&text).map_err(|source| Error::Json { path, source })
    }

    /// A consistent snapshot: the current generation, or the empty first one.
    pub fn snapshot(&self) -> Result<Generation> {
        match self.read_current()? {
            Some(n) => self.read_generation(n),
            None => Ok(Generation::first()),
        }
    }

    /// The frames the snapshot knows, as a table.
    pub fn frames(&self) -> Result<FrameTable> {
        Ok(FrameTable {
            frames: self.snapshot()?.frames,
        })
    }

    /// Every committed operation record, in commit order.
    pub fn records(&self) -> Result<Vec<OperationRecord>> {
        let generation = self.snapshot()?;
        generation
            .records
            .iter()
            .map(|entry| {
                let mut record = self.read_record(&entry.identity)?;
                record.seq = entry.seq;
                record.duration_ms = entry.duration_ms;
                Ok(record)
            })
            .collect()
    }

    /// One operation record by identity.
    pub fn read_record(&self, identity: &str) -> Result<OperationRecord> {
        let hex = identity.strip_prefix("sha256:").unwrap_or(identity);
        let path = self.root.join("ops").join(format!("{hex}.json"));
        let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        serde_json::from_str(&text).map_err(|source| Error::Json { path, source })
    }

    /// The journal.
    pub fn journal_path(&self) -> PathBuf {
        self.root.join("journal.jsonl")
    }

    /// Appends a note to the journal.
    pub fn note(&self, text: impl Into<String>) -> Result<()> {
        Journal::open(&self.journal_path())?.note(text).map(|_| ())
    }

    // --- loading artifacts --------------------------------------------------------------------

    /// Loads an artifact by local id or identity from the snapshot.
    pub fn load(&self, id_or_identity: &str) -> Result<Artifact> {
        let generation = self.snapshot()?;
        if let Some(plan) = generation.plan(id_or_identity) {
            return Ok(Artifact::Plan(Arc::new(plan.plan.clone())));
        }
        if let Some(location) = generation.location(id_or_identity) {
            return Ok(Artifact::Location {
                location: location.location.clone(),
                identity: crate::identity::Identity::parse_or_hash(&location.identity),
            });
        }
        let entry = generation.artifact(id_or_identity).ok_or_else(|| {
            Error::usage(format!(
                "{id_or_identity}: no such artifact in {}",
                self.root.display()
            ))
        })?;
        self.load_entry(entry)
    }

    fn load_entry(&self, entry: &ArtifactEntry) -> Result<Artifact> {
        let path = self.root.join(&entry.path);
        match entry.artifact_type {
            ArtifactType::Image => {
                let record_path = PathBuf::from(format!("{}.json", path.display()));
                let text = std::fs::read_to_string(&record_path)
                    .map_err(|e| Error::io(&record_path, e))?;
                let record: ImageRecord =
                    serde_json::from_str(&text).map_err(|source| Error::Json {
                        path: record_path,
                        source,
                    })?;
                let pixels = crate::image::decode_limited(&path)?;
                Ok(Artifact::Image(Arc::new(Image { record, pixels })))
            }
            ArtifactType::Mask => Ok(Artifact::Mask(Arc::new(Mask::read(&path)?))),
            ArtifactType::BandProfile => {
                Ok(Artifact::BandProfile(Arc::new(BandProfile::read(&path)?)))
            }
            ArtifactType::Series => Ok(Artifact::Series(Arc::new(Series::read(&path)?))),
            ArtifactType::Location | ArtifactType::Plan => Err(Error::usage(format!(
                "{}: a {} is a registry entry, not a file",
                entry.id, entry.artifact_type
            ))),
        }
    }

    // --- importing and adding -----------------------------------------------------------------

    /// Imports a job image as a canonical image in its raw frame and commits it as an
    /// `import` record. Returns the local id, the frame and the artifact. An image exactly 2:1
    /// without a projection is assumed equirectangular, and the journal says so.
    pub fn import(
        &self,
        path: &Path,
        projection: Option<crate::frame::Projection>,
    ) -> Result<(String, Frame, Artifact)> {
        let (image, frame, assumed) = Image::import(path, projection)?;
        if assumed {
            self.note(format!(
                "projection assumed equirectangular: {} is exactly 2:1 and --projection was not given",
                path.display()
            ))?;
        }
        let source = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let (id, artifact) = self.add_canonical(image, &frame, &source)?;
        Ok((id, frame, artifact))
    }

    /// Adds an already decoded canonical image and its raw frame as an `import` record.
    pub fn add_canonical(
        &self,
        image: Image,
        frame: &Frame,
        source: &str,
    ) -> Result<(String, Artifact)> {
        if image.record.frame != frame.id {
            return Err(Error::usage(format!(
                "the image is in frame {} but {} was given",
                image.record.frame, frame.id
            )));
        }
        self.add_external(
            image,
            frame,
            "import",
            serde_json::json!({ "source": source, "projection": frame.projection }),
        )
    }

    /// Adds an image that came from outside the op graph (an import, a filler's answer) as a
    /// committed record under `op_name`, with `params` saying where it came from; the frame
    /// is registered if new. Returns the id and the artifact.
    pub fn add_external(
        &self,
        image: Image,
        frame: &Frame,
        op_name: &str,
        params: serde_json::Value,
    ) -> Result<(String, Artifact)> {
        if image.record.frame != frame.id {
            return Err(Error::usage(format!(
                "the image is in frame {} but {} was given",
                image.record.frame, frame.id
            )));
        }
        let started = Instant::now();
        let artifact = Artifact::Image(Arc::new(image));
        let mut outputs = BTreeMap::new();
        outputs.insert("image".to_string(), artifact.clone());
        let mut envelope = Envelope::ok(format!("{op_name}: an image in {}", frame.id));
        envelope.artifacts.insert(
            "image".to_string(),
            op::ArtifactRef {
                artifact_type: ArtifactType::Image,
                id: "image".to_string(),
                identity: artifact.identity().as_str().to_string(),
            },
        );
        envelope.provenance = Provenance {
            tool: op_name.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            duration_ms: 0,
        };
        let mut record = OperationRecord::new(
            op_name,
            op_name,
            env!("CARGO_PKG_VERSION"),
            params,
            BTreeMap::new(),
            BTreeMap::new(),
            envelope,
            None,
            0,
        );
        let (_, ids) = self.publish(&outputs, vec![frame.clone()], &mut record, started)?;
        self.journal_record(&record)?;
        Ok((ids["image"].clone(), artifact))
    }

    /// Adds a location to the registry as a committed `locate` record and returns its id.
    pub fn add_location(&self, location: Location) -> Result<(String, Artifact)> {
        let frame = self
            .frames()?
            .get(location.frame())
            .cloned()
            .ok_or_else(|| {
                Error::usage(format!(
                    "frame {} is not in the workspace",
                    location.frame()
                ))
            })?;
        location.validate(&frame)?;
        let identity = location.identity(&frame.identity);
        let artifact = Artifact::Location {
            location: location.clone(),
            identity: identity.clone(),
        };
        let mut outputs = BTreeMap::new();
        outputs.insert("location".to_string(), artifact.clone());
        let mut envelope = Envelope::ok(format!("{} location in {}", location.kind(), frame.id));
        envelope.artifacts.insert(
            "location".to_string(),
            op::ArtifactRef {
                artifact_type: ArtifactType::Location,
                id: "location".to_string(),
                identity: identity.as_str().to_string(),
            },
        );
        envelope.provenance = Provenance {
            tool: "locate".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            duration_ms: 0,
        };
        let params = serde_json::to_value(&location).unwrap_or_default();
        let mut record = OperationRecord::new(
            "locate",
            "locate",
            env!("CARGO_PKG_VERSION"),
            params,
            BTreeMap::new(),
            BTreeMap::new(),
            envelope,
            None,
            0,
        );
        let (_, ids) = self.publish(&outputs, Vec::new(), &mut record, Instant::now())?;
        self.journal_record(&record)?;
        Ok((ids["location"].clone(), artifact))
    }

    // --- invoking ops -------------------------------------------------------------------------

    /// Runs one op against the workspace: validates inputs, resolves parameters, runs, checks
    /// the envelope, and when the status is `ok` stages and commits the outputs with an
    /// operation record. Every invocation appends one journal line, publishing or not,
    /// successful or not; a bad parameter is journaled as `invalid_args` and is a usage error.
    pub fn invoke(
        &self,
        step_id: &str,
        op_name: &str,
        registry: &OpRegistry,
        inputs: &Inputs,
        params: &Params,
    ) -> Result<Invocation> {
        self.invoke_with(step_id, op_name, registry, inputs, params, None)
    }

    /// `invoke` with a cancellation flag.
    #[allow(clippy::too_many_arguments)]
    pub fn invoke_with(
        &self,
        step_id: &str,
        op_name: &str,
        registry: &OpRegistry,
        inputs: &Inputs,
        params: &Params,
        cancel: Option<&Cancel>,
    ) -> Result<Invocation> {
        let op = registry.require(op_name)?;
        let manifest = op.manifest();
        op::validate_inputs(&manifest, inputs)?;
        let input_identities: BTreeMap<String, String> = inputs
            .iter()
            .map(|(k, a)| (k.clone(), a.identity().as_str().to_string()))
            .collect();
        // One span per op: its name, step and input identities; never a key or a pixel.
        let span = tracing::info_span!("op", op = %manifest.name, step = %step_id, inputs = %serde_json::to_string(&input_identities).unwrap_or_default());
        let _entered = span.enter();
        let started = Instant::now();
        let mut given = params.clone();
        let constants = given.remove("constants");
        let constants_name = constants
            .as_ref()
            .and_then(|c| c.as_str())
            .map(str::to_string);
        if let Some(name) = &constants_name
            && !manifest.constants.iter().any(|k| k == name)
        {
            let envelope = self.failed_envelope(&manifest, format!("no constant set {name}"), 0);
            self.journal_line(
                step_id,
                &manifest,
                params,
                input_identities,
                BTreeMap::new(),
                Some(envelope),
                0,
            )?;
            return Err(Error::usage(format!(
                "step {step_id}: {}: no constant set {name}",
                manifest.name
            )));
        }
        // The selected (or default) constant set supplies the defaults the step did not give.
        let selected = constants_name
            .clone()
            .or_else(|| manifest.constants.first().cloned());
        if let Some(name) = &selected {
            for (key, value) in op.constant_set(name) {
                if manifest.param(&key).is_some() {
                    given.entry(key).or_insert(value);
                }
            }
        }
        let resolved = match manifest.resolve_params(&given) {
            Ok(mut resolved) => {
                match constants {
                    Some(c) => {
                        resolved.insert("constants".to_string(), c);
                    }
                    None => {
                        if let Some(first) = manifest.constants.first() {
                            resolved.insert(
                                "constants".to_string(),
                                toml::Value::String(first.clone()),
                            );
                        }
                    }
                }
                resolved
            }
            Err(message) => {
                let envelope = self.failed_envelope(&manifest, message.clone(), 0);
                self.journal_line(
                    step_id,
                    &manifest,
                    params,
                    input_identities,
                    BTreeMap::new(),
                    Some(envelope),
                    0,
                )?;
                return Err(Error::usage(format!(
                    "step {step_id}: {}: {message}",
                    manifest.name
                )));
            }
        };
        let frames = self.frames()?;
        let context = Context { frames: &frames };
        let result = op.run(&context, inputs, &resolved).and_then(|output| {
            let bytes = serde_json::to_vec(&output.envelope)
                .map(|v| v.len() as u64)
                .unwrap_or(u64::MAX);
            if bytes > manifest.returns.max_envelope_bytes {
                return Err(Error::op(
                    &manifest.name,
                    format!(
                        "the envelope is {bytes} bytes, over the manifest's max_envelope_bytes {}",
                        manifest.returns.max_envelope_bytes
                    ),
                ));
            }
            Ok(output)
        });
        let duration_ms = started.elapsed().as_millis() as u64;
        tracing::debug!(op = %manifest.name, step = %step_id, duration_ms, ok = result.is_ok(), "op finished");
        let mut output = match result {
            Ok(output) => output,
            Err(error) => {
                let mut envelope = self.failed_envelope(&manifest, error.to_string(), duration_ms);
                envelope.summary = format!("failed: {error}");
                self.journal_line(
                    step_id,
                    &manifest,
                    &resolved,
                    input_identities,
                    BTreeMap::new(),
                    Some(envelope),
                    duration_ms,
                )?;
                return Err(error);
            }
        };
        output.envelope.provenance = Provenance {
            tool: manifest.name.clone(),
            version: manifest.version.clone(),
            duration_ms,
        };
        if let Err(m) = output.envelope.check(&manifest) {
            let error = Error::op(&manifest.name, format!("envelope rejected: {m}"));
            let mut envelope = self.failed_envelope(&manifest, error.to_string(), duration_ms);
            envelope.summary = format!("failed: {error}");
            self.journal_line(
                step_id,
                &manifest,
                &resolved,
                input_identities,
                BTreeMap::new(),
                Some(envelope),
                duration_ms,
            )?;
            return Err(error);
        }
        let output_identities: BTreeMap<String, String> = output
            .outputs
            .iter()
            .map(|(k, a)| (k.clone(), a.identity().as_str().to_string()))
            .collect();
        if output.envelope.status != Status::Ok {
            // Publishes nothing: no artifact, no registry change, no record. One journal line.
            self.journal_line(
                step_id,
                &manifest,
                &resolved,
                input_identities,
                BTreeMap::new(),
                Some(output.envelope.clone()),
                duration_ms,
            )?;
            if output.envelope.status == Status::InvalidArgs {
                return Err(Error::usage(format!(
                    "step {step_id}: {}: {}",
                    manifest.name,
                    output.envelope.reason.clone().unwrap_or_default()
                )));
            }
            return Ok(Invocation {
                output,
                committed: None,
                ids: BTreeMap::new(),
                cancelled: false,
            });
        }
        let mut record = OperationRecord::new(
            step_id,
            &manifest.name,
            &manifest.version,
            serde_json::to_value(&resolved).unwrap_or_default(),
            input_identities.clone(),
            output_identities.clone(),
            output.envelope.clone(),
            constants_name,
            duration_ms,
        );
        // New frames an op introduced (cut, window) are those its outputs are in and the
        // snapshot does not know.
        let new_frames: Vec<Frame> = output.frames.clone();
        let published =
            self.publish_cancellable(&output.outputs, new_frames, &mut record, started, cancel)?;
        let cancelled = cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::SeqCst));
        match published {
            Some((committed, ids)) => {
                self.journal_line(
                    step_id,
                    &manifest,
                    &resolved,
                    input_identities,
                    output_identities,
                    Some(output.envelope.clone()),
                    duration_ms,
                )?;
                Ok(Invocation {
                    output,
                    committed: Some(committed),
                    ids,
                    cancelled,
                })
            }
            None => {
                self.note(format!(
                    "step {step_id}: cancelled before the writer lock; nothing published"
                ))?;
                Ok(Invocation {
                    output,
                    committed: None,
                    ids: BTreeMap::new(),
                    cancelled: true,
                })
            }
        }
    }

    /// Journals a committed built-in record (import, locate).
    fn journal_record(&self, record: &OperationRecord) -> Result<()> {
        Journal::open(&self.journal_path())?
            .append(JournalLine {
                journal_version: 1,
                seq: 0,
                event: Event::Op,
                step: Some(record.step.clone()),
                op: Some(record.op.clone()),
                version: Some(record.version.clone()),
                params: Some(record.params.clone()),
                inputs: record.inputs.clone(),
                outputs: record
                    .envelope
                    .artifacts
                    .iter()
                    .map(|(k, a)| (k.clone(), a.identity.clone()))
                    .collect(),
                envelope: Some(record.envelope.clone()),
                duration_ms: Some(record.duration_ms),
                note: None,
            })
            .map(|_| ())
    }

    fn failed_envelope(
        &self,
        manifest: &op::Manifest,
        reason: String,
        duration_ms: u64,
    ) -> Envelope {
        let mut envelope = Envelope::invalid_args(reason);
        envelope.provenance = Provenance {
            tool: manifest.name.clone(),
            version: manifest.version.clone(),
            duration_ms,
        };
        envelope
    }

    #[allow(clippy::too_many_arguments)]
    fn journal_line(
        &self,
        step_id: &str,
        manifest: &op::Manifest,
        params: &Params,
        inputs: BTreeMap<String, String>,
        outputs: BTreeMap<String, String>,
        envelope: Option<Envelope>,
        duration_ms: u64,
    ) -> Result<()> {
        Journal::open(&self.journal_path())?
            .append(JournalLine {
                journal_version: 1,
                seq: 0,
                event: Event::Op,
                step: Some(step_id.to_string()),
                op: Some(manifest.name.clone()),
                version: Some(manifest.version.clone()),
                params: Some(serde_json::to_value(params).unwrap_or_default()),
                inputs,
                outputs,
                envelope,
                duration_ms: Some(duration_ms),
                note: None,
            })
            .map(|_| ())
    }

    /// Stages the outputs, commits them with the record, and returns the commit and the new
    /// local ids by output port.
    fn publish(
        &self,
        outputs: &BTreeMap<String, Artifact>,
        frames: Vec<Frame>,
        record: &mut OperationRecord,
        started: Instant,
    ) -> Result<(Committed, BTreeMap<String, String>)> {
        self.publish_cancellable(outputs, frames, record, started, None)?
            .ok_or_else(|| Error::usage("cancelled"))
    }

    /// `publish`, returning `None` when cancellation was requested before the writer lock.
    fn publish_cancellable(
        &self,
        outputs: &BTreeMap<String, Artifact>,
        frames: Vec<Frame>,
        record: &mut OperationRecord,
        started: Instant,
        cancel: Option<&Cancel>,
    ) -> Result<Option<(Committed, BTreeMap<String, String>)>> {
        let staging = Staging::create(&self.root.join("staging"), &staging::token())?;
        let fs = self.fs.clone();
        let result = (|| {
            let mut pending: Vec<Pending> = Vec::new();
            for (port, artifact) in outputs {
                let identity = artifact.identity();
                let base = Pending {
                    port: port.clone(),
                    prefix: "x",
                    artifact_type: artifact.artifact_type(),
                    identity: identity.as_str().to_string(),
                    frame: artifact.frame().to_string(),
                    staged: None,
                    sidecar: None,
                    location: None,
                    plan: None,
                };
                let item = match artifact {
                    Artifact::Image(image) => {
                        let dir = if image.record.role == Role::Canonical {
                            "images"
                        } else {
                            "artifacts"
                        };
                        let name = format!("{}.png", identity.hex());
                        image.write(&staging.dir)?;
                        Pending {
                            prefix: "img",
                            staged: Some((staging.path(&name), format!("{dir}/{name}"))),
                            sidecar: Some((
                                staging.path(&format!("{name}.json")),
                                format!("{dir}/{name}.json"),
                            )),
                            ..base
                        }
                    }
                    Artifact::Mask(mask) => {
                        let name = mask.dir_name();
                        mask.write(&staging.path(&name))?;
                        Pending {
                            prefix: "mask",
                            staged: Some((staging.path(&name), format!("artifacts/{name}"))),
                            ..base
                        }
                    }
                    Artifact::BandProfile(profile) => {
                        let name = format!("{}.profile.json", identity.hex());
                        profile.write(&staging.path(&name))?;
                        Pending {
                            prefix: "band",
                            staged: Some((staging.path(&name), format!("artifacts/{name}"))),
                            ..base
                        }
                    }
                    Artifact::Series(series) => {
                        let name = format!("{}.series.json", identity.hex());
                        series.write(&staging.path(&name))?;
                        Pending {
                            prefix: "series",
                            staged: Some((staging.path(&name), format!("artifacts/{name}"))),
                            ..base
                        }
                    }
                    Artifact::Location { location, .. } => Pending {
                        prefix: "loc",
                        location: Some(location.clone()),
                        ..base
                    },
                    Artifact::Plan(plan) => Pending {
                        prefix: "plan",
                        plan: Some((**plan).clone()),
                        ..base
                    },
                };
                pending.push(item);
            }
            record.duration_ms = started.elapsed().as_millis() as u64;
            commit::commit(
                self,
                fs.as_ref(),
                &staging.dir,
                &pending,
                &frames,
                record,
                cancel,
            )
        })();
        match result {
            Ok(Some(done)) => Ok(Some(done)),
            Ok(None) => {
                staging.discard();
                Ok(None)
            }
            Err(e) => {
                staging.discard();
                Err(e)
            }
        }
    }

    /// Lists the snapshot: `(kind, id, identity, frame)` for every frame, location and artifact.
    pub fn list(&self) -> Result<Vec<(String, String, String, String)>> {
        let generation = self.snapshot()?;
        let mut rows = Vec::new();
        for f in &generation.frames {
            rows.push((
                "Frame".to_string(),
                f.id.clone(),
                f.identity.clone(),
                f.id.clone(),
            ));
        }
        for l in &generation.locations {
            rows.push((
                "Location".to_string(),
                l.id.clone(),
                l.identity.clone(),
                l.location.frame().to_string(),
            ));
        }
        for p in &generation.plans {
            rows.push((
                "Plan".to_string(),
                p.id.clone(),
                p.identity.clone(),
                p.plan.parent.clone(),
            ));
        }
        for a in &generation.artifacts {
            if a.id.is_empty() {
                continue;
            }
            rows.push((
                a.artifact_type.to_string(),
                a.id.clone(),
                a.identity.clone(),
                a.frame.clone(),
            ));
        }
        Ok(rows)
    }

    /// The identity of a serializable value, for callers that need one for a note.
    pub fn identity_of<T: serde::Serialize>(kind: &str, value: &T) -> crate::identity::Identity {
        canonical::identity_of(kind, value)
    }
}
