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

/// Task 8.6: `[limits]` hold `serve` to a per-call cap a call may only lower and a session
/// budget; `estimate` says so before anything runs; a refused call is an `error` result with
/// code `budget`.
#[test]
fn the_cap_and_the_budget_refuse_a_call_that_would_spend_too_much() {
    let dir = scratch("budget");
    let bin = env!("CARGO_BIN_EXE_rascal");
    let config = dir.join("limits.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[limits]\ncall_cap_usd = 0.10\nsession_budget_usd = 0.05\n[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\nrevision = \"fake\"\nspend_usd_per_call = 0.04\n"
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
    let mut client = Client::start(&mut command).unwrap();
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let estimate = client
        .call(
            "estimate",
            serde_json::json!({ "action": "seam", "filler": "fake" }),
        )
        .unwrap()
        .structured
        .unwrap();
    assert_eq!(estimate["worst_case_usd"], 0.04);
    assert_eq!(estimate["allowed"], true);
    // Lowered under the price: refused before anything runs.
    let low = client
        .call("seam", serde_json::json!({ "image": { "path": pano.to_str().unwrap() }, "filler": "fake", "max_spend_usd": 0.01 }))
        .unwrap();
    assert!(low.is_error);
    let s = low.structured.unwrap();
    assert_eq!(s["status"], "error");
    assert_eq!(s["error"]["code"], "budget");
    // The first call fits the budget and spends 0.04; the second would go over 0.05.
    let first = client
        .call(
            "seam",
            serde_json::json!({ "image": { "path": pano.to_str().unwrap() }, "filler": "fake" }),
        )
        .unwrap();
    assert_eq!(first.structured.unwrap()["spend"]["usd"], 0.04);
    let after = client
        .call(
            "estimate",
            serde_json::json!({ "action": "seam", "filler": "fake" }),
        )
        .unwrap()
        .structured
        .unwrap();
    assert_eq!(after["allowed"], false);
    let second = client
        .call(
            "seam",
            serde_json::json!({ "image": { "path": pano.to_str().unwrap() }, "filler": "fake" }),
        )
        .unwrap();
    assert_eq!(second.structured.unwrap()["error"]["code"], "budget");
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
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
/// budget for one call, two calls at once give one run and one refusal before any filler call;
/// while a run is under way its worst case is held, and its end settles it into the spend.
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
                serde_json::json!({ "action": "seam", "filler": "slow" }),
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
    // The run is under way once its first progress names it.
    loop {
        let message = client.next_message().unwrap();
        if message["method"] == "notifications/progress" {
            break;
        }
    }
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
            serde_json::json!({ "name": "estimate", "arguments": { "action": "seam", "filler": "slow" } }),
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
    loop {
        let message = client.next_message().unwrap();
        if message["method"] == "notifications/progress" {
            break;
        }
    }
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
