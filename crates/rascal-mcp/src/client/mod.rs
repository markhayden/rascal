//! The pack client (SPEC.md "The MCP server and client"): a configured pack is a child process
//! speaking MCP over its stdio. rascal starts it with an allowlisted environment, checks the
//! revision and the `tools` capability, lists its tools to exhaustion and validates every
//! manifest; each call carries `_harness`; every answer is validated before anything is
//! committed. A timeout, a cancellation or the process dying fails every in-flight call on the
//! pack and shuts it down (stdin close, a grace period, SIGTERM, a grace period, SIGKILL)
//! before any reuse. Captured stderr is redacted and journaled by the op that made the call.
//!
//! [`Pack`] is the process and its calls; [`op::PackOp`] is the `Op` a tool becomes in the
//! registry; [`replay::Replay`] is a run's recorded calls, for `--replay`.

pub mod env;
pub mod op;
pub mod replay;
pub mod validate;

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rascal_core::Error;
use rascal_core::config::PackConfig;
use rascal_core::error::Result;
use rascal_core::op::Manifest;
use rascal_core::pack::{
    CompatManifest, MAX_ANSWER_BYTES, META_MANIFEST, META_RASCAL, PackAnnotation,
};
use rascal_core::schemas::SchemaDir;
use rmcp::ServiceError;
use rmcp::model::{
    CallToolRequest, CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig,
    ClientRequest, ProtocolVersion, ServerResult, Tool,
};
use rmcp::service::{PeerRequestOptions, RoleClient, RunningService, serve_client_with_ct};
use rmcp::transport::async_rw::AsyncRwTransport;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::transport;

/// The grace the escalation waits at each step (stdin close, SIGTERM) before the next.
pub const GRACE: Duration = Duration::from_secs(5);

/// The most stderr lines kept between reads; older lines are dropped and counted.
const STDERR_LINES: usize = 256;

/// The permits of a tool whose calls may overlap: in practice bounded by the pack.
const CONCURRENT_PERMITS: usize = 64;

/// One tool of a pack as rascal sees it: the native manifest built from the copied-shape
/// manifest and the annotation block, and the two as they came.
#[derive(Debug, Clone)]
pub struct PackTool {
    pub manifest: Manifest,
    pub compat: CompatManifest,
    pub annotation: PackAnnotation,
}

impl PackTool {
    /// A tool from its MCP listing: `_meta.manifest` and `_meta.rascal`, each parsed as its
    /// closed type and, when a checkout's schemas are at hand, validated against its schema.
    pub fn from_tool(tool: &Tool, schemas: Option<&SchemaDir>) -> Result<PackTool> {
        let name = tool.name.to_string();
        let meta = tool
            .meta
            .as_ref()
            .ok_or_else(|| Error::op(&name, "the tool carries no _meta"))?;
        let compat_json = meta
            .get(META_MANIFEST)
            .ok_or_else(|| Error::op(&name, format!("the tool's _meta has no {META_MANIFEST}")))?;
        let annotation_json = meta
            .get(META_RASCAL)
            .ok_or_else(|| Error::op(&name, format!("the tool's _meta has no {META_RASCAL}")))?;
        if let Some(schemas) = schemas {
            for (what, schema, value) in [
                ("manifest", "copied/manifest-v1.json", compat_json),
                (
                    "annotation",
                    "rascal/pack-annotation-v1.json",
                    annotation_json,
                ),
            ] {
                let validator = schemas.validator(schema)?;
                if let Err(e) = validator.validate(value) {
                    return Err(Error::op(
                        &name,
                        format!("the {what} does not validate against {schema}: {e}"),
                    ));
                }
            }
        }
        let compat: CompatManifest = serde_json::from_value(compat_json.clone())
            .map_err(|e| Error::op(&name, format!("the manifest does not parse: {e}")))?;
        let annotation: PackAnnotation = serde_json::from_value(annotation_json.clone())
            .map_err(|e| Error::op(&name, format!("the annotation does not parse: {e}")))?;
        if compat.name != name {
            return Err(Error::op(
                &name,
                format!("the manifest names {} but the tool is {name}", compat.name),
            ));
        }
        let manifest = compat.with_annotation(&annotation)?;
        Ok(PackTool {
            manifest,
            compat,
            annotation,
        })
    }
}

