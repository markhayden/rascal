//! A pack tool as an op in the registry. `run` builds `_harness` from the bound images and the
//! frames they are in, calls the pack (or answers from a recorded run), journals the call and
//! the pack's stderr, validates the answer, and turns its regions and images into artifacts on
//! the tool's `many` output ports: `region_0`, `region_1`, … and `plane_0`, `plane_1`, …, or
//! whatever the annotation names them.

use std::collections::BTreeMap;
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::canonical;
use rascal_core::config::Accepts;
use rascal_core::error::Result;
use rascal_core::frame::Frame;
use rascal_core::image::{Image, Role};
use rascal_core::op::{ArtifactRef, Context, Inputs, Manifest, Op, Output, Params, Port};
use rascal_core::pack::{ContextImage, HARNESS, PackContext};
use rascal_core::workspace::Workspace;

use super::replay::{MANIFEST_NOTE, STORED_NOTE, StoredCall, StoredManifest, StoredOutput};
use super::validate::validate;
use super::{Pack, PackTool, STDERR_NOTE};

/// The most stderr lines journaled per call; the rest are counted in one line.
const STDERR_PER_CALL: usize = 32;

/// One tool of a pack, registered as an op. Calls are journaled into `workspace`.
pub struct PackOp {
    pack: Arc<Pack>,
    tool: PackTool,
    workspace: Arc<Workspace>,
}

/// Every tool of a pack as ops, journaling into `workspace`; the listing is journaled too, so
/// a replay knows the manifests without the pack.
pub fn ops_for(pack: &Arc<Pack>, workspace: &Arc<Workspace>) -> Result<Vec<Box<dyn Op>>> {
    let tools = pack.tools()?;
    if pack.replay().is_none() {
        for tool in &tools {
            workspace.note(format!(
                "{MANIFEST_NOTE}{}",
                serde_json::to_string(&StoredManifest {
                    pack: pack.name().to_string(),
                    revision: pack.config().revision.clone(),
                    manifest: tool.compat.clone(),
                    rascal: tool.annotation.clone(),
                })
                .unwrap_or_default()
            ))?;
        }
    }
    Ok(tools
        .into_iter()
        .map(|tool| {
            Box::new(PackOp {
                pack: pack.clone(),
                tool,
                workspace: workspace.clone(),
            }) as Box<dyn Op>
        })
        .collect())
}

impl PackOp {
    /// The `_harness` context and the frames it names, from the bound inputs.
    fn context(
        &self,
        context: &Context<'_>,
        inputs: &Inputs,
        call_id: &str,
    ) -> Result<(PackContext, BTreeMap<String, Frame>)> {
        let name = &self.tool.manifest.name;
        let mut frames: BTreeMap<String, Frame> = BTreeMap::new();
        let mut images = Vec::new();
        for port in &self.tool.manifest.inputs {
            let Some(Artifact::Image(image)) = inputs.get(&port.name) else {
                continue;
            };
            for frame in context.frames.chain(&image.record.frame)? {
                frames.insert(frame.id.clone(), frame.clone());
            }
            let (width, height) = image.dimensions();
            let (path, base64) = match self.pack.config().accepts {
                Accepts::Path => {
                    let path = self.workspace.image_path(image);
                    if !path.is_file() {
                        return Err(Error::op(
                            name,
                            format!(
                                "input {} ({}) is not on disk at {}; a pack that accepts paths takes committed images",
                                port.name,
                                image.record.id,
                                path.display()
                            ),
                        ));
                    }
                    let absolute = std::fs::canonicalize(&path).map_err(|e| Error::io(&path, e))?;
                    (Some(absolute.display().to_string()), None)
                }
                Accepts::Base64 => {
                    let mut bytes = Vec::new();
                    image
                        .pixels
                        .write_to(
                            &mut std::io::Cursor::new(&mut bytes),
                            image::ImageFormat::Png,
                        )
                        .map_err(|source| Error::Image {
                            path: image.record.file.clone().into(),
                            source,
                        })?;
                    (None, Some(rascal_core::panel::base64(&bytes)))
                }
            };
            images.push(ContextImage {
                port: port.name.clone(),
                id: image.record.id.clone(),
                identity: image.record.identity.clone(),
                frame: image.record.frame.clone(),
                width,
                height,
                path,
                base64,
                record: image.record.clone(),
            });
        }
        let context = PackContext {
            context_version: 1,
            run_id: self.workspace.root.display().to_string(),
            call_id: call_id.to_string(),
            images,
            frames: frames.values().cloned().collect(),
        };
        Ok((context, frames))
    }

