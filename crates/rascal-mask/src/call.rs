//! `rascal call <action> --arguments <file.json|->` (CONTRACT 4.8; PLAN.md 10.12): runs any
//! action from exactly the arguments an MCP call would send, through the same
//! `ContractActions` as `serve` (the same parsing, readiness, binding, limits and run) with no
//! session: `session_id` null, no budget ledger, the cap still applied. Paths in the arguments
//! follow the path rule with the given `--allow-dir` (none refusing every path; inline images
//! need none). The run goes under `--runs` and the store is `--workspace`, `serve`'s defaults.
//!
//! It prints the result, the run's `run.json`, on one line of stdout and exits as a run does:
//! 0 `completed`, 3 `error`, 64 `invalid`, 130 or 143 when SIGINT or SIGTERM cancelled it. A
//! refusal `serve` sends as a JSON-RPC error (`not_ready`, `binding`, `invalid_arguments`)
//! prints its refusal JSON and exits 64; anything else wrong before a run (an unknown action, a
//! reserved tool, arguments that are not an object) is said on stderr and exits 64. So a
//! result's `invocation.arguments`, with a `path` beside each image's `expect`, are the file for
//! `rascal call <invocation.action>`.
//!
//! `rascal call --bundle <path>` (CONTRACT 4.9; PLAN.md 10.13) replays an invocation bundle a
//! host built ([`replay`]): the file under the path rule, checked whole by
//! `rascal_core::invocation_bundle::read` before anything acts; every step refused up front when
//! it is another tool's or lists `reference_only` inputs or `unportable` artifacts; then each
//! step in order: its artifact members imported (10.10's import, checked whole, idempotent), its
//! input members copied into rascal's replay area (`$XDG_DATA_HOME/rascal/replay/<bundle
//! digest>/`, which the path rule admits for the replay), its invocation's arguments called with
//! a `path` beside each image's `expect`, and a reference to an earlier step's output bound to
//! that step's replayed output by port. One result per line; it stops at the first step that
//! does not complete, with that step's exit code.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use rascal_core::workspace::Cancel;
use rascal_mcp::server::{Actions, CallError, CallMeta, Progress};
use serde_json::{Map, Value};

use rascal_core::artifact::ArtifactType;
use rascal_core::config::ConfigState;
use rascal_core::invocation_bundle::{self, InvocationBundle};
use rascal_core::paths::PathRule;

use crate::actions::ContractActions;
use crate::exit;
use crate::serve::{ServeArgs, contract_actions};

/// What `rascal call --bundle` was given.
pub struct BundleArgs {
    pub bundle: PathBuf,
    pub config: Option<PathBuf>,
    pub allow_dirs: Vec<PathBuf>,
    pub workspace: Option<PathBuf>,
    pub runs: Option<PathBuf>,
}

/// What `rascal call` was given.
pub struct CallArgs {
    pub action: String,
    /// The arguments' file, or `-` for stdin.
    pub arguments: PathBuf,
    pub config: Option<PathBuf>,
    pub allow_dirs: Vec<PathBuf>,
    pub workspace: Option<PathBuf>,
    pub runs: Option<PathBuf>,
}

/// The exit code of a cancelling signal, once one arrived (0 until then).
static SIGNALLED: AtomicI32 = AtomicI32::new(0);

/// Runs the call and returns the process's exit code.
pub fn call(args: &CallArgs) -> i32 {
    let arguments = match read_arguments(&args.arguments) {
        Ok(arguments) => arguments,
        Err(message) => {
            eprintln!("rascal call: {message}");
            return exit::USAGE;
        }
    };
    let actions = match contract_actions(
        &ServeArgs {
            workspace: args.workspace.clone(),
            config: args.config.clone(),
            concurrency: 1,
            runs: args.runs.clone(),
            allow_dirs: args.allow_dirs.clone(),
        },
        None,
        "rascal call",
    ) {
        Ok(actions) => actions,
        Err(error) => {
            eprintln!("rascal call: {error}");
            return exit::code_for(&error);
        }
    };
    run(&actions, &args.action, &arguments)
}

