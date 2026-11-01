//! Task 3.2: `rascal serve` driven by the independent client: import the floor fixture,
//! `rect`, `rasterize`, `render grey`, read the rendering; every registered op reachable;
//! `workspace_read` on a mask is `invalid_args`; an operational failure is a JSON-RPC error
//! with its class; the CLI reads the same workspace afterwards.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use rascal_testkit::mcp_client::{Client, Failure};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-serve-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn serve(ws: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command
        .args(["serve", "--workspace", ws.to_str().unwrap()])
        .env_remove("RASCAL_LOG");
    command
}

#[test]
fn the_independent_client_imports_finds_masks_renders_and_reads_back() {
    let dir = scratch("walk");
    let ws = dir.join("ws");
    let mut client = Client::start(&mut serve(&ws)).unwrap();
    assert_eq!(client.server["serverInfo"]["name"], "rascal");
    let tools = client.list_tools().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    // Every registered op is a tool, plus the five workspace tools; the catalogue from the shell
    // agrees.
    let out = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(["ops", "list", "--json"])
        .output()
        .unwrap();
    let catalogue: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    for m in &catalogue {
        assert!(
            names.contains(&m["name"].as_str().unwrap()),
            "{} is served",
            m["name"]
        );
    }
    for w in [
        "workspace_import",
        "workspace_list",
        "workspace_describe",
        "workspace_read",
        "workspace_dash",
    ] {
        assert!(names.contains(&w), "{w}");
    }
    // The ops, the five workspace tools, and the contract's actions (ADR 0020): one per shipped
    // recipe, `estimate` and `describe`; no name twice.
    assert_eq!(
        names.len(),
        catalogue.len() + 5 + 9,
        "no other tool: {names:?}"
    );
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), names.len(), "no name twice");
    // The op tools take ordinary arguments: ids for ports, parameters by name.
    let rect = tools.iter().find(|t| t.name == "rect").unwrap();
    assert_eq!(rect.input_schema["properties"]["image"]["type"], "string");
    assert_eq!(rect.input_schema["properties"]["bbox"]["type"], "array");
    assert!(
        rect.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r == "image")
    );
    let fan = tools.iter().find(|t| t.name == "fan").unwrap();
    assert_eq!(
        fan.input_schema["properties"]["constants"]["enum"][0],
        "fan-v3"
    );

    // Import, rect, rasterize, render, read.
    let floor = repo_root().join("fixtures/images/floor.png");
    let imported = client
        .call(
            "workspace_import",
            serde_json::json!({ "path": floor.to_str().unwrap() }),
        )
        .unwrap();
    assert!(!imported.is_error, "{imported:?}");
    let image_id = imported.structured.as_ref().unwrap()["artifacts"]["image"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(image_id, "img-1");
    let found = client
        .call(
            "rect",
            serde_json::json!({ "image": image_id, "bbox": [0.2, 0.2, 0.6, 0.6] }),
        )
        .unwrap();
    assert_eq!(found.structured.as_ref().unwrap()["status"], "ok");
    let location_id = found.structured.as_ref().unwrap()["artifacts"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let masked = client
        .call(
            "rasterize",
            serde_json::json!({ "image": &image_id, "location": location_id }),
        )
        .unwrap();
    assert_eq!(masked.structured.as_ref().unwrap()["status"], "ok");
    assert!(masked.images.is_empty(), "a pure op returns no image block");
    let mask_id = masked.structured.as_ref().unwrap()["artifacts"]["mask"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rendered = client
        .call(
            "render",
            serde_json::json!({ "mask": &mask_id, "for": "grey" }),
        )
        .unwrap();
    assert_eq!(rendered.structured.as_ref().unwrap()["status"], "ok");
    let rendering_id = rendered.structured.as_ref().unwrap()["artifacts"]["image"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let read = client
        .call("workspace_read", serde_json::json!({ "id": &rendering_id }))
        .unwrap();
    assert_eq!(
        read.images.len(),
        1,
        "one image block, as the envelope's images say"
    );
    let (mime, bytes) = &read.images[0];
    assert_eq!(mime, "image/png");
    let decoded = image::load_from_memory(bytes).unwrap().to_luma8();
    assert_eq!(decoded.dimensions(), (320, 200));
    assert_eq!(decoded.get_pixel(100, 100)[0], 255, "inside the box");
    assert_eq!(decoded.get_pixel(10, 10)[0], 0, "outside");
    assert_eq!(
        read.structured.as_ref().unwrap()["images"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // describe and list.
    let described = client
        .call("workspace_describe", serde_json::json!({ "id": &mask_id }))
        .unwrap();
    assert_eq!(
        described.structured.as_ref().unwrap()["artifact_type"],
        "Mask"
    );
    assert_eq!(
        described.structured.as_ref().unwrap()["record"]["use_case"],
        "custom"
    );
    let listed = client
        .call("workspace_list", serde_json::json!({ "type": "Mask" }))
        .unwrap();
    assert_eq!(
        listed.structured.as_ref().unwrap()["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // workspace_read on a mask is invalid_args in the envelope, not a JSON-RPC error.
    let refused = client
        .call("workspace_read", serde_json::json!({ "id": &mask_id }))
        .unwrap();
    assert!(refused.is_error);
    assert_eq!(
        refused.structured.as_ref().unwrap()["status"],
        "invalid_args"
    );
    // A bad parameter is invalid_args in the envelope too; an unknown id is a JSON-RPC error
    // with the usage class; an operational failure carries its class.
    let bad = client
        .call(
            "rect",
            serde_json::json!({ "image": &image_id, "bbox": [0.9, 0.2, 0.1, 0.6] }),
        )
        .unwrap();
    assert_eq!(bad.structured.as_ref().unwrap()["status"], "invalid_args");
    match client.call(
        "rect",
        serde_json::json!({ "image": "img-99", "bbox": [0.1, 0.1, 0.2, 0.2] }),
    ) {
        Err(Failure::Rpc { code, data, .. }) => {
            assert_eq!(code, -32602);
            assert_eq!(data.unwrap()["class"], "usage");
        }
        other => panic!("{other:?}"),
    }
    match client.call(
        "workspace_import",
        serde_json::json!({ "path": "/nonexistent/photo.png" }),
    ) {
        Err(Failure::Rpc { code, data, .. }) => {
            assert_eq!(code, -32603);
            assert_eq!(data.unwrap()["class"], "io");
        }
        other => panic!("{other:?}"),
    }
    match client.call("nope", serde_json::json!({})) {
        Err(Failure::Rpc { code, .. }) => assert_eq!(code, -32602),
        other => panic!("{other:?}"),
    }
    // The run page, into the workspace.
    let page = client
        .call("workspace_dash", serde_json::json!({}))
        .unwrap();
    let path = PathBuf::from(page.structured.as_ref().unwrap()["path"].as_str().unwrap());
    assert!(path.exists() && path.starts_with(&ws));
    // Every registered op is reachable: a call with no arguments is routed to the op and
    // refused for its missing ports, never "no tool".
    for m in &catalogue {
        let name = m["name"].as_str().unwrap();
        match client.call(name, serde_json::json!({})) {
            Err(Failure::Rpc { code, message, .. }) => {
                assert_eq!(code, -32602, "{name}: {message}");
                assert!(!message.contains("no tool"), "{name}: {message}");
            }
            Ok(result) => assert!(result.is_error || result.structured.is_some(), "{name}"),
            Err(other) => panic!("{name}: {other}"),
        }
    }
    let status = client.close().unwrap();
    assert!(status.success(), "{status}");
    // The CLI/serve interchange: the shell sees the same mask in the same workspace.
    let out = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args([
            "workspace",
            "list",
            ws.to_str().unwrap(),
            "--type",
            "Mask",
            "--json",
        ])
        .output()
        .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], mask_id);
    let identity = rows[0]["identity"]
        .as_str()
        .unwrap()
        .trim_start_matches("sha256:")
        .to_string();
    let out = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args([
            "inspect",
            ws.join("artifacts")
                .join(format!("{identity}.mask"))
                .to_str()
                .unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["identity"], rows[0]["identity"]);
    // The journal holds every call the server made, as it would from the shell.
    let journal = std::fs::read_to_string(ws.join("journal.jsonl")).unwrap();
    let ops: Vec<String> = journal
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["event"] == "op")
        .map(|v| v["op"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(&ops[..4], ["import", "rect", "rasterize", "render"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_base64_import_lands_like_a_path_import_and_nothing_but_the_protocol_reaches_stdout() {
    let dir = scratch("b64");
    let ws = dir.join("ws");
    let bytes = std::fs::read(repo_root().join("fixtures/images/floor.png")).unwrap();
    let data = rascal_core::panel::base64(&bytes);
    let mut client = Client::start(&mut serve(&ws)).unwrap();
    let imported = client
        .call("workspace_import", serde_json::json!({ "base64": data }))
        .unwrap();
    let structured = imported.structured.unwrap();
    assert_eq!(structured["frame"]["id"], "raw:320x200:flat");
    let by_path = client.call("workspace_import", serde_json::json!({ "path": repo_root().join("fixtures/images/floor.png").to_str().unwrap() })).unwrap();
    assert_eq!(
        by_path.structured.unwrap()["artifacts"]["image"]["identity"],
        structured["artifacts"]["image"]["identity"],
        "the same pixels, the same identity"
    );
    // Both at once is a usage error; so is neither.
    assert!(matches!(
        client.call("workspace_import", serde_json::json!({})),
        Err(Failure::Rpc { code: -32602, .. })
    ));
    // With tracing on, stderr carries the spans and stdout stayed pure JSON-RPC (the client
    // parsed every line it read; a stray print would have broken it).
    client.close().unwrap();
    let mut traced = serve(&ws);
    traced.env("RASCAL_LOG", "debug");
    let mut client = Client::start(&mut traced).unwrap();
    let listed = client
        .call("workspace_list", serde_json::json!({}))
        .unwrap();
    assert!(!listed.is_error);
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A configuration with the fake pack as `[packs.fake]`.
fn pack_config(dir: &Path) -> PathBuf {
    let path = dir.join("rascal.toml");
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[packs.fake]\ncommand = [{:?}, \"fake-pack\", \"exact\"]\ncall_timeout_s = 30\n",
            env!("CARGO_BIN_EXE_rascal")
        ),
    )
    .unwrap();
    path
}

#[test]
fn a_cancelled_pack_call_fails_with_its_class_and_the_pack_restarts() {
    let dir = scratch("cancel");
    let ws = dir.join("ws");
    let cfg = pack_config(&dir);
    let mut command = serve(&ws);
    command.args(["--config", cfg.to_str().unwrap()]);
    let mut client = Client::start(&mut command).unwrap();
    let names: Vec<String> = client
        .list_tools()
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert!(
        names.contains(&"segment".to_string()),
        "the pack's tool is served: {names:?}"
    );
    let floor = repo_root().join("fixtures/images/floor.png");
    let imported = client
        .call(
            "workspace_import",
            serde_json::json!({ "path": floor.to_str().unwrap() }),
        )
        .unwrap();
    let image = imported.structured.unwrap()["artifacts"]["image"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    // A long call, cancelled by the client while in flight.
    let id = client
        .begin(
            "tools/call",
            serde_json::json!({ "name": "segment", "arguments": { "image": image, "prompt": "x", "delay_ms": 8000 } }),
        )
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
    client
        .notify(
            "notifications/cancelled",
            serde_json::json!({ "requestId": id, "reason": "the test moved on" }),
        )
        .unwrap();
    // The SDK sends no response for a cancelled request (the protocol allows either); what
    // can be observed is the journal and the next call.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    // The pack was shut down and restarts for the next call, which succeeds.
    let again = client
        .call(
            "segment",
            serde_json::json!({ "image": image, "prompt": "y" }),
        )
        .unwrap();
    assert!(!again.is_error);
    assert_eq!(again.structured.unwrap()["status"], "ok");
    let journal = rascal_core::journal::Journal::read(&ws.join("journal.jsonl")).unwrap();
    let statuses: Vec<String> = journal
        .iter()
        .filter_map(|l| {
            l.note
                .as_deref()?
                .strip_prefix("pack call: ")
                .map(str::to_string)
        })
        .map(|t| {
            serde_json::from_str::<serde_json::Value>(&t).unwrap()["status"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        statuses,
        vec!["cancelled".to_string(), "ok".to_string()],
        "{statuses:?}"
    );
    // Nothing of the cancelled call was published: one canonical image plus the second
    // call's two planes.
    let out = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args([
            "workspace",
            "list",
            ws.to_str().unwrap(),
            "--type",
            "Image",
            "--json",
        ])
        .output()
        .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 3, "{rows:?}");
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_base64_import_over_the_sdks_default_line_limit_is_answered() {
    // 2400x2400 of noise is about 17 MB as PNG and 23 MB as base64: over the 16 MiB line the
    // SDK would otherwise read, under the 256 MiB rascal's transport carries.
    let dir = scratch("large");
    let ws = dir.join("ws");
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let noise = image::RgbImage::from_fn(2400, 2400, |_, _| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        image::Rgb([state as u8, (state >> 8) as u8, (state >> 16) as u8])
    });
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgb8(noise)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    assert!(bytes.len() > 16 << 20, "{} bytes", bytes.len());
    let encoded = rascal_core::panel::base64(&bytes);
    let mut client = Client::start(&mut serve(&ws)).unwrap();
    let result = client
        .call("workspace_import", serde_json::json!({ "base64": encoded }))
        .unwrap();
    assert!(!result.is_error);
    let structured = result.structured.unwrap();
    assert_eq!(structured["status"], "ok");
    assert_eq!(structured["frame"]["id"], "raw:2400x2400:flat");
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
