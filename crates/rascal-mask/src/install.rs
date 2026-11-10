//! The reserved `install` and `uninstall` controls (table-manners 5.3; SPEC.md "Choosable
//! segmenters"): tools beside `describe` and `estimate`, never actions. No run, no workspace, no
//! spend. The work is done by the segmenter pack's installer, `rascal-tools get` and `rascal-tools
//! remove`, started as its own process with the pack's minimal environment (no credential is
//! involved), its output to a log beside the installer's status file, and in its own process
//! group, so an install outlives the request that started it: a client that disconnects leaves it
//! running, and a later `describe` shows `installing` and then how it ended (`last_outcome`). MCP
//! cancellation of the request stops it. An `install` for a model already installing attaches to
//! that install. Progress goes out with the installable's id in `_meta`, never a run.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use rascal_core::config::Config;
use rascal_core::contract::table_manners::{
    InstallError, InstallErrorCode, InstallResult, InstallableState, Refusal, RefusalReason,
    Requirement, RequirementKind, RequirementState,
};
use rascal_core::workspace::Cancel;
use rascal_mcp::server::{ActionResult, CallError, McpTool as Tool, Progress};
use serde_json::{Map, Value, json};

use crate::readiness::MODELS_ENV;
use crate::segmenters::{Segmenters, State};

/// The two controls' names.
pub const INSTALL: &str = "install";
pub const UNINSTALL: &str = "uninstall";

/// An environment, as name and value pairs.
pub type Env = Vec<(String, String)>;

/// How often a running install is looked at.
const POLL: Duration = Duration::from_millis(300);

/// How long an uninstall waits for a cancelled install to end.
const STOP_WAIT: Duration = Duration::from_secs(10);

pub fn handles(name: &str) -> bool {
    name == INSTALL || name == UNINSTALL
}

/// The two tools, each taking `{ "id": <installable> }`.
pub fn tools() -> Vec<Tool> {
    let schema = json!({
        "type": "object",
        "properties": { "id": { "type": "string", "minLength": 1,
            "description": "An installable's id, as the description lists it." } },
        "required": ["id"],
        "additionalProperties": false,
    });
    let schema = Arc::new(schema.as_object().cloned().unwrap_or_default());
    vec![
        Tool::new(
            INSTALL,
            "Install a model the description lists under installables (table-manners install): progress names the installable, cancelling the request stops it, and it keeps going if the client goes away. Runs nothing, spends nothing.",
            schema.clone(),
        ),
        Tool::new(
            UNINSTALL,
            "Remove an installed model, or stop one that is installing (table-manners uninstall). Runs nothing, spends nothing.",
            schema,
        ),
    ]
}

/// Answers one `install` or `uninstall` call.
pub fn call(
    name: &str,
    args: &Map<String, Value>,
    config: Option<&Config>,
    progress: &(dyn Fn(Progress) + Send + Sync),
    cancel: &Cancel,
) -> Result<ActionResult, CallError> {
    let id = args.get("id").and_then(Value::as_str).ok_or_else(|| {
        CallError::from(format!(
            "{name}: the arguments are {{\"id\": <installable>}}"
        ))
    })?;
    let Some((segmenters, launcher, env)) = config.and_then(installer) else {
        return Err(not_ready(name));
    };
    let Some(entry) = segmenters.entry(id).cloned() else {
        return Err(unknown(name, id, &segmenters));
    };
    let result = if name == INSTALL {
        install(&segmenters, &entry.id, &launcher, &env, progress, cancel)
    } else {
        uninstall(&segmenters, &entry.id, &launcher, &env)
    };
    Ok(ActionResult {
        structured: serde_json::to_value(result).unwrap_or(Value::Null),
        is_error: false,
        images: Vec::new(),
    })
}

/// The installer as `rascal tools models|get|remove` runs it: the pack's launcher and its
/// environment (the baseline and the models directory the pack reads). None without a segmenter
/// pack whose install has a catalogue.
pub fn installer_command(config: &Config) -> Option<(PathBuf, Env)> {
    installer(config).map(|(_, launcher, env)| (launcher, env))
}

/// The segmenter pack's catalogue, its installer (the pack's launcher) and the environment the
/// installer gets: the baseline, and the models directory the pack reads.
fn installer(config: &Config) -> Option<(Segmenters, PathBuf, Env)> {
    let segmenters = Segmenters::of(config)?;
    let (_, entry, _) = crate::readiness::segmenter_pack(config)?;
    let launcher = PathBuf::from(entry.command.first()?);
    if !launcher.is_file() {
        return None;
    }
    let mut env: Env = rascal_mcp::client::env::BASELINE
        .iter()
        .filter_map(|name| Some((name.to_string(), std::env::var(name).ok()?)))
        .collect();
    env.push((
        MODELS_ENV.to_string(),
        segmenters.models.display().to_string(),
    ));
    Some((segmenters, launcher, env))
}

