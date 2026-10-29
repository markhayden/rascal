//! The server behind `rascal serve` (SPEC.md "The MCP server and client"): one tool per op
//! from the registry with ordinary arguments (artifact ids and parameters), the workspace
//! tools, the envelope as structured content with image blocks in exact correspondence, errors
//! mapped, calls serial per workspace by default. Nothing is written to stdout but the
//! protocol; tracing goes to stderr.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::artifact::Artifact;
use rascal_core::error::Result;
use rascal_core::image::Image;
use rascal_core::op::{Inputs, Param, ParamKind, Params};
use rascal_core::registry::OpRegistry;
use rascal_core::workspace::Workspace;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ErrorData, InitializeResult,
    ListToolsResult, PaginatedRequestParams, ProtocolVersion, ServerCapabilities, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use tokio::sync::Semaphore;

use crate::client::Pack;
use crate::jsonrpc::{self, image_blocks_of, invalid, operational};
use crate::transport;

/// Writes the run page into a workspace and returns its path.
pub type DashWriter = Arc<dyn Fn(&std::path::Path) -> Result<PathBuf> + Send + Sync>;

/// The workspace tools, with their descriptions.
pub const WORKSPACE_TOOLS: [(&str, &str); 5] = [
    (
        "workspace_import",
        "Import an image (a path, or base64 PNG/JPEG bytes) as a canonical Image in its raw frame; returns its id and frame.",
    ),
    (
        "workspace_list",
        "Every frame, location and artifact in the current generation, optionally one type.",
    ),
    (
        "workspace_describe",
        "Any artifact's record as JSON, by id or identity.",
    ),
    (
        "workspace_read",
        "An Image artifact's PNG bytes as one image content block; any other type is invalid_args.",
    ),
    (
        "workspace_dash",
        "Write the run page into the workspace and return its path.",
    ),
];

/// How many tools a `tools/list` page holds.
pub const PAGE: usize = 32;

/// The server.
#[derive(Clone)]
pub struct Server {
    inner: Arc<Inner>,
}

struct Inner {
    workspace: Arc<Workspace>,
    registry: OpRegistry,
    /// Calls run under this permit: one by default, `--concurrency n` for more.
    permits: Arc<Semaphore>,
    /// The pack behind each pack op, by tool name: a client's cancellation of a call fails
    /// that pack's in-flight calls and shuts it down, and no other's.
    packs: BTreeMap<String, Arc<Pack>>,
    /// The largest image `workspace_read` returns.
    max_image_bytes: u64,
    /// A hook for the dash writer, supplied by the binary (the page template lives there).
    dash: Option<DashWriter>,
}

impl Server {
    /// A server over a workspace: the registry's ops and the five workspace tools, `dash`
    /// writing the run page (the binary supplies it; the page template lives there), and the
    /// packs whose tools the registry holds.
    pub fn new(
        workspace: Arc<Workspace>,
        registry: OpRegistry,
        concurrency: usize,
        dash: Option<DashWriter>,
        packs: &[Arc<Pack>],
    ) -> Result<Server> {
        let mut by_tool = BTreeMap::new();
        for pack in packs {
            for tool in pack.tools()? {
                by_tool.insert(tool.manifest.name, pack.clone());
            }
        }
        Ok(Server {
            inner: Arc::new(Inner {
                workspace,
                registry,
                permits: Arc::new(Semaphore::new(concurrency.max(1))),
                packs: by_tool,
                max_image_bytes: 64 << 20,
                dash,
            }),
        })
    }

    /// The pack a tool belongs to, when it is a pack's.
    pub fn pack_for(&self, tool: &str) -> Option<&Arc<Pack>> {
        self.inner.packs.get(tool)
    }