/// Calls `action` with `arguments` on `actions` and prints what it gave: the run's result, or
/// the refusal. Returns the exit code. Shared with the bundle replay (PLAN.md 10.13), which
/// prints one line per step.
pub fn run(actions: &ContractActions, action: &str, arguments: &Map<String, Value>) -> i32 {
    match outcome(actions, action, arguments) {
        Outcome::Result(result) => {
            println!("{result}");
            exit_of(&result)
        }
        Outcome::Refused(data) => {
            println!("{data}");
            exit::USAGE
        }
        Outcome::Failed(message) => {
            eprintln!("rascal call {action}: {message}");
            exit::USAGE
        }
    }
}

/// How a call ended: a run's result, a refusal the contract names, or a failure before a run.
pub enum Outcome {
    Result(Value),
    Refused(Value),
    Failed(String),
}

/// Calls `action` as `serve` would, with no session, cancelled by SIGINT or SIGTERM.
pub fn outcome(actions: &ContractActions, action: &str, arguments: &Map<String, Value>) -> Outcome {
    if !actions.is_action(action) {
        return Outcome::Failed(format!(
            "no action {action} (rascal describe --json lists them; describe, estimate, export and import are tools, not actions)"
        ));
    }
    let cancel = cancel_flag();
    let run_id = std::sync::Mutex::new(None::<String>);
    let progress = |p: Progress| match p {
        Progress::Run(reference) => {
            if let Some(id) = reference["id"].as_str() {
                *run_id.lock().unwrap_or_else(|e| e.into_inner()) = Some(id.to_string());
            }
        }
        Progress::Line(line) => eprintln!("rascal call {action}: {line}"),
    };
    match actions.call(action, arguments, &CallMeta::default(), &progress, cancel) {
        Ok(result) => Outcome::Result(result.structured),
        Err(CallError {
            data: Some(data), ..
        }) => Outcome::Refused(data),
        Err(CallError { message, .. }) => {
            // A cancelled run writes its `run.json` and returns no result: print that.
            let id = run_id.lock().unwrap_or_else(|e| e.into_inner()).clone();
            match id.and_then(|id| written(&actions.runs, &id)) {
                Some(result) => Outcome::Result(result),
                None => Outcome::Failed(message),
            }
        }
    }
}

/// The exit code of a run's result (table-manners 6): its status's, a cancellation the
/// signal's.
pub fn exit_of(result: &Value) -> i32 {
    match result["status"].as_str() {
        Some("completed") => exit::OK,
        Some("error") => exit::OPERATIONAL,
        Some("cancelled") => match SIGNALLED.load(Ordering::SeqCst) {
            0 => exit::INTERRUPTED,
            code => code,
        },
        _ => exit::USAGE,
    }
}

/// A run's `run.json`, when it wrote one.
fn written(runs: &Path, id: &str) -> Option<Value> {
    let text = std::fs::read_to_string(runs.join(id).join(crate::run_json::FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// The arguments: one JSON object, from the file or stdin.
fn read_arguments(path: &Path) -> Result<Map<String, Value>, String> {
    let text = if path == Path::new("-") {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| format!("--arguments -: {e}"))?;
        text
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("--arguments {}: {e}", path.display()))?
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err("--arguments: not a JSON object (an MCP call's arguments)".to_string()),
        Err(e) => Err(format!("--arguments: not JSON: {e}")),
    }
}

/// The process's cancel flag, its signal watcher started the first time it is asked for.
fn cancel_flag() -> Arc<Cancel> {
    static FLAG: std::sync::OnceLock<Arc<Cancel>> = std::sync::OnceLock::new();
    FLAG.get_or_init(|| {
        let cancel = Arc::new(Cancel::new(false));
        watch_signals(cancel.clone());
        cancel
    })
    .clone()
}

/// Sets `cancel` when SIGINT or SIGTERM arrives, remembering the exit code (130, 143): the run
/// stops before its next commit and writes `run.json` `cancelled`, as a cancelled MCP call does.
fn watch_signals(cancel: Arc<Cancel>) {
    std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        let code = runtime.block_on(async {
            use tokio::signal::unix::{SignalKind, signal};
            let (Ok(mut int), Ok(mut term)) = (
                signal(SignalKind::interrupt()),
                signal(SignalKind::terminate()),
            ) else {
                return None;
            };
            tokio::select! {
                _ = int.recv() => Some(exit::INTERRUPTED),
                _ = term.recv() => Some(exit::TERMINATED),
            }
        });
        if let Some(code) = code {
            SIGNALLED.store(code, Ordering::SeqCst);
            cancel.store(true, Ordering::SeqCst);
        }
    });
}

