//! One check (SPEC.md "Self-correcting remove", part 3): the source and the candidate copied
//! into the try's own `check/` directory, the configured arguments rendered with their paths and
//! the prompt and written there with `max_spend_usd`, and the checker run as
//! `<command> --arguments <file> --allow-dir <that directory>`. Its printed table-manners result
//! is read, never its exit code, at most [`MAX_ANSWER_BYTES`] within `timeout_s`: a `completed`
//! result with a `verdict` is an answer; anything else is a check failure, with its reason.
//!
//! The journal gets the call as a pack call does: `checker call started:` (the price) before it,
//! `checker call:` (the command, duration, exit, the checker's own spend; never a key) after it,
//! and `checker stored:` with the request's identity and the verdict when it answered.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rascal_core::Error;
use rascal_core::config::CheckerConfig;
use rascal_core::contract::table_manners::{Spend, Verdict};
use rascal_core::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::io::AsyncReadExt;

use crate::filler::{Charge, runtime};

/// The most a checker may print: its result is a header and a few places, far below it.
pub const MAX_ANSWER_BYTES: u64 = 1 << 20;

/// The prefix of a [`CallStarted`] note.
pub const STARTED_PREFIX: &str = "checker call started: ";
/// The prefix of a [`CheckerCall`] note.
pub const CALL_PREFIX: &str = "checker call: ";
/// The prefix of a [`StoredCheck`] note.
pub const STORED_PREFIX: &str = "checker stored: ";

/// The files a check is staged as, in its directory.
const SOURCE: &str = "source.png";
const CANDIDATE: &str = "candidate.png";
const ARGUMENTS: &str = "arguments.json";
/// The checker's printed result, kept beside the check as its evidence.
pub const RESULT: &str = "result.json";

/// What a check judges: the input image (the source) and a try's result (the candidate), with
/// the call's prompt.
pub struct Request<'a> {
    pub source: &'a Path,
    pub candidate: &'a Path,
    pub prompt: &'a str,
}

/// A check staged in its directory, ready to call.
pub struct Staged {
    /// The directory the checker may read, and nothing else.
    pub dir: PathBuf,
    /// The arguments as written to the `--arguments` file.
    pub arguments: Value,
    /// The request's identity: the checker, the source's and the candidate's bytes and the
    /// arguments with digests in place of the paths, so the same request is recognised from
    /// another run directory.
    pub identity: String,
    /// The source's and the candidate's file digests.
    pub source: String,
    pub candidate: String,
}

/// The note journaled before a check, so a run stopped mid-call still counts it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallStarted {
    /// The checker's price per call (`spend_usd_per_call`).
    pub spend_usd: f64,
}

/// What the journal records about a check: never a key, never a pixel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckerCall {
    /// The command as configured.
    pub target: String,
    pub duration_ms: u64,
    /// The process's exit code, recorded and never read (squint exits 1 on `fail`).
    pub exit: Option<i32>,
    pub response_bytes: u64,
    /// The verdict's value, or the check failure.
    pub outcome: String,
    /// Whether the call may have been charged: `known` when its result reports its spend.
    pub charge: Charge,
    /// The spend the checker's result reports, when it printed one.
    pub spend: Option<Spend>,
}

/// A check that answered, for the record and for replay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredCheck {
    /// The try it judged, from 1.
    #[serde(rename = "try")]
    pub attempt: u32,
    /// [`Staged::identity`].
    pub request: String,
    /// The checker's `revision`.
    pub revision: Option<String>,
    /// The source's and the candidate's file digests.
    pub source: String,
    pub candidate: String,
    /// The arguments as the checker was given them.
    pub arguments: Value,
    pub verdict: Verdict,
    /// What the try changed for the next one (the located retry); none for the last try.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<Transition>,
}

