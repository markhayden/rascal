//! The scripted checker (SPEC.md "Self-correcting remove", part 3): a table-manners tool call,
//! `fake-checker <script> --arguments <file> --allow-dir <dir>`, that answers from a script file
//! the way a checker answers, so `heal remove-prompt`'s check and retry run offline. It is
//! reachable as `rascal fake-checker` in a `--features testkit` build.
//!
//! The script is JSON:
//!
//! ```json
//! { "answers": [ { "verdict": "fail", "where": [[0.1, 0.2, 0.3, 0.4]] }, { "verdict": "pass" } ],
//!   "spend_usd": 0.004 }
//! ```
//!
//! The `n`th call (counted in `<script>.calls` beside the script) takes `answers[n]`, the last
//! answer once they run out. An answer holds:
//!
//! - `verdict`: `pass`, `fail` or `unsure`;
//! - `where`: the places as boxes `[x0, y0, x1, y1]` in fractions of the candidate, each written
//!   as a table-manners `bbox` location on the candidate (its file digest and size);
//! - `foreign`: the places name another image than the candidate;
//! - `delay_ms`: a pause before answering (a timeout);
//! - `failure`: `status` (a completed run is not what it answers: `error`), `no-verdict` (a
//!   completed result without a verdict), `garbage` (text that is not JSON), `oversize` (a result
//!   over 1 MiB) or `outside` (it also tries to read the script, outside `--allow-dir`, and
//!   refuses itself as a real tool refuses a path outside its allowed directories).
//!
//! Like squint it reads both images (each `{"path": …}` in the arguments, which must lie inside
//! `--allow-dir`, else the run is `invalid`), prints its result on stdout and exits 1 on `fail`
//! and 2 on `unsure`, so a caller that reads the exit code is wrong. Each call's arguments, the
//! allowed directory, the working directory, the candidate's file digest and the process id are
//! written to
//! `<script>.seen-<n>.json`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

/// Runs one call: `args` are what follows `fake-checker`. Returns the exit code.
pub fn run(args: &[String]) -> i32 {
    match call(args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("fake-checker: {message}");
            64
        }
    }
}

fn call(args: &[String]) -> Result<i32, String> {
    let (script, arguments, allow_dir) = match args {
        [script, a, arguments, d, allow_dir] if a == "--arguments" && d == "--allow-dir" => (
            PathBuf::from(script),
            PathBuf::from(arguments),
            PathBuf::from(allow_dir),
        ),
        _ => {
            return Err(
                "usage: fake-checker <script> --arguments <file> --allow-dir <dir>".to_string(),
            );
        }
    };
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let plan: Value = serde_json::from_str(&read(&script)?).map_err(|e| e.to_string())?;
    let calls = script.with_extension("calls");
    let n: usize = std::fs::read_to_string(&calls)
        .ok()
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0);
    std::fs::write(&calls, (n + 1).to_string()).map_err(|e| e.to_string())?;
    let answers = plan["answers"].as_array().cloned().unwrap_or_default();
    let answer = answers
        .get(n)
        .or(answers.last())
        .cloned()
        .unwrap_or_else(|| json!({ "verdict": "pass" }));
    let arguments: Value = serde_json::from_str(&read(&arguments)?).map_err(|e| e.to_string())?;
    let seen = script.with_extension(format!("seen-{n}.json"));
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let candidate_sha256 = arguments["candidate"]["path"]
        .as_str()
        .and_then(|p| std::fs::read(p).ok())
        .map(|bytes| rascal_core::contract::file_sha256(&bytes));
    std::fs::write(
        &seen,
        json!({ "arguments": arguments, "allow_dir": allow_dir, "cwd": cwd, "candidate_sha256": candidate_sha256, "pid": std::process::id() })
            .to_string(),
    )
    .map_err(|e| e.to_string())?;
    if let Some(ms) = answer["delay_ms"].as_u64() {
        std::thread::sleep(Duration::from_millis(ms));
    }
    let failure = answer["failure"].as_str().unwrap_or("");
    let mut paths = Vec::new();
    paths_in(&arguments, &mut paths);
    if failure == "outside" {
        paths.push(script.clone());
    }
    let spend = plan["spend_usd"].as_f64().unwrap_or(0.0);
    let mut images = Vec::new();
    for path in &paths {
        match read_inside(path, &allow_dir) {
            Ok(image) => images.push((path.clone(), image)),
            Err(reason) => return Ok(print(&header("invalid", Some(reason), 0.0, None), 64)),
        }
    }
    let candidate = arguments["candidate"]["path"]
        .as_str()
        .and_then(|c| images.iter().find(|(p, _)| p == Path::new(c)))
        .map(|(_, image)| image.clone())
        .ok_or("the arguments name no candidate {\"path\": …}")?;
    let value = answer["verdict"].as_str().unwrap_or("pass");
    let mut reference = candidate;
    if answer["foreign"].as_bool() == Some(true) {
        reference["file_sha256"] = json!("0".repeat(64));
    }
    let places: Vec<Value> = answer["where"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|bbox| {
            json!({
                "image": reference, "kind": "bbox", "parts": [{ "bbox": bbox }],
                "straddles_wrap": false,
                "source": { "tool": "fake-checker", "version": "0", "item": "RQ1" }
            })
        })
        .collect();
    let verdict = json!({ "value": value, "where": places });
    let code = match value {
        "fail" => 1,
        "unsure" => 2,
        _ => 0,
    };
    match failure {
        "status" => Ok(print(
            &header(
                "error",
                Some("the provider answered 529 three times".to_string()),
                spend,
                None,
            ),
            3,
        )),
        "no-verdict" => Ok(print(&header("completed", None, spend, None), 0)),
        "garbage" => {
            println!("this is not a result");
            Ok(0)
        }
        "oversize" => {
            let mut result = header("completed", None, spend, Some(verdict));
            result["notes"] = json!(["x".repeat(1 << 20)]);
            Ok(print(&result, code))
        }
        _ => Ok(print(
            &header("completed", None, spend, Some(verdict)),
            code,
        )),
    }
}

