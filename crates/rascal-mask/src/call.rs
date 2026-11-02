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

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use rascal_core::workspace::Cancel;
use rascal_mcp::server::{Actions, CallError, CallMeta, Progress};
use serde_json::{Map, Value};

use crate::actions::ContractActions;
use crate::exit;
use crate::serve::{ServeArgs, contract_actions};

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
    let cancel: Arc<Cancel> = Arc::new(Cancel::new(false));
    watch_signals(cancel.clone());
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
