//! Task 10.10 through `serve` (CONTRACT 5.1): the reserved `export` and `import` tools over the
//! mask bundles (10.5, ADR 0022) and the contract's location JSON. They are controls: in
//! `tools/list`, never in the description's `actions`, never a run, answering while a run holds
//! the permit. Export writes only into rascal's export area and gives the same bytes every time;
//! import is idempotent by identity; every wrong request is a `-32602` refusal.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rascal_core::contract::table_manners;
use rascal_testkit::mcp_client::{Client, Failure};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-transfer-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn rascal(args: &[&str]) -> Output {
    let out = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .env_remove("RASCAL_LOG")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
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

/// A configuration with a slow filler, so a run can hold the permit.
fn config(dir: &Path) -> PathBuf {
    let path = dir.join("rascal.toml");
    let bin = env!("CARGO_BIN_EXE_rascal");
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[fillers.slow]\ncommand = [{bin:?}, \"fake-filler\", \"slow\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\ntimeout_s = 30\nrevision = \"slow\"\n"
        ),
    )
    .unwrap();
    path
}

/// `serve` on the store `store`, its runs under `runs`, its data home `data` (the export area is
/// `data/rascal/exports`).
fn start(dir: &Path, store: &str) -> Client {
    let mut command = rascal_testkit::mcp_client::serve_command(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "--workspace",
            s(&dir.join(store)),
            "--runs",
            s(&dir.join(format!("{store}-runs"))),
            "--config",
            s(&config(dir)),
        ])
        .env("XDG_DATA_HOME", dir.join("data"))
        .env_remove("RASCAL_LOG");
    Client::start(&mut command).unwrap()
}