    /// Every tool, in a stable order: the ops by name, then the workspace tools.
    pub fn tools(&self) -> Vec<Tool> {
        let mut tools: Vec<Tool> = self
            .inner
            .registry
            .manifests()
            .into_iter()
            .map(|m| {
                let schema = tool_schema(&m);
                let description = format!(
                    "{} When to use: {} When not to use: {} Output: {}",
                    m.purpose, m.when_to_use, m.when_not_to_use, m.output_guidance
                );
                Tool::new(m.name.clone(), description, Arc::new(schema))
            })
            .collect();
        for (name, description) in WORKSPACE_TOOLS {
            tools.push(Tool::new(
                name,
                description,
                Arc::new(workspace_tool_schema(name)),
            ));
        }
        tools
    }

    /// Serves over stdio until the client goes away.
    pub async fn serve_stdio(self) -> Result<()> {
        use rmcp::ServiceExt;
        // One request per line; the limit is what a pack answer may carry, so an import of a
        // large image as base64 is not dropped by the SDK's default (16 MiB).
        let (stdin, stdout) = rmcp::transport::io::stdio();
        let transport = rmcp::transport::async_rw::AsyncRwTransport::new_server(stdin, stdout)
            .with_max_line_length(rascal_core::pack::MAX_ANSWER_BYTES as usize);
        let service = self
            .serve(transport)
            .await
            .map_err(|e| Error::op("serve", format!("mcp: {e}")))?;
        service
            .waiting()
            .await
            .map_err(|e| Error::op("serve", format!("mcp: {e}")))?;
        Ok(())
    }