/// Replays an invocation bundle (module docs) and returns the process's exit code.
pub fn replay(args: &BundleArgs) -> i32 {
    let fail = |message: String| {
        eprintln!("rascal call --bundle: {message}");
        exit::USAGE
    };
    let state = ConfigState::read(args.config.as_deref());
    let rule = match crate::serve::allowed(&args.allow_dirs, &state, "rascal call") {
        Ok(rule) => rule,
        Err(error) => return fail(error.to_string()),
    };
    let bundle = match rule
        .admit("--bundle", &args.bundle)
        .and_then(|path| invocation_bundle::read_file(&path))
        .and_then(|bytes| invocation_bundle::read(&bytes))
    {
        Ok(bundle) => bundle,
        Err(error) => return fail(error.to_string()),
    };
    if let Err(message) = replayable(&bundle) {
        return fail(message);
    }
    let area = match replay_area(&bundle) {
        Ok(area) => area,
        Err(message) => return fail(message),
    };
    let mut actions = match contract_actions(
        &ServeArgs {
            workspace: args.workspace.clone(),
            config: args.config.clone(),
            concurrency: 1,
            runs: args.runs.clone(),
            allow_dirs: args.allow_dirs.clone(),
        },
        None,
        "rascal call",
    ) {
        Ok(actions) => actions,
        Err(error) => {
            eprintln!("rascal call --bundle: {error}");
            return exit::code_for(&error);
        }
    };
    // The replay area is rascal's own, never a caller's path: the rule admits it for the replay.
    if let PathRule::Allowed(dirs) = &mut actions.paths {
        dirs.push(area.clone());
    }
    // Each earlier step's outputs: the identity the bundle named, to the replayed reference.
    let mut replayed: BTreeMap<String, (String, Value)> = BTreeMap::new();
    for (n, step) in bundle.steps.iter().enumerate() {
        let at = n + 1;
        let action = step.invocation["action"].as_str().unwrap_or_default();
        for artifact in &step.artifacts {
            let bytes = &bundle.members[&artifact.member];
            if let Err(failure) = crate::transfer::import_bytes(
                &actions.store,
                bytes,
                None,
                &artifact.member,
                Some(&artifact.sha256),
            ) {
                return fail(format!("step {at} ({action}): {}", failure.message()));
            }
        }
        let mut arguments = step.invocation["arguments"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        for input in &step.inputs {
            let file = area.join(input.member.trim_start_matches(invocation_bundle::INPUTS));
            match arguments.get_mut(input.name()) {
                Some(Value::Object(given)) if given.contains_key("identity") => {
                    // Bound by reference: the image must be in the store, by its file digest.
                    if let Err(message) = hold_image(&actions, &file, &input.image) {
                        return fail(format!("step {at} ({action}): {message}"));
                    }
                }
                Some(Value::Object(given)) => {
                    given.insert(
                        "path".to_string(),
                        Value::String(file.display().to_string()),
                    );
                }
                _ => {}
            }
        }
        let rebound = rebind(&mut arguments, &replayed);
        match outcome(&actions, action, &arguments) {
            Outcome::Result(mut result) => {
                if let Some(notes) = result["notes"].as_array_mut() {
                    notes.extend(rebound.into_iter().map(Value::String));
                }
                println!("{result}");
                if result["status"] != "completed" {
                    return exit_of(&result);
                }
                record_outputs(&step.result, &result, &mut replayed);
            }
            Outcome::Refused(data) => {
                println!("{data}");
                return exit::USAGE;
            }
            Outcome::Failed(message) => return fail(format!("step {at} ({action}): {message}")),
        }
    }
    exit::OK
}

/// Refuses, before anything runs, a step of another tool, one with `reference_only` inputs or
/// `unportable` artifacts, and one whose invocation names no action and arguments.
fn replayable(bundle: &InvocationBundle) -> Result<(), String> {
    for (n, step) in bundle.steps.iter().enumerate() {
        let at = n + 1;
        let action = step.invocation["action"].as_str().unwrap_or_default();
        if step.tool.name != rascal_core::artifact::TOOL {
            return Err(format!(
                "step {at} ({action}) was made by {} {}; rascal replays only its own steps; nothing ran",
                step.tool.name, step.tool.version
            ));
        }
        if !step.reference_only.is_empty() {
            let named: Vec<String> = step
                .reference_only
                .iter()
                .map(|r| {
                    format!(
                        "{} ({})",
                        r.slot.as_deref().or(r.port.as_deref()).unwrap_or_default(),
                        r.why
                    )
                })
                .collect();
            return Err(format!(
                "step {at} ({action}) lists reference_only inputs, which the bundle does not carry: {}; nothing ran",
                named.join(", ")
            ));
        }
        if !step.unportable.is_empty() {
            let named: Vec<String> = step
                .unportable
                .iter()
                .map(|u| {
                    format!(
                        "{} {} ({})",
                        u.port,
                        u.artifact["identity"].as_str().unwrap_or_default(),
                        u.reason
                    )
                })
                .collect();
            return Err(format!(
                "step {at} ({action}) lists unportable artifacts, so it depends on a workspace the bundle does not carry: {}; nothing ran",
                named.join(", ")
            ));
        }
        if action.is_empty() || !step.invocation["arguments"].is_object() {
            return Err(format!(
                "step {at}: its invocation names no action and arguments; nothing ran"
            ));
        }
    }
    Ok(())
}

/// rascal's replay area for this bundle, `$XDG_DATA_HOME/rascal/replay/<digest>/`, with every
/// input member written there (whole: a temporary name, then a rename), canonical.
fn replay_area(bundle: &InvocationBundle) -> Result<PathBuf, String> {
    let digest = bundle.sha256.trim_start_matches("sha256:");
    let dir = crate::actions::data_home()
        .join("rascal/replay")
        .join(digest);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (member, bytes) in &bundle.members {
        let Some(name) = member.strip_prefix(invocation_bundle::INPUTS) else {
            continue;
        };
        let path = dir.join(name);
        if std::fs::read(&path).is_ok_and(|held| &held == bytes) {
            continue;
        }
        let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
        std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    std::fs::canonicalize(&dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// Puts the image file into the store when it does not hold it by file digest, in the
/// projection its reference names, so a reference to it binds.
fn hold_image(actions: &ContractActions, file: &Path, image: &Value) -> Result<(), String> {
    let digest = image["file_sha256"].as_str().unwrap_or_default();
    let held = actions
        .store
        .resolve(ArtifactType::Image, &format!("sha256:{digest}"))
        .map_err(|e| e.to_string())?;
    if held.is_none() {
        let projection = match image["projection"].as_str() {
            Some("equirectangular") => Some(rascal_core::frame::Projection::Equirectangular),
            Some("flat") => Some(rascal_core::frame::Projection::Flat),
            _ => None,
        };
        actions
            .store
            .import(file, projection)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Binds each reference to an earlier step's output to that step's replayed output (CONTRACT
/// 4.9: by port, whatever its identity). Returns a note for each one whose identity changed.
fn rebind(
    arguments: &mut Map<String, Value>,
    replayed: &BTreeMap<String, (String, Value)>,
) -> Vec<String> {
    let mut notes = Vec::new();
    for (name, value) in arguments.iter_mut() {
        let items: Vec<&mut Value> = match value {
            Value::Array(items) => items.iter_mut().collect(),
            other => vec![other],
        };
        for item in items {
            let Some(identity) = item.get("identity").and_then(Value::as_str) else {
                continue;
            };
            let Some((port, reference)) = replayed.get(identity) else {
                continue;
            };
            if reference["identity"] != item["identity"] {
                notes.push(format!(
                    "replay: {name} named the earlier step's {port} output {identity}; bound to the replayed {}",
                    reference["identity"].as_str().unwrap_or_default()
                ));
            }
            *item = reference.clone();
        }
    }
    notes
}

/// Each output the bundle's result names, to the replayed result's output at the same place
/// (the same port and position).
fn record_outputs(
    original: &Value,
    result: &Value,
    replayed: &mut BTreeMap<String, (String, Value)>,
) {
    let theirs = original["artifacts"]["out"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let ours = result["artifacts"]["out"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for (then, now) in theirs.iter().zip(&ours) {
        if then["port"] != now["port"] {
            continue;
        }
        if let Some(identity) = then["artifact"]["identity"].as_str() {
            replayed.insert(
                identity.to_string(),
                (
                    then["port"].as_str().unwrap_or_default().to_string(),
                    now["artifact"].clone(),
                ),
            );
        }
    }
}
