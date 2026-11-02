//! The server behind `rascal serve` (SPEC.md "The MCP server and client"; CONTRACT 5): one MCP
//! tool per action of the description plus the reserved tools, every call through
//! [`Actions`], runs under a permit (serial by default), progress and cancellation forwarded,
//! the correlation ids claimed, and the actions' resources served. The server knows no op:
//! what an action is and how it runs is the binary's. Nothing is written to stdout but the
//! protocol; tracing goes to stderr.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::error::Result;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData,
    InitializeResult, ListResourceTemplatesResult, ListResourcesResult, ListToolsResult,
    PaginatedRequestParams, ProgressNotificationParam, ProtocolVersion, ReadResourceRequestParams,
    ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, ResourceTemplate,
    ServerCapabilities, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use tokio::sync::Semaphore;

use crate::jsonrpc::{self, invalid};
use crate::transport;

/// The MCP types an [`Actions`] implementation builds, so it needs no SDK of its own.
pub use rmcp::model::{
    Resource as McpResource, ResourceTemplate as McpResourceTemplate, Tool as McpTool,
};

/// What a contract action returned (table-manners 2 and 5; ADR 0020): the result as
/// structured content, whether it is an error result, and image blocks the caller asked for.
pub struct ActionResult {
    pub structured: serde_json::Value,
    pub is_error: bool,
    pub images: Vec<Vec<u8>>,
}

/// A failure before a run exists (table-manners 5): sent as a JSON-RPC invalid-params error,
/// with `data` when it is a refusal the contract names (`schemas/core/refusal.json`, such as
/// `not_ready`), else with the usage class. Either releases the request's correlation id.
#[derive(Debug, Clone, PartialEq)]
pub struct CallError {
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl CallError {
    /// A refusal the contract names, `data` being its refusal object.
    pub fn refused(message: impl Into<String>, data: serde_json::Value) -> CallError {
        CallError {
            message: message.into(),
            data: Some(data),
        }
    }
}

impl From<CallError> for ErrorData {
    /// The JSON-RPC invalid-params error a refusal or a caller's mistake is sent as.
    fn from(error: CallError) -> ErrorData {
        match error.data {
            Some(data) => ErrorData::invalid_params(error.message, Some(data)),
            None => invalid(error.message),
        }
    }
}

impl From<String> for CallError {
    fn from(message: String) -> CallError {
        CallError {
            message,
            data: None,
        }
    }
}

impl From<&str> for CallError {
    fn from(message: &str) -> CallError {
        CallError::from(message.to_string())
    }
}

/// What a call's request `_meta` carries for the action (table-manners 4.4, 4.5): its
/// correlation id, already claimed, and the estimate it names, as sent (the action judges it,
/// since by then a run exists). Both are none for a call that is not a run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallMeta {
    pub client_run_id: Option<String>,
    pub estimate: Option<serde_json::Value>,
}

/// A resource's body: text with its media type, or bytes (sent as base64).
pub enum ResourceBody {
    Text { mime: String, text: String },
    Blob { mime: String, bytes: Vec<u8> },
}

/// The contract's tools and resources, supplied by the binary (the recipes, the ops, `run` and
/// `heal` live there): the server lists the tools, runs them with progress and cancellation,
/// and serves the resources.
pub trait Actions: Send + Sync {
    /// The action tools, `describe` included.
    fn tools(&self) -> Vec<Tool>;
    /// Whether a tool name is one of them.
    fn handles(&self, name: &str) -> bool;
    /// Whether a call is a run: it holds the call permit while it works and may carry a
    /// correlation id; a read (`describe`, `estimate`) does neither, so it answers while a run is
    /// under way.
    fn exclusive(&self, name: &str) -> bool;
    /// Runs one. `meta` holds the caller's correlation id, already claimed (table-manners 4.4),
    /// and the estimate the call names (4.5); `progress` takes the run's reference at allocation, then a line about each step;
    /// `cancel` is set when the client cancels. `Err` is a failure before a run exists (a
    /// JSON-RPC error, carrying the refusal data when it is one the contract names, which
    /// releases the correlation id when no run was allocated); a run's own outcome, `invalid`
    /// and `error` included, is an `ActionResult`. A cancelled run returns `Err`, and the
    /// server sends no result for it.
    fn call(
        &self,
        name: &str,
        args: &serde_json::Map<String, serde_json::Value>,
        meta: &CallMeta,
        progress: &(dyn Fn(Progress) + Send + Sync),
        cancel: Arc<rascal_core::workspace::Cancel>,
    ) -> std::result::Result<ActionResult, CallError>;
    /// Fixed resources (`rascal://runs`).
    fn resources(&self) -> Vec<Resource>;
    /// Resource templates (`rascal://run/{id}/run.json`, …).
    fn templates(&self) -> Vec<ResourceTemplate>;
    /// A resource by URI: `Ok(None)` when there is none, `Err` when the read is refused (the
    /// artifact resource's `binding` refusal, CONTRACT 4.7), sent like a call's.
    fn read(&self, uri: &str) -> std::result::Result<Option<ResourceBody>, CallError>;
}

