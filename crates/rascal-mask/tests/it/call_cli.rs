//! Task 10.12 (CONTRACT 4.8): `rascal call <action> --arguments <file|->` runs any action from
//! exactly the arguments an MCP call sends, through the same actions as `serve` with no
//! session. A result's invocation, with a `path` on each image, called again gives the same
//! invocation and the same outputs, for a task and for an operation. The exit codes are the
//! commands' (0, 3, 64, 130), a refusal prints its JSON and exits 64, a path outside
//! `--allow-dir` is refused, and an inline image needs no flag.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use rascal_core::contract::table_manners;
use rascal_testkit::mcp_client::Client;

fn repo_root() -> PathBuf {
    std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap()
}

fn pano() -> PathBuf {
    repo_root().join("fixtures/images/step-pano.png")
}

fn floor() -> PathBuf {
    repo_root().join("fixtures/images/floor.png")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-call-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A configuration with `fake`, a command filler that answers, and `failing`, one that fails.
fn config(dir: &Path) -> PathBuf {
    let path = dir.join("rascal.toml");
    let bin = env!("CARGO_BIN_EXE_rascal");
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\ntimeout_s = 30\nrevision = \"fake\"\n\n[fillers.failing]\ncommand = [{bin:?}, \"fake-filler\", \"failing\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\ntimeout_s = 30\nrevision = \"failing\"\n"
        ),
    )
    .unwrap();
    path
}

/// `serve` over `dir`'s store and runs, with `config` when given.
fn serve(dir: &Path, config: Option<&Path>) -> Client {
    let mut command = rascal_testkit::mcp_client::serve_command(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "--workspace",
            dir.join("ws").to_str().unwrap(),
            "--runs",
            dir.join("served").to_str().unwrap(),
        ])
        .env_remove("RASCAL_LOG");
    if let Some(config) = config {
        command.args(["--config", config.to_str().unwrap()]);
    }
    Client::start(&mut command).unwrap()
}

/// `rascal call <action> --arguments <file>` over `dir`'s store, its runs under `dir/runs`.
fn call_command(
    dir: &Path,
    action: &str,
    arguments: &serde_json::Value,
    extra: &[&str],
) -> Command {
    let file = dir.join(format!("{action}-args.json"));
    std::fs::write(&file, arguments.to_string()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "call",
            action,
            "--arguments",
            file.to_str().unwrap(),
            "--workspace",
            dir.join("ws").to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .args(extra)
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .env_remove("RASCAL_LOG")
        .env_remove("RASCAL_TESTKIT_OPERATION_DELAY_MS");
    command
}

fn call(dir: &Path, action: &str, arguments: &serde_json::Value, extra: &[&str]) -> Output {
    call_command(dir, action, arguments, extra)
        .output()
        .unwrap()
}

/// What a call printed: one JSON document on one line.
fn printed(out: &Output) -> serde_json::Value {
    let text = String::from_utf8(out.stdout.clone()).unwrap();
    assert_eq!(text.trim_end().lines().count(), 1, "{text}");
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{e}: {text}\n{}", String::from_utf8_lossy(&out.stderr)))
}

struct Embedded;

impl jsonschema::Retrieve for Embedded {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let text = table_manners::SCHEMAS
            .iter()
            .find(|(id, _)| *id == uri.as_str())
            .map(|(_, text)| *text)
            .ok_or_else(|| format!("no embedded schema {uri}"))?;
        Ok(serde_json::from_str(text)?)
    }
}