/// What one try changed for the next, recorded with its check (`checker stored:`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    /// The crops segmented, one per place, as raw-frame pixel boxes `[x0, y0, x1, y1)`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crops: Vec<[u32; 4]>,
    /// The planes added, by identity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planes: Vec<String>,
    /// The next try's `grow.px`, when an empty answer widened the mask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grow_px: Option<f64>,
}

impl CallStarted {
    pub fn note(&self) -> String {
        format!(
            "{STARTED_PREFIX}{}",
            serde_json::to_string(self).unwrap_or_default()
        )
    }
}

impl CheckerCall {
    pub fn note(&self) -> String {
        format!(
            "{CALL_PREFIX}{}",
            serde_json::to_string(self).unwrap_or_default()
        )
    }
}

impl StoredCheck {
    pub fn note(&self) -> String {
        format!(
            "{STORED_PREFIX}{}",
            serde_json::to_string(self).unwrap_or_default()
        )
    }
}

/// The check a recorded run stored for `request` at try `attempt` (`checker stored:`), from the
/// journal in `dir`: the run's directory, or its workspace when it used one. A checker may
/// answer the same request differently from one try to the next, so a try is answered only by
/// the record of the same try. None when the recording holds none.
pub fn recorded(dir: &Path, request: &str, attempt: u32) -> Result<Option<StoredCheck>> {
    let workspace = rascal_core::workspace::Workspace::open_read_only(dir)?;
    let lines = rascal_core::journal::Journal::read(&workspace.journal_path())?;
    Ok(lines.into_iter().find_map(|line| {
        let text = line.note?.strip_prefix(STORED_PREFIX)?.to_string();
        serde_json::from_str::<StoredCheck>(&text)
            .ok()
            .filter(|stored| stored.request == request && stored.attempt == attempt)
    }))
}

/// Whether the recorded run in `dir` holds any check (`checker stored:`): a replay of it is
/// checked from the recording, needing no checker key.
pub fn any_recorded(dir: &Path) -> Result<bool> {
    let workspace = rascal_core::workspace::Workspace::open_read_only(dir)?;
    let lines = rascal_core::journal::Journal::read(&workspace.journal_path())?;
    Ok(lines.iter().any(|line| {
        line.note
            .as_deref()
            .is_some_and(|n| n.starts_with(STORED_PREFIX))
    }))
}

/// Copies the source and the candidate into `dir` and writes the rendered arguments there.
pub fn stage(config: &CheckerConfig, request: &Request, dir: &Path) -> Result<Staged> {
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let dir = dir.canonicalize().map_err(|e| Error::io(dir, e))?;
    let mut digests = Vec::new();
    for (from, name) in [(request.source, SOURCE), (request.candidate, CANDIDATE)] {
        let bytes = std::fs::read(from).map_err(|e| Error::io(from, e))?;
        let to = dir.join(name);
        std::fs::write(&to, &bytes).map_err(|e| Error::io(&to, e))?;
        digests.push(rascal_core::contract::file_sha256(&bytes));
    }
    let template = serde_json::to_value(&config.arguments)
        .map_err(|e| Error::usage(format!("checker: arguments: {e}")))?;
    let with = |source: &str, candidate: &str| {
        let mut value = render(&template, source, candidate, request.prompt);
        if let Value::Object(object) = &mut value {
            object.insert(
                "max_spend_usd".to_string(),
                serde_json::json!(config.spend_usd_per_call),
            );
        }
        value
    };
    let path = |name: &str| dir.join(name).display().to_string();
    let arguments = with(&path(SOURCE), &path(CANDIDATE));
    let file = dir.join(ARGUMENTS);
    std::fs::write(&file, arguments.to_string()).map_err(|e| Error::io(&file, e))?;
    let identity = {
        let keyed = serde_json::json!({
            "command": config.command,
            "revision": config.revision,
            "arguments": with(&format!("sha256:{}", digests[0]), &format!("sha256:{}", digests[1])),
        });
        format!(
            "sha256:{}",
            rascal_core::contract::file_sha256(keyed.to_string().as_bytes())
        )
    };
    let candidate = digests.pop().unwrap_or_default();
    let source = digests.pop().unwrap_or_default();
    Ok(Staged {
        dir,
        arguments,
        identity,
        source,
        candidate,
    })
}