/// What an action tells the server while it works.
#[derive(Debug, Clone)]
pub enum Progress {
    /// The run is allocated: its reference (table-manners 4.3), which every later progress
    /// notification carries in `_meta` and a reused correlation id is answered with.
    Run(serde_json::Value),
    /// A line about a step, for people.
    Line(String),
}

/// The request `_meta` key a caller's correlation id comes under (table-manners 4.4).
pub const CLIENT_RUN_ID: &str = "table-manners/client_run_id";

/// The request `_meta` key a caller names its estimate's binding under (table-manners 4.5).
pub const ESTIMATE: &str = "table-manners/estimate";

/// The notification `_meta` key the run's reference goes under (table-manners 4.1).
pub const RUN_META: &str = "table-manners/run";

/// A correlation id's form: 1 to 128 of `[A-Za-z0-9._:-]` (table-manners 4.4).
fn valid_client_run_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

/// How many tools a `tools/list` page holds.
pub const PAGE: usize = 32;

/// The server.
#[derive(Clone)]
pub struct Server {
    inner: Arc<Inner>,
}

struct Inner {
    /// The contract's tools and resources, supplied by the binary.
    actions: Arc<dyn Actions>,
    /// Runs hold this permit while they work: one by default, `--concurrency n` for more.
    permits: Arc<Semaphore>,
    /// The correlation ids claimed in this session, each with its run's reference once the run
    /// is allocated (table-manners 4.4). A claim is released only when its request ends in a
    /// JSON-RPC error while it still has no run; a request cancelled before allocation keeps it.
    claims: std::sync::Mutex<BTreeMap<String, Option<serde_json::Value>>>,
}

impl Server {
    /// A server over the contract's actions (ADR 0020; CONTRACT 5): one MCP tool per action of
    /// the description plus the reserved tools, and nothing else; `concurrency` runs at once.
    pub fn new(actions: Arc<dyn Actions>, concurrency: usize) -> Server {
        Server {
            inner: Arc::new(Inner {
                actions,
                permits: Arc::new(Semaphore::new(concurrency.max(1))),
                claims: std::sync::Mutex::default(),
            }),
        }
    }

    /// Every tool, in the order the actions give them.
    pub fn tools(&self) -> Vec<Tool> {
        self.inner.actions.tools()
    }