fn valid(id: &str, value: &serde_json::Value) {
    let (_, text) = table_manners::SCHEMAS
        .iter()
        .find(|(i, _)| *i == id)
        .unwrap();
    let schema: serde_json::Value = serde_json::from_str(text).unwrap();
    let validator = jsonschema::options()
        .with_retriever(Embedded)
        .build(&schema)
        .unwrap();
    let errors: Vec<String> = validator
        .iter_errors(value)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{id}: {errors:?}\n{value:#}");
}

/// A result of `rascal call` with status `status` and exit `code`, valid and session-less.
fn result(out: &Output, status: &str, code: i32) -> serde_json::Value {
    let r = printed(out);
    assert_eq!(
        out.status.code(),
        Some(code),
        "{r:#}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    valid("urn:table-manners:core:result", &r);
    assert_eq!(r["status"], status, "{r:#}");
    assert_eq!(r["run"]["session_id"], serde_json::Value::Null);
    r
}

/// An invocation's arguments as a call's: a `path` beside each image's `expect`.
fn with_path(invocation: &serde_json::Value, slot: &str, path: &Path) -> serde_json::Value {
    let mut arguments = invocation["arguments"].clone();
    arguments[slot]["path"] = serde_json::json!(path.to_str().unwrap());
    arguments
}

#[test]
fn a_task_invocation_called_again_gives_the_same_invocation_and_outputs() {
    let dir = scratch("task");
    let config = config(&dir);
    let mut client = serve(&dir, Some(&config));
    let served = client
        .call(
            "seam",
            serde_json::json!({ "image": { "path": pano().to_str().unwrap() }, "filler": "fake" }),
        )
        .unwrap()
        .structured
        .unwrap();
    client.close().unwrap();
    assert_eq!(served["status"], "completed", "{served:#}");
    let invocation = &served["invocation"];
    let root = repo_root();
    let out = call(
        &dir,
        "seam",
        &with_path(invocation, "image", &pano()),
        &[
            "--config",
            config.to_str().unwrap(),
            "--allow-dir",
            root.to_str().unwrap(),
        ],
    );
    let r = result(&out, "completed", 0);
    assert_eq!(&r["invocation"], invocation);
    // The same Mask, and the same images out (by digest; each run locates its own files).
    assert_eq!(
        r["artifacts"]["out"][0]["artifact"],
        served["artifacts"]["out"][0]["artifact"]
    );
    let digests = |r: &serde_json::Value| -> Vec<serde_json::Value> {
        r["images"]["out"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["image"].clone())
            .collect()
    };
    assert_eq!(digests(&r), digests(&served));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Line → Rasterize → Feather served; the Feather's invocation called again on the store.
fn feathered(dir: &Path) -> serde_json::Value {
    let mut client = serve(dir, None);
    let line = client
        .call(
            "line",
            serde_json::json!({ "image": { "path": floor().to_str().unwrap() }, "column": 0.5, "width_px": 24 }),
        )
        .unwrap()
        .structured
        .unwrap();
    let rasterized = client
        .call(
            "rasterize",
            serde_json::json!({ "location": line["artifacts"]["out"][0]["artifact"],
                                "image": line["artifacts"]["in"][0]["artifact"] }),
        )
        .unwrap()
        .structured
        .unwrap();
    let feathered = client
        .call(
            "feather",
            serde_json::json!({ "mask": rasterized["artifacts"]["out"][0]["artifact"], "sigma_px": 6 }),
        )
        .unwrap()
        .structured
        .unwrap();
    client.close().unwrap();
    assert_eq!(feathered["status"], "completed", "{feathered:#}");
    feathered
}

#[test]
fn an_operation_invocation_called_again_gives_the_same_invocation_and_outputs() {
    let dir = scratch("operation");
    let served = feathered(&dir);
    let invocation = &served["invocation"];
    let out = call(&dir, "feather", &invocation["arguments"], &[]);
    let r = result(&out, "completed", 0);
    assert_eq!(&r["invocation"], invocation);
    assert_eq!(
        r["artifacts"]["out"][0]["artifact"],
        served["artifacts"]["out"][0]["artifact"]
    );
    // `--arguments -` reads them from stdin.
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    let mut child = command
        .args([
            "call",
            "feather",
            "--arguments",
            "-",
            "--workspace",
            dir.join("ws").to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin
            .write_all(invocation["arguments"].to_string().as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    let r = result(&out, "completed", 0);
    assert_eq!(&r["invocation"], invocation);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn each_outcome_has_its_exit_code() {
    let dir = scratch("codes");
    let config = config(&dir);
    let root = repo_root();
    let allow = ["--allow-dir", root.to_str().unwrap()];
    // 3: a filler that fails.
    let mut flags = vec!["--config", config.to_str().unwrap()];
    flags.extend(allow);
    let out = call(
        &dir,
        "seam",
        &serde_json::json!({ "image": { "path": pano().to_str().unwrap() }, "filler": "failing" }),
        &flags,
    );
    result(&out, "error", 3);
    // 64: an invalid run (an argument the operation does not take).
    let out = call(
        &dir,
        "line",
        &serde_json::json!({ "image": { "path": floor().to_str().unwrap() }, "column": 0.5,
                             "width_px": 24, "colour": "red" }),
        &allow,
    );
    result(&out, "invalid", 64);
    // 64: a path outside every allowed directory, refused as serve refuses it.
    let outside = dir.join("pano.png");
    std::fs::copy(pano(), &outside).unwrap();
    let out = call(
        &dir,
        "seam",
        &serde_json::json!({ "image": { "path": outside.to_str().unwrap() } }),
        &allow,
    );
    let r = result(&out, "invalid", 64);
    assert_eq!(r["error"]["code"], "input");
    assert!(
        r["error"]["reason"]
            .as_str()
            .unwrap()
            .contains("this path is not readable here"),
        "{r:#}"
    );
    // 64: a binding refusal, its JSON on stdout and no run.
    let missing = serde_json::json!({
        "type": "rascal:Mask", "schema_version": 1, "tool": "rascal",
        "identity": format!("sha256:{}", "0".repeat(64)), "frame": null,
        "resource": format!("rascal://artifact/sha256:{}", "0".repeat(64)),
    });
    let runs_before = std::fs::read_dir(dir.join("runs")).unwrap().count();
    let out = call(
        &dir,
        "feather",
        &serde_json::json!({ "mask": missing, "sigma_px": 6 }),
        &[],
    );
    assert_eq!(out.status.code(), Some(64));
    let refusal = printed(&out);
    valid("urn:table-manners:core:refusal", &refusal);
    assert_eq!(refusal["reason"], "binding", "{refusal:#}");
    assert_eq!(
        std::fs::read_dir(dir.join("runs")).unwrap().count(),
        runs_before
    );
    // 64: no such action, and a reserved tool is no action.
    for name in ["no_such_action", "estimate"] {
        let out = call(&dir, name, &serde_json::json!({}), &[]);
        assert_eq!(out.status.code(), Some(64), "{name}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_inline_image_needs_no_flag() {
    let dir = scratch("inline");
    let bytes = std::fs::read(floor()).unwrap();
    let out = call(
        &dir,
        "line",
        &serde_json::json!({
            "image": { "inline": { "media_type": "image/png", "data": rascal_core::panel::base64(&bytes) } },
            "column": 0.5, "width_px": 24,
        }),
        &[],
    );
    result(&out, "completed", 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sigint_cancels_the_call_and_exits_130() {
    let dir = scratch("signal");
    let served = feathered(&dir);
    let child = call_command(&dir, "feather", &served["invocation"]["arguments"], &[])
        .env("RASCAL_TESTKIT_OPERATION_DELAY_MS", "20000")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // The run is allocated once its association is written.
    let started = std::time::Instant::now();
    while !std::fs::read_dir(dir.join("runs")).is_ok_and(|mut entries| {
        entries.any(|e| e.is_ok_and(|e| e.path().join("association.json").is_file()))
    }) {
        assert!(started.elapsed().as_secs() < 30, "no run was allocated");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let kill = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(kill.success());
    let out = child.wait_with_output().unwrap();
    let r = result(&out, "cancelled", 130);
    assert_eq!(r["invocation"], served["invocation"]);
    let _ = std::fs::remove_dir_all(&dir);
}