/// What the journal records about a call: never a key, never a pixel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackCall {
    pub pack: String,
    pub tool: String,
    /// The command as configured.
    pub target: String,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub duration_ms: u64,
    /// `ok`, `timeout`, `cancelled`, `died`, `error`.
    pub status: String,
    /// `ok`, or the error the call ended in.
    pub outcome: String,
}

/// The journal note prefix of a call record.
pub const CALL_NOTE: &str = "pack call: ";
/// The journal note prefix of a captured stderr line.
pub const STDERR_NOTE: &str = "pack stderr: ";

impl PackCall {
    /// The journal note text: the prefix and the record as JSON.
    pub fn note(&self) -> String {
        format!(
            "{CALL_NOTE}{}",
            serde_json::to_string(self).unwrap_or_default()
        )
    }
}

/// What a call produced: the raw result when the pack answered, and the record either way.
pub struct CallOutcome {
    pub result: Result<CallToolResult>,
    pub call: PackCall,
}

/// A running pack: the service, the process, and the shared state of its calls.
struct Live {
    service: RunningService<RoleClient, ClientConfig>,
    /// The process; taken by whoever stops it, so it is waited for exactly once.
    child: Mutex<Option<tokio::process::Child>>,
    /// Cancelled when any call on this instance fails: every in-flight call returns.
    epoch: CancellationToken,
    /// One semaphore per tool: one permit when either the config or the manifest says serial.
    permits: BTreeMap<String, Arc<Semaphore>>,
}

/// Captured stderr, redacted, kept on the pack (not the process) so a dying pack's last
/// words are journaled with the call that saw it die.
#[derive(Default)]
struct Stderr {
    lines: VecDeque<String>,
    dropped: u64,
}

/// A configured pack. Cheap to share; the process starts on first use and restarts after a
/// failure. One tokio runtime per pack, so a synchronous op can call it from any thread.
pub struct Pack {
    name: String,
    config: PackConfig,
    key_envs: Vec<String>,
    secrets: Vec<String>,
    /// Own runtime, so a synchronous op can call from any thread; `None` only while dropping.
    runtime: Option<tokio::runtime::Runtime>,
    live: tokio::sync::Mutex<Option<Arc<Live>>>,
    stderr: Arc<Mutex<Stderr>>,
    tools: Mutex<Option<Vec<PackTool>>>,
    calls: AtomicU64,
    grace: Duration,
    replay: Option<Arc<replay::Replay>>,
    schemas: Option<SchemaDir>,
}

impl Pack {
    fn rt(&self) -> Result<&tokio::runtime::Runtime> {
        self.runtime
            .as_ref()
            .ok_or_else(|| Error::op(&self.name, "the pack is being dropped"))
    }
}

impl std::fmt::Debug for Pack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pack").field("name", &self.name).finish()
    }
}

