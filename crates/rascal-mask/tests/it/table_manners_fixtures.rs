//! Tasks 11.6 and 12.9 (table-manners 7, item 7): rascal's conformance fixtures for the
//! contract's `fixtures/rascal/`, produced by `rascal serve` and `rascal estimate` through the
//! independent client and never written by hand.
//!
//! v0.4 (task 10.2) adds `kind` to every action, `allowed_dirs` to the session and `invocation`
//! to every result. M11's set: the description, a run per status (an unresolved charge among them), two pages of
//! the runs list, the session, and both reuse refusals. v0.3's: the description with no
//! configuration and with a key missing, present (every action `ready`), valid (after a 2xx)
//! and invalid (after a 401); estimates allowed, refused for the cap, the budget and readiness,
//! with no file (a mask-only call at 0 under the built-in defaults' identity) and from the
//! command line (no session); runs failed `stale_estimate` for a changed image, a changed
//! argument, another session (a server restarted under another configuration, which for rascal
//! is the same answer: one fixture stands for both) and an expired estimate, for the cap, the
//! budget and a rejected key; the `not_ready` refusal, the accepted retry carrying the id it
//! released, and an id cancelled before allocation still refused.
//!
//! Machine-specific values are replaced by fixed stand-ins that stay valid (paths under `/runs`,
//! `/data`, `/work` and `/tmp`, one time, session ids, run ids, estimate bindings and configuration
//! identities in their order, the fake filler's address), so the files compare equal on any
//! machine. `UPDATE_FIXTURES=1` rewrites `fixtures/table-manners/`; otherwise the output must
//! match it, file for file. Every file holds to the contract exactly as table-manners'
//! `tests/fixtures.rs` holds it: the schema its name chooses, the contract's version, the tool's
//! own name, the crate's types, and at most six decimals in every dollar amount.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rascal_core::contract::table_manners;
use rascal_testkit::fake_filler::{Behaviour, spawn};
use rascal_testkit::mcp_client::Client;
use serde_json::{Value, json};

/// The variable the HTTP filler's key is read from, and the values a server is started with.
const KEY_ENV: &str = "RASCAL_FIXTURE_KEY";
const KEY: &str = "fixture-key-value";
const WRONG_KEY: &str = "wrong-key-value";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-tm-fixtures-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// The HTTP filler served by the fake, priced, its key in [`KEY_ENV`].
fn hosted(url: &str, revision: &str) -> String {
    format!(
        "[fillers.hosted]\nurl = {url:?}\nkey_env = \"{KEY_ENV}\"\nprofile = \"grey\"\ntimeout_s = 30\nrevision = \"{revision}\"\nspend_usd_per_call = 0.04\n\n"
    )
}

/// M11's configuration (three priced command fillers and a session budget), with the HTTP
/// filler and a per-call cap.
fn main_config(dir: &Path, url: &str) -> PathBuf {
    let bin = env!("CARGO_BIN_EXE_rascal");
    let filler = |name: &str, behaviour: &str| {
        format!(
            "[fillers.{name}]\ncommand = [{bin:?}, \"fake-filler\", \"{behaviour}\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\ntimeout_s = 30\nrevision = \"{behaviour}\"\nspend_usd_per_call = 0.04\n\n"
        )
    };
    let config = dir.join("rascal.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[limits]\ncall_cap_usd = 0.10\nsession_budget_usd = 1.0\n\n{}{}{}{}",
            filler("fake", "exact"),
            filler("failing", "failing"),
            filler("slow", "slow"),
            hosted(url, "hosted-1"),
        ),
    )
    .unwrap();
    config
}

/// The configuration of the v0.3 servers: the HTTP filler alone (at `revision`), a cap of 0.10
/// and a budget of 0.05, which one paid call nearly uses up.
fn hosted_config(dir: &Path, url: &str, file: &str, revision: &str) -> PathBuf {
    let config = dir.join(file);
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[limits]\ncall_cap_usd = 0.10\nsession_budget_usd = 0.05\n\n{}",
            hosted(url, revision)
        ),
    )
    .unwrap();
    config
}

/// `rascal serve` over the shared workspace and runs root, with `config` when given and
/// exactly the variables in `env` (the key and the testkit's estimate lifetime otherwise unset).
fn serve(dir: &Path, config: Option<&Path>, env: &[(&str, &str)]) -> Client {
    let mut command = rascal_testkit::mcp_client::serve_command(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "--workspace",
            dir.join("ws").to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env_remove("RASCAL_LOG")
        .env_remove(KEY_ENV)
        .env_remove("RASCAL_TESTKIT_ESTIMATE_TTL_MS");
    if let Some(config) = config {
        command.args(["--config", config.to_str().unwrap()]);
    }
    for (name, value) in env {
        command.env(name, value);
    }
    Client::start(&mut command).unwrap()
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

fn validator(schema: &str) -> jsonschema::Validator {
    let id = if schema.contains(':') {
        format!("urn:table-manners:{schema}")
    } else {
        format!("urn:table-manners:core:{schema}")
    };
    let (_, text) = table_manners::SCHEMAS
        .iter()
        .find(|(i, _)| *i == id)
        .unwrap_or_else(|| panic!("no schema {id}"));
    let schema: Value = serde_json::from_str(text).unwrap();
    jsonschema::options()
        .with_retriever(Embedded)
        .build(&schema)
        .unwrap()
}

/// Every dollar amount (`usd` or a key ending `_usd`) has at most six decimals (4.6), which the
/// schemas cannot say.
fn six_decimals(name: &str, key: &str, value: &Value) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                six_decimals(name, k, v);
            }
        }
        Value::Array(items) => {
            for v in items {
                six_decimals(name, key, v);
            }
        }
        Value::Number(n) if key == "usd" || key.ends_with("_usd") => {
            let text = n.to_string();
            let decimals = match text.split_once('.') {
                _ if text.contains(['e', 'E']) => usize::MAX,
                Some((_, fraction)) => fraction.len(),
                None => 0,
            };
            assert!(
                decimals <= 6,
                "{name}: {key} is {text}, more than six decimals"
            );
        }
        _ => {}
    }
}