/// Every `{"path": …}` value in the arguments.
fn paths_in(value: &Value, out: &mut Vec<PathBuf>) {
    match value {
        Value::Object(object) => {
            if let Some(Value::String(path)) = object.get("path") {
                out.push(PathBuf::from(path));
            }
            for v in object.values() {
                paths_in(v, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|v| paths_in(v, out)),
        _ => {}
    }
}

/// The image at `path`, read only when it lies inside `allow_dir`, as its table-manners
/// reference (its file digest and size, a flat bounded picture).
fn read_inside(path: &Path, allow_dir: &Path) -> Result<Value, String> {
    let dir = allow_dir
        .canonicalize()
        .map_err(|e| format!("--allow-dir {}: {e}", allow_dir.display()))?;
    let file = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !file.starts_with(&dir) {
        return Err(format!(
            "{} is outside the allowed directories",
            path.display()
        ));
    }
    let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let image = image::load_from_memory(&bytes)
        .map_err(|e| format!("{}: not an image: {e}", file.display()))?;
    Ok(json!({
        "file_sha256": rascal_core::contract::file_sha256(&bytes),
        "width": image.width(), "height": image.height(),
        "projection": "flat", "topology": "bounded"
    }))
}

/// A table-manners result header with the given status, error reason, spend and verdict.
fn header(status: &str, reason: Option<String>, spend: f64, verdict: Option<Value>) -> Value {
    let mut result = json!({
        "contract": rascal_core::contract::CONTRACT_VERSION,
        "tool": { "name": "fake-checker", "version": "0" },
        "action": "check",
        "run": { "id": "check-1", "resource": "fake-checker://run/check-1/run.json",
                 "session_id": null, "client_run_id": null, "dir": null },
        "status": status,
        "error": reason.map(|r| json!({
            "code": if status == "invalid" { "input" } else { "provider" },
            "retryable": status != "invalid",
            "reason": r
        })),
        "started": "2026-11-04T00:00:00.000Z",
        "finished": "2026-11-04T00:00:00.000Z",
        "duration_ms": 0,
        "input_identity": "sha256:0",
        "invocation": null,
        "spend": { "usd": spend, "unresolved_usd": 0.0, "basis": "measured" },
        "cache": "off",
        "notes": [],
        "body": {}
    });
    if let Some(verdict) = verdict {
        result["verdict"] = verdict;
    }
    result
}

fn print(result: &Value, code: i32) -> i32 {
    println!("{result}");
    code
}