    /// Runs one tool; `cancel` is checked before an op's commit, as the CLI's flag is.
    fn call(
        &self,
        name: &str,
        args: &serde_json::Map<String, serde_json::Value>,
        cancel: &rascal_core::workspace::Cancel,
    ) -> Result<CallToolResult> {
        let inner = &self.inner;
        match name {
            "workspace_import" => {
                let projection = match args.get("projection").and_then(|v| v.as_str()) {
                    None => None,
                    Some("flat") => Some(rascal_core::frame::Projection::Flat),
                    Some("equirectangular") => {
                        Some(rascal_core::frame::Projection::Equirectangular)
                    }
                    Some(other) => {
                        return Err(Error::usage(format!(
                            "projection {other} is not flat or equirectangular"
                        )));
                    }
                };
                let (id, frame, artifact) = match (
                    args.get("path").and_then(|v| v.as_str()),
                    args.get("base64").and_then(|v| v.as_str()),
                ) {
                    (Some(path), None) => inner
                        .workspace
                        .import(std::path::Path::new(path), projection)?,
                    (None, Some(data)) => {
                        let bytes = jsonrpc::base64_decode(data)
                            .ok_or_else(|| Error::usage("base64 does not decode"))?;
                        inner.workspace.import_bytes(&bytes, "base64", projection)?
                    }
                    _ => {
                        return Err(Error::usage(
                            "workspace_import takes exactly one of path and base64",
                        ));
                    }
                };
                let envelope = serde_json::json!({
                    "envelope_version": 1, "status": "ok",
                    "summary": format!("imported {id} in {}", frame.id),
                    "artifacts": { "image": { "artifact_type": "Image", "id": id, "identity": artifact.identity().as_str() } },
                    "frame": frame,
                });
                Ok(jsonrpc::tool_result(envelope, &[]))
            }
            "workspace_list" => {
                let filter = args.get("type").and_then(|v| v.as_str());
                let rows: Vec<serde_json::Value> = inner
                    .workspace
                    .list()?
                    .into_iter()
                    .filter(|(kind, _, _, _)| filter.is_none_or(|t| t.eq_ignore_ascii_case(kind)))
                    .map(|(kind, id, identity, frame)| serde_json::json!({ "type": kind, "id": id, "identity": identity, "frame": frame }))
                    .collect();
                let envelope = serde_json::json!({
                    "envelope_version": 1, "status": "ok",
                    "summary": format!("{} entries", rows.len()),
                    "entries": rows,
                });
                Ok(jsonrpc::tool_result(envelope, &[]))
            }
            "workspace_describe" => {
                let id = required_str(args, "id")?;
                let artifact = inner.workspace.load(id)?;
                let envelope = serde_json::json!({
                    "envelope_version": 1, "status": "ok",
                    "summary": format!("{} {id}", artifact.artifact_type()),
                    "artifact_type": artifact.artifact_type().to_string(),
                    "identity": artifact.identity().as_str(),
                    "record": describe(&artifact)?,
                });
                Ok(jsonrpc::tool_result(envelope, &[]))
            }
            "workspace_read" => {
                let id = required_str(args, "id")?;
                let artifact = inner.workspace.load(id)?;
                let Artifact::Image(image) = &artifact else {
                    let envelope = serde_json::json!({
                        "envelope_version": 1, "status": "invalid_args",
                        "summary": format!("invalid arguments: {id} is a {}, not an Image", artifact.artifact_type()),
                        "reason": format!("{id} is a {}, not an Image", artifact.artifact_type()),
                    });
                    return Ok(jsonrpc::tool_result(envelope, &[]));
                };
                let bytes = png_bytes(image)?;
                if bytes.len() as u64 > inner.max_image_bytes {
                    return Err(Error::op(
                        "serve",
                        format!(
                            "{id} is {} bytes as PNG, over max_image_bytes {}",
                            bytes.len(),
                            inner.max_image_bytes
                        ),
                    ));
                }
                let envelope = serde_json::json!({
                    "envelope_version": 1, "status": "ok",
                    "summary": format!("{id}: {}x{} PNG, {} bytes", image.pixels.width(), image.pixels.height(), bytes.len()),
                    "images": [{ "frame": image.record.frame, "identity": image.record.identity, "width": image.pixels.width(), "height": image.pixels.height(), "media_type": jsonrpc::PNG }],
                });
                Ok(jsonrpc::tool_result(envelope, &[bytes]))
            }
            "workspace_dash" => {
                let dash = inner
                    .dash
                    .as_ref()
                    .ok_or_else(|| Error::op("serve", "no run page writer in this server"))?;
                let path = dash(&inner.workspace.root)?;
                let envelope = serde_json::json!({
                    "envelope_version": 1, "status": "ok",
                    "summary": format!("run page written to {}", path.display()),
                    "path": path.display().to_string(),
                });
                Ok(jsonrpc::tool_result(envelope, &[]))
            }
            op_name => {
                let op = inner
                    .registry
                    .get(op_name)
                    .ok_or_else(|| Error::usage(format!("no tool {op_name}")))?;
                let manifest = op.manifest();
                let mut inputs: Inputs = BTreeMap::new();
                let mut params: Params = Params::new();
                for (key, value) in args {
                    if let Some(port) = manifest.inputs.iter().find(|p| &p.name == key) {
                        let id = value.as_str().ok_or_else(|| {
                            Error::usage(format!("{}: an artifact id (a string)", port.name))
                        })?;
                        inputs.insert(port.name.clone(), inner.workspace.load(id)?);
                    } else if key == "constants" {
                        let set = value.as_str().ok_or_else(|| {
                            Error::usage(format!("{op_name}: constants names a set (a string)"))
                        })?;
                        params.insert(key.clone(), toml::Value::String(set.to_string()));
                    } else if let Some(param) = manifest.param(key) {
                        params.insert(key.clone(), json_param(param, value)?);
                    } else {
                        return Err(Error::usage(format!("{op_name}: unknown argument {key}")));
                    }
                }
                let invocation = inner.workspace.invoke_with(
                    op_name,
                    op_name,
                    &inner.registry,
                    &inputs,
                    &params,
                    Some(cancel),
                )?;
                let mut envelope = serde_json::to_value(&invocation.output.envelope)
                    .map_err(|e| Error::op("serve", e.to_string()))?;
                // The envelope's artifact table names outputs by port; a client needs the ids
                // the workspace published, so it can pass them to the next call.
                if let Some(table) = envelope
                    .get_mut("artifacts")
                    .and_then(|a| a.as_object_mut())
                {
                    for (port, id) in &invocation.ids {
                        if let Some(entry) = table.get_mut(port).and_then(|e| e.as_object_mut()) {
                            entry.insert("id".to_string(), serde_json::Value::String(id.clone()));
                        }
                    }
                }
                let images = image_blocks_of(&invocation.output)?;
                Ok(jsonrpc::tool_result(envelope, &images))
            }
        }
    }
}