impl Pack {
    /// A pack from its entry. `key_envs` are the names no pack may see (every filler's
    /// `key_env`); the secrets redacted from stderr are read from the environment now.
    pub fn new(name: &str, config: PackConfig, key_envs: Vec<String>) -> Result<Pack> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name(format!("pack-{name}"))
            .enable_all()
            .build()
            .map_err(|e| Error::op(name, format!("runtime: {e}")))?;
        let secrets = env::secrets(&key_envs, env::parent_environment());
        Ok(Pack {
            name: name.to_string(),
            config,
            key_envs,
            secrets,
            runtime: Some(runtime),
            live: tokio::sync::Mutex::new(None),
            stderr: Arc::new(Mutex::new(Stderr::default())),
            tools: Mutex::new(None),
            calls: AtomicU64::new(0),
            grace: GRACE,
            replay: None,
            schemas: SchemaDir::discover(),
        })
    }

    /// A pack that never starts its process: its tools come from a recorded run and every
    /// call is answered from that run's journal or fails naming the request.
    pub fn replayed(
        name: &str,
        config: PackConfig,
        key_envs: Vec<String>,
        replay: Arc<replay::Replay>,
    ) -> Result<Pack> {
        let tools = replay.tools_for(name)?.ok_or_else(|| {
            Error::op(
                name,
                format!(
                    "--replay {}: its journal records no manifests for packs.{name}",
                    replay.dir().display()
                ),
            )
        })?;
        let mut pack = Pack::new(name, config, key_envs)?;
        *pack.tools.lock().unwrap_or_else(|e| e.into_inner()) = Some(tools);
        pack.replay = Some(replay);
        Ok(pack)
    }

    /// A shorter grace for tests of the escalation.
    pub fn with_grace(mut self, grace: Duration) -> Pack {
        self.grace = grace;
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn config(&self) -> &PackConfig {
        &self.config
    }

    /// The recorded run this pack answers from, when replaying.
    pub fn replay(&self) -> Option<&Arc<replay::Replay>> {
        self.replay.as_ref()
    }

    /// The next call id's number.
    pub fn next_call(&self) -> u64 {
        self.calls.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// The environment the process gets.
    pub fn environment(&self) -> BTreeMap<String, String> {
        env::allowlist(&self.config.env, &self.key_envs, env::parent_environment())
    }

    /// The tools, starting the process when they are not known yet.
    pub fn tools(&self) -> Result<Vec<PackTool>> {
        if let Some(tools) = self.tools.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return Ok(tools);
        }
        self.rt()?.block_on(async {
            self.ensure_live().await?;
            Ok(self
                .tools
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
                .unwrap_or_default())
        })
    }

    /// True while the process is up.
    pub fn is_live(&self) -> bool {
        self.rt()
            .map(|rt| rt.block_on(async { self.live.lock().await.is_some() }))
            .unwrap_or(false)
    }

    /// The process id while the process is up.
    pub fn pid(&self) -> Option<u32> {
        self.rt().ok()?.block_on(async {
            self.live.lock().await.as_ref().and_then(|l| {
                l.child
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .and_then(|c| c.id())
            })
        })
    }

    /// The redacted stderr lines captured since the last take, oldest first, whether or not
    /// the process is still up; a dropped count becomes a line of its own.
    pub fn take_stderr(&self) -> Vec<String> {
        let mut stderr = self.stderr.lock().unwrap_or_else(|e| e.into_inner());
        let mut lines: Vec<String> = std::mem::take(&mut stderr.lines).into();
        if stderr.dropped > 0 {
            lines.insert(0, format!("[{} earlier lines dropped]", stderr.dropped));
            stderr.dropped = 0;
        }
        lines
    }

    /// Calls a tool with the arguments as rascal built them (`_harness` included). The answer
    /// is the raw result; validation is the op's. The record is returned either way.
    pub fn call(
        &self,
        tool: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> CallOutcome {
        match self.rt() {
            Ok(rt) => rt.block_on(self.call_async(tool, arguments)),
            Err(e) => CallOutcome {
                call: PackCall {
                    pack: self.name.clone(),
                    tool: tool.to_string(),
                    target: self.config.command.join(" "),
                    request_bytes: 0,
                    response_bytes: 0,
                    duration_ms: 0,
                    status: "error".to_string(),
                    outcome: e.to_string(),
                },
                result: Err(e),
            },
        }
    }

    /// Fails every in-flight call and shuts the process down, so the next use starts afresh.
    pub fn fail_in_flight(&self, reason: &str) {
        if let Ok(rt) = self.rt() {
            rt.block_on(async {
                let live = self.live.lock().await.clone();
                if let Some(live) = live {
                    self.fail(&live, reason).await;
                }
            });
        }
    }

    /// Shuts the process down if it is up: the escalation, then the next use restarts it.
    pub fn shutdown(&self) {
        if let Ok(rt) = self.rt() {
            rt.block_on(async {
                let mut guard = self.live.lock().await;
                if let Some(live) = guard.take() {
                    self.stop(live).await;
                }
            });
        }
    }

    // --- the process ----------------------------------------------------------------------

    async fn ensure_live(&self) -> Result<Arc<Live>> {
        let mut guard = self.live.lock().await;
        if let Some(live) = guard.as_ref() {
            return Ok(live.clone());
        }
        if self.replay.is_some() {
            return Err(Error::op(
                &self.name,
                "replaying: the pack is not started and no call is made",
            ));
        }
        let live = Arc::new(self.start().await?);
        *guard = Some(live.clone());
        Ok(live)
    }

    async fn start(&self) -> Result<Live> {
        let name = &self.name;
        // One deadline for the whole startup: initialize and the listing together.
        let deadline = Instant::now() + Duration::from_secs(self.config.startup_timeout_s);
        let remaining = || deadline.saturating_duration_since(Instant::now());
        let mut command = tokio::process::Command::new(&self.config.command[0]);
        command
            .args(&self.config.command[1..])
            .env_clear()
            .envs(self.environment())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(false);
        let mut child = command.spawn().map_err(|e| {
            Error::op(
                name,
                format!("could not start {}: {e}", self.config.command[0]),
            )
        })?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(Error::op(name, "the process has no piped stdio"));
        };
        let secrets = self.secrets.clone();
        let sink = self.stderr.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut sink = sink.lock().unwrap_or_else(|e| e.into_inner());
                if sink.lines.len() >= STDERR_LINES {
                    sink.lines.pop_front();
                    sink.dropped += 1;
                }
                sink.lines.push_back(env::redact(&line, &secrets));
            }
        });
        let info = ClientConfig::new(ClientCapabilities::default(), transport::implementation())
            .with_protocol_version(ProtocolVersion::V_2025_11_25);
        let ct = CancellationToken::new();
        // The transport reads one answer per line; the SDK's default line limit (16 MiB) is
        // below what a tool may declare, so the limit is the ceiling the manifests are held to.
        let transport = AsyncRwTransport::new_client(stdout, stdin)
            .with_max_line_length(MAX_ANSWER_BYTES as usize);
        let connected =
            tokio::time::timeout(remaining(), serve_client_with_ct(info, transport, ct)).await;
        let service = match connected {
            Ok(Ok(service)) => service,
            Ok(Err(e)) => {
                Self::kill(&mut child, self.grace).await;
                return Err(Error::op(
                    name,
                    format!("initialize failed: {}", summarize_init_error(&e)),
                ));
            }
            Err(_) => {
                Self::kill(&mut child, self.grace).await;
                return Err(Error::op(
                    name,
                    format!(
                        "no initialize answer within startup_timeout_s = {}",
                        self.config.startup_timeout_s
                    ),
                ));
            }
        };
        let info = service.peer_info();
        let refused = match info.as_deref() {
            None => Some("the server sent no initialize result".to_string()),
            Some(info) if !transport::accepts(&info.protocol_version) => Some(format!(
                "the server speaks revision {} and rascal speaks {}",
                info.protocol_version,
                transport::PROTOCOL
            )),
            Some(info) if info.capabilities.tools.is_none() => {
                Some("the server does not advertise tools".to_string())
            }
            Some(_) => None,
        };
        if let Some(why) = refused {
            let _ = service.cancel().await;
            Self::kill(&mut child, self.grace).await;
            return Err(Error::op(name, format!("refused at initialize: {why}")));
        }
        let listed = tokio::time::timeout(remaining(), service.list_all_tools()).await;
        let listed = match listed {
            Ok(Ok(tools)) => tools,
            Ok(Err(e)) => {
                let _ = service.cancel().await;
                Self::kill(&mut child, self.grace).await;
                return Err(Error::op(name, format!("tools/list failed: {e}")));
            }
            Err(_) => {
                let _ = service.cancel().await;
                Self::kill(&mut child, self.grace).await;
                return Err(Error::op(
                    name,
                    format!(
                        "tools/list did not finish within startup_timeout_s = {}",
                        self.config.startup_timeout_s
                    ),
                ));
            }
        };
        let mut tools = Vec::with_capacity(listed.len());
        for tool in &listed {
            match PackTool::from_tool(tool, self.schemas.as_ref()) {
                Ok(t) => tools.push(t),
                Err(e) => {
                    let _ = service.cancel().await;
                    Self::kill(&mut child, self.grace).await;
                    return Err(Error::op(name, format!("tool {} rejected: {e}", tool.name)));
                }
            }
        }
        let permits = tools
            .iter()
            .map(|t| {
                let concurrent = self.config.concurrent && t.compat.concurrent;
                (
                    t.manifest.name.clone(),
                    Arc::new(Semaphore::new(if concurrent {
                        CONCURRENT_PERMITS
                    } else {
                        1
                    })),
                )
            })
            .collect();
        let known: Option<Vec<Manifest>> = self
            .tools
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|known| known.iter().map(|t| t.manifest.clone()).collect());
        match known {
            Some(before) => {
                // A restart must serve the tools the registry was built from: the same names
                // with the same manifests, ports and bounds included.
                let after: Vec<Manifest> = tools.iter().map(|t| t.manifest.clone()).collect();
                if before != after {
                    let _ = service.cancel().await;
                    Self::kill(&mut child, self.grace).await;
                    let names =
                        |m: &[Manifest]| m.iter().map(|t| t.name.clone()).collect::<Vec<_>>();
                    return Err(Error::op(
                        name,
                        format!(
                            "the restarted pack does not list the tools it was registered with (before {:?}, after {:?}, or a manifest changed)",
                            names(&before),
                            names(&after)
                        ),
                    ));
                }
            }
            None => {
                *self.tools.lock().unwrap_or_else(|e| e.into_inner()) = Some(tools);
            }
        }
        tracing::info!(pack = %name, pid = child.id().unwrap_or(0), "pack started");
        Ok(Live {
            service,
            child: Mutex::new(Some(child)),
            epoch: CancellationToken::new(),
            permits,
        })
    }

    /// Fails every in-flight call on `live` and shuts it down; a later call starts afresh.
    async fn fail(&self, live: &Arc<Live>, reason: &str) {
        tracing::warn!(pack = %self.name, reason, "pack failed; shutting it down");
        live.epoch.cancel();
        // The slot is held for the whole escalation: a second failer waits here and finds
        // nothing left to stop, and no restart can begin while the old process is going.
        let mut guard = self.live.lock().await;
        if guard.as_ref().is_some_and(|l| Arc::ptr_eq(l, live)) {
            guard.take();
            self.stop(live.clone()).await;
        }
    }

    /// The escalation: stdin close, grace, SIGTERM, grace, SIGKILL. Waits for the exit.
    async fn stop(&self, live: Arc<Live>) {
        // The service owns the transport; cancelling it closes the pack's stdin.
        live.service.cancellation_token().cancel();
        let child = live.child.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(mut child) = child {
            Self::kill(&mut child, self.grace).await;
            tracing::info!(pack = %self.name, "pack stopped");
        }
    }

    async fn kill(child: &mut tokio::process::Child, grace: Duration) {
        drop(child.stdin.take());
        if tokio::time::timeout(grace, child.wait()).await.is_ok() {
            return;
        }
        if let Some(pid) = child.id() {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGTERM,
            );
        }
        if tokio::time::timeout(grace, child.wait()).await.is_ok() {
            return;
        }
        let _ = child.kill().await;
    }

    // --- calls ------------------------------------------------------------------------------

    async fn call_async(
        &self,
        tool: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> CallOutcome {
        let request_bytes = serde_json::to_vec(&arguments)
            .map(|v| v.len() as u64)
            .unwrap_or(0);
        let mut call = PackCall {
            pack: self.name.clone(),
            tool: tool.to_string(),
            target: self.config.command.join(" "),
            request_bytes,
            response_bytes: 0,
            duration_ms: 0,
            status: "error".to_string(),
            outcome: String::new(),
        };
        let started = Instant::now();
        let result = self.call_inner(tool, arguments, &mut call).await;
        call.duration_ms = started.elapsed().as_millis() as u64;
        match &result {
            Ok(_) => {
                call.status = "ok".to_string();
                call.outcome = "ok".to_string();
            }
            Err(e) => {
                call.outcome = e.to_string();
            }
        }
        tracing::debug!(pack = %self.name, tool, status = %call.status, request_bytes, response_bytes = call.response_bytes, duration_ms = call.duration_ms, "pack call finished");
        CallOutcome { result, call }
    }

    async fn call_inner(
        &self,
        tool: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
        call: &mut PackCall,
    ) -> Result<CallToolResult> {
        let name = &self.name;
        let live = self.ensure_live().await?;
        let permits = live
            .permits
            .get(tool)
            .ok_or_else(|| Error::op(name, format!("no tool {tool} in the pack")))?
            .clone();
        let permit = tokio::select! {
            p = permits.acquire() => p.map_err(|e| Error::op(name, e.to_string()))?,
            () = live.epoch.cancelled() => {
                call.status = "cancelled".to_string();
                return Err(Error::op(name, "the pack was shut down before the call started"));
            }
        };
        let timeout = Duration::from_secs(self.config.call_timeout_s);
        let request = ClientRequest::CallToolRequest(CallToolRequest::new(
            CallToolRequestParams::new(tool.to_string()).with_arguments(arguments),
        ));
        let mut options = PeerRequestOptions::no_options();
        options.timeout = Some(timeout);
        let handle = match live
            .service
            .send_request_with_option(request, options)
            .await
        {
            Ok(handle) => handle,
            Err(e) => {
                drop(permit);
                if live.epoch.is_cancelled() {
                    call.status = "cancelled".to_string();
                    return Err(Error::op(
                        name,
                        format!("{tool}: failed because another call on the pack failed"),
                    ));
                }
                call.status = "died".to_string();
                self.fail(&live, "the request could not be sent").await;
                return Err(Error::op(name, format!("{tool}: could not send: {e}")));
            }
        };
        let answered = tokio::select! {
            r = handle.await_response() => r,
            () = live.epoch.cancelled() => {
                drop(permit);
                call.status = "cancelled".to_string();
                return Err(Error::op(name, format!("{tool}: failed because another call on the pack failed")));
            }
        };
        drop(permit);
        match answered {
            Ok(ServerResult::CallToolResult(result)) => {
                call.response_bytes = serde_json::to_vec(&result)
                    .map(|v| v.len() as u64)
                    .unwrap_or(0);
                Ok(result)
            }
            Ok(other) => {
                self.fail(&live, "an unexpected response").await;
                Err(Error::op(
                    name,
                    format!("{tool}: the answer was not a tool result ({other:?})"),
                ))
            }
            Err(ServiceError::Timeout { .. }) => {
                call.status = "timeout".to_string();
                self.fail(&live, "a call timed out").await;
                Err(Error::op(
                    name,
                    format!(
                        "{tool}: no answer within call_timeout_s = {}; the pack was shut down",
                        self.config.call_timeout_s
                    ),
                ))
            }
            Err(ServiceError::Cancelled { reason }) => {
                call.status = "cancelled".to_string();
                self.fail(&live, "a call was cancelled").await;
                Err(Error::op(
                    name,
                    format!(
                        "{tool}: cancelled ({}); the pack was shut down",
                        reason.unwrap_or_default()
                    ),
                ))
            }
            Err(ServiceError::McpError(data)) => {
                // The pack answered an error for this call: the pack is fine, the call is not.
                Err(Error::op(
                    name,
                    format!(
                        "{tool}: the pack answered error {}: {}{}",
                        data.code.0,
                        data.message,
                        data.data
                            .as_ref()
                            .and_then(|d| d.get("class"))
                            .and_then(|c| c.as_str())
                            .map(|c| format!(" (class {c})"))
                            .unwrap_or_default()
                    ),
                ))
            }
            Err(e) => {
                call.status = "died".to_string();
                self.fail(&live, "the connection failed").await;
                Err(Error::op(
                    name,
                    format!("{tool}: the pack went away ({e}); it was shut down"),
                ))
            }
        }
    }
}