/// Holds one fixture to the contract as table-manners' `tests/fixtures.rs` does: the file's
/// name chooses its schema (`describe*` a description, `estimate-*` an estimate, `run-*` a
/// result, `runs*` a runs page, `session.json` the session, `refusal-*` a refusal's data); it
/// is valid against it, reads into the crate's type, names rascal and this contract version, and
/// no dollar amount has more than six decimals.
fn conforms(name: &str, value: &Value) {
    let schema = if name.starts_with("bundle-") {
        "core:invocation-bundle"
    } else if name.starts_with("artifact-") {
        "operations:artifact"
    } else if name.starts_with("export-") {
        "operations:export-response"
    } else if name.starts_with("import-") {
        "operations:import-response"
    } else if name == "describe.json" || name.starts_with("describe-") {
        "description"
    } else if name.starts_with("estimate-") {
        "estimate"
    } else if name.starts_with("run-") {
        "result"
    } else if name.starts_with("runs") {
        "runs"
    } else if name == "session.json" || name.starts_with("session-") {
        "session"
    } else if name.starts_with("refusal-") {
        "refusal"
    } else {
        panic!("{name}: not a fixture table-manners knows");
    };
    let errors: Vec<String> = validator(schema)
        .iter_errors(value)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{name}: {errors:?}\n{value:#}");
    six_decimals(name, "", value);
    if let Some(version) = value.get("contract") {
        assert_eq!(version, table_manners::CONTRACT_VERSION, "{name}");
    }
    let v = value.clone();
    let tool = match schema {
        "description" => Some(
            serde_json::from_value::<table_manners::Description>(v)
                .unwrap()
                .tool
                .name,
        ),
        "estimate" => Some(
            serde_json::from_value::<table_manners::Estimate>(v)
                .unwrap()
                .tool
                .name,
        ),
        "result" => Some(
            serde_json::from_value::<table_manners::RunResult>(v)
                .unwrap()
                .tool
                .name,
        ),
        "runs" => Some(
            serde_json::from_value::<table_manners::RunsList>(v)
                .unwrap()
                .tool
                .name,
        ),
        "session" => Some(
            serde_json::from_value::<table_manners::Session>(v)
                .unwrap()
                .tool
                .name,
        ),
        "operations:export-response" => Some(
            serde_json::from_value::<table_manners::operations::ExportResponse>(v)
                .unwrap()
                .tool
                .name,
        ),
        "operations:import-response" => Some(
            serde_json::from_value::<table_manners::operations::ImportResponse>(v)
                .unwrap()
                .tool
                .name,
        ),
        "operations:artifact" => v["tool"]["name"].as_str().map(str::to_string),
        "core:invocation-bundle" => {
            for step in v["steps"].as_array().unwrap() {
                assert_eq!(step["tool"]["name"], "rascal", "{name}");
            }
            None
        }
        _ => {
            let _: table_manners::Refusal = serde_json::from_value(v).unwrap();
            None
        }
    };
    if let Some(tool) = tool {
        assert_eq!(tool, "rascal", "{name}: the tool's own name");
    }
}

fn call(token: &str, name: &str, arguments: Value, id: Option<&str>) -> Value {
    let mut meta = json!({ "progressToken": token });
    if let Some(id) = id {
        meta["table-manners/client_run_id"] = json!(id);
    }
    json!({ "name": name, "arguments": arguments, "_meta": meta })
}

/// A call naming `binding` as its estimate.
fn bound(token: &str, name: &str, arguments: Value, binding: &Value) -> Value {
    let mut params = call(token, name, arguments, None);
    params["_meta"]["table-manners/estimate"] = binding.clone();
    params
}

/// The response to request `id`, skipping everything else.
fn response(client: &mut Client, id: u64) -> Value {
    loop {
        let message = client.next_message().unwrap();
        if message.get("id").and_then(|v| v.as_u64()) == Some(id) {
            return message;
        }
    }
}

/// The result of a call that runs (whatever its status).
fn result(client: &mut Client, params: Value) -> Value {
    let id = client.begin("tools/call", params).unwrap();
    let message = response(client, id);
    assert!(message.get("error").is_none(), "{message:#}");
    message["result"]["structuredContent"].clone()
}

/// The JSON-RPC error of a call refused before any run: `-32602` and its refusal data.
fn refusal(client: &mut Client, params: Value) -> Value {
    let id = client.begin("tools/call", params).unwrap();
    let message = response(client, id);
    assert_eq!(message["error"]["code"], -32602, "{message:#}");
    message["error"]["data"].clone()
}

fn describe(client: &mut Client) -> Value {
    client
        .call("describe", json!({}))
        .unwrap()
        .structured
        .unwrap()
}

/// The estimate of `action` with `arguments`, noted in `issued` so its binding and its
/// configuration identity get stand-ins.
fn estimate(client: &mut Client, issued: &mut Vec<Value>, action: &str, arguments: Value) -> Value {
    let out = client
        .call(
            "estimate",
            json!({ "action": action, "arguments": arguments }),
        )
        .unwrap();
    assert!(!out.is_error);
    let e = out.structured.unwrap();
    issued.push(e.clone());
    e
}

fn session_id(client: &mut Client) -> String {
    read(client, "rascal://session")["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// The run named by the first progress notification of the call now under way.
fn first_run(client: &mut Client) -> String {
    loop {
        let message = client.next_message().unwrap();
        if message["method"] == "notifications/progress" {
            return message["params"]["_meta"]["table-manners/run"]["id"]
                .as_str()
                .unwrap()
                .to_string();
        }
    }
}

fn read(client: &mut Client, uri: &str) -> Value {
    let result = client
        .request("resources/read", json!({ "uri": uri }))
        .unwrap();
    serde_json::from_str(result["contents"][0]["text"].as_str().unwrap()).unwrap()
}

/// Waits until `run`'s journal shows the filler call under way, so a cancel lands at the same
/// step every time.
fn wait_for_fill(client: &mut Client, run: &str) {
    let since = std::time::Instant::now();
    while !client
        .request(
            "resources/read",
            json!({ "uri": format!("rascal://run/{run}/journal") }),
        )
        .is_ok_and(|r| {
            r["contents"][0]["text"]
                .as_str()
                .is_some_and(|j| j.contains("filler call started"))
        })
    {
        assert!(
            since.elapsed().as_secs() < 30,
            "the run never reached the fill"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Waits until `run` is no longer `running`.
fn wait_for_end(client: &mut Client, run: &str) {
    let since = std::time::Instant::now();
    loop {
        let page = read(client, "rascal://runs");
        if page["runs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == run && r["status"] != "running")
        {
            return;
        }
        assert!(since.elapsed().as_secs() < 30, "the run never ended");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Replaces what differs between machines and runs with fixed stand-ins that stay valid.
struct Normalise {
    replacements: Vec<(String, String)>,
}

impl Normalise {
    /// `sessions` and `runs` in the order they were made; `issued` every estimate made and then
    /// every result's invocation, whose bindings and configuration identities (but the built-in
    /// defaults', which is the same on every machine) are replaced in order of appearance; `url`
    /// the fake filler's address.
    fn new(
        dir: &Path,
        sessions: &[String],
        runs: &[String],
        issued: &[Value],
        url: &str,
    ) -> Normalise {
        let mut ordered: Vec<&String> = runs.iter().collect();
        ordered.sort_by_key(|id| {
            let mut parts = id
                .split('-')
                .skip(1)
                .map(|p| p.parse::<u128>().unwrap_or(0));
            (parts.next(), parts.next())
        });
        let mut replacements: Vec<(String, String)> = ordered
            .into_iter()
            .enumerate()
            .map(|(i, id)| {
                (
                    id.clone(),
                    format!("run-{}-1", 1_767_225_600_000_u128 + i as u128),
                )
            })
            .collect();
        // Longest first, so a run id is never cut by a shorter one's replacement.
        replacements.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
        let builtin = rascal_core::config::Config::builtin().identity();
        let mut digests: Vec<(String, String)> = Vec::new();
        for (key, fill) in [("binding", 'b'), ("config_identity", 'c')] {
            let mut seen: Vec<String> = Vec::new();
            for e in issued {
                let value = if key == "binding" {
                    &e["binding"]
                } else {
                    &e["effective"]["config_identity"]
                };
                if let Some(digest) = value.as_str()
                    && digest != builtin.as_str()
                    && !seen.iter().any(|s| s == digest)
                {
                    seen.push(digest.to_string());
                }
            }
            for (i, digest) in seen.into_iter().enumerate() {
                digests.push((
                    digest,
                    format!("sha256:{}{:04}", fill.to_string().repeat(60), i + 1),
                ));
            }
        }
        replacements.extend(digests);
        replacements.push((
            dir.join("runs").to_string_lossy().into_owned(),
            "/runs".to_string(),
        ));
        replacements.push((
            repo_root().join("fixtures").to_string_lossy().into_owned(),
            "/data".to_string(),
        ));
        replacements.push((
            repo_root().to_string_lossy().into_owned(),
            "/rascal".to_string(),
        ));
        replacements.push((dir.to_string_lossy().into_owned(), "/work".to_string()));
        // The temporary directory `serve_command` allows, resolved (the session's
        // `allowed_dirs`), after the scratch directory inside it.
        replacements.push((
            std::env::temp_dir()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            "/tmp".to_string(),
        ));
        let host = url
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap()
            .to_string();
        replacements.push((host, "127.0.0.1:8080".to_string()));
        for (i, session) in sessions.iter().enumerate() {
            replacements.push((
                session.clone(),
                format!("00000000-0000-4000-8000-{:012}", i + 1),
            ));
        }
        Normalise { replacements }
    }

    fn apply(&self, value: &Value) -> Value {
        match value {
            Value::String(s) => Value::String(self.text(s)),
            Value::Array(items) => Value::Array(items.iter().map(|v| self.apply(v)).collect()),
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(k, v)| {
                        let v = match k.as_str() {
                            "duration_ms" => json!(0),
                            "input_identity" => {
                                json!(format!("sha256:{}", "0".repeat(64)))
                            }
                            _ => self.apply(v),
                        };
                        (k.clone(), v)
                    })
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    fn text(&self, s: &str) -> String {
        let mut out = s.to_string();
        for (from, to) in &self.replacements {
            out = out.replace(from, to);
        }
        // Durations inside the journal's notes.
        let mut fixed = String::new();
        let mut rest = out.as_str();
        while let Some(at) = rest.find("\"duration_ms\":") {
            let (head, tail) = rest.split_at(at + "\"duration_ms\":".len());
            fixed.push_str(head);
            fixed.push('0');
            rest = tail.trim_start_matches(|c: char| c.is_ascii_digit());
        }
        fixed.push_str(rest);
        times(&fixed)
    }
}

/// Every RFC 3339 UTC time as rascal writes them (`2026-01-01T00:00:00.000Z`), whole or inside
/// a sentence, replaced by one fixed time.
fn times(s: &str) -> String {
    const FIXED: &str = "2026-01-01T00:00:00.000Z";
    let b = s.as_bytes();
    let at = |i: usize| -> bool {
        let Some(w) = b.get(i..i + 24) else {
            return false;
        };
        w.iter().enumerate().all(|(j, c)| match j {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'.',
            23 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
    };
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if at(i) {
            out.push_str(FIXED);
            i += 24;
        } else {
            let c = s[i..].chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

/// M11's set, from a server under [`main_config`] with the key set; its session is the first.
fn m11_set(
    dir: &Path,
    pano: &Path,
    url: &str,
    out: &mut BTreeMap<&'static str, Value>,
    sessions: &mut Vec<String>,
) {
    let seam =
        |filler: &str| json!({ "image": { "path": pano.to_str().unwrap() }, "filler": filler });
    let config = main_config(dir, url);
    let mut client = serve(dir, Some(&config), &[(KEY_ENV, KEY)]);
    let session = session_id(&mut client);
    sessions.push(session);
    out.insert("describe.json", describe(&mut client));

    // Completed, with a correlation id.
    let completed = result(
        &mut client,
        call("completed", "seam", seam("fake"), Some("fixture.completed")),
    );
    assert_eq!(completed["status"], "completed", "{completed:#}");
    out.insert("run-completed.json", completed);

    // An error whose filler call was sent and never settled: the charge is unresolved.
    let error = result(&mut client, call("error", "seam", seam("failing"), None));
    assert_eq!(error["status"], "error", "{error:#}");
    assert_eq!(error["spend"]["unresolved_usd"], 0.04);
    out.insert("run-error-unresolved.json", error);

    // Invalid: the image is not the one expected.
    let wrong = json!({ "path": pano.to_str().unwrap(), "expect": {
        "file_sha256": "0".repeat(64), "width": 800, "height": 400,
        "projection": "equirectangular", "topology": "wrap" } });
    let invalid = result(
        &mut client,
        call("invalid", "seam", json!({ "image": wrong }), None),
    );
    assert_eq!(invalid["status"], "invalid", "{invalid:#}");
    out.insert("run-invalid.json", invalid);

    // Cancelled during the fill: the call's price stays spent.
    let id = client
        .begin(
            "tools/call",
            call("cancelled", "seam", seam("slow"), Some("fixture.cancelled")),
        )
        .unwrap();
    let cancelled_run = first_run(&mut client);
    wait_for_fill(&mut client, &cancelled_run);
    client
        .notify(
            "notifications/cancelled",
            json!({ "requestId": id, "reason": "fixture" }),
        )
        .unwrap();
    wait_for_end(&mut client, &cancelled_run);
    let cancelled = read(
        &mut client,
        &format!("rascal://run/{cancelled_run}/run.json"),
    );
    assert_eq!(cancelled["status"], "cancelled", "{cancelled:#}");
    out.insert("run-cancelled.json", cancelled);

    // Reuse of an id whose run ended: the refusal names it.
    let reused = refusal(
        &mut client,
        call("reuse", "seam", seam("fake"), Some("fixture.completed")),
    );
    out.insert("refusal-earlier-run.json", reused);

    // Reuse of ids whose calls were cancelled while queued, before any run was allocated: no
    // run to name, and the id stays claimed (4.4), unlike one refused with no run.
    let holder = client
        .begin("tools/call", call("holder", "seam", seam("slow"), None))
        .unwrap();
    let holder_run = first_run(&mut client);
    for (token, claimed) in [
        ("queued", "fixture.queued"),
        ("before-allocation", "fixture.cancelled-before-allocation"),
    ] {
        let queued = client
            .begin(
                "tools/call",
                call(token, "seam", seam("fake"), Some(claimed)),
            )
            .unwrap();
        client
            .notify(
                "notifications/cancelled",
                json!({ "requestId": queued, "reason": "fixture" }),
            )
            .unwrap();
    }
    let _ = response(&mut client, holder);
    wait_for_end(&mut client, &holder_run);
    for (token, claimed, name) in [
        ("retry", "fixture.queued", "refusal-no-run.json"),
        (
            "retry-before-allocation",
            "fixture.cancelled-before-allocation",
            "refusal-cancelled-before-allocation.json",
        ),
    ] {
        let none = refusal(
            &mut client,
            call(token, "seam", seam("fake"), Some(claimed)),
        );
        assert_eq!(none["reason"], "client_run_id_reused", "{none:#}");
        assert_eq!(none["run"], Value::Null, "{none:#}");
        out.insert(name, none);
    }

    // Enough quick invalid runs for a second page of the runs list.
    for i in 0..50 {
        let _ = result(
            &mut client,
            call(
                &format!("fill-{i}"),
                "seam",
                json!({ "image": wrong }),
                None,
            ),
        );
    }
    let first = read(&mut client, "rascal://runs");
    let cursor = first["next_cursor"].as_str().unwrap().to_string();
    let second = read(&mut client, &format!("rascal://runs?cursor={cursor}"));
    assert_eq!(second["next_cursor"], Value::Null);
    out.insert("runs.json", first);
    out.insert("runs-page-2.json", second);
    out.insert("session.json", read(&mut client, "rascal://session"));
    client.close().unwrap();
}

/// v0.3's set: readiness, estimates, bound calls, the split limits, `credential` and the claim
/// released with no run.
fn v03_set(
    dir: &Path,
    url: &str,
    filler_requests: &dyn Fn() -> usize,
    out: &mut BTreeMap<&'static str, Value>,
    sessions: &mut Vec<String>,
    issued: &mut Vec<Value>,
) {
    let image = dir.join("pano.png");
    std::fs::copy(repo_root().join("fixtures/images/step-pano.png"), &image).unwrap();
    let mask_only = json!({ "image": { "path": image.to_str().unwrap() } });
    let paid = json!({ "image": { "path": image.to_str().unwrap() }, "filler": "hosted" });
    let with = |key: &str, value: Value| {
        let mut args = paid.clone();
        args[key] = value;
        args
    };
    let config = hosted_config(dir, url, "hosted.toml", "hosted-1");
    let other = hosted_config(dir, url, "other.toml", "hosted-2");
    let requirement = |doc: &Value, action: &str, kind: &str| -> Value {
        doc["actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == action)
            .unwrap()["readiness"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == kind)
            .cloned()
            .unwrap_or(Value::Null)
    };

    // No configuration: the mask alone is ready and free, priced under the defaults' identity.
    let mut client = serve(dir, None, &[]);
    sessions.push(session_id(&mut client));
    let unconfigured = describe(&mut client);
    assert_eq!(
        requirement(&unconfigured, "remove_prompt", "config")["state"],
        "missing"
    );
    out.insert("describe-unconfigured.json", unconfigured);
    let free = estimate(&mut client, issued, "seam", mask_only.clone());
    assert_eq!(free["allowed"], true, "{free:#}");
    assert_eq!(free["worst_case_usd"], 0.0);
    assert_eq!(
        free["effective"]["config_identity"],
        rascal_core::config::Config::builtin().identity().as_str()
    );
    out.insert("estimate-unconfigured.json", free);
    client.close().unwrap();

    // The key missing: every action needs setup; a call that needs the key is refused before
    // any run and releases its id, which the next request takes.
    let mut client = serve(dir, Some(&config), &[]);
    sessions.push(session_id(&mut client));
    let missing = describe(&mut client);
    assert_eq!(
        requirement(&missing, "seam", "credential")["state"],
        "missing"
    );
    out.insert("describe-credential-missing.json", missing);
    let not_ready = refusal(
        &mut client,
        call("not-ready", "seam", paid.clone(), Some("fixture.released")),
    );
    assert_eq!(not_ready["reason"], "not_ready", "{not_ready:#}");
    assert_eq!(not_ready["run"], Value::Null);
    out.insert("refusal-not-ready.json", not_ready);
    let released = result(
        &mut client,
        call(
            "released",
            "seam",
            mask_only.clone(),
            Some("fixture.released"),
        ),
    );
    assert_eq!(released["status"], "completed", "{released:#}");
    assert_eq!(released["run"]["client_run_id"], "fixture.released");
    out.insert("run-claim-released.json", released);
    client.close().unwrap();
    assert_eq!(filler_requests(), 0);

    // The key present: every action ready; estimates; the cap; stale bindings; then a paid call
    // makes the key valid and leaves too little budget for another.
    let mut client = serve(dir, Some(&config), &[(KEY_ENV, KEY)]);
    sessions.push(session_id(&mut client));
    let present = describe(&mut client);
    assert!(
        present["actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["name"] != "remove_prompt")
            .all(|a| a["readiness"]["state"] == "ready"),
        "{present:#}"
    );
    assert_eq!(
        requirement(&present, "seam", "credential")["state"],
        "present"
    );
    out.insert("describe-credential-present.json", present);
    let allowed = estimate(&mut client, issued, "seam", paid.clone());
    assert_eq!(allowed["allowed"], true, "{allowed:#}");
    assert_eq!(allowed["worst_case_usd"], 0.04);
    let binding = allowed["binding"].clone();
    out.insert("estimate-allowed.json", allowed);
    let cap = estimate(
        &mut client,
        issued,
        "seam",
        with("max_spend_usd", json!(0.03)),
    );
    assert_eq!(cap["refused"], "cap", "{cap:#}");
    out.insert("estimate-refused-cap.json", cap);
    let unpriced = estimate(
        &mut client,
        issued,
        "remove_prompt",
        json!({ "image": { "path": image.to_str().unwrap() }, "prompt": "a lamp" }),
    );
    assert_eq!(unpriced["refused"], "readiness", "{unpriced:#}");
    assert_eq!(unpriced["worst_case_usd"], Value::Null);
    out.insert("estimate-refused-readiness.json", unpriced);
    let run_cap = result(
        &mut client,
        call("cap", "seam", with("max_spend_usd", json!(0.03)), None),
    );
    assert_eq!(run_cap["error"]["code"], "cap", "{run_cap:#}");
    out.insert("run-cap.json", run_cap);
    let argument = result(
        &mut client,
        bound("argument", "seam", with("strict", json!(true)), &binding),
    );
    assert_eq!(argument["error"]["code"], "stale_estimate", "{argument:#}");
    out.insert("run-stale-estimate-argument.json", argument);
    let mut bytes = std::fs::read(&image).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(&image, &bytes).unwrap();
    let changed = result(&mut client, bound("image", "seam", paid.clone(), &binding));
    assert_eq!(changed["error"]["code"], "stale_estimate", "{changed:#}");
    out.insert("run-stale-estimate-image.json", changed);
    bytes[last] ^= 1;
    std::fs::write(&image, &bytes).unwrap();
    assert_eq!(filler_requests(), 0);
    let spent = result(&mut client, call("paid", "seam", paid.clone(), None));
    assert_eq!(spent["status"], "completed", "{spent:#}");
    assert_eq!(filler_requests(), 1);
    let valid = describe(&mut client);
    assert_eq!(requirement(&valid, "seam", "credential")["state"], "valid");
    out.insert("describe-credential-valid.json", valid);
    let budget = estimate(&mut client, issued, "seam", paid.clone());
    assert_eq!(budget["refused"], "budget", "{budget:#}");
    out.insert("estimate-refused-budget.json", budget);
    let run_budget = result(&mut client, call("budget", "seam", paid.clone(), None));
    assert_eq!(run_budget["error"]["code"], "budget", "{run_budget:#}");
    out.insert("run-budget.json", run_budget);
    client.close().unwrap();
    assert_eq!(filler_requests(), 1);

    // Restarted under another configuration with a key the provider rejects: the earlier
    // session's estimate was never issued here (another session's and another configuration's
    // alike), and the rejection is `credential`, which makes the key invalid.
    let mut client = serve(dir, Some(&other), &[(KEY_ENV, WRONG_KEY)]);
    sessions.push(session_id(&mut client));
    let restarted = result(
        &mut client,
        bound("session", "seam", paid.clone(), &binding),
    );
    assert_eq!(
        restarted["error"]["code"], "stale_estimate",
        "{restarted:#}"
    );
    out.insert("run-stale-estimate-session.json", restarted);
    let rejected = result(&mut client, call("rejected", "seam", paid.clone(), None));
    assert_eq!(rejected["error"]["code"], "credential", "{rejected:#}");
    assert_eq!(rejected["spend"]["usd"], 0.0, "{rejected:#}");
    assert_eq!(rejected["spend"]["basis"], "measured", "{rejected:#}");
    out.insert("run-credential-rejected.json", rejected);
    assert_eq!(filler_requests(), 2);
    let invalid = describe(&mut client);
    assert_eq!(
        requirement(&invalid, "seam", "credential")["state"],
        "invalid"
    );
    out.insert("describe-credential-invalid.json", invalid);
    client.close().unwrap();

    // An estimate honoured for no time at all (a testkit build's lifetime): expired.
    let mut client = serve(
        dir,
        Some(&config),
        &[(KEY_ENV, KEY), ("RASCAL_TESTKIT_ESTIMATE_TTL_MS", "0")],
    );
    sessions.push(session_id(&mut client));
    let short = estimate(&mut client, issued, "seam", paid.clone());
    let expired = result(
        &mut client,
        bound("expired", "seam", paid.clone(), &short["binding"]),
    );
    assert_eq!(expired["error"]["code"], "stale_estimate", "{expired:#}");
    out.insert("run-stale-estimate-expired.json", expired);
    client.close().unwrap();
    assert_eq!(filler_requests(), 2);

    // The command line's estimate: no session.
    let mut child = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args([
            "estimate",
            "--json",
            "--config",
            config.to_str().unwrap(),
            "-",
        ])
        .env_remove("RASCAL_LOG")
        .env(KEY_ENV, KEY)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            json!({ "action": "seam", "arguments": paid })
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    let cli = child.wait_with_output().unwrap();
    assert_eq!(
        cli.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let cli: Value = serde_json::from_slice(&cli.stdout).unwrap();
    assert_eq!(cli["session_id"], Value::Null);
    assert_eq!(cli["allowed"], true, "{cli:#}");
    issued.push(cli.clone());
    out.insert("estimate-cli.json", cli);
}

/// A `rascal serve` over `ws` with the runs under `dir/runs` and rascal's data and cache under
/// `dir`, allowed `allow` (the repository and the temporary directory when `None`).
fn serve_v04(dir: &Path, ws: &Path, allow: Option<&[&Path]>) -> Client {
    let mut command = match allow {
        None => rascal_testkit::mcp_client::serve_command(env!("CARGO_BIN_EXE_rascal")),
        Some(dirs) => {
            let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
            command.arg("serve");
            for d in dirs {
                command.arg("--allow-dir").arg(d);
            }
            command
        }
    };
    command
        .args([
            "--workspace",
            ws.to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env("XDG_DATA_HOME", dir.join("data"))
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .env_remove("RASCAL_LOG")
        .env_remove(KEY_ENV);
    Client::start(&mut command).unwrap()
}

/// A completed call's result, by `tools/call`.
fn done(client: &mut Client, token: &str, name: &str, arguments: Value) -> Value {
    let r = result(client, call(token, name, arguments, None));
    assert_eq!(r["status"], "completed", "{name}: {r:#}");
    r
}

/// The reference an operation or task made on `port`.
fn made(result: &Value, port: &str) -> Value {
    result["artifacts"]["out"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["port"] == port)
        .unwrap_or_else(|| panic!("no {port} in {result:#}"))["artifact"]
        .clone()
}

/// The refusal data of a resource read.
fn read_refused(client: &mut Client, uri: &str) -> Value {
    match client.request("resources/read", json!({ "uri": uri })) {
        Err(rascal_testkit::mcp_client::Failure::Rpc { code, data, .. }) => {
            assert_eq!(code, -32602);
            data.unwrap()
        }
        other => panic!("{uri}: not refused: {other:?}"),
    }
}

/// A control's answer (`export`, `import`) or its refusal data.
fn control(client: &mut Client, name: &str, arguments: Value) -> Result<Value, Value> {
    match client.call(name, arguments) {
        Ok(r) => Ok(r.structured.unwrap()),
        Err(rascal_testkit::mcp_client::Failure::Rpc { data, .. }) => Err(data.unwrap()),
        Err(other) => panic!("{name}: {other:?}"),
    }
}

/// v0.4's set (task 10.14): the operations Line → Rasterize → Feather, a task's Mask bound by
/// an operation, every binding refusal from a call and from `estimate` (an index in a `many`
/// port, the released id accepted again), the artifact resource, export and import, a
/// `rascal call` of an invocation, a bundle and its replay from another directory with the
/// original store gone, a session with no `--allow-dir`, and every path the rule refuses. The
/// bundle's members go into `raw`, byte for byte.
fn v04_set(
    dir: &Path,
    out: &mut BTreeMap<&'static str, Value>,
    raw: &mut BTreeMap<String, Vec<u8>>,
    sessions: &mut Vec<String>,
) {
    let images = repo_root().join("fixtures/images");
    let floor = images.join("floor.png");
    let chair = images.join("chair-blob.png");
    let pano = images.join("step-pano.png");
    let path = |p: &Path| json!({ "path": p.to_str().unwrap() });
    let ws = dir.join("v04/ws");
    let mut client = serve_v04(dir, &ws, None);
    sessions.push(session_id(&mut client));

    let line = done(
        &mut client,
        "line",
        "line",
        json!({ "image": path(&floor), "column": 0.5, "width_px": 24 }),
    );
    let location = made(&line, "location");
    let image = line["artifacts"]["in"][0]["artifact"].clone();
    let rasterized = done(
        &mut client,
        "rasterize",
        "rasterize",
        json!({ "location": location, "image": image }),
    );
    let mask = made(&rasterized, "mask");
    let feathered = done(
        &mut client,
        "feather",
        "feather",
        json!({ "mask": mask, "sigma_px": 6 }),
    );
    let removed = done(
        &mut client,
        "remove",
        "remove",
        json!({ "image": path(&chair), "location": { "kind": "bbox", "bbox": [0.27, 0.21, 0.53, 0.79] } }),
    );
    let chair_mask = made(&removed, "mask");
    let from_remove = done(
        &mut client,
        "feather-from-remove",
        "feather",
        json!({ "mask": chair_mask, "sigma_px": 3 }),
    );

    // Every binding problem, from a call and from `estimate`.
    let zeros = format!("sha256:{}", "0".repeat(64));
    let mut unresolved = mask.clone();
    unresolved["identity"] = json!(zeros);
    unresolved["resource"] = json!(format!("rascal://artifact/{zeros}"));
    let mut newer = mask.clone();
    newer["schema_version"] = json!(2);
    let problems: [(&'static str, &'static str, &str, Value); 4] = [
        (
            "refusal-binding-wrong-type.json",
            "refusal-binding-estimate-wrong-type.json",
            "feather",
            json!({ "mask": location, "sigma_px": 6 }),
        ),
        (
            "refusal-binding-wrong-version.json",
            "refusal-binding-estimate-wrong-version.json",
            "feather",
            json!({ "mask": newer, "sigma_px": 6 }),
        ),
        (
            "refusal-binding-wrong-frame.json",
            "refusal-binding-estimate-wrong-frame.json",
            "subtract",
            json!({ "mask": mask, "subtract": chair_mask }),
        ),
        (
            "refusal-binding-unresolved.json",
            "refusal-binding-estimate-unresolved.json",
            "feather",
            json!({ "mask": unresolved, "sigma_px": 6 }),
        ),
    ];
    for (from_call, from_estimate, action, arguments) in problems {
        let refused = refusal(
            &mut client,
            call("refused", action, arguments.clone(), None),
        );
        let estimated = refusal(
            &mut client,
            json!({ "name": "estimate", "arguments": { "action": action, "arguments": arguments } }),
        );
        assert_eq!(refused["bindings"], estimated["bindings"], "{from_call}");
        out.insert(from_call, refused);
        out.insert(from_estimate, estimated);
    }
    let many = refusal(
        &mut client,
        call(
            "many",
            "select",
            json!({ "plane": [location, unresolved] }),
            None,
        ),
    );
    assert!(many["bindings"][1]["index"] == 1, "{many:#}");
    out.insert("refusal-binding-many-index.json", many);
    // A refused call releases its id; the retry under it is accepted.
    let id = Some("fixture.binding");
    let first = refusal(
        &mut client,
        call(
            "released",
            "feather",
            json!({ "mask": unresolved, "sigma_px": 6 }),
            id,
        ),
    );
    assert_eq!(first["reason"], "binding");
    let retried = result(
        &mut client,
        call(
            "retried",
            "feather",
            json!({ "mask": mask, "sigma_px": 5 }),
            id,
        ),
    );
    assert_eq!(retried["status"], "completed", "{retried:#}");
    out.insert("run-binding-claim-released.json", retried);

    // The artifact resource.
    out.insert(
        "artifact-mask.json",
        read(&mut client, mask["resource"].as_str().unwrap()),
    );
    out.insert(
        "artifact-location.json",
        read(&mut client, location["resource"].as_str().unwrap()),
    );
    let cut = done(
        &mut client,
        "cut",
        "cut",
        json!({ "image": path(&pano), "strip": 0.1 }),
    );
    let plan = made(&cut, "plan");
    let plan_body = read(&mut client, plan["resource"].as_str().unwrap());
    assert_eq!(plan_body["preview"], Value::Null);
    out.insert("artifact-plan.json", plan_body);
    out.insert(
        "refusal-artifact-unresolved.json",
        read_refused(&mut client, &format!("rascal://artifact/{zeros}")),
    );

    // Export, and the refusals.
    let exported = control(&mut client, "export", json!({ "artifact": mask })).unwrap();
    let mask_file = PathBuf::from(exported["file"]["path"].as_str().unwrap());
    out.insert("export-mask.json", exported);
    let exported_location =
        control(&mut client, "export", json!({ "artifact": location })).unwrap();
    let location_bytes =
        std::fs::read(exported_location["file"]["path"].as_str().unwrap()).unwrap();
    out.insert(
        "refusal-export-not-portable.json",
        control(&mut client, "export", json!({ "artifact": plan })).unwrap_err(),
    );
    out.insert(
        "refusal-export-unresolved.json",
        control(&mut client, "export", json!({ "artifact": unresolved })).unwrap_err(),
    );
    client.close().unwrap();

    // Import into another store.
    let mut other = serve_v04(dir, &dir.join("v04/other"), None);
    sessions.push(session_id(&mut other));
    let file = json!({ "file": { "path": mask_file.to_str().unwrap() } });
    let imported = control(&mut other, "import", file.clone()).unwrap();
    assert_eq!(imported["already_present"], false);
    out.insert("import-mask.json", imported);
    let again = control(&mut other, "import", file).unwrap();
    assert_eq!(again["already_present"], true);
    out.insert("import-mask-again.json", again);
    // A location needs its image in the store first: any action that takes it.
    done(
        &mut other,
        "other-line",
        "line",
        json!({ "image": path(&floor), "column": 0.25, "width_px": 16 }),
    );
    let inline = json!({ "inline": { "media_type": "application/json",
        "data": rascal_core::panel::base64(&location_bytes) } });
    let location_in = control(&mut other, "import", inline).unwrap();
    assert_eq!(location_in["already_present"], false, "{location_in:#}");
    out.insert("import-location-inline.json", location_in);
    let png = json!({ "inline": { "media_type": "image/png",
        "data": rascal_core::panel::base64(&std::fs::read(&floor).unwrap()) } });
    out.insert(
        "refusal-import-png.json",
        control(&mut other, "import", png).unwrap_err(),
    );
    other.close().unwrap();

    // `rascal call` of the Feather's invocation.
    let args_file = dir.join("v04/feather-args.json");
    std::fs::write(&args_file, feathered["invocation"]["arguments"].to_string()).unwrap();
    let called = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args([
            "call",
            "feather",
            "--arguments",
            args_file.to_str().unwrap(),
            "--workspace",
            ws.to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env("XDG_DATA_HOME", dir.join("data"))
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .env_remove("RASCAL_LOG")
        .output()
        .unwrap();
    assert_eq!(called.status.code(), Some(0));
    let called: Value = serde_json::from_slice(&called.stdout).unwrap();
    assert_eq!(called["invocation"], feathered["invocation"]);
    out.insert("run-call-feather.json", called);

    // The chain as a bundle, replayed elsewhere with the original store gone.
    use rascal_testkit::invocation_bundle::{self as writer, ArtifactFile, InputFile, Step};
    let floor_bytes = std::fs::read(&floor).unwrap();
    let inputs = |result: &Value| -> Vec<InputFile> {
        result["images"]["in"]
            .as_array()
            .unwrap()
            .iter()
            .map(|taken| InputFile {
                slot: None,
                port: Some(taken["slot"].as_str().unwrap().to_string()),
                image: taken["image"].clone(),
                bytes: floor_bytes.clone(),
                ext: "png".to_string(),
            })
            .collect()
    };
    let steps = vec![
        Step {
            result: line.clone(),
            inputs: inputs(&line),
            artifacts: vec![],
            reference_only: vec![],
            unportable: vec![],
        },
        Step {
            result: rasterized.clone(),
            inputs: inputs(&rasterized),
            artifacts: vec![ArtifactFile {
                port: "location".to_string(),
                artifact: location.clone(),
                bytes: location_bytes,
                ext: "location.json".to_string(),
            }],
            reference_only: vec![],
            unportable: vec![],
        },
        Step {
            result: feathered.clone(),
            inputs: vec![],
            artifacts: vec![ArtifactFile {
                port: "mask".to_string(),
                artifact: mask.clone(),
                bytes: std::fs::read(&mask_file).unwrap(),
                ext: "mask.tar".to_string(),
            }],
            reference_only: vec![],
            unportable: vec![],
        },
    ];
    let (manifest, members) = writer::parts(&steps);
    let bundle = dir.join("v04/bundle/chain.tar");
    std::fs::create_dir_all(bundle.parent().unwrap()).unwrap();
    std::fs::write(&bundle, writer::write(&steps)).unwrap();
    std::fs::remove_dir_all(&ws).unwrap();
    let replayed = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args([
            "call",
            "--bundle",
            bundle.to_str().unwrap(),
            "--workspace",
            dir.join("v04/replay").to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
            "--allow-dir",
            bundle.parent().unwrap().to_str().unwrap(),
        ])
        .env("XDG_DATA_HOME", dir.join("data"))
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .env_remove("RASCAL_LOG")
        .output()
        .unwrap();
    assert_eq!(
        replayed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    let lines: Vec<Value> = String::from_utf8(replayed.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    for ((name, original), replayed) in [
        ("run-replay-line.json", &line),
        ("run-replay-rasterize.json", &rasterized),
        ("run-replay-feather.json", &feathered),
    ]
    .into_iter()
    .zip(lines)
    {
        assert_eq!(replayed["invocation"], original["invocation"], "{name}");
        out.insert(name, replayed);
    }
    out.insert(BUNDLE_MANIFEST, manifest);
    for (member, bytes) in members {
        raw.insert(format!("{BUNDLE_DIR}/{member}"), bytes);
    }
    out.insert("run-op-line.json", line);
    out.insert("run-op-rasterize.json", rasterized);
    out.insert("run-op-feather.json", feathered);
    out.insert("run-remove-mask.json", removed);
    out.insert("run-op-feather-from-remove.json", from_remove);

    // The path rule: a server allowed one directory, and one allowed none.
    let allowed = dir.join("v04/allowed");
    let outside = dir.join("v04/outside");
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let outside_pano = outside.join("pano.png");
    std::fs::copy(&pano, &outside_pano).unwrap();
    std::fs::copy(&pano, allowed.join("pano.png")).unwrap();
    std::os::unix::fs::symlink(&outside_pano, allowed.join("link.png")).unwrap();
    let allowed_dir = allowed.canonicalize().unwrap();
    let mut ruled = serve_v04(dir, &dir.join("v04/ruled"), Some(&[allowed_dir.as_path()]));
    sessions.push(session_id(&mut ruled));
    for (name, given) in [
        ("run-path-outside.json", outside_pano.clone()),
        ("run-path-dotdot.json", allowed.join("../outside/pano.png")),
        ("run-path-symlink.json", allowed.join("link.png")),
        ("run-path-missing.json", allowed.join("missing.png")),
    ] {
        let r = result(
            &mut ruled,
            call("path", "seam", json!({ "image": path(&given) }), None),
        );
        assert_eq!(
            (&r["status"], &r["error"]["code"]),
            (&json!("invalid"), &json!("input")),
            "{name}: {r:#}"
        );
        out.insert(name, r);
    }
    ruled.close().unwrap();
    let mut closed = serve_v04(dir, &dir.join("v04/closed"), Some(&[]));
    sessions.push(session_id(&mut closed));
    let session = read(&mut closed, "rascal://session");
    assert_eq!(session["allowed_dirs"], json!([]));
    out.insert("session-no-allow-dir.json", session);
    let r = result(
        &mut closed,
        call(
            "closed",
            "seam",
            json!({ "image": path(&allowed.join("pano.png")) }),
            None,
        ),
    );
    assert_eq!(r["error"]["code"], "input", "{r:#}");
    out.insert("run-path-no-allow-dir.json", r);
    closed.close().unwrap();
}

/// The bundle fixture's directory and its manifest.
const BUNDLE_DIR: &str = "bundle-line-rasterize-feather";
const BUNDLE_MANIFEST: &str = "bundle-line-rasterize-feather/invocation.json";

#[test]
fn the_table_manners_fixtures_hold() {
    let dir = scratch();
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let (url, seen) = spawn(Behaviour::Exact, Some(KEY.to_string()));
    let filler_requests = || seen.lock().unwrap().len();
    let mut out: BTreeMap<&'static str, Value> = BTreeMap::new();
    let mut sessions = Vec::new();
    let mut issued = Vec::new();
    let mut raw: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    m11_set(&dir, &pano, &url, &mut out, &mut sessions);
    v03_set(
        &dir,
        &url,
        &filler_requests,
        &mut out,
        &mut sessions,
        &mut issued,
    );
    v04_set(&dir, &mut out, &mut raw, &mut sessions);

    let runs: Vec<String> = std::fs::read_dir(dir.join("runs"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("run-") && !n.ends_with(".in"))
        .collect();
    // The invocations' configuration identities after the estimates', in file order.
    issued.extend(
        out.values()
            .filter_map(|v| v.get("invocation"))
            .filter(|i| i.is_object())
            .cloned(),
    );
    let normalise = Normalise::new(&dir, &sessions, &runs, &issued, &url);
    let stored = repo_root().join("fixtures/table-manners");
    let update = std::env::var_os("UPDATE_FIXTURES").is_some();
    if update {
        let _ = std::fs::remove_dir_all(&stored);
        std::fs::create_dir_all(&stored).unwrap();
    }
    for (name, value) in &out {
        let value = normalise.apply(value);
        let text = serde_json::to_string_pretty(&value).unwrap() + "\n";
        for secret in [KEY, WRONG_KEY] {
            assert!(!text.contains(secret), "{name} carries a key's value");
        }
        conforms(name, &value);
        // Every result records its invocation (4.8), but the runs whose arguments did not
        // resolve: the image that is not the one it expected, and each path the rule refuses.
        if name.starts_with("run-") {
            assert_eq!(
                value["invocation"].is_null(),
                *name == "run-invalid.json" || name.starts_with("run-path-"),
                "{name}: {value:#}"
            );
        }
        let path = stored.join(name);
        if update {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &text).unwrap();
        }
        let expected: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            value,
            expected,
            "{name}: UPDATE_FIXTURES=1 regenerates {}",
            path.display()
        );
    }
    // The bundle's members, byte for byte.
    for (name, bytes) in &raw {
        let path = stored.join(name);
        if update {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
        }
        assert_eq!(
            &std::fs::read(&path).unwrap(),
            bytes,
            "{name}: UPDATE_FIXTURES=1 regenerates it"
        );
    }
    let mut files = Vec::new();
    let mut pending = vec![stored.clone()];
    while let Some(at) = pending.pop() {
        for entry in std::fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(
                    path.strip_prefix(&stored)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    files.sort();
    let mut made: Vec<String> = out
        .keys()
        .map(|k| k.to_string())
        .chain(raw.keys().cloned())
        .collect();
    made.sort();
    assert_eq!(
        files, made,
        "fixtures/table-manners holds exactly the generated set (UPDATE_FIXTURES=1)"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