fn install(
    segmenters: &Segmenters,
    id: &str,
    launcher: &Path,
    env: &[(String, String)],
    progress: &(dyn Fn(Progress) + Send + Sync),
    cancel: &Cancel,
) -> InstallResult {
    let Some(entry) = segmenters.entry(id) else {
        return result(segmenters, id, None);
    };
    if segmenters.state(entry) == State::Unavailable {
        return InstallResult {
            id: id.to_string(),
            state: InstallableState::Unavailable,
            installation: None,
            error: Some(InstallError {
                code: InstallErrorCode::Unavailable,
                message: format!(
                    "{id} does not run on {} (runs on {})",
                    crate::segmenters::platform(),
                    entry.platforms.join(", ")
                ),
            }),
        };
    }
    progress(Progress::Installable(id.to_string()));
    // Attach to an install already under way; else start one.
    let attached = live_pid(segmenters, id);
    let mut child = match attached {
        Some(_) => None,
        None => match start(
            launcher,
            env,
            &["get", id, "--no-wait"],
            &log(segmenters, id),
        ) {
            Ok(child) => Some(child),
            Err(e) => {
                return failed(
                    segmenters,
                    id,
                    format!("could not start the installer: {e}"),
                );
            }
        },
    };
    let mut last = -1_i64;
    let exit = loop {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            let pid = child.as_ref().map(|c| c.id()).or(attached);
            if let Some(pid) = pid {
                terminate(pid);
            }
            if let Some(child) = child.as_mut() {
                let _ = child.wait();
            }
            return result(segmenters, id, Some(InstallErrorCode::Cancelled));
        }
        let done = match child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(status)) => Some(status.code()),
                Ok(None) => None,
                Err(_) => Some(None),
            },
            None => (live_pid(segmenters, id).is_none()).then_some(None),
        };
        if let Some(code) = done {
            break code;
        }
        if let Some(fraction) = segmenters
            .status(id)
            .and_then(|s| s.get("progress").and_then(Value::as_f64))
        {
            let percent = (fraction.clamp(0.0, 1.0) * 100.0).round() as i64;
            if percent != last {
                last = percent;
                progress(Progress::Line(format!("{id}: {percent}%")));
            }
        }
        std::thread::sleep(POLL);
    };
    if segmenters
        .entry(id)
        .is_some_and(|e| segmenters.state(e) == State::Installed)
        && exit.is_none_or(|c| c == 0)
    {
        return result(segmenters, id, None);
    }
    // It did not install: say why, from the installer's own record of how it ended.
    let outcome = segmenters
        .status(id)
        .and_then(|s| s.get("last_outcome").cloned())
        .filter(|o| o.get("action").and_then(Value::as_str) == Some(INSTALL));
    match outcome.as_ref().and_then(|o| o.get("state")?.as_str()) {
        Some("cancelled") => result(segmenters, id, Some(InstallErrorCode::Cancelled)),
        Some("failed") => {
            let message = outcome
                .as_ref()
                .and_then(|o| o.get("error")?.as_str())
                .unwrap_or("failed")
                .to_string();
            failed(segmenters, id, message)
        }
        _ => {
            let tail = log_tail(&log(segmenters, id));
            if tail.contains("busy") || tail.contains("another") {
                busy(segmenters, id, tail)
            } else {
                failed(segmenters, id, tail)
            }
        }
    }
}

fn uninstall(
    segmenters: &Segmenters,
    id: &str,
    launcher: &Path,
    env: &[(String, String)],
) -> InstallResult {
    // An install under way is cancelled; nothing else is removed.
    if let Some(pid) = live_pid(segmenters, id) {
        terminate(pid);
        let started = std::time::Instant::now();
        while live_pid(segmenters, id).is_some() && started.elapsed() < STOP_WAIT {
            std::thread::sleep(POLL);
        }
        return result(segmenters, id, None);
    }
    let out = start(
        launcher,
        env,
        &["remove", id, "--no-wait"],
        &log(segmenters, id),
    )
    .and_then(|mut child| child.wait());
    match out {
        Ok(status) if status.success() => result(segmenters, id, None),
        Ok(_) => {
            let tail = log_tail(&log(segmenters, id));
            if tail.contains("busy") || tail.contains("another") {
                busy(segmenters, id, tail)
            } else {
                failed(segmenters, id, tail)
            }
        }
        Err(e) => failed(
            segmenters,
            id,
            format!("could not start the installer: {e}"),
        ),
    }
}

