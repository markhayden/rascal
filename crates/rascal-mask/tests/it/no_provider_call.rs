//! Task 12.9 (table-manners 7, item 7): `describe` and `estimate` make no provider call. Across
//! every configuration state (no `--config`, a path that names no file, a file that does not
//! load, a loaded file) and every credential state (missing, present, valid after a 2xx,
//! invalid after a 401), `describe` (the command line, with and without `--json`, and the tool),
//! `estimate` (the command line and the tool; allowed and refused for the cap, the budget and
//! readiness) and every `not_ready` refusal (the tool's and the command line's) leave the
//! recording fake HTTP filler with no request, the fake segmenter pack never started (its
//! start marker absent), and the runs root and the workspace (its registry generations among
//! its files) as they were.
//!
//! `serve` starts its configured packs when it starts, which is no provider call, so the
//! servers here are configured without one and the pack's marker is checked on the command
//! line, which starts no pack to describe or estimate.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};

use rascal_testkit::fake_filler::{Behaviour, Seen, spawn};
use rascal_testkit::fake_install;
use rascal_testkit::mcp_client::{Client, Failure};
use serde_json::{Value, json};

const KEY_ENV: &str = "RASCAL_NO_CALL_KEY";
const KEY: &str = "no-call-key";
const WRONG_KEY: &str = "not-the-key";

fn pano() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/images/step-pano.png")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-no-call-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The configuration states, as `--config` arguments: none, a path naming no file, a file that
/// does not load, and `loaded`.
fn config_states(dir: &Path, loaded: &Path) -> Vec<(&'static str, Option<PathBuf>)> {
    let invalid = dir.join("invalid.toml");
    std::fs::write(&invalid, "config_version = [ this is not toml\n").unwrap();
    vec![
        ("none", None),
        ("absent", Some(dir.join("absent.toml"))),
        ("invalid", Some(invalid)),
        ("loaded", Some(loaded.to_path_buf())),
    ]
}

/// A priced HTTP filler at `url` (its key in [`KEY_ENV`]), a cap and a budget one paid call
/// nearly uses up, and `more`.
fn config(dir: &Path, file: &str, url: &str, more: &str) -> PathBuf {
    let path = dir.join(file);
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[limits]\ncall_cap_usd = 0.10\nsession_budget_usd = 0.05\n\n[fillers.hosted]\nurl = {url:?}\nkey_env = \"{KEY_ENV}\"\nprofile = \"grey\"\nrevision = \"hosted-1\"\nspend_usd_per_call = 0.04\n\n{more}"
        ),
    )
    .unwrap();
    path
}

/// The arguments of every estimate made: mask-only, paid, paid under the price (the cap), and
/// `remove_prompt` (not ready without its pack, or with no file).
/// The image inline: estimates and refusals work in every configuration state, and inline
/// bytes are no caller path (a configuration that does not load allows none, task 10.1).
fn image() -> Value {
    let bytes = std::fs::read(pano()).unwrap();
    json!({ "inline": { "media_type": "image/png", "data": rascal_core::panel::base64(&bytes) } })
}

fn requests() -> Vec<(&'static str, Value)> {
    let image = image();
    vec![
        ("seam", json!({ "image": image })),
        ("seam", json!({ "image": image, "filler": "hosted" })),
        (
            "seam",
            json!({ "image": image, "filler": "hosted", "max_spend_usd": 0.03 }),
        ),
        (
            "remove_prompt",
            json!({ "image": image, "prompt": "a lamp" }),
        ),
    ]
}

/// Every file under `dir` with the digest of its bytes, sorted.
fn listing(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap_or_default();
                out.push((path, rascal_core::contract::file_sha256(&bytes)));
            }
        }
    }
    out.sort();
    out
}

fn requests_seen(seen: &Arc<Mutex<Vec<Seen>>>) -> usize {
    seen.lock().unwrap().len()
}

/// `rascal` with `args`, `HOME` at `home`, the key set to `key` when given, `stdin` written.
fn rascal(home: &Path, args: &[&str], key: Option<&str>, stdin: Option<&Value>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command
        .args(args)
        .env_remove("RASCAL_LOG")
        .env_remove("RASCAL_TOOLS_MODELS")
        .env_remove(KEY_ENV)
        .env("HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(key) = key {
        command.env(KEY_ENV, key);
    }
    let mut child = command.spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    if let Some(stdin) = stdin {
        input.write_all(stdin.to_string().as_bytes()).unwrap();
    }
    drop(input);
    child.wait_with_output().unwrap()
}