/// Every file under `dir`, relative, sorted.
fn files(dir: &Path) -> Vec<PathBuf> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.push(path.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// A seam Mask made on the panorama by the command line, imported into the store `a` as a
/// bundle (`workspace import-mask`), with its reference as the store gives it and its bundle's
/// bytes as `workspace export-mask` writes them from `a`.
struct MaskSetup {
    reference: serde_json::Value,
    bundle: Vec<u8>,
}

fn mask_setup(dir: &Path) -> MaskSetup {
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let made = dir.join("made");
    rascal(&["run", "seam", s(&pano), "--out", s(&made)]);
    let ws = rascal_core::workspace::Workspace::open(&made).unwrap();
    let identity = ws
        .list()
        .unwrap()
        .into_iter()
        .rfind(|(kind, ..)| kind == "Mask")
        .unwrap()
        .2;
    drop(ws);
    let first = dir.join("first.mask.tar");
    rascal(&[
        "workspace",
        "export-mask",
        s(&made),
        &identity,
        "--out",
        s(&first),
    ]);
    rascal(&["workspace", "import-mask", s(&dir.join("a")), s(&first)]);
    let again = dir.join("again.mask.tar");
    rascal(&[
        "workspace",
        "export-mask",
        s(&dir.join("a")),
        &identity,
        "--out",
        s(&again),
    ]);
    let store = rascal_core::workspace::Workspace::open(&dir.join("a")).unwrap();
    let reference =
        serde_json::to_value(rascal_core::contract::artifact_ref(&store, &identity).unwrap())
            .unwrap();
    MaskSetup {
        reference,
        bundle: std::fs::read(&again).unwrap(),
    }
}

/// The refusal a control answered with: `-32602`, its data valid against `refusal.json`.
fn refusal(outcome: Result<rascal_testkit::mcp_client::CallResult, Failure>) -> serde_json::Value {
    match outcome {
        Err(Failure::Rpc { code, data, .. }) => {
            assert_eq!(code, -32602);
            let data = data.unwrap();
            valid("urn:table-manners:core:refusal", &data);
            assert_eq!(data["run"], serde_json::Value::Null);
            data
        }
        other => panic!("not refused: {other:?}"),
    }
}

fn structured(
    outcome: Result<rascal_testkit::mcp_client::CallResult, Failure>,
) -> serde_json::Value {
    let result = outcome.unwrap();
    assert!(!result.is_error);
    result.structured.unwrap()
}

#[test]
fn the_controls_are_tools_and_never_actions() {
    let dir = scratch("listed");
    let mut client = start(&dir, "a");
    let tools = client.list_tools().unwrap();
    for name in ["export", "import"] {
        assert!(tools.iter().any(|t| t.name == name), "{name} not listed");
    }
    let description = structured(client.call("describe", serde_json::json!({})));
    for action in description["actions"].as_array().unwrap() {
        assert!(!["export", "import"].contains(&action["name"].as_str().unwrap()));
    }
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_mask_exports_imports_into_an_empty_store_and_again_with_one_reference() {
    let dir = scratch("mask");
    let m = mask_setup(&dir);
    let mut a = start(&dir, "a");
    let before = files(&dir);

    let exported = structured(a.call("export", serde_json::json!({ "artifact": m.reference })));
    valid("urn:table-manners:operations:export-response", &exported);
    assert_eq!(exported["contract"], table_manners::CONTRACT_VERSION);
    assert_eq!(exported["tool"]["name"], "rascal");
    assert_eq!(exported["artifact"], m.reference);
    let file = &exported["file"];
    let path = PathBuf::from(file["path"].as_str().unwrap());
    let hex = m.reference["identity"]
        .as_str()
        .unwrap()
        .strip_prefix("sha256:")
        .unwrap();
    assert_eq!(
        path,
        dir.join(format!("data/rascal/exports/sha256-{hex}.mask.tar"))
    );
    assert_eq!(file["media_type"], "application/x-tar");
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes, m.bundle, "the same bytes as workspace export-mask");
    assert_eq!(file["bytes"], bytes.len());
    assert_eq!(file["sha256"], rascal_core::contract::file_sha256(&bytes));
    // Again: the same document, the same bytes.
    let twice = structured(a.call("export", serde_json::json!({ "artifact": m.reference })));
    assert_eq!(twice, exported);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    // Export wrote only its file, under the export area, and no run.
    let added: Vec<PathBuf> = files(&dir)
        .into_iter()
        .filter(|f| !before.contains(f))
        .collect();
    assert_eq!(
        added,
        vec![PathBuf::from(format!(
            "data/rascal/exports/sha256-{hex}.mask.tar"
        ))]
    );
    a.close().unwrap();

    // Into an empty store, by path with its digest, then again.
    let mut b = start(&dir, "b");
    let request = serde_json::json!({ "file": { "path": s(&path) }, "sha256": file["sha256"] });
    valid("urn:table-manners:operations:import-request", &request);
    let imported = structured(b.call("import", request.clone()));
    valid("urn:table-manners:operations:import-response", &imported);
    assert_eq!(imported["already_present"], false);
    assert_eq!(imported["artifact"], m.reference);
    let again = structured(b.call("import", request));
    assert_eq!(again["already_present"], true);
    assert_eq!(again["artifact"], m.reference);
    // Neither control wrote a run.
    assert!(files(&dir.join("b-runs")).is_empty());
    b.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_location_exports_as_json_and_imports_inline() {
    let dir = scratch("location");
    let floor = repo_root().join("fixtures/images/floor.png");
    let file_sha256 = rascal_core::contract::file_sha256(&std::fs::read(&floor).unwrap());
    let (w, h) = image::image_dimensions(&floor).unwrap();
    let location = serde_json::json!({
        "image": { "file_sha256": file_sha256, "width": w, "height": h,
                   "projection": "flat", "topology": "bounded" },
        "kind": "bbox",
        "parts": [{ "bbox": [0.2, 0.25, 0.6, 0.75] }],
        "straddles_wrap": false
    });
    let given = dir.join("box.json");
    std::fs::write(&given, location.to_string()).unwrap();
    for store in ["a", "b"] {
        rascal(&["workspace", "import", s(&dir.join(store)), s(&floor)]);
    }
    rascal(&[
        "workspace",
        "import-location",
        s(&dir.join("a")),
        s(&given),
        "--image",
        "img-1",
    ]);
    let ws = rascal_core::workspace::Workspace::open(&dir.join("a")).unwrap();
    let identity = ws
        .list()
        .unwrap()
        .into_iter()
        .find(|(kind, ..)| kind == "Location")
        .unwrap()
        .2;
    let reference =
        serde_json::to_value(rascal_core::contract::artifact_ref(&ws, &identity).unwrap()).unwrap();
    drop(ws);

    let mut a = start(&dir, "a");
    let exported = structured(a.call("export", serde_json::json!({ "artifact": reference })));
    valid("urn:table-manners:operations:export-response", &exported);
    assert_eq!(exported["file"]["media_type"], "application/json");
    let path = exported["file"]["path"].as_str().unwrap();
    assert!(path.ends_with(".location.json"), "{path}");
    let bytes = std::fs::read(path).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    valid("urn:table-manners:images:location", &json);
    assert_eq!(json["kind"], "bbox");
    assert_eq!(json["image"]["file_sha256"], file_sha256.as_str());
    a.close().unwrap();

    let mut b = start(&dir, "b");
    let inline = serde_json::json!({ "inline": {
        "media_type": "application/json",
        "data": rascal_testkit::fake_pack::base64_encode(&bytes),
    }});
    valid("urn:table-manners:operations:import-request", &inline);
    let imported = structured(b.call("import", inline.clone()));
    valid("urn:table-manners:operations:import-response", &imported);
    assert_eq!(imported["already_present"], false);
    assert_eq!(imported["artifact"], reference);
    assert_eq!(
        structured(b.call("import", inline))["already_present"],
        true
    );
    b.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wrong_requests_are_refused_and_record_nothing() {
    let dir = scratch("refused");
    let m = mask_setup(&dir);
    let mut a = start(&dir, "a");
    let current = || std::fs::read_to_string(dir.join("a/CURRENT")).unwrap();
    let before = current();

    // A PNG given as a file to import: input, naming what it is.
    let png = repo_root().join("fixtures/images/floor.png");
    let data = refusal(a.call("import", serde_json::json!({ "file": { "path": s(&png) } })));
    assert_eq!(data["reason"], "invalid_arguments");
    assert_eq!(data["error"]["code"], "input");
    assert_eq!(data["error"]["retryable"], false);
    assert!(
        data["error"]["reason"].as_str().unwrap().contains("PNG"),
        "{data:#}"
    );

    // A `portable: none` type: usage, naming the type.
    let mut plan = m.reference.clone();
    plan["type"] = serde_json::json!("rascal:Plan");
    let data = refusal(a.call("export", serde_json::json!({ "artifact": plan })));
    assert_eq!(data["reason"], "invalid_arguments");
    assert_eq!(data["error"]["code"], "usage");
    assert!(
        data["error"]["reason"]
            .as_str()
            .unwrap()
            .contains("rascal:Plan"),
        "{data:#}"
    );

    // An identity the store does not hold: the binding refusal on port `artifact`.
    let mut missing = m.reference.clone();
    let other = format!("sha256:{}", "b".repeat(64));
    missing["identity"] = serde_json::json!(other);
    missing["resource"] = serde_json::json!(format!("rascal://artifact/{other}"));
    let data = refusal(a.call("export", serde_json::json!({ "artifact": missing })));
    assert_eq!(data["reason"], "binding");
    assert_eq!(data["bindings"][0]["port"], "artifact");
    assert_eq!(data["bindings"][0]["problem"], "unresolved");
    assert_eq!(data["bindings"][0]["got"], serde_json::json!(other));

    // A digest that is not the file's: input.
    let exported = structured(a.call("export", serde_json::json!({ "artifact": m.reference })));
    let path = exported["file"]["path"].as_str().unwrap();
    let data = refusal(a.call(
        "import",
        serde_json::json!({ "file": { "path": path }, "sha256": "c".repeat(64) }),
    ));
    assert_eq!(data["reason"], "invalid_arguments");
    assert_eq!(data["error"]["code"], "input");

    // A path outside the allowed directories reads exactly as a missing file inside them.
    let outside = refusal(a.call(
        "import",
        serde_json::json!({ "file": { "path": "/etc/hosts" } }),
    ));
    let absent = refusal(a.call(
        "import",
        serde_json::json!({ "file": { "path": s(&dir.join("absent.mask.tar")) } }),
    ));
    assert_eq!(outside, absent);
    assert_eq!(outside["error"]["code"], "input");

    assert_eq!(current(), before, "no refusal recorded anything");
    assert!(files(&dir.join("a-runs")).is_empty());
    a.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn both_answer_while_a_slow_run_holds_the_permit() {
    let dir = scratch("permit");
    let m = mask_setup(&dir);
    let mut a = start(&dir, "a");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let run = a
        .begin(
            "tools/call",
            serde_json::json!({
                "name": "seam",
                "arguments": { "image": { "path": s(&pano) }, "filler": "slow" },
                "_meta": { "progressToken": "permit" }
            }),
        )
        .unwrap();
    // The run holds the permit once its first progress names it.
    loop {
        let message = a.next_message().unwrap();
        if message["method"] == "notifications/progress" {
            break;
        }
    }
    let export = a
        .begin(
            "tools/call",
            serde_json::json!({ "name": "export", "arguments": { "artifact": m.reference } }),
        )
        .unwrap();
    let import = a
        .begin(
            "tools/call",
            serde_json::json!({ "name": "import", "arguments": { "inline": {
                "media_type": "application/x-tar",
                "data": rascal_testkit::fake_pack::base64_encode(&m.bundle),
            }}}),
        )
        .unwrap();
    let mut order = Vec::new();
    while order.len() < 3 {
        let message = a.next_message().unwrap();
        if let Some(id) = message.get("id").and_then(|v| v.as_u64())
            && [run, export, import].contains(&id)
        {
            if id == import {
                assert_eq!(
                    message["result"]["structuredContent"]["already_present"], true,
                    "{message:#}"
                );
            }
            order.push(id);
        }
    }
    assert_eq!(order.last(), Some(&run), "the controls waited for the run");
    a.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