    /// The arguments without `_harness`: the parameters as JSON, an image port's id, a
    /// location port's location.
    fn arguments(
        &self,
        inputs: &Inputs,
        params: &Params,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let name = &self.tool.manifest.name;
        let mut args = serde_json::Map::new();
        for (key, value) in params {
            let json = serde_json::to_value(value)
                .map_err(|e| Error::op(name, format!("parameter {key} does not serialize: {e}")))?;
            args.insert(key.clone(), json);
        }
        for port in &self.tool.manifest.inputs {
            let Some(artifact) = inputs.get(&port.name) else {
                continue;
            };
            let value = match artifact {
                Artifact::Image(image) => serde_json::Value::String(image.record.id.clone()),
                Artifact::Location { location, .. } => serde_json::to_value(location)
                    .map_err(|e| Error::op(name, format!("location {}: {e}", port.name)))?,
                other => {
                    return Err(Error::op(
                        name,
                        format!(
                            "input {} is a {}; a pack tool takes Image and Location inputs",
                            port.name,
                            other.artifact_type()
                        ),
                    ));
                }
            };
            args.insert(port.name.clone(), value);
        }
        Ok(args)
    }

    /// The many port of a type, or the single one, for the envelope's regions or images.
    fn port_for(&self, artifact_type: ArtifactType) -> Option<&Port> {
        let outputs = &self.tool.manifest.outputs;
        outputs
            .iter()
            .find(|p| p.many && p.artifact_type == artifact_type)
            .or_else(|| outputs.iter().find(|p| p.artifact_type == artifact_type))
    }

    fn journal_stderr(&self) -> Result<()> {
        let lines = self.pack.take_stderr();
        let shown = lines.len().min(STDERR_PER_CALL);
        for line in &lines[..shown] {
            self.workspace
                .note(format!("{STDERR_NOTE}{}: {line}", self.pack.name()))?;
        }
        if lines.len() > shown {
            self.workspace.note(format!(
                "{STDERR_NOTE}{}: [{} more lines]",
                self.pack.name(),
                lines.len() - shown
            ))?;
        }
        Ok(())
    }
}