#[test]
fn the_command_line_describes_and_estimates_without_a_provider_call() {
    let dir = scratch("cli");
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let bin = env!("CARGO_BIN_EXE_rascal");
    let marker = dir.join("pack-started");
    let install = fake_install::write(
        &dir.join("pack"),
        &[bin, "fake-pack", "mutate", marker.to_str().unwrap()],
        true,
    )
    .unwrap();
    let (url, seen) = spawn(Behaviour::Exact, Some(KEY.to_string()));
    let loaded = config(
        &dir,
        "rascal.toml",
        &url,
        &format!("[packs.segmenter]\ncommand = {}\n", install.command_toml()),
    );
    let out_dir = dir.join("out");
    let states = config_states(&dir, &loaded);
    let before = listing(&dir);
    for (state, config) in states {
        let with_config = |args: &[&str]| -> Vec<String> {
            let mut all: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            if let Some(config) = &config {
                all.push("--config".to_string());
                all.push(config.to_str().unwrap().to_string());
            }
            all
        };
        for key in [None, Some(KEY)] {
            let what = format!("config {state}, key {key:?}");
            for args in [
                with_config(&["describe", "--json"]),
                with_config(&["describe"]),
            ] {
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                let out = rascal(&home, &args, key, None);
                assert_eq!(out.status.code(), Some(0), "{what}: {args:?}");
            }
            for (action, arguments) in requests() {
                let mut args = with_config(&["estimate", "--json"]);
                args.push("-".to_string());
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                let request = json!({ "action": action, "arguments": arguments });
                let out = rascal(&home, &args, key, Some(&request));
                assert!(
                    matches!(out.status.code(), Some(0 | 64)),
                    "{what}: {request}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            // A paid heal that is not ready (no file, or no key) is refused before its run.
            if state != "loaded" || key.is_none() {
                let mut args = with_config(&["heal", "seam", pano().to_str().unwrap()]);
                args.extend(
                    ["--filler", "hosted", "--out", out_dir.to_str().unwrap()].map(String::from),
                );
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                let out = rascal(&home, &args, key, None);
                assert_eq!(
                    out.status.code(),
                    Some(64),
                    "{what}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            assert_eq!(
                requests_seen(&seen),
                0,
                "{what}: a request reached the filler"
            );
            assert!(!marker.exists(), "{what}: the pack was started");
            assert!(!out_dir.exists(), "{what}: a refused heal made its run");
        }
    }
    assert_eq!(listing(&dir), before, "the command line wrote something");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `rascal serve` over `dir`'s workspace and runs root.
fn serve(dir: &Path, config: Option<&Path>, key: Option<&str>) -> Client {
    let mut command = rascal_testkit::mcp_client::serve_command(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "--workspace",
            dir.join("ws").to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env_remove("RASCAL_LOG")
        .env_remove(KEY_ENV);
    if let Some(config) = config {
        command.args(["--config", config.to_str().unwrap()]);
    }
    if let Some(key) = key {
        command.env(KEY_ENV, key);
    }
    Client::start(&mut command).unwrap()
}

/// Asserts a call is refused `not_ready` before any run.
fn not_ready(client: &mut Client, name: &str, arguments: Value, id: &str, what: &str) {
    match client.call_with_meta(
        name,
        arguments,
        json!({ "table-manners/client_run_id": id }),
    ) {
        Err(Failure::Rpc {
            code,
            data: Some(data),
            ..
        }) => {
            assert_eq!(code, -32602, "{what}");
            assert_eq!(data["reason"], "not_ready", "{what}: {data:#}");
        }
        other => panic!("{what}: {name} was not refused not_ready: {other:?}"),
    }
}

#[test]
fn the_server_describes_estimates_and_refuses_without_a_provider_call() {
    let dir = scratch("serve");
    let (url, seen) = spawn(Behaviour::Exact, Some(KEY.to_string()));
    let loaded = config(&dir, "rascal.toml", &url, "");
    let paid = json!({ "image": image(), "filler": "hosted" });
    let mut answered = 0;
    for (state, config) in config_states(&dir, &loaded) {
        // Missing, present, valid (after a 2xx) and invalid (after a 401).
        for (credential, key) in [
            ("missing", None),
            ("present", Some(KEY)),
            ("valid", Some(KEY)),
            ("invalid", Some(WRONG_KEY)),
        ] {
            if state != "loaded" && matches!(credential, "valid" | "invalid") {
                continue;
            }
            let what = format!("config {state}, credential {credential}");
            let mut client = serve(&dir, config.as_deref(), key);
            if matches!(credential, "valid" | "invalid") {
                let ran = client.call("seam", paid.clone()).unwrap();
                let s = ran.structured.unwrap();
                let expected = if credential == "valid" {
                    "completed"
                } else {
                    "error"
                };
                assert_eq!(s["status"], expected, "{what}: {s:#}");
                answered += 1;
            }
            let requests_before = requests_seen(&seen);
            assert_eq!(requests_before, answered, "{what}");
            let runs_before = listing(&dir.join("runs"));
            let ws_before = listing(&dir.join("ws"));

            let described = client.call("describe", json!({})).unwrap();
            assert!(described.structured.is_some(), "{what}");
            let mut refusals = Vec::new();
            for (action, arguments) in requests() {
                let e = client
                    .call(
                        "estimate",
                        json!({ "action": action, "arguments": arguments }),
                    )
                    .unwrap()
                    .structured
                    .unwrap();
                refusals.push(e["refused"].clone());
            }
            assert!(
                refusals.contains(&json!("readiness")),
                "{what}: {refusals:?}"
            );
            if state == "loaded" && credential != "missing" && credential != "invalid" {
                let expected = if credential == "valid" {
                    "budget"
                } else {
                    "cap"
                };
                assert!(refusals.contains(&json!(expected)), "{what}: {refusals:?}");
            }
            not_ready(
                &mut client,
                "remove_prompt",
                json!({ "image": image(), "prompt": "a lamp" }),
                &format!("no-call.{state}.{credential}.prompt"),
                &what,
            );
            if state != "loaded" || credential == "missing" || credential == "invalid" {
                not_ready(
                    &mut client,
                    "seam",
                    paid.clone(),
                    &format!("no-call.{state}.{credential}.paid"),
                    &what,
                );
            }

            assert_eq!(
                requests_seen(&seen),
                requests_before,
                "{what}: a request reached the filler"
            );
            assert_eq!(listing(&dir.join("runs")), runs_before, "{what}: runs root");
            assert_eq!(listing(&dir.join("ws")), ws_before, "{what}: workspace");
            client.close().unwrap();
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
