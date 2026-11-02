//! Task 10.7 (CONTRACT 4.7): `rascal://artifact/{identity}` reads any artifact the store holds
//! as `{contract, tool, artifact, preview, record}`, valid against v0.4, its preview drawn once
//! into rascal's cache and equal to the run's; `/preview` is that PNG. A host keeps references
//! across a restart of `serve` and a move of the workspace directory, and they read and bind
//! again. An identity the store does not hold is the `binding`/`unresolved` refusal with
//! `port: null`, and a read never waits for a run's permit.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use rascal_core::contract::table_manners;
use rascal_testkit::mcp_client::{Client, Failure};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rascal-artifact-resource-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `serve` on the store `ws`, its runs and its cache under `dir`.
fn serve(dir: &Path, ws: &Path) -> Command {
    let mut command = rascal_testkit::mcp_client::serve_command(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "--workspace",
            ws.to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .env_remove("RASCAL_LOG")
        .env_remove("RASCAL_TESTKIT_OPERATION_DELAY_MS");
    command
}

fn start(dir: &Path, ws: &Path) -> Client {
    Client::start(&mut serve(dir, ws)).unwrap()
}

fn fixture(name: &str) -> String {
    repo_root()
        .join("fixtures/images")
        .join(name)
        .to_str()
        .unwrap()
        .to_string()
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

fn completed(client: &mut Client, name: &str, args: serde_json::Value) -> serde_json::Value {
    let result = client.call(name, args).unwrap();
    let structured = result.structured.unwrap();
    assert_eq!(structured["status"], "completed", "{name}: {structured:#}");
    valid("urn:table-manners:core:result", &structured);
    structured
}

/// The entry an operation made on `port`.
fn out(result: &serde_json::Value, port: &str) -> serde_json::Value {
    result["artifacts"]["out"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["port"] == port)
        .unwrap_or_else(|| panic!("no out {port} in {result:#}"))
        .clone()
}

fn read(client: &mut Client, uri: &str) -> Result<serde_json::Value, Failure> {
    client
        .request("resources/read", serde_json::json!({ "uri": uri }))
        .map(|r| r["contents"][0].clone())
}

/// The artifact resource of `reference`, read through its own `resource` and validated.
fn resource(client: &mut Client, reference: &serde_json::Value) -> serde_json::Value {
    let uri = reference["resource"].as_str().unwrap();
    assert_eq!(
        uri,
        format!(
            "rascal://artifact/{}",
            reference["identity"].as_str().unwrap()
        )
    );
    let contents = read(client, uri).unwrap();
    assert_eq!(contents["mimeType"], "application/json");
    let body: serde_json::Value = serde_json::from_str(contents["text"].as_str().unwrap()).unwrap();
    valid("urn:table-manners:operations:artifact", &body);
    assert_eq!(&body["artifact"], reference);
    assert_eq!(body["tool"]["name"], "rascal");
    body
}

/// The `binding` refusal a read gets: its data, validated against `refusal.json`.
fn refused(result: Result<serde_json::Value, Failure>) -> serde_json::Value {
    match result {
        Err(Failure::Rpc { code, data, .. }) => {
            assert_eq!(code, -32602);
            let data = data.unwrap();
            valid("urn:table-manners:core:refusal", &data);
            assert_eq!(data["reason"], "binding", "{data:#}");
            assert_eq!(data["run"], serde_json::Value::Null);
            data
        }
        other => panic!("not refused: {other:?}"),
    }
}

/// Line → Rasterize on the floor fixture: the location's and the Mask's entries.
fn line_then_rasterize(client: &mut Client) -> (serde_json::Value, serde_json::Value) {
    let line = completed(
        client,
        "line",
        serde_json::json!({ "image": { "path": fixture("floor.png") }, "column": 0.5, "width_px": 24 }),
    );
    let location = out(&line, "location");
    let image = line["artifacts"]["in"][0]["artifact"].clone();
    let rasterized = completed(
        client,
        "rasterize",
        serde_json::json!({ "location": location["artifact"], "image": image }),
    );
    (location, out(&rasterized, "mask"))
}

fn path_of(image_out: &serde_json::Value) -> PathBuf {
    PathBuf::from(image_out["path"].as_str().unwrap())
}

#[test]
fn a_mask_and_a_location_read_with_their_previews_and_a_plan_without() {
    let dir = scratch("read");
    let ws = dir.join("ws");
    let mut client = start(&dir, &ws);
    // Both templates are advertised and in the description's resources.
    let templates = client
        .request("resources/templates/list", serde_json::json!({}))
        .unwrap();
    let uris: Vec<&str> = templates["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["uriTemplate"].as_str().unwrap())
        .collect();
    let described = client
        .call("describe", serde_json::json!({}))
        .unwrap()
        .structured
        .unwrap();
    for template in [
        "rascal://artifact/{identity}",
        "rascal://artifact/{identity}/preview",
    ] {
        assert!(uris.contains(&template), "{uris:?}");
        assert!(
            described["resources"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r == template),
            "{template}"
        );
    }

    let (location, mask) = line_then_rasterize(&mut client);
    for entry in [&mask, &location] {
        let body = resource(&mut client, &entry["artifact"]);
        let preview = &body["preview"];
        assert_eq!(preview["role"], "preview");
        // Drawn once into the cache, outside the store and every run directory, and equal to
        // the run's own preview.
        let cached = path_of(preview);
        assert!(
            cached.starts_with(dir.join("cache/rascal/previews")),
            "{cached:?}"
        );
        let bytes = std::fs::read(&cached).unwrap();
        let run_preview = std::fs::read(path_of(&entry["preview"])).unwrap();
        assert_eq!(bytes, run_preview);
        assert_eq!(preview["image"], entry["preview"]["image"]);
        // `/preview` is the PNG itself.
        let uri = format!(
            "{}/preview",
            entry["artifact"]["resource"].as_str().unwrap()
        );
        let blob = read(&mut client, &uri).unwrap();
        assert_eq!(blob["mimeType"], "image/png");
        let served =
            rascal_testkit::mcp_client::base64_decode(blob["blob"].as_str().unwrap()).unwrap();
        assert_eq!(served, run_preview);
        assert!(body["record"].is_object(), "{body:#}");
    }

    // A Plan (from `cut`) has no preview, and `/preview` of it is the refusal.
    let cut = completed(
        &mut client,
        "cut",
        serde_json::json!({ "image": { "path": fixture("step-pano.png") }, "strip": 0.1 }),
    );
    let plan = out(&cut, "plan");
    assert_eq!(plan["artifact"]["type"], "rascal:Plan");
    let body = resource(&mut client, &plan["artifact"]);
    assert_eq!(body["preview"], serde_json::Value::Null);
    let uri = format!("{}/preview", plan["artifact"]["resource"].as_str().unwrap());
    let data = refused(read(&mut client, &uri));
    assert_eq!(data["bindings"][0]["port"], serde_json::Value::Null);
    assert_eq!(data["bindings"][0]["problem"], "unresolved");
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn after_a_restart_and_a_move_the_references_read_and_bind() {
    let dir = scratch("reopen");
    let ws = dir.join("ws");
    let mut client = start(&dir, &ws);
    let (location, mask) = line_then_rasterize(&mut client);
    client.close().unwrap();
    // The host kept the references; nothing else crosses.
    let location = location["artifact"].clone();
    let mask = mask["artifact"].clone();

    let reopen = |client: &mut Client| {
        let held = resource(client, &mask);
        assert_eq!(held["artifact"], mask);
        resource(client, &location);
        let feathered = completed(
            client,
            "feather",
            serde_json::json!({ "mask": mask, "sigma_px": 6 }),
        );
        assert_eq!(out(&feathered, "mask")["artifact"]["frame"], mask["frame"]);
    };
    // A new `serve` on the same workspace.
    let mut client = start(&dir, &ws);
    reopen(&mut client);
    client.close().unwrap();
    // The workspace directory moved; `serve` started on the new path.
    let moved = dir.join("moved/ws");
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::rename(&ws, &moved).unwrap();
    let mut client = start(&dir, &moved);
    reopen(&mut client);
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_identity_the_store_lacks_is_refused_and_a_read_never_waits_for_a_run() {
    let dir = scratch("refused");
    let ws = dir.join("ws");
    let mut client = start(&dir, &ws);
    let (location, mask) = line_then_rasterize(&mut client);
    let missing = format!("sha256:{}", "0".repeat(64));
    for uri in [
        format!("rascal://artifact/{missing}"),
        format!("rascal://artifact/{missing}/preview"),
    ] {
        let data = refused(read(&mut client, &uri));
        let binding = &data["bindings"][0];
        assert_eq!(binding["port"], serde_json::Value::Null);
        assert_eq!(binding["problem"], "unresolved");
        assert_eq!(binding["got"], missing.as_str());
        assert!(!binding["remedy"].as_str().unwrap().is_empty());
        assert_eq!(data["bindings"].as_array().unwrap().len(), 1);
    }
    client.close().unwrap();

    // A slow server on the same store: a run holds the permit while the read answers.
    let mut slow = serve(&dir, &ws);
    slow.env("RASCAL_TESTKIT_OPERATION_DELAY_MS", "20000");
    let mut client = Client::start(&mut slow).unwrap();
    let running = client
        .begin(
            "tools/call",
            serde_json::json!({ "name": "feather",
                                "arguments": { "mask": mask["artifact"], "sigma_px": 6 },
                                "_meta": { "progressToken": "p-1" } }),
        )
        .unwrap();
    loop {
        let message = client.next_message().unwrap();
        if message["params"]["_meta"]["table-manners/run"].is_object() {
            break;
        }
    }
    let started = std::time::Instant::now();
    resource(&mut client, &location["artifact"]);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "the read waited for the run"
    );
    client
        .notify(
            "notifications/cancelled",
            serde_json::json!({ "requestId": running, "reason": "the test moved on" }),
        )
        .unwrap();
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