/// `value` with `{source}`, `{candidate}` and `{prompt}` replaced in every string.
fn render(value: &Value, source: &str, candidate: &str, prompt: &str) -> Value {
    match value {
        Value::String(text) => Value::String(
            text.replace("{source}", source)
                .replace("{candidate}", candidate)
                .replace("{prompt}", prompt),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| render(v, source, candidate, prompt))
                .collect(),
        ),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(k, v)| (k.clone(), render(v, source, candidate, prompt)))
                .collect::<Map<_, _>>(),
        ),
        other => other.clone(),
    }
}

/// What a check produced: the verdict, or why the check could not run; and the record either
/// way.
pub struct Called {
    pub answer: std::result::Result<Verdict, String>,
    pub call: CheckerCall,
    /// The run was cancelled while the check was under way, and the checker stopped.
    pub cancelled: bool,
}

/// How long a stopped checker gets at each step (stdin closed, SIGTERM) before the next, as a
/// pack does.
pub const GRACE: Duration = rascal_mcp::client::GRACE;

/// The checker process under way, so a signal that ends the command stops it too.
static RUNNING: std::sync::Mutex<Option<u32>> = std::sync::Mutex::new(None);

/// Sends SIGTERM to the checker under way, if any: the command line's signal watcher calls it
/// before the process exits.
pub fn stop_running() {
    let pid = RUNNING.lock().ok().and_then(|mut r| r.take());
    rascal_mcp::client::terminate(pid);
}

/// Why a check gave no verdict.
enum Stopped {
    Failed(String),
    Cancelled,
}

impl From<String> for Stopped {
    fn from(reason: String) -> Stopped {
        Stopped::Failed(reason)
    }
}

impl From<&str> for Stopped {
    fn from(reason: &str) -> Stopped {
        Stopped::Failed(reason.to_string())
    }
}

/// Runs the staged check. When `cancel` is set while it runs, the checker is stopped (stdin
/// closed, then SIGTERM, then SIGKILL, each after [`GRACE`]) and the call comes back cancelled.
pub fn call(
    config: &CheckerConfig,
    staged: &Staged,
    cancel: Option<&rascal_core::workspace::Cancel>,
) -> Called {
    let mut call = CheckerCall {
        target: config.command.join(" "),
        duration_ms: 0,
        exit: None,
        response_bytes: 0,
        outcome: String::new(),
        charge: Charge::NotSent,
        spend: None,
    };
    let (answer, cancelled) = match run(config, staged, cancel, &mut call) {
        Ok(verdict) => (Ok(verdict), false),
        Err(Stopped::Failed(reason)) => (Err(reason), false),
        Err(Stopped::Cancelled) => (Err("cancelled".to_string()), true),
    };
    call.outcome = match &answer {
        Ok(verdict) => serde_json::to_value(verdict.value)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default(),
        Err(reason) => reason.clone(),
    };
    Called {
        answer,
        call,
        cancelled,
    }
}

