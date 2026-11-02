//! Task 8.5 (ADR 0020): `rascal serve` under table-manners, driven by the independent client:
//! `describe`; a `seam` action healed through the fake filler with progress strictly increasing
//! and every notification naming the run in `_meta`; the run as resources (the list, run.json, an
//! image); an image that does not match its `expect` is an `invalid` result; a relative path is
//! refused; a cancelled call gets no result and its run says `cancelled`.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use rascal_core::contract::table_manners;
use rascal_testkit::mcp_client::{Client, Failure, base64_decode};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-serve-tm-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn config(dir: &Path) -> PathBuf {
    let path = dir.join("rascal.toml");
    let bin = env!("CARGO_BIN_EXE_rascal");
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\ntimeout_s = 30\nrevision = \"fake\"\n\n[fillers.slow]\ncommand = [{bin:?}, \"fake-filler\", \"slow\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\ntimeout_s = 30\nrevision = \"slow\"\nspend_usd_per_call = 0.04\n\n[fillers.failing]\ncommand = [{bin:?}, \"fake-filler\", \"failing\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\ntimeout_s = 30\nrevision = \"failing\"\nspend_usd_per_call = 0.04\n"
        ),
    )
    .unwrap();
    path
}

fn start(dir: &Path) -> Client {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "serve",
            "--workspace",
            dir.join("ws").to_str().unwrap(),
            "--config",
            config(dir).to_str().unwrap(),
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

/// Asserts `value` is valid against the contract's schema `id`.
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

/// `rascal://session`, valid against the contract's session schema.
fn session(client: &mut Client) -> serde_json::Value {
    let text = read_resource(client, "rascal://session")["text"]
        .as_str()
        .unwrap()
        .to_string();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    valid("urn:table-manners:core:session", &value);
    value
}

fn read_resource(client: &mut Client, uri: &str) -> serde_json::Value {
    let result = client
        .request("resources/read", serde_json::json!({ "uri": uri }))
        .unwrap();
    result["contents"][0].clone()
}

#[test]
fn an_action_runs_with_progress_and_its_run_is_a_resource() {
    let dir = scratch("action");
    let mut client = start(&dir);
    let names: Vec<String> = client
        .list_tools()
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    for name in [
        "seam",
        "remove_prompt",
        "describe",
        "rasterize",
        "workspace_import",
    ] {
        assert!(names.iter().any(|n| n == name), "{name} in {names:?}");
    }
    let described = client.call("describe", serde_json::json!({})).unwrap();
    let described = described.structured.unwrap();
    assert_eq!(described["tool"]["name"], "rascal");
    assert_eq!(described["profiles"], serde_json::json!(["images"]));

    let pano = repo_root().join("fixtures/images/step-pano.png");
    let id = client
        .begin(
            "tools/call",
            serde_json::json!({
                "name": "seam",
                "arguments": { "image": { "path": pano.to_str().unwrap() }, "filler": "fake",
                               "image_blocks": true },
                "_meta": { "progressToken": "p-1" }
            }),
        )
        .unwrap();
    let (result, notifications) = client.finish_collecting(id).unwrap();
    let structured = &result["structuredContent"];
    assert_eq!(structured["status"], "completed", "{structured:#}");
    assert_eq!(result["isError"], false);
    let progress: Vec<&serde_json::Value> = notifications
        .iter()
        .filter(|n| n["method"] == "notifications/progress")
        .collect();
    assert!(progress.len() >= 3, "{notifications:#?}");
    let values: Vec<f64> = progress
        .iter()
        .map(|n| n["params"]["progress"].as_f64().unwrap())
        .collect();
    assert!(values.windows(2).all(|w| w[1] > w[0]), "{values:?}");
    let run_id = structured["run"]["id"].as_str().unwrap().to_string();
    // Every notification names the run in `_meta`, as the result's reference without `dir`;
    // the words are for people and never carry it (table-manners 4.1).
    let mut reference = structured["run"].clone();
    reference.as_object_mut().unwrap().remove("dir");
    for n in &progress {
        assert_eq!(
            n["params"]["_meta"]["table-manners/run"], reference,
            "{n:#}"
        );
        assert!(
            !n["params"]["message"]
                .as_str()
                .unwrap()
                .contains("rascal://"),
            "{n:#}"
        );
    }
    valid("urn:table-manners:core:run-ref", &reference);
    assert!(
        progress
            .iter()
            .any(|n| n["params"]["message"].as_str().unwrap().contains("(fan)"))
    );
    // Image blocks were asked for: the mask and the result.
    let blocks: Vec<&serde_json::Value> = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["type"] == "image")
        .collect();
    assert_eq!(blocks.len(), 2);

    // The run as resources.
    let runs: serde_json::Value = serde_json::from_str(
        read_resource(&mut client, "rascal://runs")["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(runs["runs"][0]["id"], run_id.as_str());
    assert_eq!(runs["runs"][0]["status"], "completed");
    assert_eq!(runs["runs"][0]["action"], "seam");
    let stored: serde_json::Value = serde_json::from_str(
        read_resource(&mut client, &format!("rascal://run/{run_id}/run.json"))["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(&stored, structured);
    let image = read_resource(&mut client, &format!("rascal://run/{run_id}/images/result"));
    assert_eq!(image["mimeType"], "image/png");
    let png = base64_decode(image["blob"].as_str().unwrap()).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    let journal = read_resource(&mut client, &format!("rascal://run/{run_id}/journal"));
    assert!(journal["text"].as_str().unwrap().contains("\"fan\""));
    // A path outside the runs, or a made-up id, is not a resource.
    assert!(matches!(
        client.request(
            "resources/read",
            serde_json::json!({ "uri": "rascal://run/../../etc/run.json" })
        ),
        Err(Failure::Rpc { .. })
    ));
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_image_that_is_not_the_one_expected_is_an_invalid_result() {
    let dir = scratch("expect");
    let mut client = start(&dir);
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let bytes = std::fs::read(&pano).unwrap();
    let data = rascal_core::panel::base64(&bytes);
    let wrong = serde_json::json!({ "file_sha256": "0".repeat(64), "width": 800, "height": 400,
                                    "projection": "equirectangular", "topology": "wrap" });
    let out = client
        .call(
            "seam",
            serde_json::json!({ "image": { "inline": { "media_type": "image/png", "data": data }, "expect": wrong } }),
        )
        .unwrap();
    assert!(out.is_error);
    let s = out.structured.unwrap();
    assert_eq!(s["status"], "invalid", "{s:#}");
    assert_eq!(s["error"]["code"], "input");
    // The same image inline with the right reference runs, and returns its images inline.
    let right = serde_json::json!({ "file_sha256": rascal_core::contract::file_sha256(&bytes),
                                    "width": 800, "height": 400,
                                    "projection": "equirectangular", "topology": "wrap" });
    let out = client
        .call(
            "seam",
            serde_json::json!({ "image": { "inline": { "media_type": "image/png", "data": rascal_core::panel::base64(&bytes) }, "expect": right },
                                "return_images": "inline" }),
        )
        .unwrap();
    let s = out.structured.unwrap();
    assert_eq!(s["status"], "completed", "{s:#}");
    assert!(
        s["images"]["out"][0]["inline"]["data"]
            .as_str()
            .unwrap()
            .len()
            > 100
    );
    assert!(s["images"]["out"][0].get("path").is_none());
    // A relative path is refused.
    let out = client
        .call(
            "seam",
            serde_json::json!({ "image": { "path": "fixtures/images/step-pano.png" } }),
        )
        .unwrap();
    assert_eq!(out.structured.unwrap()["status"], "invalid");
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cancelled_call_gets_no_result_and_its_run_says_cancelled() {
    let dir = scratch("cancel");
    let mut client = start(&dir);
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let id = client
        .begin(
            "tools/call",
            serde_json::json!({
                "name": "seam",
                "arguments": { "image": { "path": pano.to_str().unwrap() }, "filler": "slow" },
                "_meta": { "progressToken": "p-2" }
            }),
        )
        .unwrap();
    // Wait until the run is under way (its first progress names it), then cancel.
    let first = loop {
        let message = client.next_message().unwrap();
        if message["method"] == "notifications/progress" {
            break message;
        }
    };
    let run_id = first["params"]["_meta"]["table-manners/run"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    // Let the run reach the slow fill before cancelling: the call is journaled as started.
    let reached = std::time::Instant::now();
    // The journal is a resource once the run has written its first line.
    while !client
        .request(
            "resources/read",
            serde_json::json!({ "uri": format!("rascal://run/{run_id}/journal") }),
        )
        .is_ok_and(|r| {
            r["contents"][0]["text"]
                .as_str()
                .is_some_and(|j| j.contains("filler call started"))
        })
    {
        assert!(
            reached.elapsed().as_secs() < 30,
            "the run never reached the fill"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    client
        .notify(
            "notifications/cancelled",
            serde_json::json!({ "requestId": id, "reason": "test" }),
        )
        .unwrap();
    // The next answer is for a later request: the cancelled one gets none.
    let started = std::time::Instant::now();
    let status = loop {
        let runs = client
            .request(
                "resources/read",
                serde_json::json!({ "uri": "rascal://runs" }),
            )
            .unwrap();
        let list: serde_json::Value =
            serde_json::from_str(runs["contents"][0]["text"].as_str().unwrap()).unwrap();
        let run = list["runs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == run_id.as_str())
            .cloned();
        if let Some(run) = run
            && run["status"] != "running"
        {
            break run["status"].clone();
        }
        assert!(started.elapsed().as_secs() < 30, "the run never ended");
        std::thread::sleep(std::time::Duration::from_millis(200));
    };
    assert_eq!(status, "cancelled");
    let stored: serde_json::Value = serde_json::from_str(
        read_resource(&mut client, &format!("rascal://run/{run_id}/run.json"))["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(stored["error"], serde_json::Value::Null);
    // The run stops before its next step; a filler call already in flight runs to its end, and
    // its price stays spent: cancelling does not erase the cost (task 9.1).
    assert_eq!(stored["spend"]["usd"], 0.04, "{stored:#}");
    assert_eq!(stored["spend"]["unresolved_usd"], 0.0);
    // The run stops before its next step; a filler call already in flight runs to its end.
    assert!(
        stored["body"]["stopped_before"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "{stored:#}"
    );
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// `serve` under `[limits]` with one priced HTTP filler, `hosted` at 0.04 a call, served by the
/// fake on loopback; returns the client and the requests the fake saw.
fn limits_server(
    dir: &Path,
    limits: &str,
) -> (
    Client,
    std::sync::Arc<std::sync::Mutex<Vec<rascal_testkit::fake_filler::Seen>>>,
) {
    let (url, seen) =
        rascal_testkit::fake_filler::spawn(rascal_testkit::fake_filler::Behaviour::Exact, None);
    let config = dir.join("limits.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[limits]\n{limits}\n[fillers.hosted]\nurl = {url:?}\nprofile = \"grey\"\nrevision = \"hosted\"\nspend_usd_per_call = 0.04\n"
        ),
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
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
    (Client::start(&mut command).unwrap(), seen)
}

/// A `seam` call healed by `hosted`, lowered to `max_spend_usd` when given; its result,
/// validated.
fn spend_call(client: &mut Client, max_spend_usd: Option<f64>) -> serde_json::Value {
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let mut args =
        serde_json::json!({ "image": { "path": pano.to_str().unwrap() }, "filler": "hosted" });
    if let Some(asked) = max_spend_usd {
        args["max_spend_usd"] = serde_json::json!(asked);
    }
    let out = client.call("seam", args).unwrap();
    let s = out.structured.unwrap();
    valid("urn:table-manners:core:result", &s);
    assert_eq!(out.is_error, s["status"] != "completed", "{s:#}");
    s
}

/// Asserts a call was refused at the limits check: the status and code, never retryable, zero
/// spend, no request to the filler, and nothing left reserved in the session.
fn assert_refused(
    client: &mut Client,
    seen: &std::sync::Mutex<Vec<rascal_testkit::fake_filler::Seen>>,
    s: &serde_json::Value,
    status: &str,
    code: &str,
    requests_before: usize,
) {
    assert_eq!(s["status"], status, "{s:#}");
    assert_eq!(s["error"]["code"], code, "{s:#}");
    assert_eq!(s["error"]["retryable"], false, "{s:#}");
    assert_eq!(s["spend"]["usd"], 0.0, "{s:#}");
    assert_eq!(seen.lock().unwrap().len(), requests_before, "{s:#}");
    let ledger = session(client);
    assert_eq!(ledger["reserved_usd"], 0.0, "{ledger:#}");
}

/// Task 12.4 (table-manners 2.2, 4.5): the per-call cap and the session budget fail apart. A
/// `max_spend_usd` above the configured cap is a bad argument naming both amounts, never
/// silently lowered; a worst case over the effective cap is `cap`; a session that cannot
/// reserve it is `budget`. Each spends nothing and never reaches the filler.
#[test]
fn the_cap_and_the_budget_fail_with_their_own_codes() {
    let dir = scratch("budget");
    let (mut client, seen) = limits_server(&dir, "call_cap_usd = 0.10\nsession_budget_usd = 0.05");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let estimate = |client: &mut Client, extra: serde_json::Value| {
        let mut args = serde_json::json!({ "image": { "path": pano.to_str().unwrap() },
                                           "filler": "hosted" });
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        client.call(
            "estimate",
            serde_json::json!({ "action": "seam", "arguments": args }),
        )
    };
    let before = estimate(&mut client, serde_json::json!({}))
        .unwrap()
        .structured
        .unwrap();
    assert_eq!(before["worst_case_usd"], 0.04);
    assert_eq!(before["cap_usd"], 0.10);
    assert_eq!(before["allowed"], true);

    // Above the configured cap: refused as an argument, naming both amounts.
    let over = spend_call(&mut client, Some(0.2));
    assert_refused(&mut client, &seen, &over, "invalid", "usage", 0);
    let reason = over["error"]["reason"].as_str().unwrap();
    assert!(
        reason.contains("max_spend_usd") && reason.contains("0.2") && reason.contains("0.1"),
        "{reason}"
    );
    assert!(matches!(
        estimate(&mut client, serde_json::json!({ "max_spend_usd": 0.2 })),
        Err(Failure::Rpc { .. })
    ));

    // Lowered under the price: the call's worst case is over its effective cap.
    let low = spend_call(&mut client, Some(0.03));
    assert_refused(&mut client, &seen, &low, "error", "cap", 0);
    assert!(
        low["error"]["reason"].as_str().unwrap().contains("cap"),
        "{low:#}"
    );

    // The first call fits the budget and spends 0.04; the second would go over 0.05.
    let first = spend_call(&mut client, None);
    assert_eq!(first["status"], "completed", "{first:#}");
    assert_eq!(first["spend"]["usd"], 0.04);
    assert_eq!(seen.lock().unwrap().len(), 1);
    let after = estimate(&mut client, serde_json::json!({}))
        .unwrap()
        .structured
        .unwrap();
    assert_eq!(after["allowed"], false);
    let second = spend_call(&mut client, None);
    assert_refused(&mut client, &seen, &second, "error", "budget", 1);
    assert!(
        second["error"]["reason"]
            .as_str()
            .unwrap()
            .contains("session"),
        "{second:#}"
    );
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Task 12.4: with no configured cap the caller's `max_spend_usd` is the cap.
#[test]
fn uncapped_the_callers_max_spend_is_the_cap() {
    let dir = scratch("uncapped");
    let (mut client, seen) = limits_server(&dir, "");
    let low = spend_call(&mut client, Some(0.03));
    assert_refused(&mut client, &seen, &low, "error", "cap", 0);
    let enough = spend_call(&mut client, Some(0.05));
    assert_eq!(enough["status"], "completed", "{enough:#}");
    assert_eq!(enough["spend"]["usd"], 0.04);
    assert_eq!(seen.lock().unwrap().len(), 1);
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Reads messages until a run's first step line: the journal is watched only once the limits
/// check has passed, so its worst case is reserved by then ("run started" comes before it).
fn wait_reserved(client: &mut Client) {
    loop {
        let message = client.next_message().unwrap();
        if message["method"] == "notifications/progress"
            && message["params"]["message"]
                .as_str()
                .is_some_and(|m| m.starts_with("step "))
        {
            return;
        }
    }
}

/// Reads messages until every id in `ids` has its response; returns them by id.
fn responses(
    client: &mut Client,
    ids: &[u64],
) -> std::collections::BTreeMap<u64, serde_json::Value> {
    let mut got = std::collections::BTreeMap::new();
    while got.len() < ids.len() {
        let message = client.next_message().unwrap();
        if let Some(id) = message.get("id").and_then(|v| v.as_u64())
            && ids.contains(&id)
        {
            got.insert(id, message);
        }
    }
    got
}

/// Task 9.2: the limits check and the reservation are one step. Under `--concurrency 2` with a
/// budget for one call, two calls at once give one run and one refusal (`budget`, task 12.4)
/// before any filler call; while a run is under way its worst case is held, and its end settles
/// it into the spend.
#[test]
fn concurrent_calls_reserve_the_budget_atomically() {
    let dir = scratch("reserve");
    let bin = env!("CARGO_BIN_EXE_rascal");
    let config = dir.join("reserve.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[limits]\nsession_budget_usd = 0.05\n[fillers.slow]\ncommand = [{bin:?}, \"fake-filler\", \"slow\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\ntimeout_s = 30\nrevision = \"slow\"\nspend_usd_per_call = 0.04\n"
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
            "--concurrency",
            "2",
        ])
        .env_remove("RASCAL_LOG");
    let mut client = Client::start(&mut command).unwrap();
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let call = serde_json::json!({ "name": "seam", "arguments": { "image": { "path": pano.to_str().unwrap() }, "filler": "slow" } });
    let a = client.begin("tools/call", call.clone()).unwrap();
    let b = client.begin("tools/call", call.clone()).unwrap();
    let got = responses(&mut client, &[a, b]);
    let statuses: Vec<(String, f64)> = got
        .values()
        .map(|m| {
            let s = &m["result"]["structuredContent"];
            (
                s["status"].as_str().unwrap().to_string(),
                s["spend"]["usd"].as_f64().unwrap(),
            )
        })
        .collect();
    assert!(
        statuses.contains(&("completed".to_string(), 0.04)),
        "{statuses:?} {got:#?}"
    );
    assert!(
        statuses.contains(&("error".to_string(), 0.0)),
        "{statuses:?}"
    );
    let refused = got
        .values()
        .map(|m| &m["result"]["structuredContent"])
        .find(|s| s["status"] == "error")
        .unwrap();
    assert_eq!(refused["error"]["code"], "budget");
    let estimate = |client: &mut Client| {
        client
            .call(
                "estimate",
                serde_json::json!({ "action": "seam", "arguments": call["arguments"] }),
            )
            .unwrap()
            .structured
            .unwrap()
    };
    let after = estimate(&mut client);
    assert_eq!(after["allowed"], false, "{after:#}");
    let ledger = session(&mut client);
    assert_eq!(ledger["spent_usd"], 0.04, "{ledger:#}");
    assert_eq!(ledger["reserved_usd"], 0.0);
    assert_eq!(ledger["remaining_usd"], 0.01);
    client.close().unwrap();

    // A run under way holds its worst case: with a budget for two, a second call is allowed
    // and a third, while both are held, is not.
    std::fs::write(
        &config,
        std::fs::read_to_string(&config)
            .unwrap()
            .replace("session_budget_usd = 0.05", "session_budget_usd = 0.10"),
    )
    .unwrap();
    let mut command = Command::new(bin);
    command
        .args([
            "serve",
            "--workspace",
            dir.join("ws2").to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
            "--runs",
            dir.join("runs2").to_str().unwrap(),
            "--concurrency",
            "2",
        ])
        .env_remove("RASCAL_LOG");
    let mut client = Client::start(&mut command).unwrap();
    let mut with_token = call.clone();
    with_token["_meta"] = serde_json::json!({ "progressToken": "r-1" });
    let a = client.begin("tools/call", with_token).unwrap();
    wait_reserved(&mut client);
    let held = estimate(&mut client);
    assert_eq!(held["allowed"], true, "{held:#}");
    assert_eq!(session(&mut client)["reserved_usd"], 0.04);
    let got = responses(&mut client, &[a]);
    assert_eq!(got[&a]["result"]["structuredContent"]["spend"]["usd"], 0.04);
    let settled = session(&mut client);
    assert_eq!(settled["reserved_usd"], 0.0, "{settled:#}");
    assert_eq!(settled["spent_usd"], 0.04);
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Tasks 9.3 and 9.4: with the one call permit held by a slow run, `estimate`, `describe` and
/// the runs list answer before the run ends, and the run is listed `running` from the moment
/// its first progress names it.
#[test]
fn reads_answer_while_a_run_holds_the_permit_and_the_run_is_listed() {
    let dir = scratch("reads");
    let mut client = start(&dir);
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let run = client
        .begin(
            "tools/call",
            serde_json::json!({
                "name": "seam",
                "arguments": { "image": { "path": pano.to_str().unwrap() }, "filler": "slow" },
                "_meta": { "progressToken": "reads" }
            }),
        )
        .unwrap();
    let first = loop {
        let message = client.next_message().unwrap();
        if message["method"] == "notifications/progress" {
            break message;
        }
    };
    let run_id = first["params"]["_meta"]["table-manners/run"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    // Listed, running, from its first notification.
    let runs = client
        .request(
            "resources/read",
            serde_json::json!({ "uri": "rascal://runs" }),
        )
        .unwrap();
    let list: serde_json::Value =
        serde_json::from_str(runs["contents"][0]["text"].as_str().unwrap()).unwrap();
    let listed = list["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == run_id.as_str())
        .cloned();
    assert_eq!(
        listed.as_ref().map(|r| r["status"].clone()),
        Some(serde_json::json!("running")),
        "{list:#}"
    );
    assert_eq!(listed.unwrap()["action"], "seam");
    // The reads answer before the run does.
    let estimate = client
        .begin(
            "tools/call",
            serde_json::json!({ "name": "estimate", "arguments": { "action": "seam",
                "arguments": { "image": { "path": pano.to_str().unwrap() }, "filler": "slow" } } }),
        )
        .unwrap();
    let describe = client
        .begin(
            "tools/call",
            serde_json::json!({ "name": "describe", "arguments": {} }),
        )
        .unwrap();
    let mut order = Vec::new();
    while order.len() < 3 {
        let message = client.next_message().unwrap();
        if let Some(id) = message.get("id").and_then(|v| v.as_u64())
            && [run, estimate, describe].contains(&id)
        {
            order.push(id);
        }
    }
    assert_eq!(order.last(), Some(&run), "the reads waited for the run");
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Task 11.2 (table-manners 4.6): `rascal://session` is one atomic observation of the server's
/// ledger under a random UUID per process. Uncapped, the budget and what remains are both null;
/// it answers while a run holds the permit and shows the run's reservation; a run carries its
/// server's session id; an unresolved charge is counted in `unresolved_usd` and in `spent_usd`.
#[test]
fn the_session_is_one_observation_of_the_servers_ledger() {
    let dir = scratch("session");
    let mut client = start(&dir);
    let resources = client
        .request("resources/list", serde_json::json!({}))
        .unwrap();
    assert!(
        resources["resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["uri"] == "rascal://session"),
        "{resources:#}"
    );
    let described = client
        .call("describe", serde_json::json!({}))
        .unwrap()
        .structured
        .unwrap();
    assert!(
        described["resources"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("rascal://session"))
    );
    let first = session(&mut client);
    let id = first["id"].as_str().unwrap().to_string();
    assert_eq!(first["budget_usd"], serde_json::Value::Null);
    assert_eq!(first["remaining_usd"], serde_json::Value::Null);
    assert_eq!(first["spent_usd"], 0.0);

    // While a run holds the permit, the session answers and shows its reservation.
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let run = client
        .begin(
            "tools/call",
            serde_json::json!({
                "name": "seam",
                "arguments": { "image": { "path": pano.to_str().unwrap() }, "filler": "slow" },
                "_meta": { "progressToken": "session" }
            }),
        )
        .unwrap();
    wait_reserved(&mut client);
    let held = session(&mut client);
    assert_eq!(held["reserved_usd"], 0.04, "{held:#}");
    assert_eq!(held["id"], id.as_str());
    let got = responses(&mut client, &[run]);
    let result = &got[&run]["result"]["structuredContent"];
    assert_eq!(result["run"]["session_id"], id.as_str(), "{result:#}");
    let settled = session(&mut client);
    assert_eq!(settled["spent_usd"], 0.04, "{settled:#}");
    assert_eq!(settled["reserved_usd"], 0.0);
    assert_eq!(settled["unresolved_usd"], 0.0);

    // A filler that fails after the request was sent leaves its charge unresolved.
    let failed = client
        .call(
            "seam",
            serde_json::json!({ "image": { "path": pano.to_str().unwrap() }, "filler": "failing" }),
        )
        .unwrap()
        .structured
        .unwrap();
    assert_eq!(failed["spend"]["unresolved_usd"], 0.04, "{failed:#}");
    let after = session(&mut client);
    assert_eq!(after["unresolved_usd"], 0.04, "{after:#}");
    assert_eq!(after["spent_usd"], 0.08);
    client.close().unwrap();

    // Another server is another session.
    let mut other = start(&dir);
    assert_ne!(session(&mut other)["id"], id.as_str());
    other.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `seam` call through the slow filler with a progress token and, when given, a correlation id.
fn seam_call(token: &str, client_run_id: Option<&str>) -> serde_json::Value {
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let mut meta = serde_json::json!({ "progressToken": token });
    if let Some(id) = client_run_id {
        meta["table-manners/client_run_id"] = serde_json::json!(id);
    }
    serde_json::json!({
        "name": "seam",
        "arguments": { "image": { "path": pano.to_str().unwrap() }, "filler": "slow" },
        "_meta": meta
    })
}

/// The runs list, valid against the contract.
fn runs(client: &mut Client) -> serde_json::Value {
    let text = read_resource(client, "rascal://runs")["text"]
        .as_str()
        .unwrap()
        .to_string();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    valid("urn:table-manners:core:runs", &value);
    value
}

/// A JSON-RPC error that refuses a reused correlation id: `-32602` with valid refusal data.
fn refusal(message: &serde_json::Value) -> serde_json::Value {
    assert_eq!(message["error"]["code"], -32602, "{message:#}");
    let data = message["error"]["data"].clone();
    valid("urn:table-manners:core:refusal", &data);
    assert_eq!(data["reason"], "client_run_id_reused");
    data
}

/// Task 11.3 (table-manners 4.4): a correlation id is claimed when the call is accepted. Two
/// calls with one id give one run and one refusal; the run is listed `running` with its ids from
/// allocation; a reuse after it ended names it; a malformed id is refused before any run; a call
/// without one has `client_run_id: null`.
#[test]
fn a_correlation_id_runs_at_most_once_and_a_reuse_names_the_earlier_run() {
    let dir = scratch("correlation");
    let mut client = start(&dir);
    let first = client
        .begin("tools/call", seam_call("one", Some("host-7f3a.attempt:1")))
        .unwrap();
    let named = loop {
        let message = client.next_message().unwrap();
        if message["method"] == "notifications/progress" {
            break message["params"]["_meta"]["table-manners/run"].clone();
        }
    };
    let second = client
        .begin("tools/call", seam_call("two", Some("host-7f3a.attempt:1")))
        .unwrap();
    // The duplicate is refused at once, while the first is still under way, with its run.
    let got = responses(&mut client, &[second]);
    assert_eq!(refusal(&got[&second])["run"], named);
    let listed = runs(&mut client);
    let running: Vec<&serde_json::Value> = listed["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["client_run_id"] == "host-7f3a.attempt:1")
        .collect();
    assert_eq!(running.len(), 1, "{listed:#}");
    assert_eq!(running[0]["status"], "running");
    let got = responses(&mut client, &[first]);
    let result = got[&first]["result"]["structuredContent"].clone();
    assert_eq!(result["status"], "completed", "{result:#}");
    assert_eq!(result["run"]["client_run_id"], "host-7f3a.attempt:1");
    let mut reference = result["run"].clone();
    reference.as_object_mut().unwrap().remove("dir");
    assert_eq!(named, reference);
    // A reuse after the run ended names it.
    let third = client
        .begin(
            "tools/call",
            seam_call("three", Some("host-7f3a.attempt:1")),
        )
        .unwrap();
    let got = responses(&mut client, &[third]);
    assert_eq!(refusal(&got[&third])["run"], reference);
    // A malformed id is refused before any run.
    let bad = client
        .begin("tools/call", seam_call("four", Some("not valid")))
        .unwrap();
    let got = responses(&mut client, &[bad]);
    assert_eq!(got[&bad]["error"]["code"], -32602, "{:#}", got[&bad]);
    // One run in all, and a call without an id has `client_run_id: null`.
    assert_eq!(runs(&mut client)["runs"].as_array().unwrap().len(), 1);
    let plain = client.begin("tools/call", seam_call("five", None)).unwrap();
    let got = responses(&mut client, &[plain]);
    let plain = &got[&plain]["result"]["structuredContent"];
    assert_eq!(
        plain["run"]["client_run_id"],
        serde_json::Value::Null,
        "{plain:#}"
    );
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Tasks 11.3 and 11.4 (table-manners 4.1, 4.2, 4.4): a call queued behind the permit sends no
/// notification naming a run; cancelled there, it leaves no run, and its correlation id stays
/// claimed, so a retry under the same id is refused with `run: null`.
#[test]
fn a_call_cancelled_while_queued_leaves_no_run_and_keeps_its_id_claimed() {
    let dir = scratch("queued");
    let mut client = start(&dir);
    let holder = client
        .begin("tools/call", seam_call("holder", None))
        .unwrap();
    loop {
        let message = client.next_message().unwrap();
        if message["method"] == "notifications/progress" {
            break;
        }
    }
    let queued = client
        .begin("tools/call", seam_call("queued", Some("retry-me")))
        .unwrap();
    client
        .notify(
            "notifications/cancelled",
            serde_json::json!({ "requestId": queued, "reason": "test" }),
        )
        .unwrap();
    // Everything up to the holder's answer: nothing for the queued call names a run.
    let mut seen = Vec::new();
    loop {
        let message = client.next_message().unwrap();
        if message.get("id").and_then(|v| v.as_u64()) == Some(holder) {
            break;
        }
        seen.push(message);
    }
    assert!(
        seen.iter()
            .filter(|m| m["params"]["progressToken"] == "queued")
            .all(|m| m["params"]["_meta"].get("table-manners/run").is_none()),
        "{seen:#?}"
    );
    assert!(
        !seen
            .iter()
            .any(|m| m.get("id").and_then(|v| v.as_u64()) == Some(queued)
                && m.get("result").is_some()),
        "the cancelled call got a result"
    );
    let listed = runs(&mut client);
    assert_eq!(listed["runs"].as_array().unwrap().len(), 1, "{listed:#}");
    assert!(
        listed["runs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["client_run_id"].is_null())
    );
    let retry = client
        .begin("tools/call", seam_call("retry", Some("retry-me")))
        .unwrap();
    let got = responses(&mut client, &[retry]);
    assert_eq!(refusal(&got[&retry])["run"], serde_json::Value::Null);
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A run directory as `serve` allocates one, with its association, for a listing test.
fn allocated(runs: &Path, ms: u128) -> String {
    let id = format!("run-{ms}-4242");
    std::fs::create_dir_all(runs.join(&id)).unwrap();
    std::fs::create_dir_all(runs.join(format!("{id}.in"))).unwrap();
    std::fs::write(
        runs.join(format!("{id}.in/association.json")),
        r#"{"action":"seam","session_id":null,"client_run_id":null}"#,
    )
    .unwrap();
    id
}

/// One page of the runs list, valid against the contract.
fn page(client: &mut Client, uri: &str) -> serde_json::Value {
    let text = read_resource(client, uri)["text"]
        .as_str()
        .unwrap()
        .to_string();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    valid("urn:table-manners:core:runs", &value);
    value
}

/// Task 11.5 (table-manners 4.3): `rascal://runs{?cursor}` pages newest first, 50 a page, with
/// no gap or repeat and a null `next_cursor` on the last; a run started after page one was read
/// is on a fresh page one, not on a later page; a cursor that is not a run id is a resource error.
#[test]
fn the_runs_list_pages_by_cursor() {
    let dir = scratch("pages");
    let runs_root = dir.join("runs");
    let mut made: Vec<String> = (0..120)
        .map(|i| allocated(&runs_root, 1_793_000_000_000 + i))
        .collect();
    made.reverse();
    let mut client = start(&dir);
    let described = client
        .call("describe", serde_json::json!({}))
        .unwrap()
        .structured
        .unwrap();
    assert!(
        described["resources"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("rascal://runs{?cursor}"))
    );
    let first = page(&mut client, "rascal://runs");
    let mut seen: Vec<String> = Vec::new();
    let mut current = first.clone();
    let mut pages = 0;
    loop {
        pages += 1;
        let entries = current["runs"].as_array().unwrap();
        assert!(entries.len() <= 50);
        seen.extend(
            entries
                .iter()
                .map(|r| r["id"].as_str().unwrap().to_string()),
        );
        match current["next_cursor"].as_str() {
            Some(cursor) => {
                assert!(
                    cursor
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._~-".contains(&b))
                );
                current = page(&mut client, &format!("rascal://runs?cursor={cursor}"));
            }
            None => break,
        }
    }
    assert_eq!(pages, 3);
    assert_eq!(seen, made, "a gap, a repeat or the wrong order");

    // A run started after page one was read: not on page two, but on a fresh page one.
    let late = allocated(&runs_root, 1_793_000_000_500);
    let cursor = first["next_cursor"].as_str().unwrap().to_string();
    let second = page(&mut client, &format!("rascal://runs?cursor={cursor}"));
    assert!(
        !second["runs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == late.as_str())
    );
    let fresh = page(&mut client, "rascal://runs");
    assert_eq!(fresh["runs"][0]["id"], late.as_str());

    // A cursor that is not a run id is an error, not an empty page.
    let bad = client.request(
        "resources/read",
        serde_json::json!({ "uri": "rascal://runs?cursor=nonsense" }),
    );
    assert!(bad.is_err(), "{bad:?}");
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every tool's input schema compiles on its own as a JSON Schema (2020-12, MCP's default), so a
/// strict client can discover rascal: no reference that points outside the schema it sits in.
#[test]
fn every_tool_input_schema_stands_alone() {
    let dir = scratch("schemas");
    let mut client = start(&dir);
    // The check catches the failure it is for: a reference to a definition that is not there.
    let dangling = serde_json::json!({ "properties": { "kind": { "$ref": "#/$defs/Kind" } } });
    assert!(jsonschema::options().build(&dangling).is_err());
    let tools = client.list_tools().unwrap();
    assert!(tools.len() > 10, "{}", tools.len());
    for tool in &tools {
        if let Err(error) = jsonschema::options().build(&tool.input_schema) {
            panic!("{}: {error}\n{:#}", tool.name, tool.input_schema);
        }
    }
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Task 12.1 (table-manners v0.3, 3 and 5): `estimate` is a reserved tool, listed beside
/// `describe` but never an action; each action tool's input schema declares the same
/// `max_spend_usd` as its `params_schema` in the description.
#[test]
fn estimate_is_a_reserved_tool_and_each_cap_agrees_with_the_description() {
    let dir = scratch("reserved");
    let mut client = start(&dir);
    let tools = client.list_tools().unwrap();
    for name in ["estimate", "describe"] {
        assert!(tools.iter().any(|t| t.name == name), "{name}");
    }
    let described = client
        .call("describe", serde_json::json!({}))
        .unwrap()
        .structured
        .unwrap();
    valid("urn:table-manners:core:description", &described);
    let actions = described["actions"].as_array().unwrap();
    assert!(!actions.iter().any(|a| a["name"] == "estimate"));
    for action in actions {
        let name = action["name"].as_str().unwrap();
        let tool = tools.iter().find(|t| t.name == name).unwrap();
        let declared = &action["params_schema"]["properties"]["max_spend_usd"];
        assert_eq!(
            declared,
            &serde_json::json!({ "type": "number", "exclusiveMinimum": 0,
                                 "description": "Lower the per-call spend cap for this call (USD, above zero)." })
        );
        assert_eq!(
            &tool.input_schema["properties"]["max_spend_usd"], declared,
            "{name}"
        );
    }
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Task 12.1 (table-manners 4.5): a cap of zero is not a cap; `max_spend_usd: 0` is a bad
/// argument, refused in the run as `invalid`/`usage` with nothing spent.
#[test]
fn a_cap_of_zero_is_an_invalid_run_that_spends_nothing() {
    let dir = scratch("zero-cap");
    let mut client = start(&dir);
    let pano = repo_root().join("fixtures/images/step-pano.png");
    for asked in [serde_json::json!(0), serde_json::json!(-0.5)] {
        let out = client
            .call(
                "seam",
                serde_json::json!({ "image": { "path": pano.to_str().unwrap() }, "filler": "slow",
                                    "max_spend_usd": asked }),
            )
            .unwrap();
        assert!(out.is_error);
        let s = out.structured.unwrap();
        valid("urn:table-manners:core:result", &s);
        assert_eq!(s["status"], "invalid", "{s:#}");
        assert_eq!(s["error"]["code"], "usage");
        assert!(
            s["error"]["reason"]
                .as_str()
                .unwrap()
                .contains("max_spend_usd"),
            "{s:#}"
        );
        assert_eq!(s["spend"]["usd"], 0.0);
    }
    let ledger = session(&mut client);
    assert_eq!(ledger["spent_usd"], 0.0, "{ledger:#}");
    assert_eq!(ledger["reserved_usd"], 0.0);
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// `rascal serve` with `args` and `env` (with `HOME` and `XDG_DATA_HOME` in `dir`, so nothing
/// defaults into the real home).
fn start_with(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Client {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command
        .arg("serve")
        .args(args)
        .env_remove("RASCAL_LOG")
        .env_remove("RASCAL_TOOLS_MODELS")
        .env("HOME", dir.join("home"))
        .env("XDG_DATA_HOME", dir.join("data"));
    for (name, value) in env {
        command.env(name, value);
    }
    Client::start(&mut command).unwrap()
}

/// `rascal describe --json` under the same environment as [`start_with`].
fn cli_describe(dir: &Path, config: Option<&Path>, env: &[(&str, &str)]) -> serde_json::Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command
        .args(["describe", "--json"])
        .env_remove("RASCAL_LOG")
        .env_remove("RASCAL_TOOLS_MODELS")
        .env("HOME", dir.join("home"))
        .env("XDG_DATA_HOME", dir.join("data"));
    if let Some(config) = config {
        command.args(["--config", config.to_str().unwrap()]);
    }
    for (name, value) in env {
        command.env(name, value);
    }
    let out = command.output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    serde_json::from_slice(&out.stdout).unwrap()
}

fn readiness_of<'a>(description: &'a serde_json::Value, action: &str) -> &'a serde_json::Value {
    &description["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == action)
        .unwrap()["readiness"]
}

/// A `not_ready` refusal: `-32602` with refusal data valid against the contract.
fn not_ready(failure: Failure) -> serde_json::Value {
    let Failure::Rpc { code, data, .. } = failure else {
        panic!("not a JSON-RPC error: {failure}");
    };
    assert_eq!(code, -32602);
    let data = data.unwrap();
    valid("urn:table-manners:core:refusal", &data);
    assert_eq!(data["reason"], "not_ready", "{data:#}");
    assert_eq!(data["run"], serde_json::Value::Null);
    data
}

fn run_dirs(runs: &Path) -> Vec<String> {
    std::fs::read_dir(runs)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| n.starts_with("run-"))
                .collect()
        })
        .unwrap_or_default()
}

/// Task 12.2 (table-manners 3.1, 5): `serve` starts with nothing but `--runs`, with a
/// configuration that does not load, and with a pack whose launcher is missing; its `describe`
/// agrees with the command line's; a mask-only call completes in every state.
#[test]
fn serve_starts_in_every_configuration_state() {
    let dir = scratch("states");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let bad = dir.join("bad.toml");
    std::fs::write(&bad, "config_version = 1\n[fillers\n").unwrap();
    let no_launcher = dir.join("no-launcher.toml");
    std::fs::write(
        &no_launcher,
        format!(
            "config_version = 1\n[packs.segmenter]\ncommand = [{:?}, \"serve\"]\n",
            dir.join("pack/.venv/bin/rascal-tools")
                .display()
                .to_string()
        ),
    )
    .unwrap();
    let runs = dir.join("runs");
    for (config, remove_prompt) in [
        (None, "missing"),
        (Some(&bad), "invalid"),
        (Some(&no_launcher), "missing"),
    ] {
        let mut args = vec!["--runs", runs.to_str().unwrap()];
        if let Some(config) = config {
            args.extend(["--config", config.to_str().unwrap()]);
        }
        let mut client = start_with(&dir, &args, &[]);
        let described = client
            .call("describe", serde_json::json!({}))
            .unwrap()
            .structured
            .unwrap();
        valid("urn:table-manners:core:description", &described);
        assert_eq!(
            described,
            cli_describe(&dir, config.map(PathBuf::as_path), &[]),
            "serve and the command line describe alike"
        );
        let readiness = readiness_of(&described, "remove_prompt");
        assert_eq!(readiness["state"], "needs_setup");
        let first = &readiness["requirements"][0];
        if config == Some(&no_launcher) {
            assert_eq!(first["state"], "present", "{readiness:#}");
            assert_eq!(readiness["requirements"][1]["name"], "packs.segmenter");
            assert_eq!(readiness["requirements"][1]["state"], remove_prompt);
        } else {
            assert_eq!(first["name"], "rascal.toml");
            assert_eq!(first["state"], remove_prompt, "{readiness:#}");
        }
        let out = client
            .call(
                "seam",
                serde_json::json!({ "image": { "path": pano.to_str().unwrap() } }),
            )
            .unwrap();
        let s = out.structured.unwrap();
        assert_eq!(s["status"], "completed", "{s:#}");
        client.close().unwrap();
    }
    // Without --workspace the op tools' workspace is beside the default runs root.
    assert!(dir.join("data/rascal/workspace").is_dir());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Task 12.2 (table-manners 3.1, 4.4): a call that needs what is missing is refused `not_ready`
/// before any run, naming exactly that: no run directory, no runs entry, no request to the
/// filler, and its correlation id free for the next request. No credential value appears in
/// the description or the refusal.
#[test]
fn a_call_that_needs_what_is_missing_is_refused_not_ready() {
    let dir = scratch("not-ready");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let runs_dir = dir.join("runs");

    // No configuration: a filler call needs it.
    let mut client = start_with(&dir, &["--runs", runs_dir.to_str().unwrap()], &[]);
    let call = |client: &mut Client, args: serde_json::Value, id: &str| {
        client.request(
            "tools/call",
            serde_json::json!({ "name": "seam", "arguments": args,
                                "_meta": { "table-manners/client_run_id": id } }),
        )
    };
    let refused = not_ready(
        call(
            &mut client,
            serde_json::json!({ "image": { "path": pano.to_str().unwrap() }, "filler": "fake" }),
            "retry-1",
        )
        .unwrap_err(),
    );
    assert_eq!(
        refused["requirements"],
        serde_json::json!([{ "kind": "config", "name": "rascal.toml", "state": "missing",
                             "setup": "Start rascal with `--config <path>` naming the filler." }])
    );
    assert!(run_dirs(&runs_dir).is_empty(), "no run directory");
    assert!(runs(&mut client)["runs"].as_array().unwrap().is_empty());
    // The id was released: the corrected call runs under it.
    let done = call(
        &mut client,
        serde_json::json!({ "image": { "path": pano.to_str().unwrap() } }),
        "retry-1",
    )
    .unwrap();
    assert_eq!(done["structuredContent"]["status"], "completed");
    assert_eq!(done["structuredContent"]["run"]["client_run_id"], "retry-1");
    client.close().unwrap();

    // A filler whose key is unset: refused naming the credential, and nothing is sent.
    let (url, seen) =
        rascal_testkit::fake_filler::spawn(rascal_testkit::fake_filler::Behaviour::Exact, None);
    let config = dir.join("rascal.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[fillers.remote]\nurl = \"{url}\"\nprofile = \"grey\"\nkey_env = \"SERVE_UNSET_KEY\"\nspend_usd_per_call = 0.04\n[fillers.hosted]\nurl = \"{url}\"\nprofile = \"grey\"\nkey_env = \"SERVE_SET_KEY\"\n"
        ),
    )
    .unwrap();
    let sentinel = "zq-serve-sentinel-zq";
    let runs_dir = dir.join("runs-2");
    let mut client = start_with(
        &dir,
        &[
            "--runs",
            runs_dir.to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
        ],
        &[("SERVE_SET_KEY", sentinel)],
    );
    let described = client
        .call("describe", serde_json::json!({}))
        .unwrap()
        .structured
        .unwrap();
    let refused = not_ready(
        client
            .call(
                "seam",
                serde_json::json!({ "image": { "path": pano.to_str().unwrap() }, "filler": "remote" }),
            )
            .unwrap_err(),
    );
    assert_eq!(
        refused["requirements"],
        serde_json::json!([{ "kind": "credential", "name": "SERVE_UNSET_KEY", "state": "missing",
                             "setup": "Set `SERVE_UNSET_KEY` in the environment rascal is started with." }])
    );
    assert!(seen.lock().unwrap().is_empty(), "no request to the filler");
    assert!(run_dirs(&runs_dir).is_empty());
    assert!(!described.to_string().contains(sentinel));
    assert!(!refused.to_string().contains(sentinel));
    let ledger = session(&mut client);
    assert_eq!(ledger["spent_usd"], 0.0);
    assert_eq!(ledger["reserved_usd"], 0.0);
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The state of the credential `var` in `seam`'s readiness, from the server's `describe`.
fn credential_state(client: &mut Client, var: &str) -> String {
    let described = client
        .call("describe", serde_json::json!({}))
        .unwrap()
        .structured
        .unwrap();
    valid("urn:table-manners:core:description", &described);
    readiness_of(&described, "seam")["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "credential" && r["name"] == var)
        .unwrap()["state"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Task 12.3 (table-manners 2.2, 3.1): the session keeps what providers said about each
/// credential. A fresh session shows `present`; a 2xx makes the key `valid`; a 401 ends the run
/// `error`/`credential`, never retryable, spending nothing, and makes it `invalid`; the next
/// call with it is refused `not_ready` naming it, with no run and no request; a new session
/// shows `present` again.
#[test]
fn the_session_marks_credentials_by_what_providers_answered() {
    let dir = scratch("answers");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let (url, seen) = rascal_testkit::fake_filler::spawn(
        rascal_testkit::fake_filler::Behaviour::Exact,
        Some("the-right-key".to_string()),
    );
    let config = dir.join("rascal.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[fillers.good]\nurl = \"{url}\"\nprofile = \"grey\"\nkey_env = \"SERVE_GOOD_KEY\"\nspend_usd_per_call = 0.04\n[fillers.bad]\nurl = \"{url}\"\nprofile = \"grey\"\nkey_env = \"SERVE_BAD_KEY\"\nspend_usd_per_call = 0.04\n"
        ),
    )
    .unwrap();
    let runs_dir = dir.join("runs");
    let args = [
        "--runs",
        runs_dir.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
    ];
    let env = [
        ("SERVE_GOOD_KEY", "the-right-key"),
        ("SERVE_BAD_KEY", "zq-wrong-key-zq"),
    ];
    let mut client = start_with(&dir, &args, &env);
    assert_eq!(credential_state(&mut client, "SERVE_GOOD_KEY"), "present");
    assert_eq!(credential_state(&mut client, "SERVE_BAD_KEY"), "present");
    let seam = |filler: &str| serde_json::json!({ "image": { "path": pano.to_str().unwrap() }, "filler": filler });

    let out = client.call("seam", seam("good")).unwrap();
    assert_eq!(out.structured.unwrap()["status"], "completed");
    assert_eq!(credential_state(&mut client, "SERVE_GOOD_KEY"), "valid");
    assert_eq!(credential_state(&mut client, "SERVE_BAD_KEY"), "present");

    let out = client.call("seam", seam("bad")).unwrap();
    assert!(out.is_error);
    let r = out.structured.unwrap();
    valid("urn:table-manners:core:result", &r);
    assert_eq!(r["status"], "error", "{r:#}");
    assert_eq!(r["error"]["code"], "credential");
    assert_eq!(r["error"]["retryable"], false);
    assert_eq!(r["spend"]["usd"], 0.0);
    assert_eq!(r["spend"]["basis"], "measured", "{r:#}");
    assert!(
        r["images"]["out"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["role"] == "mask"),
        "{r:#}"
    );
    assert!(!r.to_string().contains("zq-wrong-key-zq"));
    assert_eq!(credential_state(&mut client, "SERVE_BAD_KEY"), "invalid");
    assert_eq!(credential_state(&mut client, "SERVE_GOOD_KEY"), "valid");

    // The next call with the rejected key is not sent.
    let requests = seen.lock().unwrap().len();
    let runs_before = run_dirs(&runs_dir).len();
    let refused = not_ready(client.call("seam", seam("bad")).unwrap_err());
    assert_eq!(
        refused["requirements"],
        serde_json::json!([{ "kind": "credential", "name": "SERVE_BAD_KEY", "state": "invalid",
                             "setup": "A provider rejected `SERVE_BAD_KEY` in this session. Set a working key in the environment and restart rascal." }])
    );
    assert_eq!(seen.lock().unwrap().len(), requests, "no request");
    assert_eq!(run_dirs(&runs_dir).len(), runs_before, "no run");
    let ledger = session(&mut client);
    assert_eq!(ledger["spent_usd"], 0.04, "{ledger:#}");
    assert_eq!(ledger["reserved_usd"], 0.0);
    client.close().unwrap();

    // A new session has heard nothing yet.
    let mut client = start_with(&dir, &args, &env);
    assert_eq!(credential_state(&mut client, "SERVE_GOOD_KEY"), "present");
    assert_eq!(credential_state(&mut client, "SERVE_BAD_KEY"), "present");
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