/// The installable's state after the call, read from disk, with `code` as its error (none when
/// it did what was asked).
fn result(segmenters: &Segmenters, id: &str, code: Option<InstallErrorCode>) -> InstallResult {
    let (state, installation) = state_of(segmenters, id);
    InstallResult {
        id: id.to_string(),
        state,
        installation,
        error: code.map(|code| InstallError {
            code,
            message: match code {
                InstallErrorCode::Cancelled => format!("{id}: cancelled"),
                _ => format!("{id}: did not finish"),
            },
        }),
    }
}

fn failed(segmenters: &Segmenters, id: &str, message: String) -> InstallResult {
    with_error(segmenters, id, InstallErrorCode::Failed, message)
}

fn busy(segmenters: &Segmenters, id: &str, message: String) -> InstallResult {
    with_error(segmenters, id, InstallErrorCode::Busy, message)
}

fn with_error(
    segmenters: &Segmenters,
    id: &str,
    code: InstallErrorCode,
    message: String,
) -> InstallResult {
    let (state, installation) = state_of(segmenters, id);
    InstallResult {
        id: id.to_string(),
        state,
        installation,
        error: Some(InstallError {
            code,
            message: if message.trim().is_empty() {
                format!("{id}: the installer stopped without saying why")
            } else {
                message.trim().to_string()
            },
        }),
    }
}

/// The state a result reports: never `installing` (the call waits for the install to end).
fn state_of(segmenters: &Segmenters, id: &str) -> (InstallableState, Option<String>) {
    let Some(entry) = segmenters.entry(id) else {
        return (InstallableState::NotInstalled, None);
    };
    match segmenters.state(entry) {
        State::Unavailable => (InstallableState::Unavailable, None),
        State::Installed => (InstallableState::Installed, segmenters.current(id)),
        _ => (InstallableState::NotInstalled, None),
    }
}

/// The process of an install under way, from the installer's status file, when it is alive.
fn live_pid(segmenters: &Segmenters, id: &str) -> Option<u32> {
    let status = segmenters.status(id)?;
    if status.get("state").and_then(Value::as_str) != Some("installing") {
        return None;
    }
    let pid = u32::try_from(status.get("pid")?.as_u64()?).ok()?;
    alive(pid).then_some(pid)
}

fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// SIGTERM, which the installer turns into a clean cancel (its staging deleted, nothing else).
fn terminate(pid: u32) {
    let _ = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Where the installer's output goes: beside its status file, so no pipe ties it to this process.
fn log(segmenters: &Segmenters, id: &str) -> PathBuf {
    segmenters.models.join(".status").join(format!("{id}.log"))
}

fn log_tail(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .to_string()
}

/// Starts the installer with exactly `env`, its output to `log`, in its own process group.
fn start(
    launcher: &Path,
    env: &[(String, String)],
    args: &[&str],
    log: &Path,
) -> std::io::Result<Child> {
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let out = std::fs::File::create(log)?;
    let err = out.try_clone()?;
    let mut command = Command::new(launcher);
    command
        .args(args)
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn()
}

/// No segmenter pack with a catalogue: the controls cannot run (3.1's `not_ready`).
fn not_ready(name: &str) -> CallError {
    let requirement = Requirement {
        kind: RequirementKind::Dependency,
        name: format!("packs.{}", rascal_mcp::provision::PACK_NAME),
        state: RequirementState::Missing,
        setup: Some("Run `rascal tools setup --bundle <bundle> --config <path>`.".to_string()),
        optional: false,
        needed_for: None,
        for_choice: None,
        installable: None,
    };
    let refusal = Refusal {
        reason: RefusalReason::NotReady,
        run: None,
        requirements: Some(vec![requirement]),
        error: None,
        bindings: None,
    };
    CallError::refused(
        format!("{name} is not ready: no segmenter pack with a catalogue is configured"),
        serde_json::to_value(&refusal).unwrap_or(Value::Null),
    )
}

/// An id the description does not list (5.3's `unknown_installable`).
fn unknown(name: &str, id: &str, segmenters: &Segmenters) -> CallError {
    let refusal = Refusal {
        reason: RefusalReason::UnknownInstallable,
        run: None,
        requirements: None,
        error: None,
        bindings: None,
    };
    let ids: Vec<&str> = segmenters.entries.iter().map(|e| e.id.as_str()).collect();
    CallError::refused(
        format!("{name}: no installable {id:?} (have {})", ids.join(", ")),
        serde_json::to_value(&refusal).unwrap_or(Value::Null),
    )
}
