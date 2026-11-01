//! Task 12.5 (table-manners 4.5): `estimate` is the reserved tool. Its input is `{action,
//! arguments}`, the arguments exactly the paid call's; it answers with `estimate.json`, allowed
//! or refused for `cap`, `budget` or `readiness`, bound to the call's arguments (images by their
//! bytes, delivery choices left out), the configuration, the session and the version. It is never
//! a run: it leaves the runs root, the workspace and the session's reservations as they were. A
//! wrong request is a JSON-RPC `-32602` (with `invalid_arguments` refusal data when the request
//! has the estimate's shape); `rascal estimate --json` answers with no session.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use rascal_core::contract::table_manners;
use rascal_testkit::fake_filler::{Behaviour, spawn};
use rascal_testkit::mcp_client::{Client, Failure};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pano() -> PathBuf {
    repo_root().join("fixtures/images/step-pano.png")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-estimate-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A configuration with `hosted`, a priced HTTP filler served by the fake (key in
/// `ESTIMATE_TEST_KEY`, which these tests set or leave unset), `free`, an unpriced command
/// filler, and `limits` under `[limits]`.
fn config(dir: &Path, url: &str, limits: &str) -> PathBuf {
    let path = dir.join("rascal.toml");
    let bin = env!("CARGO_BIN_EXE_rascal");
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[limits]\n{limits}\n[fillers.hosted]\nurl = {url:?}\nkey_env = \"ESTIMATE_TEST_KEY\"\nprofile = \"grey\"\nrevision = \"hosted-1\"\nspend_usd_per_call = 0.04\nfill_max_side = 512\n\n[fillers.free]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\nrevision = \"free\"\n"
        ),
    )
    .unwrap();
    path
}

/// `rascal serve` over `dir`, with `config` when given and the key set when `key` is.
fn serve(dir: &Path, config: Option<&Path>, key: bool) -> Client {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "serve",
            "--workspace",
            dir.join("ws").to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env_remove("RASCAL_LOG")
        .env_remove("ESTIMATE_TEST_KEY");
    if let Some(config) = config {
        command.args(["--config", config.to_str().unwrap()]);
    }
    if key {
        command.env("ESTIMATE_TEST_KEY", "sentinel-key-value");
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

/// The estimate of `action` with `arguments`, valid against the contract's schema.
fn estimate(client: &mut Client, action: &str, arguments: serde_json::Value) -> serde_json::Value {
    let out = client
        .call(
            "estimate",
            serde_json::json!({ "action": action, "arguments": arguments }),
        )
        .unwrap();
    assert!(!out.is_error);
    let e = out.structured.unwrap();
    valid("urn:table-manners:core:estimate", &e);
    e
}

/// The JSON-RPC error an estimate request gets.
fn refused(client: &mut Client, request: serde_json::Value) -> (i64, Option<serde_json::Value>) {
    match client.call("estimate", request) {
        Err(Failure::Rpc { code, data, .. }) => (code, data),
        other => panic!("expected a JSON-RPC error, got {other:?}"),
    }
}

/// `rascal://session`.
fn session(client: &mut Client) -> serde_json::Value {
    let result = client
        .request(
            "resources/read",
            serde_json::json!({ "uri": "rascal://session" }),
        )
        .unwrap();
    serde_json::from_str(result["contents"][0]["text"].as_str().unwrap()).unwrap()
}

/// `rascal estimate` with `request` on stdin.
fn cli(config: Option<&Path>, request: &serde_json::Value, json: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command.arg("estimate");
    if json {
        command.arg("--json");
    }
    if let Some(config) = config {
        command.args(["--config", config.to_str().unwrap()]);
    }
    command
        .arg("-")
        .env_remove("RASCAL_LOG")
        .env_remove("ESTIMATE_TEST_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(request.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// The command line's estimate for `request`, exit 0 and valid.
fn cli_estimate(config: Option<&Path>, request: &serde_json::Value) -> serde_json::Value {
    let out = cli(config, request, true);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let e: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    valid("urn:table-manners:core:estimate", &e);
    e
}

/// Every file under `dir`, with its size, sorted.
fn listing(dir: &Path) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push((path, entry.metadata().map(|m| m.len()).unwrap_or(0)));
            }
        }
    }
    out.sort();
    out
}