fn run(
    config: &CheckerConfig,
    staged: &Staged,
    cancel: Option<&rascal_core::workspace::Cancel>,
    call: &mut CheckerCall,
) -> std::result::Result<Verdict, Stopped> {
    let timeout = Duration::from_secs(config.timeout_s);
    let started = Instant::now();
    let rt = runtime().map_err(|e| e.to_string())?;
    let printed = rt.block_on(async {
        let mut command = tokio::process::Command::new(&config.command[0]);
        command
            .args(&config.command[1..])
            .arg("--arguments")
            .arg(staged.dir.join(ARGUMENTS))
            .arg("--allow-dir")
            .arg(&staged.dir)
            .current_dir(&staged.dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", config.command[0]))?;
        // Started: whatever happens now may have been charged.
        call.charge = Charge::Unresolved;
        if let Ok(mut running) = RUNNING.lock() {
            *running = child.id();
        }
        let Some(stdout) = child.stdout.take() else {
            return Err(Stopped::from("no stdout to read"));
        };
        let read = async {
            let mut bytes = Vec::new();
            stdout
                .take(MAX_ANSWER_BYTES + 1)
                .read_to_end(&mut bytes)
                .await?;
            let exit = if bytes.len() as u64 > MAX_ANSWER_BYTES {
                None
            } else {
                child.wait().await?.code()
            };
            Ok::<_, std::io::Error>((bytes, exit))
        };
        let watch = async {
            while !cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::SeqCst)) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        };
        let outcome = tokio::select! {
            outcome = tokio::time::timeout(timeout, read) => Some(outcome),
            () = watch => None,
        };
        let Some(outcome) = outcome else {
            rascal_mcp::client::stop_child(&mut child, GRACE).await;
            if let Ok(mut running) = RUNNING.lock() {
                *running = None;
            }
            return Err(Stopped::Cancelled);
        };
        let _ = child.start_kill();
        let _ = child.wait().await;
        if let Ok(mut running) = RUNNING.lock() {
            *running = None;
        }
        Ok(match outcome {
            Err(_) => Err(format!(
                "no answer within {} s; the checker was stopped",
                config.timeout_s
            )),
            Ok(Err(e)) => Err(format!("reading its answer: {e}")),
            Ok(Ok((bytes, _))) if bytes.len() as u64 > MAX_ANSWER_BYTES => Err(format!(
                "its answer is over {} bytes; the checker was stopped",
                MAX_ANSWER_BYTES
            )),
            Ok(Ok(printed)) => Ok(printed),
        }?)
    });
    call.duration_ms = started.elapsed().as_millis() as u64;
    let (bytes, exit) = printed?;
    call.exit = exit;
    call.response_bytes = bytes.len() as u64;
    let _ = std::fs::write(staged.dir.join(RESULT), &bytes);
    let result: Value = serde_json::from_slice(&bytes).map_err(|_| {
        format!(
            "it printed no table-manners result ({} bytes that are not JSON, exit {})",
            bytes.len(),
            exit.map_or("by a signal".to_string(), |c| c.to_string())
        )
    })?;
    if let Some(spend) = result
        .get("spend")
        .and_then(|s| serde_json::from_value::<Spend>(s.clone()).ok())
    {
        call.charge = Charge::Known;
        call.spend = Some(spend);
    }
    match result.get("status").and_then(Value::as_str) {
        Some("completed") => {}
        status => {
            let reason = result
                .pointer("/error/reason")
                .and_then(Value::as_str)
                .map(|r| format!(": {r}"))
                .unwrap_or_default();
            return Err(format!(
                "its run ended {}{reason}",
                status.unwrap_or("without a status")
            )
            .into());
        }
    }
    let verdict = result
        .get("verdict")
        .filter(|v| !v.is_null())
        .ok_or("its completed result has no verdict")?;
    serde_json::from_value(verdict.clone())
        .map_err(|e| Stopped::Failed(format!("its verdict does not read: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_placeholders_are_replaced_in_every_string_and_nothing_else() {
        let template = json!({
            "source": { "path": "{source}" },
            "candidate": { "path": "{candidate}" },
            "requirements": [{ "id": "RQ1", "text": "Any part of {prompt} is visible.", "weight": 2 }]
        });
        assert_eq!(
            render(&template, "/c/source.png", "/c/candidate.png", "the collar"),
            json!({
                "source": { "path": "/c/source.png" },
                "candidate": { "path": "/c/candidate.png" },
                "requirements": [{ "id": "RQ1", "text": "Any part of the collar is visible.", "weight": 2 }]
            })
        );
    }
}