impl Op for PackOp {
    fn manifest(&self) -> Manifest {
        self.tool.manifest.clone()
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let name = self.tool.manifest.name.clone();
        let pack_name = self.pack.name().to_string();
        let call_id = format!("{name}-{}", self.pack.next_call());
        let (harness, frames) = self.context(context, inputs, &call_id)?;
        let mut arguments = self.arguments(inputs, params)?;
        let input_identities: BTreeMap<&str, String> = inputs
            .iter()
            .map(|(k, a)| (k.as_str(), a.identity().as_str().to_string()))
            .collect();
        let request = canonical::identity_of(
            "pack-request",
            &serde_json::json!({
                "pack": pack_name,
                "tool": name,
                "arguments": arguments,
                "inputs": input_identities,
            }),
        )
        .as_str()
        .to_string();
        let revision = self.pack.config().revision.clone();

        // A recorded run answers instead of the pack.
        if let Some(replay) = self.pack.replay() {
            let stored = replay
                .find(&request, &pack_name, revision.as_deref(), &name)
                .ok_or_else(|| {
                    Error::op(
                        &name,
                        format!(
                            "--replay {}: no recorded call of packs.{pack_name} {name} for request {request}; run without --replay to call the pack",
                            replay.dir().display()
                        ),
                    )
                })?;
            let outputs = replay.outputs(stored)?;
            self.workspace.note(format!(
                "pack call replayed from the recorded call of packs.{pack_name} {name} for request {request}; the pack was not started"
            ))?;
            let mut envelope = stored.envelope.clone();
            envelope.artifacts = refs(&outputs);
            return Ok(Output {
                envelope,
                outputs,
                frames: Vec::new(),
            });
        }

        // The call, with its span and its journal lines.
        arguments.insert(
            HARNESS.to_string(),
            serde_json::to_value(&harness)
                .map_err(|e| Error::op(&name, format!("the harness context: {e}")))?,
        );
        let request_bytes = serde_json::to_vec(&arguments)
            .map(|v| v.len() as u64)
            .unwrap_or(0);
        let span = tracing::info_span!(
            "pack",
            pack = %pack_name,
            tool = %name,
            call = %call_id,
            request_bytes
        );
        let outcome = span.in_scope(|| self.pack.call(&name, arguments));
        span.in_scope(|| {
            tracing::info!(status = %outcome.call.status, duration_ms = outcome.call.duration_ms, response_bytes = outcome.call.response_bytes, "pack call")
        });
        self.workspace.note(outcome.call.note())?;
        self.journal_stderr()?;
        let result = outcome.result?;

        // Validation, then the artifacts.
        let borrowed: BTreeMap<String, &Frame> =
            frames.iter().map(|(k, v)| (k.clone(), v)).collect();
        let validated = validate(&self.tool.manifest, &result, &borrowed)?;
        let mut envelope = validated.envelope;
        let mut outputs: BTreeMap<String, Artifact> = BTreeMap::new();
        if !envelope.regions.is_empty() {
            let port = self.port_for(ArtifactType::Location).ok_or_else(|| {
                Error::op(
                    &name,
                    "the answer has regions but the tool declares no Location output",
                )
            })?;
            if !port.many && envelope.regions.len() != 1 {
                return Err(Error::op(
                    &name,
                    format!(
                        "output {} takes one region, the answer has {}",
                        port.name,
                        envelope.regions.len()
                    ),
                ));
            }
            for (i, region) in envelope.regions.iter().enumerate() {
                let frame = &frames[region.location.frame()];
                let output = if port.many {
                    port.nth(i)
                } else {
                    port.name.clone()
                };
                outputs.insert(
                    output,
                    Artifact::Location {
                        location: region.location.clone(),
                        identity: region.location.identity(&frame.identity),
                    },
                );
            }
        }
        if !envelope.images.is_empty() {
            let port = self.port_for(ArtifactType::Image).ok_or_else(|| {
                Error::op(
                    &name,
                    "the answer has images but the tool declares no Image output",
                )
            })?;
            if !port.many && envelope.images.len() != 1 {
                return Err(Error::op(
                    &name,
                    format!(
                        "output {} takes one image, the answer has {}",
                        port.name,
                        envelope.images.len()
                    ),
                ));
            }
            for (i, pixels) in validated.images.into_iter().enumerate() {
                let listed = &mut envelope.images[i];
                let frame = &frames[&listed.frame];
                let output = if port.many {
                    port.nth(i)
                } else {
                    port.name.clone()
                };
                let image = Image::new(&output, pixels, frame, Role::Plane, None);
                listed.identity = image.record.identity.clone();
                outputs.insert(output, Artifact::Image(Arc::new(image)));
            }
        }
        envelope.artifacts = refs(&outputs);
        self.workspace.note(format!(
            "{STORED_NOTE}{}",
            serde_json::to_string(&StoredCall {
                request,
                pack: pack_name,
                revision,
                tool: name,
                envelope: envelope.clone(),
                outputs: outputs
                    .iter()
                    .map(|(k, a)| {
                        (
                            k.clone(),
                            StoredOutput {
                                artifact_type: a.artifact_type(),
                                identity: a.identity().as_str().to_string(),
                            },
                        )
                    })
                    .collect(),
            })
            .unwrap_or_default()
        ))?;
        Ok(Output {
            envelope,
            outputs,
            frames: Vec::new(),
        })
    }
}

/// The envelope's artifact table for the outputs.
fn refs(outputs: &BTreeMap<String, Artifact>) -> BTreeMap<String, ArtifactRef> {
    outputs
        .iter()
        .map(|(name, a)| {
            (
                name.clone(),
                ArtifactRef {
                    artifact_type: a.artifact_type(),
                    id: name.clone(),
                    identity: a.identity().as_str().to_string(),
                },
            )
        })
        .collect()
}