fn seam_with(filler: Option<&str>) -> serde_json::Value {
    let mut args = serde_json::json!({ "image": { "path": pano().to_str().unwrap() } });
    if let Some(filler) = filler {
        args["filler"] = serde_json::json!(filler);
    }
    args
}

#[test]
fn an_estimate_is_allowed_or_refused_for_cap_budget_or_readiness() {
    let dir = scratch("refusals");
    let (url, seen) = spawn(Behaviour::Exact, None);
    let config = config(&dir, &url, "call_cap_usd = 0.10\nsession_budget_usd = 0.05");
    let mut client = serve(&dir, Some(&config), true);
    let id = session(&mut client)["id"].as_str().unwrap().to_string();

    // Allowed: priced at the filler's price, under the cap, the session observed.
    let allowed = estimate(&mut client, "seam", seam_with(Some("hosted")));
    assert_eq!(allowed["action"], "seam");
    assert_eq!(allowed["allowed"], true, "{allowed:#}");
    assert_eq!(allowed["refused"], serde_json::Value::Null);
    assert_eq!(allowed["worst_case_usd"], 0.04);
    assert_eq!(allowed["cap_usd"], 0.10);
    assert_eq!(allowed["session_id"], id.as_str());
    assert_eq!(allowed["session"]["id"], id.as_str());
    assert_eq!(allowed["session"]["reserved_usd"], 0.0);
    assert_eq!(allowed["effective"]["model"], "fillers.hosted");
    assert_eq!(
        allowed["effective"]["x-rascal"],
        serde_json::json!({ "filler": { "name": "hosted", "kind": "http", "revision": "hosted-1",
                                        "paste": "feather", "fill_max_side": 512 } })
    );
    let readiness = &allowed["readiness"];
    assert_eq!(readiness["state"], "ready");
    assert!(
        readiness["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "credential" && r["name"] == "ESTIMATE_TEST_KEY"),
        "{readiness:#}"
    );
    let expires = allowed["expires"].as_str().unwrap();
    assert!(expires.ends_with('Z') && expires > allowed["session"]["started"].as_str().unwrap());
    assert!(!format!("{allowed}").contains("sentinel-key-value"));

    // A mask-only call: no model, nothing to spend, allowed.
    let free = estimate(&mut client, "seam", seam_with(None));
    assert_eq!(free["allowed"], true);
    assert_eq!(free["worst_case_usd"], 0.0);
    assert_eq!(free["effective"]["model"], serde_json::Value::Null);
    assert_eq!(
        free["effective"]["x-rascal"],
        serde_json::json!({ "filler": null })
    );
    assert_eq!(
        free["effective"]["config_identity"],
        allowed["effective"]["config_identity"]
    );

    // Lowered under the price: refused for the cap.
    let mut low = seam_with(Some("hosted"));
    low["max_spend_usd"] = serde_json::json!(0.03);
    let cap = estimate(&mut client, "seam", low);
    assert_eq!(cap["allowed"], false);
    assert_eq!(cap["refused"], "cap");
    assert_eq!(cap["cap_usd"], 0.03);

    // A filler that is not configured cannot be priced: refused for readiness, never zero.
    let unknown = estimate(&mut client, "seam", seam_with(Some("elsewhere")));
    assert_eq!(unknown["refused"], "readiness", "{unknown:#}");
    assert_eq!(unknown["worst_case_usd"], serde_json::Value::Null);
    assert_eq!(unknown["effective"]["model"], serde_json::Value::Null);
    assert!(unknown["effective"]["config_identity"].is_string());

    // One call spends 0.04 of the 0.05; the next would go over: refused for the budget.
    let ran = client.call("seam", seam_with(Some("hosted"))).unwrap();
    assert_eq!(ran.structured.unwrap()["status"], "completed");
    assert_eq!(seen.lock().unwrap().len(), 1);
    let budget = estimate(&mut client, "seam", seam_with(Some("hosted")));
    assert_eq!(budget["refused"], "budget", "{budget:#}");
    assert_eq!(budget["session"]["spent_usd"], 0.04);
    client.close().unwrap();

    // Without the key the call is not ready: refused for readiness, still priced.
    let mut client = serve(&dir, Some(&config), false);
    let unready = estimate(&mut client, "seam", seam_with(Some("hosted")));
    assert_eq!(unready["refused"], "readiness", "{unready:#}");
    assert_eq!(unready["readiness"]["state"], "needs_setup");
    assert_eq!(unready["worst_case_usd"], 0.04);
    client.close().unwrap();
    assert_eq!(
        seen.lock().unwrap().len(),
        1,
        "an estimate called the filler"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn with_no_file_a_mask_only_call_is_priced_and_remove_prompt_is_not() {
    let dir = scratch("unconfigured");
    let mut client = serve(&dir, None, false);
    let free = estimate(&mut client, "seam", seam_with(None));
    assert_eq!(free["allowed"], true, "{free:#}");
    assert_eq!(free["worst_case_usd"], 0.0);
    assert_eq!(free["cap_usd"], serde_json::Value::Null);
    assert_eq!(
        free["effective"]["config_identity"],
        rascal_core::config::Config::builtin().identity().as_str()
    );
    let prompt = estimate(
        &mut client,
        "remove_prompt",
        serde_json::json!({ "image": { "path": pano().to_str().unwrap() }, "prompt": "a lamp" }),
    );
    assert_eq!(prompt["allowed"], false);
    assert_eq!(prompt["refused"], "readiness");
    assert_eq!(prompt["worst_case_usd"], serde_json::Value::Null);
    assert_eq!(
        prompt["effective"]["config_identity"],
        serde_json::Value::Null
    );
    assert!(
        prompt["readiness"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "config" && r["state"] == "missing"),
        "{prompt:#}"
    );
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_binding_follows_the_call_the_configuration_and_the_session() {
    let dir = scratch("binding");
    let (url, _seen) = spawn(Behaviour::Exact, None);
    let config = config(&dir, &url, "");
    let bytes = std::fs::read(pano()).unwrap();
    let request = |arguments: serde_json::Value| serde_json::json!({ "action": "seam", "arguments": arguments });
    let binding = |config: Option<&Path>, arguments: serde_json::Value| {
        cli_estimate(config, &request(arguments))["binding"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let by_path = binding(Some(&config), seam_with(Some("free")));
    let inline = serde_json::json!({ "image": { "inline": { "media_type": "image/png",
        "data": rascal_core::panel::base64(&bytes) } }, "filler": "free" });
    assert_eq!(by_path, binding(Some(&config), inline), "path and inline");
    let mut delivered = seam_with(Some("free"));
    delivered["return_images"] = serde_json::json!("inline");
    delivered["image_blocks"] = serde_json::json!(true);
    assert_eq!(
        by_path,
        binding(Some(&config), delivered),
        "delivery choices"
    );
    let expect = serde_json::json!({ "file_sha256": rascal_core::contract::file_sha256(&bytes),
        "width": 800, "height": 400, "projection": "equirectangular", "topology": "wrap" });
    let mut expecting = seam_with(Some("free"));
    expecting["image"]["expect"] = expect;
    assert_eq!(
        by_path,
        binding(Some(&config), expecting),
        "a verified expect"
    );

    // A byte of the image changed.
    let mut changed = bytes.clone();
    let last = changed.len() - 1;
    changed[last] ^= 1;
    let other = dir.join("other.png");
    std::fs::write(&other, &changed).unwrap();
    let moved =
        serde_json::json!({ "image": { "path": other.to_str().unwrap() }, "filler": "free" });
    assert_ne!(by_path, binding(Some(&config), moved), "the image's bytes");
    // Another argument.
    let mut strict = seam_with(Some("free"));
    strict["strict"] = serde_json::json!(true);
    assert_ne!(by_path, binding(Some(&config), strict), "another argument");
    let mut lowered = seam_with(Some("free"));
    lowered["max_spend_usd"] = serde_json::json!(0.5);
    assert_ne!(by_path, binding(Some(&config), lowered), "max_spend_usd");
    // Another configuration.
    let other_config = dir.join("other.toml");
    std::fs::write(
        &other_config,
        std::fs::read_to_string(&config)
            .unwrap()
            .replace("revision = \"free\"", "revision = \"free-2\""),
    )
    .unwrap();
    assert_ne!(
        by_path,
        binding(Some(&other_config), seam_with(Some("free"))),
        "the configuration"
    );

    // A session: two servers over one configuration bind apart, and neither as the command line.
    let mut a = serve(&dir.join("a"), Some(&config), false);
    let mut b = serve(&dir.join("b"), Some(&config), false);
    let in_a = estimate(&mut a, "seam", seam_with(Some("free")));
    let in_b = estimate(&mut b, "seam", seam_with(Some("free")));
    assert_eq!(
        in_a["binding"],
        estimate(&mut a, "seam", seam_with(Some("free")))["binding"],
        "stable within a session"
    );
    assert_ne!(in_a["binding"], in_b["binding"], "the session");
    assert_ne!(in_a["binding"], by_path.as_str(), "the command line");
    a.close().unwrap();
    b.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_command_line_estimate_has_no_session() {
    let dir = scratch("cli");
    let (url, _seen) = spawn(Behaviour::Exact, None);
    let config = config(&dir, &url, "call_cap_usd = 0.10\nsession_budget_usd = 0.01");
    let e = cli_estimate(
        Some(&config),
        &serde_json::json!({ "action": "seam", "arguments": seam_with(Some("free")) }),
    );
    assert_eq!(e["session_id"], serde_json::Value::Null);
    assert_eq!(e["session"], serde_json::Value::Null);
    assert_eq!(e["allowed"], true);
    assert_eq!(e["worst_case_usd"], 0.0);
    // No ledger: the budget is never the refusal, though the cap and readiness still apply.
    let hosted = cli_estimate(
        Some(&config),
        &serde_json::json!({ "action": "seam", "arguments": seam_with(Some("hosted")) }),
    );
    assert_eq!(hosted["refused"], "readiness", "{hosted:#}");
    assert_eq!(hosted["worst_case_usd"], 0.04);
    let mut low = seam_with(Some("free"));
    low["max_spend_usd"] = serde_json::json!(0.2);
    let over = cli(
        Some(&config),
        &serde_json::json!({ "action": "seam", "arguments": low }),
        true,
    );
    assert_eq!(over.status.code(), Some(64));
    assert!(
        String::from_utf8_lossy(&over.stderr).contains("max_spend_usd"),
        "{}",
        String::from_utf8_lossy(&over.stderr)
    );
    let shapeless = cli(Some(&config), &serde_json::json!({ "arguments": {} }), true);
    assert_eq!(shapeless.status.code(), Some(64));
    // Without --json, one line for people.
    let text = cli(
        Some(&config),
        &serde_json::json!({ "action": "seam", "arguments": seam_with(Some("free")) }),
        false,
    );
    assert_eq!(text.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&text.stdout);
    assert!(
        stdout.contains("seam") && stdout.contains("allowed"),
        "{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn estimate_is_a_tool_never_an_action_and_never_a_run() {
    let dir = scratch("never-a-run");
    let (url, seen) = spawn(Behaviour::Exact, None);
    let config = config(&dir, &url, "session_budget_usd = 1.0");
    let mut client = serve(&dir, Some(&config), true);
    let tools = client.list_tools().unwrap();
    let tool = tools.iter().find(|t| t.name == "estimate").unwrap();
    assert_eq!(
        tool.input_schema["required"],
        serde_json::json!(["action", "arguments"])
    );
    assert_eq!(tool.input_schema["properties"]["action"]["type"], "string");
    assert_eq!(
        tool.input_schema["properties"]["arguments"]["type"],
        "object"
    );
    let described = client.call("describe", serde_json::json!({})).unwrap();
    assert!(
        !described.structured.unwrap()["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["name"] == "estimate")
    );
    let runs_before = listing(&dir.join("runs"));
    let ws_before = listing(&dir.join("ws"));
    let bytes = std::fs::read(pano()).unwrap();
    for arguments in [
        seam_with(Some("hosted")),
        serde_json::json!({ "image": { "inline": { "media_type": "image/png",
            "data": rascal_core::panel::base64(&bytes) } }, "filler": "hosted" }),
    ] {
        let e = estimate(&mut client, "seam", arguments);
        assert_eq!(e["allowed"], true, "{e:#}");
    }
    assert_eq!(listing(&dir.join("runs")), runs_before);
    assert_eq!(listing(&dir.join("ws")), ws_before);
    assert_eq!(session(&mut client)["reserved_usd"], 0.0);
    assert!(seen.lock().unwrap().is_empty());
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Asserts `data` is `invalid_arguments` refusal data with `code`, naming `what`.
fn invalid_arguments(data: Option<serde_json::Value>, code: &str, what: &str) {
    let data = data.unwrap();
    valid("urn:table-manners:core:refusal", &data);
    assert_eq!(data["reason"], "invalid_arguments", "{data:#}");
    assert_eq!(data["run"], serde_json::Value::Null);
    assert_eq!(data["error"]["code"], code, "{data:#}");
    assert_eq!(data["error"]["retryable"], false);
    assert!(
        data["error"]["reason"].as_str().unwrap().contains(what),
        "{what}: {data:#}"
    );
}

#[test]
fn a_wrong_request_is_invalid_params() {
    let dir = scratch("wrong");
    let (url, seen) = spawn(Behaviour::Exact, None);
    let config = config(&dir, &url, "call_cap_usd = 0.10");
    let mut client = serve(&dir, Some(&config), true);

    let (code, data) = refused(
        &mut client,
        serde_json::json!({ "action": "polish", "arguments": {} }),
    );
    assert_eq!(code, -32602);
    invalid_arguments(data, "usage", "polish");

    let mut sideways = seam_with(Some("hosted"));
    sideways["projection"] = serde_json::json!("sideways");
    let (code, data) = refused(
        &mut client,
        serde_json::json!({ "action": "seam", "arguments": sideways }),
    );
    assert_eq!(code, -32602);
    invalid_arguments(data, "usage", "projection");

    let mut stray = seam_with(Some("hosted"));
    stray["colour"] = serde_json::json!("red");
    let (_, data) = refused(
        &mut client,
        serde_json::json!({ "action": "seam", "arguments": stray }),
    );
    invalid_arguments(data, "usage", "colour");

    let (_, data) = refused(
        &mut client,
        serde_json::json!({ "action": "seam", "arguments": { "filler": "hosted" } }),
    );
    invalid_arguments(data, "usage", "image");

    let mut over = seam_with(Some("hosted"));
    over["max_spend_usd"] = serde_json::json!(0.2);
    let (_, data) = refused(
        &mut client,
        serde_json::json!({ "action": "seam", "arguments": over }),
    );
    invalid_arguments(data, "usage", "max_spend_usd");

    let mut expecting = seam_with(Some("hosted"));
    expecting["image"]["expect"] = serde_json::json!({ "file_sha256": "0".repeat(64),
        "width": 800, "height": 400, "projection": "equirectangular", "topology": "wrap" });
    let (_, data) = refused(
        &mut client,
        serde_json::json!({ "action": "seam", "arguments": expecting }),
    );
    invalid_arguments(data, "input", "image");

    // What the input schema rejects is plain invalid params, with no refusal data.
    for request in [
        serde_json::json!({ "arguments": {} }),
        serde_json::json!({ "action": "seam", "arguments": "all of them" }),
        serde_json::json!({ "action": 7, "arguments": {} }),
    ] {
        let (code, data) = refused(&mut client, request);
        assert_eq!(code, -32602);
        assert!(
            data.as_ref().is_none_or(|d| d.get("reason").is_none()),
            "{data:?}"
        );
    }
    assert!(seen.lock().unwrap().is_empty());
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