    /// Serves over stdio until the client goes away.
    pub async fn serve_stdio(self) -> Result<()> {
        use rmcp::ServiceExt;
        // One request per line; the limit is what an inline image may carry, so a large image
        // as base64 is not dropped by the SDK's default (16 MiB).
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
}

impl ServerHandler for Server {
    fn get_info(&self) -> InitializeResult {
        InitializeResult::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_protocol_version(ProtocolVersion::V_2025_11_25)
        .with_server_info(transport::implementation())
        .with_instructions(
            "rascal: finds, masks and composites regions of an image (table-manners). Call describe for the actions; each call is a run whose result is its run.json. Operations take artifacts by reference (CONTRACT 3.4) and images by path or inline.",
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
        let actions = self.inner.actions.clone();
        if !actions.handles(&name) {
            return Err(invalid(format!("no tool {name}")));
        }
        self.call_action(actions, name, args, context).await
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(
            self.inner.actions.resources(),
        ))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListResourceTemplatesResult, ErrorData> {
        Ok(ListResourceTemplatesResult::with_all_items(
            self.inner.actions.templates(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ReadResourceResponse, ErrorData> {
        let uri = request.uri.clone();
        let actions = self.inner.actions.clone();
        let lookup = uri.clone();
        let body = tokio::task::spawn_blocking(move || actions.read(&lookup))
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
            .map_err(ErrorData::from)?
            .ok_or_else(|| ErrorData::resource_not_found(format!("no resource {uri}"), None))?;
        let contents = match body {
            ResourceBody::Text { mime, text } => {
                with_mime(ResourceContents::text(text, uri.clone()), mime)
            }
            ResourceBody::Blob { mime, bytes } => with_mime(
                ResourceContents::blob(rascal_core::panel::base64(&bytes), uri.clone()),
                mime,
            ),
        };
        Ok(ReadResourceResult::new(vec![contents]).into())
    }
}

fn with_mime(contents: ResourceContents, mime: String) -> ResourceContents {
    match contents {
        ResourceContents::TextResourceContents {
            uri, text, meta, ..
        } => ResourceContents::TextResourceContents {
            uri,
            mime_type: Some(mime),
            text,
            meta,
        },
        ResourceContents::BlobResourceContents {
            uri, blob, meta, ..
        } => ResourceContents::BlobResourceContents {
            uri,
            mime_type: Some(mime),
            blob,
            meta,
        },
        other => other,
    }
}

impl Server {
    /// Runs a contract action (table-manners 4): under the call permit when it is exclusive, on
    /// a blocking thread, with every progress line forwarded and a heartbeat at least every
    /// `HEARTBEAT` while it works, `progress` strictly increasing; a client's cancellation sets
    /// the run's flag and the call returns no result.
    async fn call_action(
        &self,
        actions: Arc<dyn Actions>,
        name: String,
        args: serde_json::Map<String, serde_json::Value>,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        let exclusive = actions.exclusive(&name);
        // A run's correlation id is claimed before the permit wait, so duplicates queued
        // together execute at most once. The claim outlives its request once a run is
        // allocated, or when the request is cancelled before allocation; a request that ends in
        // a JSON-RPC error with no run releases it (table-manners 4.4, from v0.3).
        let client_run_id = match context.meta.get(CLIENT_RUN_ID) {
            _ if !exclusive => None,
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(id)) if valid_client_run_id(id) => Some(id.clone()),
            Some(other) => {
                return Err(ErrorData::invalid_params(
                    format!("{CLIENT_RUN_ID} must be 1 to 128 of [A-Za-z0-9._:-]; got {other}"),
                    None,
                ));
            }
        };
        if let Some(id) = &client_run_id {
            let mut claims = self.inner.claims.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(earlier) = claims.get(id) {
                return Err(ErrorData::invalid_params(
                    format!("{CLIENT_RUN_ID} {id} was already used in this session"),
                    Some(serde_json::json!({
                        "reason": "client_run_id_reused",
                        "run": earlier.clone().unwrap_or(serde_json::Value::Null),
                    })),
                ));
            }
            claims.insert(id.clone(), None);
        }
        let permit = if exclusive {
            let permit = tokio::select! {
                p = self.inner.permits.clone().acquire_owned() => p.map_err(|e| ErrorData::internal_error(e.to_string(), None))?,
                () = context.ct.cancelled() => return Err(cancelled_error()),
            };
            // Checked again once the permit is held: a cancel before allocation leaves no run.
            if context.ct.is_cancelled() {
                return Err(cancelled_error());
            }
            Some(permit)
        } else {
            None
        };
        let token = context.meta.get_progress_token();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();
        let cancel = Arc::new(rascal_core::workspace::Cancel::new(false));
        let flag = cancel.clone();
        let meta = CallMeta {
            client_run_id: client_run_id.clone(),
            estimate: context
                .meta
                .get(ESTIMATE)
                .filter(|v| exclusive && !v.is_null())
                .cloned(),
        };
        let mut task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let send = move |progress: Progress| {
                let _ = tx.send(progress);
            };
            actions.call(&name, &args, &meta, &send, flag)
        });
        let started = std::time::Instant::now();
        let mut sent = 0.0_f64;
        let mut heartbeat = tokio::time::interval(HEARTBEAT);
        heartbeat.tick().await;
        let mut last_line = String::from("started");
        // The run's reference, once allocated: in every later notification's `_meta`.
        let mut run: Option<serde_json::Value> = None;
        let notification = |sent: f64, message: String, run: &Option<serde_json::Value>| {
            token.clone().map(|token| {
                let mut param = ProgressNotificationParam::new(token, sent).with_message(message);
                if let Some(run) = run {
                    let mut meta = serde_json::Map::new();
                    meta.insert(RUN_META.to_string(), run.clone());
                    param.meta = Some(rmcp::model::NotificationMetaObject(
                        rmcp::model::MetaObject(meta),
                    ));
                }
                param
            })
        };
        let outcome = loop {
            tokio::select! {
                r = &mut task => break r,
                Some(progress) = rx.recv() => match progress {
                    Progress::Run(reference) => {
                        if let Some(id) = &client_run_id {
                            self.inner.claims.lock().unwrap_or_else(|e| e.into_inner())
                                .insert(id.clone(), Some(reference.clone()));
                        }
                        run = Some(reference);
                    }
                    Progress::Line(line) => {
                        last_line = line.clone();
                        sent += 1.0;
                        if let Some(param) = notification(sent, line, &run) {
                            let _ = context.peer.notify_progress(param).await;
                        }
                    }
                },
                _ = heartbeat.tick() => {
                    sent += 1.0;
                    let message = format!("{last_line}; {} s", started.elapsed().as_secs());
                    if let Some(param) = notification(sent, message, &run) {
                        let _ = context.peer.notify_progress(param).await;
                    }
                }
                () = context.ct.cancelled() => {
                    cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                    // The run stops before its next step and writes `run.json` as cancelled;
                    // the protocol sends no result for a cancelled request.
                    let _ = (&mut task).await;
                    return Err(cancelled_error());
                }
            }
        };
        // What the action sent just before it returned may still be queued: the run's reference
        // reaches the claim before the claim is judged. Lines after the end are dropped.
        while let Ok(progress) = rx.try_recv() {
            if let Progress::Run(reference) = progress
                && let Some(id) = &client_run_id
            {
                self.inner
                    .claims
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(id.clone(), Some(reference));
            }
        }
        let result = match outcome {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                self.release(client_run_id.as_deref());
                return Err(error.into());
            }
            Err(panicked) => {
                self.release(client_run_id.as_deref());
                return Err(ErrorData::internal_error(
                    format!("the call panicked: {panicked}"),
                    None,
                ));
            }
        };
        let summary = result
            .structured
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let mut content = vec![ContentBlock::text(summary)];
        for bytes in &result.images {
            content.push(ContentBlock::image(
                rascal_core::panel::base64(bytes),
                jsonrpc::PNG,
            ));
        }
        let mut call = CallToolResult::success(content);
        call.structured_content = Some(result.structured);
        call.is_error = Some(result.is_error);
        Ok(call.into())
    }
}

impl Server {
    /// Releases the claim on a correlation id whose request ended in a JSON-RPC error, when it
    /// still holds no run (table-manners 4.4): under the claims lock, so a concurrent request
    /// with the id sees the claim or nothing. A claim with a run reference is kept.
    fn release(&self, client_run_id: Option<&str>) {
        let Some(id) = client_run_id else {
            return;
        };
        let mut claims = self.inner.claims.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(claims.get(id), Some(None)) {
            claims.remove(id);
        }
    }
}

/// The longest gap between progress notifications on a live action run (table-manners 4.1
/// allows ten seconds; five leaves room for a slow client).
pub const HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(5);

fn cancelled_error() -> ErrorData {
    ErrorData::new(
        rmcp::model::ErrorCode::INTERNAL_ERROR,
        "cancelled",
        Some(serde_json::json!({ "class": "cancelled" })),
    )
}
