//! Task 14.11 (2/3), table-manners 5.3: `serve`'s `install` and `uninstall` controls over the
//! segmenter pack's installer, here a fake one (a shell script standing in for `rascal-tools get`
//! and `rascal-tools remove`, writing the installer's status file and an installation as the
//! real one does). Progress names the installable and never a run; the result is the state after
//! the call; an install outlives a client that goes away; an install already under way is attached
//! to; cancelling the request stops it; an unknown id is refused; uninstall removes, or stops an
//! install under way.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rascal_core::contract::table_manners;
use rascal_testkit::mcp_client::{Client, Failure};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-install-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A pack install whose launcher serves the fake pack and installs like the real installer
/// (`rascal_testkit::fake_install::write_installer`).
fn fake_pack(dir: &Path) -> (PathBuf, PathBuf) {
    let bin = env!("CARGO_BIN_EXE_rascal");
    let install = dir.join("pack");
    let fake =
        rascal_testkit::fake_install::write(&install, &[bin, "fake-pack", "exact"], true).unwrap();
    rascal_testkit::fake_install::write_installer(&fake, &[bin, "fake-pack", "exact"]).unwrap();
    let launcher = fake.launcher.clone();
    let config = dir.join("rascal.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[packs.segmenter]\ncommand = [{:?}, \"serve\"]\n",
            launcher.display().to_string()
        ),
    )
    .unwrap();
    (config, install.join("models"))
}

