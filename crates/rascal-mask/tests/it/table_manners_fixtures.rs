//! Task 11.6 (table-manners 7): rascal's conformance fixtures for the contract's
//! `fixtures/rascal/`, produced by `rascal serve` through the independent client and never
//! written by hand: the description, a run per status (an unresolved charge among them), two
//! pages of the runs list, the session, and both reuse refusals. Machine-specific values are
//! replaced by fixed stand-ins that stay valid (paths under `/runs` and `/data`, one time, one
//! session id, run ids in their order), so the files compare equal on any machine.
//! `UPDATE_FIXTURES=1` rewrites `fixtures/table-manners/`; otherwise the output must match it.
//! Every file is valid against its schema.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use rascal_core::contract::table_manners;
use rascal_testkit::mcp_client::Client;

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

fn start(dir: &Path) -> Client {
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
            "config_version = 1\n[limits]\nsession_budget_usd = 1.0\n\n{}{}{}",
            filler("fake", "exact"),
            filler("failing", "failing"),
            filler("slow", "slow")
        ),
    )
    .unwrap();
    let mut command = Command::new(bin);
    command
        .args([
            "serve",
            "--workspace",
            dir.join("ws").to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env_remove("RASCAL_LOG");
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

fn valid(name: &str, schema: &str, value: &serde_json::Value) {
    let (_, text) = table_manners::SCHEMAS
        .iter()
        .find(|(i, _)| *i == format!("urn:table-manners:core:{schema}"))
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
    assert!(errors.is_empty(), "{name}: {errors:?}\n{value:#}");
}

fn call(
    token: &str,
    name: &str,
    arguments: serde_json::Value,
    id: Option<&str>,
) -> serde_json::Value {
    let mut meta = serde_json::json!({ "progressToken": token });
    if let Some(id) = id {
        meta["table-manners/client_run_id"] = serde_json::json!(id);
    }
    serde_json::json!({ "name": name, "arguments": arguments, "_meta": meta })
}

/// The response to request `id`, skipping everything else.
fn response(client: &mut Client, id: u64) -> serde_json::Value {
    loop {
        let message = client.next_message().unwrap();
        if message.get("id").and_then(|v| v.as_u64()) == Some(id) {
            return message;
        }
    }
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

fn read(client: &mut Client, uri: &str) -> serde_json::Value {
    let result = client
        .request("resources/read", serde_json::json!({ "uri": uri }))
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
            serde_json::json!({ "uri": format!("rascal://run/{run}/journal") }),
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
    fn new(dir: &Path, session: &str, runs: &[String]) -> Normalise {
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
        replacements.push((
            session.to_string(),
            "00000000-0000-4000-8000-000000000001".to_string(),
        ));
        Normalise { replacements }
    }

    fn apply(&self, value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => serde_json::Value::String(self.text(s)),
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(|v| self.apply(v)).collect())
            }
            serde_json::Value::Object(map) => serde_json::Value::Object(
                map.iter()
                    .map(|(k, v)| {
                        let v = match k.as_str() {
                            "duration_ms" => serde_json::json!(0),
                            "input_identity" => {
                                serde_json::json!(format!("sha256:{}", "0".repeat(64)))
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
        // Times, and durations inside the journal's notes.
        let mut fixed = String::new();
        let mut rest = out.as_str();
        while let Some(at) = rest.find("\"duration_ms\":") {
            let (head, tail) = rest.split_at(at + "\"duration_ms\":".len());
            fixed.push_str(head);
            fixed.push('0');
            rest = tail.trim_start_matches(|c: char| c.is_ascii_digit());
        }
        fixed.push_str(rest);
        if is_time(&fixed) {
            return "2026-01-01T00:00:00.000Z".to_string();
        }
        fixed
    }
}

/// An RFC 3339 UTC time as rascal writes them.
fn is_time(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 24
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[19] == b'.'
        && b[23] == b'Z'
}

#[test]
fn the_table_manners_fixtures_hold() {
    let dir = scratch();
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let seam = |filler: &str| serde_json::json!({ "image": { "path": pano.to_str().unwrap() }, "filler": filler });
    let mut client = start(&dir);
    let mut out: BTreeMap<&str, (serde_json::Value, &str)> = BTreeMap::new();

    let described = client
        .call("describe", serde_json::json!({}))
        .unwrap()
        .structured
        .unwrap();
    out.insert("describe.json", (described, "description"));

    // Completed, with a correlation id.
    let id = client
        .begin(
            "tools/call",
            call("completed", "seam", seam("fake"), Some("fixture.completed")),
        )
        .unwrap();
    let completed = response(&mut client, id)["result"]["structuredContent"].clone();
    assert_eq!(completed["status"], "completed", "{completed:#}");
    out.insert("run-completed.json", (completed, "result"));

    // An error whose filler call was sent and never settled: the charge is unresolved.
    let id = client
        .begin("tools/call", call("error", "seam", seam("failing"), None))
        .unwrap();
    let error = response(&mut client, id)["result"]["structuredContent"].clone();
    assert_eq!(error["status"], "error", "{error:#}");
    assert_eq!(error["spend"]["unresolved_usd"], 0.04);
    out.insert("run-error-unresolved.json", (error, "result"));

    // Invalid: the image is not the one expected.
    let wrong = serde_json::json!({ "path": pano.to_str().unwrap(), "expect": {
        "file_sha256": "0".repeat(64), "width": 800, "height": 400,
        "projection": "equirectangular", "topology": "wrap" } });
    let id = client
        .begin(
            "tools/call",
            call(
                "invalid",
                "seam",
                serde_json::json!({ "image": wrong }),
                None,
            ),
        )
        .unwrap();
    let invalid = response(&mut client, id)["result"]["structuredContent"].clone();
    assert_eq!(invalid["status"], "invalid", "{invalid:#}");
    out.insert("run-invalid.json", (invalid, "result"));

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
            serde_json::json!({ "requestId": id, "reason": "fixture" }),
        )
        .unwrap();
    wait_for_end(&mut client, &cancelled_run);
    let cancelled = read(
        &mut client,
        &format!("rascal://run/{cancelled_run}/run.json"),
    );
    assert_eq!(cancelled["status"], "cancelled", "{cancelled:#}");
    out.insert("run-cancelled.json", (cancelled, "result"));

    // Reuse of an id whose run ended: the refusal names it.
    let id = client
        .begin(
            "tools/call",
            call("reuse", "seam", seam("fake"), Some("fixture.completed")),
        )
        .unwrap();
    let reused = response(&mut client, id)["error"]["data"].clone();
    out.insert("refusal-earlier-run.json", (reused, "refusal"));

    // Reuse of an id whose call was cancelled while queued: no run to name.
    let holder = client
        .begin("tools/call", call("holder", "seam", seam("slow"), None))
        .unwrap();
    let holder_run = first_run(&mut client);
    let queued = client
        .begin(
            "tools/call",
            call("queued", "seam", seam("fake"), Some("fixture.queued")),
        )
        .unwrap();
    client
        .notify(
            "notifications/cancelled",
            serde_json::json!({ "requestId": queued, "reason": "fixture" }),
        )
        .unwrap();
    let _ = response(&mut client, holder);
    wait_for_end(&mut client, &holder_run);
    let id = client
        .begin(
            "tools/call",
            call("retry", "seam", seam("fake"), Some("fixture.queued")),
        )
        .unwrap();
    let none = response(&mut client, id)["error"]["data"].clone();
    assert_eq!(none["run"], serde_json::Value::Null, "{none:#}");
    out.insert("refusal-no-run.json", (none, "refusal"));

    // Enough quick invalid runs for a second page of the runs list.
    for i in 0..50 {
        let id = client
            .begin(
                "tools/call",
                call(
                    &format!("fill-{i}"),
                    "seam",
                    serde_json::json!({ "image": wrong }),
                    None,
                ),
            )
            .unwrap();
        let _ = response(&mut client, id);
    }
    let first = read(&mut client, "rascal://runs");
    let cursor = first["next_cursor"].as_str().unwrap().to_string();
    let second = read(&mut client, &format!("rascal://runs?cursor={cursor}"));
    assert_eq!(second["next_cursor"], serde_json::Value::Null);
    out.insert("runs.json", (first, "runs"));
    out.insert("runs-page-2.json", (second, "runs"));
    let session = read(&mut client, "rascal://session");
    let session_id = session["id"].as_str().unwrap().to_string();
    out.insert("session.json", (session, "session"));
    client.close().unwrap();

    let runs: Vec<String> = std::fs::read_dir(dir.join("runs"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("run-") && !n.ends_with(".in"))
        .collect();
    let normalise = Normalise::new(&dir, &session_id, &runs);
    let stored = repo_root().join("fixtures/table-manners");
    let update = std::env::var_os("UPDATE_FIXTURES").is_some();
    if update {
        let _ = std::fs::remove_dir_all(&stored);
        std::fs::create_dir_all(&stored).unwrap();
    }
    for (name, (value, schema)) in &out {
        let value = normalise.apply(value);
        valid(name, schema, &value);
        let path = stored.join(name);
        if update {
            std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap() + "\n").unwrap();
        }
        let expected: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            value,
            expected,
            "{name}: UPDATE_FIXTURES=1 regenerates {}",
            path.display()
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
