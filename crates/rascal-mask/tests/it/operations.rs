//! Task 10.6 (CONTRACT 4.7): an operation call is a run. `tools/list` is the description's
//! actions plus the reserved tools; Line, Rasterize and Feather run one call at a time through
//! the independent client, bound by reference, each `run.json` valid with its artifacts in and
//! out, a preview on every Mask and location out, the same images and locations in `images` and
//! `locations`, and the runs as resources; `line` is never rerun. A `many` port takes a list of
//! references, and a bad item is refused naming its index. Each binding problem (`wrong_type`,
//! `wrong_version`, `unresolved`, `wrong_frame`) is the `-32602` `binding` refusal, valid
//! against `refusal.json`, with no run and the `client_run_id` released. `not_applicable`
//! completes with a note and nothing out; `invalid_args` is `invalid`/`usage`; a cancel while
//! queued leaves no run and during the run a `cancelled` `run.json`. The same artifact gives
//! byte-identical previews.
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
    let dir = std::env::temp_dir().join(format!("rascal-operations-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn serve(dir: &Path) -> Command {
    let mut command = rascal_testkit::mcp_client::serve_command(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "--workspace",
            dir.join("ws").to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env_remove("RASCAL_LOG")
        .env_remove("RASCAL_TESTKIT_OPERATION_DELAY_MS");
    command
}

fn start(dir: &Path) -> Client {
    Client::start(&mut serve(dir)).unwrap()
}

fn floor() -> String {
    repo_root()
        .join("fixtures/images/floor.png")
        .to_str()
        .unwrap()
        .to_string()
}

fn pano() -> String {
    repo_root()
        .join("fixtures/images/step-pano.png")
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

/// The description, from the `describe` tool.
fn description(client: &mut Client) -> table_manners::Description {
    let described = client.call("describe", serde_json::json!({})).unwrap();
    serde_json::from_value(described.structured.unwrap()).unwrap()
}

/// A result valid against v0.4 and the rules no schema says (table-manners `tests/fixtures.rs`):
/// the invocation is of the result's action and names no locator; an out artifact has a preview
/// exactly when its type declares one; an image artifact's identity is `sha256:` and the
/// `file_sha256` of an image in `images`; every preview is also in `images.out`.
fn checked(result: &serde_json::Value, description: &table_manners::Description) {
    valid("urn:table-manners:core:result", result);
    let r: table_manners::RunResult = serde_json::from_value(result.clone()).unwrap();
    if let Some(invocation) = &r.invocation {
        assert_eq!(invocation.action, r.action);
        let text = serde_json::to_string(&invocation.arguments).unwrap();
        assert!(
            !text.contains("\"path\"") && !text.contains("\"inline\""),
            "{text}"
        );
    }
    let artifacts = r.artifacts.as_ref().unwrap();
    let images = r.images.as_ref().unwrap();
    let files: Vec<&str> = images
        .taken
        .iter()
        .map(|t| &t.image)
        .chain(images.out.iter().map(|o| &o.image))
        .map(|i| i.file_sha256.as_str())
        .collect();
    for entry in artifacts.taken.iter().chain(&artifacts.out) {
        assert_eq!(entry.artifact.tool, "rascal");
        if entry.artifact.artifact_type == "image" {
            let file = entry.artifact.identity.strip_prefix("sha256:").unwrap();
            assert!(files.contains(&file), "{result:#}");
        }
    }
    for entry in &artifacts.out {
        let declared = description
            .artifact_types
            .iter()
            .flatten()
            .any(|t| t.artifact_type == entry.artifact.artifact_type && t.preview);
        assert_eq!(entry.preview.is_some(), declared, "{result:#}");
        if let Some(preview) = &entry.preview {
            assert_eq!(preview.role, "preview");
            assert!(images.out.contains(preview), "{result:#}");
        }
    }
}

/// A call's completed result, checked.
fn completed(
    client: &mut Client,
    description: &table_manners::Description,
    name: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let result = client.call(name, args).unwrap();
    let structured = result.structured.unwrap();
    assert_eq!(structured["status"], "completed", "{name}: {structured:#}");
    assert!(!result.is_error);
    checked(&structured, description);
    structured
}

fn out(result: &serde_json::Value, port: &str) -> serde_json::Value {
    result["artifacts"]["out"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["port"] == port)
        .unwrap_or_else(|| panic!("no out {port} in {result:#}"))["artifact"]
        .clone()
}

fn read_resource(client: &mut Client, uri: &str) -> serde_json::Value {
    client
        .request("resources/read", serde_json::json!({ "uri": uri }))
        .unwrap()["contents"][0]
        .clone()
}

/// The `binding` refusal of a call: its data, validated against `refusal.json`.
fn refused(result: Result<rascal_testkit::mcp_client::CallResult, Failure>) -> serde_json::Value {
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

/// The run directories under `runs`, sorted.
fn runs(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir.join("runs"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("run-") && !n.ends_with(".in"))
        .collect();
    names.sort();
    names
}

#[test]
fn tools_list_is_the_descriptions_actions_and_the_reserved_tools() {
    let dir = scratch("tools");
    let mut client = start(&dir);
    let described = description(&mut client);
    let mut expected: Vec<String> = described.actions.iter().map(|a| a.name.clone()).collect();
    expected.extend(["estimate".to_string(), "describe".to_string()]);
    let tools = client.list_tools().unwrap();
    let names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
    assert_eq!(names, expected);
    // An operation's input schema: its params_schema's properties plus one per in-port, and the
    // delivery choices; nothing else.
    for action in described
        .actions
        .iter()
        .filter(|a| a.kind == table_manners::ActionKind::Operation)
    {
        let tool = tools.iter().find(|t| t.name == action.name).unwrap();
        let mut properties: Vec<String> = tool.input_schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        properties.sort();
        let mut wanted: Vec<String> = action.params_schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .chain(
                action
                    .ports
                    .as_ref()
                    .unwrap()
                    .inputs
                    .iter()
                    .map(|p| p.name.clone()),
            )
            .chain(["return_images".to_string(), "image_blocks".to_string()])
            .collect();
        wanted.sort();
        assert_eq!(properties, wanted, "{}", action.name);
        assert_eq!(tool.input_schema["additionalProperties"], false);
    }
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn line_rasterize_feather_run_one_call_at_a_time() {
    let dir = scratch("chain");
    let mut client = start(&dir);
    let described = description(&mut client);
    let line = completed(
        &mut client,
        &described,
        "line",
        serde_json::json!({ "image": { "path": floor() }, "column": 0.5, "width_px": 24 }),
    );
    let image = line["artifacts"]["in"][0]["artifact"].clone();
    assert_eq!(line["artifacts"]["in"][0]["port"], "image");
    let location = out(&line, "location");
    assert_eq!(location["type"], "location");
    assert_eq!(line["images"]["in"][0]["slot"], "image");
    assert_eq!(line["locations"]["found"].as_array().unwrap().len(), 1);
    assert_eq!(line["invocation"]["arguments"]["column"], 0.5);
    assert!(line["invocation"]["arguments"]["image"]["expect"].is_object());
    assert_eq!(line["body"]["envelope"]["status"], "ok");

    let rasterized = completed(
        &mut client,
        &described,
        "rasterize",
        serde_json::json!({ "location": location, "image": image, "return_images": "resource" }),
    );
    let mask = out(&rasterized, "mask");
    assert_eq!(mask["type"], "rascal:Mask");
    assert_eq!(rasterized["invocation"]["arguments"]["location"], location);
    assert_eq!(rasterized["invocation"]["arguments"]["image"], image);
    assert_eq!(
        rasterized["locations"]["used"], line["locations"]["found"],
        "the location taken is the one found"
    );
    // Located as a resource: the run's preview image, readable.
    let preview = &rasterized["artifacts"]["out"][0]["preview"];
    let uri = preview["resource"].as_str().unwrap().to_string();
    let blob = read_resource(&mut client, &uri);
    assert_eq!(blob["mimeType"], "image/png");
    let bytes = rascal_testkit::mcp_client::base64_decode(blob["blob"].as_str().unwrap()).unwrap();
    assert_eq!(
        rascal_core::contract::file_sha256(&bytes),
        preview["image"]["file_sha256"].as_str().unwrap()
    );

    let feathered = completed(
        &mut client,
        &described,
        "feather",
        serde_json::json!({ "mask": mask, "sigma_px": 6 }),
    );
    let new_mask = out(&feathered, "mask");
    assert_ne!(new_mask["identity"], mask["identity"]);
    assert_eq!(new_mask["frame"], mask["frame"]);
    assert_eq!(new_mask["on"], mask["on"]);
    assert_eq!(
        feathered["invocation"]["arguments"]["constants"],
        "feather-v1"
    );
    assert_eq!(
        feathered["invocation"]["effective"]["x-rascal"]["constants"],
        "feather-v1"
    );

    // The runs list and the run resources include them, newest first.
    let list: serde_json::Value = serde_json::from_str(
        read_resource(&mut client, "rascal://runs")["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    valid("urn:table-manners:core:runs", &list);
    let actions: Vec<&str> = list["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, ["feather", "rasterize", "line"]);
    assert!(
        list["runs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["status"] == "completed")
    );
    let id = feathered["run"]["id"].as_str().unwrap();
    let text = read_resource(&mut client, &format!("rascal://run/{id}/run.json"))["text"]
        .as_str()
        .unwrap()
        .to_string();
    let on_disk: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(on_disk["artifacts"], feathered["artifacts"]);
    let journal = read_resource(&mut client, &format!("rascal://run/{id}/journal"))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(journal.lines().count(), 1, "{journal}");
    assert!(journal.contains("\"op\":\"feather\""));
    client.close().unwrap();

    // Nothing upstream reran: one line record in the store, and no workspace in a run.
    let store = std::fs::read_to_string(dir.join("ws/journal.jsonl")).unwrap();
    let ops: Vec<String> = store
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["event"] == "op")
        .map(|v| v["op"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ops, ["import", "line", "rasterize", "feather"]);
    for run in runs(&dir) {
        assert!(
            !dir.join("runs").join(&run).join("registry").exists(),
            "{run}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_many_port_takes_a_list_of_references_and_a_bad_item_names_its_index() {
    let dir = scratch("many");
    let ws = dir.join("ws");
    // The planes come from the fake segmenter, into the store.
    let cfg = dir.join("rascal.toml");
    std::fs::write(
        &cfg,
        format!(
            "config_version = 1\n[packs.fake]\ncommand = [{:?}, \"fake-pack\", \"exact\"]\ncall_timeout_s = 30\n",
            env!("CARGO_BIN_EXE_rascal")
        ),
    )
    .unwrap();
    let called = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args([
            "tools",
            "call",
            "fake",
            "segment",
            "--config",
            cfg.to_str().unwrap(),
            "--workspace",
            ws.to_str().unwrap(),
            "--image",
            &floor(),
            "--prompt",
            "chair",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        called.status.success(),
        "{}",
        String::from_utf8_lossy(&called.stderr)
    );
    let planes: Vec<serde_json::Value> = {
        let store = rascal_core::workspace::Workspace::open_read_only(&ws).unwrap();
        let mut rows: Vec<(String, String)> = store
            .list()
            .unwrap()
            .into_iter()
            .filter(|(kind, id, _, _)| kind == "Image" && id != "img-1")
            .map(|(_, id, identity, _)| (id, identity))
            .collect();
        rows.sort();
        rows.iter()
            .map(|(id, _)| {
                serde_json::to_value(rascal_core::contract::artifact_ref(&store, id).unwrap())
                    .unwrap()
            })
            .collect()
    };
    assert!(planes.len() >= 2, "{planes:#?}");
    let mut client = start(&dir);
    let described = description(&mut client);
    let selected = completed(
        &mut client,
        &described,
        "select",
        serde_json::json!({ "plane": [planes[0], planes[1]] }),
    );
    let taken = selected["artifacts"]["in"].as_array().unwrap();
    assert_eq!(taken.len(), 2);
    for (n, entry) in taken.iter().enumerate() {
        assert_eq!(entry["port"], "plane");
        assert_eq!(entry["artifact"], planes[n], "in order");
    }
    assert_eq!(
        selected["invocation"]["arguments"]["plane"],
        serde_json::json!([planes[0], planes[1]])
    );
    // A location where a plane goes, and an identity the store does not hold: each named by its
    // index, in the order given.
    let rect = completed(
        &mut client,
        &described,
        "rect",
        serde_json::json!({ "image": { "path": floor() }, "bbox": [0.1, 0.1, 0.3, 0.3] }),
    );
    let location = out(&rect, "location");
    let mut missing = planes[0].clone();
    missing["identity"] = serde_json::json!(format!("sha256:{}", "0".repeat(64)));
    let before = runs(&dir);
    let data = refused(client.call(
        "select",
        serde_json::json!({ "plane": [planes[0], location, missing] }),
    ));
    let bindings = data["bindings"].as_array().unwrap();
    assert_eq!(bindings.len(), 2, "{data:#}");
    assert_eq!(bindings[0]["port"], "plane");
    assert_eq!(bindings[0]["index"], 1);
    assert_eq!(bindings[0]["problem"], "wrong_type");
    assert_eq!(bindings[1]["index"], 2);
    assert_eq!(bindings[1]["problem"], "unresolved");
    assert!(bindings[1]["remedy"].is_string());
    assert_eq!(runs(&dir), before, "no run");
    // An empty list is not a list of references.
    assert!(matches!(
        client.call("select", serde_json::json!({ "plane": [] })),
        Err(Failure::Rpc { code: -32602, .. })
    ));
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_binding_problem_is_refused_before_a_run_and_releases_its_client_run_id() {
    let dir = scratch("refusals");
    let mut client = start(&dir);
    let described = description(&mut client);
    let rect = completed(
        &mut client,
        &described,
        "rect",
        serde_json::json!({ "image": { "path": floor() }, "bbox": [0.3, 0.3, 0.5, 0.6] }),
    );
    let image = rect["artifacts"]["in"][0]["artifact"].clone();
    let location = out(&rect, "location");
    let masked = completed(
        &mut client,
        &described,
        "rasterize",
        serde_json::json!({ "location": location, "image": image }),
    );
    let mask = out(&masked, "mask");
    let windowed = completed(
        &mut client,
        &described,
        "window",
        serde_json::json!({ "image": image, "mask": mask, "max_side": 4096 }),
    );
    let crop_mask = out(&windowed, "mask");
    let crop_image = out(&windowed, "image");
    assert_ne!(crop_mask["frame"], mask["frame"]);

    let mut version_2 = mask.clone();
    version_2["schema_version"] = serde_json::json!(2);
    let mut missing = mask.clone();
    missing["identity"] = serde_json::json!(format!("sha256:{}", "0".repeat(64)));
    let cases = [
        (
            "feather",
            serde_json::json!({ "mask": location, "sigma_px": 4 }),
            "mask",
            "wrong_type",
        ),
        (
            "feather",
            serde_json::json!({ "mask": version_2, "sigma_px": 4 }),
            "mask",
            "wrong_version",
        ),
        (
            "feather",
            serde_json::json!({ "mask": missing, "sigma_px": 4 }),
            "mask",
            "unresolved",
        ),
        // A mask drawn in a crop: frame with a base and a fill in the raw frame.
        (
            "paste",
            serde_json::json!({ "base": image, "fill": image, "mask": crop_mask }),
            "mask",
            "wrong_frame",
        ),
        // unwindow's own relation: a crop of another frame than the parent's is wrong_frame.
        (
            "unwindow",
            serde_json::json!({ "image": crop_image, "crop": image }),
            "crop",
            "wrong_frame",
        ),
    ];
    for (n, (name, args, port, problem)) in cases.iter().enumerate() {
        let before = runs(&dir);
        let id = format!("refused-{n}");
        let data = refused(client.call_with_meta(
            name,
            args.clone(),
            serde_json::json!({ "table-manners/client_run_id": id }),
        ));
        assert_eq!(data["bindings"][0]["port"], *port, "{data:#}");
        assert_eq!(data["bindings"][0]["problem"], *problem, "{data:#}");
        assert!(data["bindings"][0].get("index").is_none());
        assert_eq!(runs(&dir), before, "{name}: no run");
        // The id was released: the corrected call runs under it.
        let fixed = match *name {
            "paste" => {
                serde_json::json!({ "base": crop_image, "fill": crop_image, "mask": crop_mask })
            }
            "unwindow" => serde_json::json!({ "image": image, "crop": crop_image }),
            _ => serde_json::json!({ "mask": mask, "sigma_px": 4 }),
        };
        let ran = client
            .call_with_meta(
                name,
                fixed,
                serde_json::json!({ "table-manners/client_run_id": id }),
            )
            .unwrap()
            .structured
            .unwrap();
        assert_eq!(ran["status"], "completed", "{name}: {ran:#}");
        assert_eq!(ran["run"]["client_run_id"], id.as_str());
        checked(&ran, &described);
    }
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn not_applicable_completes_with_a_note_and_invalid_args_is_invalid() {
    let dir = scratch("outcomes");
    let mut client = start(&dir);
    let described = description(&mut client);
    // A view has no interior: rasterize is not applicable. The location given by value is
    // registered in the store and listed as taken.
    let view = completed(
        &mut client,
        &described,
        "rasterize",
        serde_json::json!({ "image": { "path": pano() },
                            "location": { "kind": "view", "yaw": 0.0, "pitch": 0.0, "fov": 60.0 } }),
    );
    assert_eq!(view["artifacts"]["out"], serde_json::json!([]));
    assert_eq!(view["images"]["out"], serde_json::json!([]));
    assert_eq!(view["artifacts"]["in"].as_array().unwrap().len(), 2);
    assert_eq!(view["body"]["envelope"]["status"], "not_applicable");
    assert!(
        view["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("not_applicable")),
        "{view:#}"
    );
    assert_eq!(
        view["invocation"]["arguments"]["location"]["kind"], "view",
        "a location by value is recorded as given"
    );
    // A parameter out of its bounds is the run's invalid/usage.
    let line = completed(
        &mut client,
        &described,
        "line",
        serde_json::json!({ "image": { "path": floor() }, "column": 0.5, "width_px": 24 }),
    );
    let image = line["artifacts"]["in"][0]["artifact"].clone();
    let masked = completed(
        &mut client,
        &described,
        "rasterize",
        serde_json::json!({ "location": out(&line, "location"), "image": image }),
    );
    for args in [
        serde_json::json!({ "mask": out(&masked, "mask"), "sigma_px": 0.001 }),
        serde_json::json!({ "mask": out(&masked, "mask"), "sigma_px": 4, "radius": 2 }),
        serde_json::json!({ "mask": out(&masked, "mask"), "sigma_px": 4, "constants": "feather-v9" }),
    ] {
        let result = client.call("feather", args).unwrap();
        assert!(result.is_error);
        let structured = result.structured.unwrap();
        assert_eq!(structured["status"], "invalid", "{structured:#}");
        assert_eq!(structured["error"]["code"], "usage", "{structured:#}");
        checked(&structured, &described);
    }
    // An image off its expect is invalid/input, as for any action.
    let mut expect = line["images"]["in"][0]["image"].clone();
    expect["file_sha256"] = serde_json::json!("0".repeat(64));
    let result = client
        .call(
            "line",
            serde_json::json!({ "image": { "path": pano(), "expect": expect }, "column": 0.5, "width_px": 4 }),
        )
        .unwrap()
        .structured
        .unwrap();
    assert_eq!(result["status"], "invalid");
    assert_eq!(result["error"]["code"], "input", "{result:#}");
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cancel_while_queued_leaves_no_run_and_during_the_run_a_cancelled_run_json() {
    let dir = scratch("cancel");
    let mut client = start(&dir);
    let described = description(&mut client);
    let line = completed(
        &mut client,
        &described,
        "line",
        serde_json::json!({ "image": { "path": floor() }, "column": 0.5, "width_px": 24 }),
    );
    let image = line["artifacts"]["in"][0]["artifact"].clone();
    let location = out(&line, "location");
    client.close().unwrap();
    let before = runs(&dir);
    // A slow server on the same store: every operation waits before its op runs.
    let mut slow = serve(&dir);
    slow.env("RASCAL_TESTKIT_OPERATION_DELAY_MS", "20000");
    let mut client = Client::start(&mut slow).unwrap();
    let args = serde_json::json!({ "location": location, "image": image });
    let running = client
        .begin(
            "tools/call",
            serde_json::json!({ "name": "rasterize", "arguments": args,
                                "_meta": { "progressToken": "p-1" } }),
        )
        .unwrap();
    // The run is allocated once its reference arrives in a progress notification.
    loop {
        let message = client.next_message().unwrap();
        if message["params"]["_meta"]["table-manners/run"].is_object() {
            break;
        }
    }
    let queued = client
        .begin(
            "tools/call",
            serde_json::json!({ "name": "rasterize", "arguments": args,
                                "_meta": { "table-manners/client_run_id": "queued-1" } }),
        )
        .unwrap();
    for id in [queued, running] {
        client
            .notify(
                "notifications/cancelled",
                serde_json::json!({ "requestId": id, "reason": "the test moved on" }),
            )
            .unwrap();
    }
    // The cancelled run writes its run.json; wait for it (an observable event, not a sleep).
    let started = std::time::Instant::now();
    let cancelled = loop {
        let new: Vec<String> = runs(&dir)
            .into_iter()
            .filter(|r| !before.contains(r))
            .collect();
        if let [one] = new.as_slice()
            && let Ok(text) = std::fs::read_to_string(dir.join("runs").join(one).join("run.json"))
        {
            break serde_json::from_str::<serde_json::Value>(&text).unwrap();
        }
        assert!(started.elapsed().as_secs() < 30, "no cancelled run.json");
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert_eq!(cancelled["status"], "cancelled", "{cancelled:#}");
    assert_eq!(cancelled["artifacts"]["out"], serde_json::json!([]));
    valid("urn:table-manners:core:result", &cancelled);
    // The queued call never ran: one run in all, and the store holds no new mask.
    client
        .request(
            "resources/read",
            serde_json::json!({ "uri": "rascal://session" }),
        )
        .unwrap();
    let new: Vec<String> = runs(&dir)
        .into_iter()
        .filter(|r| !before.contains(r))
        .collect();
    assert_eq!(new.len(), 1, "{new:?}");
    client.close().unwrap();
    let masks = rascal_core::workspace::Workspace::open_read_only(&dir.join("ws"))
        .unwrap()
        .list()
        .unwrap()
        .into_iter()
        .filter(|(kind, ..)| kind == "Mask")
        .count();
    assert_eq!(masks, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_same_artifact_gives_byte_identical_previews() {
    let dir = scratch("previews");
    let mut client = start(&dir);
    let described = description(&mut client);
    let line = completed(
        &mut client,
        &described,
        "line",
        serde_json::json!({ "image": { "path": floor() }, "column": 0.25, "width_px": 16 }),
    );
    let args = serde_json::json!({
        "location": out(&line, "location"),
        "image": line["artifacts"]["in"][0]["artifact"],
        "return_images": "inline",
    });
    let first = completed(&mut client, &described, "rasterize", args.clone());
    let second = completed(&mut client, &described, "rasterize", args);
    assert_ne!(first["run"]["id"], second["run"]["id"]);
    assert_eq!(first["artifacts"]["out"], second["artifacts"]["out"]);
    let data = first["artifacts"]["out"][0]["preview"]["inline"]["data"]
        .as_str()
        .unwrap();
    let bytes = rascal_testkit::mcp_client::base64_decode(data).unwrap();
    assert_eq!(
        rascal_core::contract::file_sha256(&bytes),
        first["artifacts"]["out"][0]["preview"]["image"]["file_sha256"]
            .as_str()
            .unwrap()
    );
    let png = image::load_from_memory(&bytes).unwrap();
    assert_eq!((png.width(), png.height()), (320, 200));
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