fn start(dir: &Path, config: &Path) -> Client {
    let mut command = rascal_testkit::mcp_client::serve_command(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "--config",
            config.to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env_remove("RASCAL_LOG")
        .env_remove("RASCAL_TOOLS_MODELS")
        .env("HOME", dir.join("home"))
        .env("XDG_DATA_HOME", dir.join("data"));
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

fn installable(client: &mut Client, id: &str) -> serde_json::Value {
    let described = client
        .call("describe", serde_json::json!({}))
        .unwrap()
        .structured
        .unwrap();
    described["installables"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == id)
        .unwrap()
        .clone()
}

fn status(models: &Path, id: &str) -> String {
    std::fs::read_to_string(models.join(".status").join(format!("{id}.json"))).unwrap_or_default()
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "timed out: {what}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn install_reports_progress_by_installable_and_returns_the_state_after() {
    let dir = scratch("progress");
    let (config, models) = fake_pack(&dir);
    let mut client = start(&dir, &config);
    let tools = client.list_tools().unwrap();
    assert!(tools.iter().any(|t| t.name == "install"));
    assert!(tools.iter().any(|t| t.name == "uninstall"));
    let id = client
        .begin(
            "tools/call",
            serde_json::json!({ "name": "install", "arguments": { "id": "sam3" },
                                "_meta": { "progressToken": "p1" } }),
        )
        .unwrap();
    let (result, notifications) = client.finish_collecting(id).unwrap();
    let structured = &result["structuredContent"];
    valid("urn:table-manners:core:install-result", structured);
    assert_eq!(structured["state"], "installed", "{structured:#}");
    assert_eq!(structured["installation"], "abc");
    assert_eq!(structured["error"], serde_json::Value::Null);
    let progress: Vec<&serde_json::Value> = notifications
        .iter()
        .filter(|n| n["method"] == "notifications/progress")
        .collect();
    assert!(!progress.is_empty(), "{notifications:#?}");
    for n in &progress {
        assert_eq!(
            n["params"]["_meta"]["table-manners/installable"], "sam3",
            "{n:#}"
        );
        assert!(
            n["params"]["_meta"].get("table-manners/run").is_none(),
            "{n:#}"
        );
    }
    let sam3 = installable(&mut client, "sam3");
    assert_eq!(sam3["state"], "installed");
    assert_eq!(sam3["last_outcome"]["state"], "succeeded");
    assert!(models.join(".got-sam3").exists());

    // Uninstall removes it.
    let out = client
        .call("uninstall", serde_json::json!({ "id": "sam3" }))
        .unwrap()
        .structured
        .unwrap();
    valid("urn:table-manners:core:install-result", &out);
    assert_eq!(out["state"], "not_installed", "{out:#}");
    assert_eq!(
        installable(&mut client, "sam3")["last_outcome"]["action"],
        "uninstall"
    );

    // An id the description does not list is refused.
    let Failure::Rpc { code, data, .. } = client
        .call("install", serde_json::json!({ "id": "giraffe" }))
        .unwrap_err()
    else {
        panic!("not a JSON-RPC error");
    };
    assert_eq!(code, -32602);
    let data = data.unwrap();
    valid("urn:table-manners:core:refusal", &data);
    assert_eq!(data["reason"], "unknown_installable");
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_install_outlives_its_client_and_a_later_one_attaches() {
    let dir = scratch("detach");
    let (config, models) = fake_pack(&dir);
    std::fs::create_dir_all(&models).unwrap();
    std::fs::write(models.join(".slow"), "").unwrap();
    let mut client = start(&dir, &config);
    client
        .begin(
            "tools/call",
            serde_json::json!({ "name": "install", "arguments": { "id": "sam3" } }),
        )
        .unwrap();
    wait_until("installing", || {
        status(&models, "sam3").contains("installing")
    });
    // The client goes away mid-install; the installer keeps going.
    drop(client);
    std::thread::sleep(Duration::from_millis(500));
    let mut client = start(&dir, &config);
    let sam3 = installable(&mut client, "sam3");
    assert_eq!(sam3["state"], "installing", "{sam3:#}");
    assert_eq!(sam3["progress"], 0.5);
    // A second install attaches (no second `get`), and ends when the first does.
    std::fs::remove_file(models.join(".got-sam3")).unwrap();
    let pending = client
        .begin(
            "tools/call",
            serde_json::json!({ "name": "install", "arguments": { "id": "sam3" } }),
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    // End the first install as a finished one would: publish, record success, and go.
    let pid = serde_json::from_str::<serde_json::Value>(&status(&models, "sam3")).unwrap()["pid"]
        .as_u64()
        .unwrap();
    std::fs::create_dir_all(models.join("sam3/abc")).unwrap();
    std::fs::write(models.join("sam3/abc/sam3.onnx"), "w").unwrap();
    std::fs::write(models.join("sam3/current"), "abc\n").unwrap();
    std::fs::write(
        models.join(".status/sam3.json"),
        r#"{"last_outcome":{"action":"install","state":"succeeded","error":null,"at":"2026-11-09T00:00:00Z"}}"#,
    )
    .unwrap();
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status();
    let result = client.finish(pending).unwrap();
    let structured = &result["structuredContent"];
    assert_eq!(structured["state"], "installed", "{structured:#}");
    assert!(
        !models.join(".got-sam3").exists(),
        "attached, not started again"
    );
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_the_request_or_uninstalling_stops_an_install() {
    let dir = scratch("cancel");
    let (config, models) = fake_pack(&dir);
    std::fs::create_dir_all(&models).unwrap();
    std::fs::write(models.join(".slow"), "").unwrap();
    let mut client = start(&dir, &config);
    let request = client
        .begin(
            "tools/call",
            serde_json::json!({ "name": "install", "arguments": { "id": "sam3" } }),
        )
        .unwrap();
    wait_until("installing", || {
        status(&models, "sam3").contains("installing")
    });
    client
        .notify(
            "notifications/cancelled",
            serde_json::json!({ "requestId": request, "reason": "test" }),
        )
        .unwrap();
    wait_until("the installer to record the cancel", || {
        status(&models, "sam3").contains("cancelled")
    });
    let sam3 = installable(&mut client, "sam3");
    assert_eq!(sam3["state"], "not_installed", "{sam3:#}");
    assert_eq!(sam3["last_outcome"]["state"], "cancelled");

    // An install under way, stopped by uninstall from another request.
    let _pending = client
        .begin(
            "tools/call",
            serde_json::json!({ "name": "install", "arguments": { "id": "sam3" } }),
        )
        .unwrap();
    wait_until("installing again", || {
        status(&models, "sam3").contains("installing")
    });
    let out = client
        .call("uninstall", serde_json::json!({ "id": "sam3" }))
        .unwrap()
        .structured
        .unwrap();
    assert_eq!(out["state"], "not_installed", "{out:#}");
    assert!(!models.join("sam3/current").exists());
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