impl Drop for Pack {
    fn drop(&mut self) {
        // Stop a live process through the same escalation. On a plain thread the runtime
        // does the waiting; inside another runtime's worker (a server dropping its last
        // handle) blocking is not allowed, so the escalation runs on a thread of its own and
        // the runtime is shut down in the background.
        let Some(runtime) = self.runtime.take() else {
            return;
        };
        let live = self.live.try_lock().ok().and_then(|mut g| g.take());
        let child = live.as_ref().and_then(|live| {
            live.service.cancellation_token().cancel();
            live.child.lock().unwrap_or_else(|e| e.into_inner()).take()
        });
        let grace = self.grace;
        if tokio::runtime::Handle::try_current().is_ok() {
            if let Some(mut child) = child {
                std::thread::spawn(move || {
                    if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        rt.block_on(Self::kill(&mut child, grace));
                    }
                });
            }
            runtime.shutdown_background();
        } else {
            if let Some(mut child) = child {
                runtime.block_on(Self::kill(&mut child, grace));
            }
            drop(runtime);
        }
    }
}

/// The error's text, cut at a character boundary: the peer wrote part of it.
fn summarize_init_error(e: &rmcp::service::ClientInitializeError) -> String {
    let text = e.to_string();
    match text.char_indices().nth(300) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text,
    }
}