/// The tool schema of an op: its parameter schema plus one string property per input port,
/// required when the port is.
fn tool_schema(manifest: &rascal_core::op::Manifest) -> serde_json::Map<String, serde_json::Value> {
    let mut schema = manifest
        .input_schema
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut properties = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .cloned()
        .unwrap_or_default();
    let mut required: Vec<serde_json::Value> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();
    for port in &manifest.inputs {
        properties.insert(
            port.name.clone(),
            serde_json::json!({
                "type": "string",
                "description": format!("{} ({} artifact id)", port.purpose, port.artifact_type),
            }),
        );
        if port.required {
            required.push(serde_json::Value::String(port.name.clone()));
        }
    }
    if !manifest.constants.is_empty() {
        properties.insert("constants".to_string(), serde_json::json!({
            "type": "string", "enum": manifest.constants,
            "description": format!("The constant set; the first is the default: {}", manifest.constants.join(", ")),
        }));
    }
    schema.insert(
        "properties".to_string(),
        serde_json::Value::Object(properties),
    );
    schema.insert("required".to_string(), serde_json::Value::Array(required));
    schema.insert(
        "title".to_string(),
        serde_json::Value::String(format!("{} arguments", manifest.name)),
    );
    schema
}

fn workspace_tool_schema(name: &str) -> serde_json::Map<String, serde_json::Value> {
    let value = match name {
        "workspace_import" => serde_json::json!({
            "type": "object", "additionalProperties": false,
            "properties": {
                "path": { "type": "string", "description": "A JPEG or PNG file the server can read." },
                "base64": { "type": "string", "description": "JPEG or PNG bytes, base64." },
                "projection": { "type": "string", "enum": ["flat", "equirectangular"], "description": "Without it an exactly 2:1 image is equirectangular." }
            }
        }),
        "workspace_list" => serde_json::json!({
            "type": "object", "additionalProperties": false,
            "properties": { "type": { "type": "string", "description": "Frame, Location, Image, Mask, BandProfile, Series or Plan." } }
        }),
        "workspace_describe" | "workspace_read" => serde_json::json!({
            "type": "object", "additionalProperties": false,
            "properties": { "id": { "type": "string", "description": "An artifact id or identity." } },
            "required": ["id"]
        }),
        _ => {
            serde_json::json!({ "type": "object", "additionalProperties": false, "properties": {} })
        }
    };
    value.as_object().cloned().unwrap_or_default()
}

fn required_str<'a>(
    args: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::usage(format!("{key} is required")))
}

/// A JSON argument as the TOML value a parameter takes.
fn json_param(param: &Param, value: &serde_json::Value) -> Result<toml::Value> {
    let bad = |what: &str| Error::usage(format!("{}: {value} is not {what}", param.name));
    Ok(match param.kind {
        ParamKind::Number => toml::Value::Float(value.as_f64().ok_or_else(|| bad("a number"))?),
        ParamKind::Integer => {
            toml::Value::Integer(value.as_i64().ok_or_else(|| bad("an integer"))?)
        }
        ParamKind::Boolean => {
            toml::Value::Boolean(value.as_bool().ok_or_else(|| bad("true or false"))?)
        }
        ParamKind::String | ParamKind::Choice => {
            toml::Value::String(value.as_str().ok_or_else(|| bad("a string"))?.to_string())
        }
        ParamKind::NumberArray => toml::Value::Array(
            value
                .as_array()
                .ok_or_else(|| bad("an array of numbers"))?
                .iter()
                .map(|v| {
                    v.as_f64()
                        .map(toml::Value::Float)
                        .ok_or_else(|| bad("an array of numbers"))
                })
                .collect::<Result<_>>()?,
        ),
        ParamKind::PointList => toml::Value::Array(
            value
                .as_array()
                .ok_or_else(|| bad("a list of [x, y] points"))?
                .iter()
                .map(|p| {
                    let pair = p
                        .as_array()
                        .filter(|a| a.len() == 2)
                        .ok_or_else(|| bad("a list of [x, y] points"))?;
                    Ok(toml::Value::Array(
                        pair.iter()
                            .map(|v| {
                                v.as_f64()
                                    .map(toml::Value::Float)
                                    .ok_or_else(|| bad("a list of [x, y] points"))
                            })
                            .collect::<Result<_>>()?,
                    ))
                })
                .collect::<Result<_>>()?,
        ),
    })
}

