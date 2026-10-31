//! Task 8.5 (ADR 0020): `rascal serve` under table-manners, driven by the independent client:
//! `describe`; a `seam` action healed through the fake filler with progress strictly increasing
//! and the first notification naming the run; the run as resources (the list, run.json, an
//! image); an image that does not match its `expect` is an `invalid` result; a relative path is
//! refused; a cancelled call gets no result and its run says `cancelled`.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

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
            "config_version = 1\n[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\ntimeout_s = 30\nrevision = \"fake\"\n\n[fillers.slow]\ncommand = [{bin:?}, \"fake-filler\", \"slow\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\ntimeout_s = 30\nrevision = \"slow\"\n"
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
    assert!(
        progress[0]["params"]["message"]
            .as_str()
            .unwrap()
            .contains(&format!("rascal://run/{run_id}/run.json")),
        "{:#}",
        progress[0]
    );
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
    let text = first["params"]["message"].as_str().unwrap().to_string();
    let run_id = text.split_whitespace().nth(1).unwrap().to_string();
    // Let the run reach the slow fill before cancelling.
    std::thread::sleep(std::time::Duration::from_millis(800));
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
