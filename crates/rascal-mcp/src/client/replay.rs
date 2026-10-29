//! A recorded run's pack calls, for `--replay` (G4): the manifests each pack listed and every
//! successful call with its outputs, read from the run's journal notes. A replayed run starts
//! no pack and makes no call; a call the journal does not record is an error naming it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::journal::Journal;
use rascal_core::op::Envelope;
use rascal_core::pack::{CompatManifest, PackAnnotation};
use rascal_core::workspace::Workspace;
use serde::{Deserialize, Serialize};

use super::PackTool;

/// The journal note prefix of a tool's manifests, written when a pack is started.
pub const MANIFEST_NOTE: &str = "pack manifest: ";
/// The journal note prefix of a stored call: the request identity and the outputs.
pub const STORED_NOTE: &str = "pack stored: ";

/// What a pack listed for one tool, as journaled at startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredManifest {
    pub pack: String,
    #[serde(default)]
    pub revision: Option<String>,
    pub manifest: CompatManifest,
    pub rascal: PackAnnotation,
}

/// A successful call: the request's identity, the pack that answered (name and revision), the
/// tool, the envelope as validated, and the published outputs by name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCall {
    pub request: String,
    pub pack: String,
    #[serde(default)]
    pub revision: Option<String>,
    pub tool: String,
    pub envelope: Envelope,
    pub outputs: BTreeMap<String, StoredOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredOutput {
    pub artifact_type: ArtifactType,
    pub identity: String,
}

/// A recorded run: its workspace and what its journal says about packs.
pub struct Replay {
    dir: PathBuf,
    workspace: Workspace,
    manifests: Vec<StoredManifest>,
    calls: Vec<StoredCall>,
}

impl Replay {
    /// Reads a run directory or workspace; its journal must exist.
    pub fn open(dir: &Path) -> Result<Replay> {
        let journal = dir.join("journal.jsonl");
        if !journal.is_file() {
            return Err(Error::usage(format!(
                "--replay {}: no journal there; give the run directory, or the workspace when the run used --workspace",
                dir.display()
            )));
        }
        let mut manifests = Vec::new();
        let mut calls = Vec::new();
        for line in Journal::read(&journal)? {
            let Some(note) = line.note.as_deref() else {
                continue;
            };
            if let Some(text) = note.strip_prefix(MANIFEST_NOTE) {
                manifests.push(serde_json::from_str::<StoredManifest>(text).map_err(|e| {
                    Error::op(
                        "replay",
                        format!(
                            "{}: journal line {}: a pack manifest note does not parse: {e}",
                            journal.display(),
                            line.seq
                        ),
                    )
                })?);
            } else if let Some(text) = note.strip_prefix(STORED_NOTE) {
                calls.push(serde_json::from_str::<StoredCall>(text).map_err(|e| {
                    Error::op(
                        "replay",
                        format!(
                            "{}: journal line {}: a stored call note does not parse: {e}",
                            journal.display(),
                            line.seq
                        ),
                    )
                })?);
            }
        }
        Ok(Replay {
            dir: dir.to_path_buf(),
            workspace: Workspace::open(dir)?,
            manifests,
            calls,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The tools a pack listed in the recorded run, as the registry needs them; the last
    /// listing per tool wins. `None` when the run never started the pack.
    pub fn tools_for(&self, pack: &str) -> Result<Option<Vec<PackTool>>> {
        let mut by_name: BTreeMap<String, PackTool> = BTreeMap::new();
        for stored in self.manifests.iter().filter(|m| m.pack == pack) {
            let manifest = stored
                .manifest
                .with_annotation(&stored.rascal)
                .map_err(|e| {
                    Error::op(
                        pack,
                        format!(
                            "--replay {}: a recorded manifest is rejected: {e}",
                            self.dir.display()
                        ),
                    )
                })?;
            by_name.insert(
                manifest.name.clone(),
                PackTool {
                    manifest,
                    compat: stored.manifest.clone(),
                    annotation: stored.rascal.clone(),
                },
            );
        }
        Ok((!by_name.is_empty()).then(|| by_name.into_values().collect()))
    }

    /// The recorded call for a request by this pack (name and revision) and tool, when there
    /// is one whose outputs the recorded run holds; the last such wins. A stored note whose
    /// outputs were never committed (the run failed after the call) is passed over.
    pub fn find(
        &self,
        request: &str,
        pack: &str,
        revision: Option<&str>,
        tool: &str,
    ) -> Result<Option<StoredCall>> {
        for stored in self.calls.iter().rev().filter(|c| {
            c.request == request
                && c.pack == pack
                && c.revision.as_deref() == revision
                && c.tool == tool
        }) {
            let committed = stored
                .outputs
                .values()
                .all(|o| self.workspace.load(&o.identity).is_ok());
            if committed {
                return Ok(Some(stored.clone()));
            }
            tracing::warn!(
                request,
                tool,
                "a recorded call's outputs are not in the recorded run; passing it over"
            );
        }
        Ok(None)
    }

    /// The recorded outputs, loaded from the recorded run's workspace by identity.
    pub fn outputs(&self, stored: &StoredCall) -> Result<BTreeMap<String, Artifact>> {
        let mut outputs = BTreeMap::new();
        for (name, output) in &stored.outputs {
            let artifact = self.workspace.load(&output.identity).map_err(|e| {
                Error::op(
                    &stored.tool,
                    format!(
                        "--replay {}: recorded output {name} ({}) cannot be loaded: {e}",
                        self.dir.display(),
                        output.identity
                    ),
                )
            })?;
            if artifact.artifact_type() != output.artifact_type {
                return Err(Error::op(
                    &stored.tool,
                    format!(
                        "--replay {}: recorded output {name} is a {}, not a {}",
                        self.dir.display(),
                        artifact.artifact_type(),
                        output.artifact_type
                    ),
                ));
            }
            outputs.insert(name.clone(), artifact);
        }
        Ok(outputs)
    }
}