/// An artifact's record as JSON.
fn describe(artifact: &Artifact) -> Result<serde_json::Value> {
    let value = match artifact {
        Artifact::Image(i) => serde_json::to_value(&i.record),
        Artifact::Location { location, .. } => serde_json::to_value(location),
        Artifact::Mask(m) => serde_json::to_value(&m.record),
        Artifact::BandProfile(b) => serde_json::to_value(&**b),
        Artifact::Series(s) => serde_json::to_value(&**s),
        Artifact::Plan(p) => serde_json::to_value(&**p),
    };
    value.map_err(|e| Error::op("serve", e.to_string()))
}

fn png_bytes(image: &Image) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    image
        .pixels
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .map_err(|source| Error::Image {
            path: "<serve>".into(),
            source,
        })?;
    Ok(bytes)
}

impl ServerHandler for Server {
    fn get_info(&self) -> InitializeResult {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
            .with_server_info(transport::implementation())
            .with_instructions(
                "rascal: the find-and-mask step of an image pipeline. Import an image with workspace_import, then call the ops by name with artifact ids; every result is an envelope as structured content.",
            )
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        transport::supported_versions()
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        let start: usize = match request.and_then(|r| r.cursor) {
            Some(c) => c.parse().map_err(|_| invalid(format!("bad cursor {c}")))?,
            None => 0,
        };
        let all = self.tools();
        let end = start.saturating_add(PAGE).min(all.len());
        let tools = all.get(start..end).unwrap_or_default().to_vec();
        let next_cursor = (end < all.len()).then(|| end.to_string());
        Ok(ListToolsResult {
            tools,
            next_cursor,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        let args = request.arguments.clone().unwrap_or_default();
        let name = request.name.to_string();
        // Serial by default: the permit is held for the whole call, cancellation included.
        let permit = tokio::select! {
            p = self.inner.permits.clone().acquire_owned() => p.map_err(|e| ErrorData::internal_error(e.to_string(), None))?,
            () = context.ct.cancelled() => return Err(ErrorData::new(rmcp::model::ErrorCode::INTERNAL_ERROR, "cancelled before the call started", Some(serde_json::json!({ "class": "cancelled" })))),
        };
        let server = self.clone();
        // The permit travels with the blocking call, so a cancelled call still counts until
        // it returns. A cancellation sets the op's flag (nothing is published after it) and,
        // for a pack op, fails that pack's in-flight calls, which brings the op back promptly.
        let cancel = Arc::new(rascal_core::workspace::Cancel::new(false));
        let flag = cancel.clone();
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            server.call(&name, &args, &flag)
        });
        let result = tokio::select! {
            r = task => r.map_err(|e| ErrorData::internal_error(format!("the call panicked: {e}"), None))?,
            () = context.ct.cancelled() => {
                cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                if let Some(pack) = self.pack_for(&request.name).cloned() {
                    let _ = tokio::task::spawn_blocking(move || pack.fail_in_flight("cancelled by the client")).await;
                }
                return Err(ErrorData::new(rmcp::model::ErrorCode::INTERNAL_ERROR, "cancelled", Some(serde_json::json!({ "class": "cancelled" }))));
            }
        };
        match result {
            Ok(result) => Ok(result.into()),
            // Validation failed: the journaled envelope is the result, flagged as an error.
            Err(Error::InvalidArgs { envelope, .. }) => {
                let json = serde_json::to_value(&*envelope)
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                Ok(jsonrpc::tool_result(json, &[]).into())
            }
            Err(e @ Error::Usage { .. }) | Err(e @ Error::Recipe { .. }) => {
                Err(invalid(e.to_string()))
            }
            Err(e) => Err(operational(&e)),
        }
    }
}
