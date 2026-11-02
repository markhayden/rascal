//! Tasks 3.2 and 10.6: `rascal serve` driven by the independent client: every built-in op is an
//! operation tool and nothing else is served but the tasks and the reserved tools; `rect` on an
//! image by path, `rasterize` and `render grey` by reference, the rendering read back from its
//! image out; an unknown tool and a call missing its ports are JSON-RPC errors; an inline image
//! lands like a path image, even over the SDK's default line limit; nothing but the protocol
//! reaches stdout; a configured pack serves no tool; the CLI reads the same store afterwards.
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

fn serve(dir: &Path) -> Command {
    let mut command = rascal_testkit::mcp_client::serve_command(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "--workspace",
            dir.join("ws").to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env_remove("RASCAL_LOG");
    command
}

/// A completed call's result.
fn completed(client: &mut Client, name: &str, args: serde_json::Value) -> serde_json::Value {
    let result = client.call(name, args).unwrap();
    let structured = result.structured.unwrap();
    assert_eq!(structured["status"], "completed", "{name}: {structured:#}");
    assert!(!result.is_error);
    structured
}

/// The reference an operation made on `port`.
fn out(result: &serde_json::Value, port: &str) -> serde_json::Value {
    result["artifacts"]["out"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["port"] == port)
        .unwrap_or_else(|| panic!("no out {port} in {result:#}"))["artifact"]
        .clone()
}

#[test]
fn the_independent_client_runs_operations_and_the_cli_reads_the_store() {
    let dir = scratch("walk");
    let ws = dir.join("ws");
    let mut client = Client::start(&mut serve(&dir)).unwrap();
    assert_eq!(client.server["serverInfo"]["name"], "rascal");
    let tools = client.list_tools().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    // Every registered op is an operation tool; the catalogue from the shell agrees.
    let out_ = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(["ops", "list", "--json"])
        .output()
        .unwrap();
    let catalogue: Vec<serde_json::Value> = serde_json::from_slice(&out_.stdout).unwrap();
    for m in &catalogue {
        assert!(
            names.contains(&m["name"].as_str().unwrap()),
            "{} is served",
            m["name"]
        );
    }
    // The ops, one tool per shipped recipe, `estimate` and `describe`; no workspace tool and no
    // name twice (CONTRACT 5).
    assert_eq!(
        names.len(),
        catalogue.len() + 7 + 2,
        "no other tool: {names:?}"
    );
    assert!(!names.iter().any(|n| n.starts_with("workspace_")));
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), names.len(), "no name twice");
    // An operation's input is its parameters plus its ports: a reference, and for an image port
    // also an image in.
    let rect = tools.iter().find(|t| t.name == "rect").unwrap();
    assert_eq!(rect.input_schema["properties"]["bbox"]["type"], "array");
    let image_port = &rect.input_schema["properties"]["image"]["anyOf"];
    assert_eq!(image_port[0]["required"][3], "identity", "{image_port:#}");
    assert!(
        image_port[1]["properties"]["path"].is_object(),
        "{image_port:#}"
    );
    assert!(
        rect.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r == "image")
    );
    let feather = tools.iter().find(|t| t.name == "feather").unwrap();
    assert_eq!(
        feather.input_schema["properties"]["mask"]["properties"]["identity"]["pattern"],
        "^sha256:[0-9a-f]{64}$"
    );
    let fan = tools.iter().find(|t| t.name == "fan").unwrap();
    assert_eq!(
        fan.input_schema["properties"]["constants"]["enum"][0],
        "fan-v3"
    );

    // rect on the image by path, rasterize and render by reference, the rendering read back.
    let floor = repo_root().join("fixtures/images/floor.png");
    let found = completed(
        &mut client,
        "rect",
        serde_json::json!({ "image": { "path": floor.to_str().unwrap() }, "bbox": [0.2, 0.2, 0.6, 0.6] }),
    );
    let image = found["artifacts"]["in"][0]["artifact"].clone();
    assert_eq!(image["type"], "image");
    let location = out(&found, "location");
    let masked = completed(
        &mut client,
        "rasterize",
        serde_json::json!({ "image": image, "location": location }),
    );
    let mask = out(&masked, "mask");
    assert_eq!(mask["type"], "rascal:Mask");
    let rendered = completed(
        &mut client,
        "render",
        serde_json::json!({ "mask": mask, "for": "grey", "image_blocks": true }),
    );
    let rendering = out(&rendered, "image");
    let image_out = rendered["images"]["out"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["role"] == "image")
        .unwrap()
        .clone();
    assert_eq!(
        rendering["identity"],
        format!(
            "sha256:{}",
            image_out["image"]["file_sha256"].as_str().unwrap()
        ),
        "an image's identity is its file's digest (CONTRACT 3.4)"
    );
    let bytes = std::fs::read(image_out["path"].as_str().unwrap()).unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap().to_luma8();
    assert_eq!(decoded.dimensions(), (320, 200));
    assert_eq!(decoded.get_pixel(100, 100)[0], 255, "inside the box");
    assert_eq!(decoded.get_pixel(10, 10)[0], 0, "outside");

    // An unknown tool, and an operation called without its ports, are JSON-RPC errors; every
    // operation is reachable (refused for its missing ports, never "no tool").
    match client.call("nope", serde_json::json!({})) {
        Err(Failure::Rpc { code, message, .. }) => {
            assert_eq!(code, -32602);
            assert_eq!(message, "no tool nope");
        }
        other => panic!("{other:?}"),
    }
    for m in &catalogue {
        let name = m["name"].as_str().unwrap();
        match client.call(name, serde_json::json!({})) {
            Err(Failure::Rpc { code, message, .. }) => {
                assert_eq!(code, -32602, "{name}: {message}");
                assert!(!message.contains("no tool"), "{name}: {message}");
            }
            Ok(result) => panic!("{name} ran without its ports: {result:?}"),
            Err(other) => panic!("{name}: {other}"),
        }
    }
    let status = client.close().unwrap();
    assert!(status.success(), "{status}");

    // The CLI and serve share the store: the shell sees the mask under the reference's identity.
    let out_ = Command::new(env!("CARGO_BIN_EXE_rascal"))
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
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out_.stdout).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["identity"], mask["identity"]);
    let hex = mask["identity"]
        .as_str()
        .unwrap()
        .trim_start_matches("sha256:")
        .to_string();
    let out_ = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args([
            "inspect",
            ws.join("artifacts")
                .join(format!("{hex}.mask"))
                .to_str()
                .unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        out_.status.success(),
        "{}",
        String::from_utf8_lossy(&out_.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out_.stdout).unwrap();
    assert_eq!(report["identity"], mask["identity"]);
    // The store's journal holds every op the server ran, as it would from the shell.
    let journal = std::fs::read_to_string(ws.join("journal.jsonl")).unwrap();
    let ops: Vec<String> = journal
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["event"] == "op")
        .map(|v| v["op"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ops, ["import", "rect", "rasterize", "render"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_inline_image_lands_like_a_path_image_and_nothing_but_the_protocol_reaches_stdout() {
    let dir = scratch("inline");
    let floor = repo_root().join("fixtures/images/floor.png");
    let data = rascal_core::panel::base64(&std::fs::read(&floor).unwrap());
    let mut client = Client::start(&mut serve(&dir)).unwrap();
    let line = |client: &mut Client, image: serde_json::Value| {
        completed(
            client,
            "line",
            serde_json::json!({ "image": image, "column": 0.5, "width_px": 8 }),
        )
    };
    let inline = line(
        &mut client,
        serde_json::json!({ "inline": { "media_type": "image/png", "data": data } }),
    );
    let by_path = line(
        &mut client,
        serde_json::json!({ "path": floor.to_str().unwrap() }),
    );
    assert_eq!(
        inline["artifacts"]["in"], by_path["artifacts"]["in"],
        "the same file, the same reference"
    );
    assert_eq!(inline["invocation"], by_path["invocation"]);
    assert_eq!(out(&inline, "location"), out(&by_path, "location"));
    // With tracing on, stderr carries the spans and stdout stayed pure JSON-RPC (the client
    // parsed every line it read; a stray print would have broken it).
    client.close().unwrap();
    let mut traced = serve(&dir);
    traced.env("RASCAL_LOG", "debug");
    let mut client = Client::start(&mut traced).unwrap();
    line(
        &mut client,
        serde_json::json!({ "path": floor.to_str().unwrap() }),
    );
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_inline_image_over_the_sdks_default_line_limit_is_answered() {
    // 2400x2400 of noise is about 17 MB as PNG and 23 MB as base64: over the 16 MiB line the
    // SDK would otherwise read, under the 256 MiB rascal's transport carries.
    let dir = scratch("large");
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
    let mut client = Client::start(&mut serve(&dir)).unwrap();
    let result = completed(
        &mut client,
        "rect",
        serde_json::json!({ "image": { "inline": { "media_type": "image/png", "data": encoded } },
                            "bbox": [0.1, 0.1, 0.2, 0.2] }),
    );
    assert_eq!(result["images"]["in"][0]["image"]["width"], 2400);
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_configured_pack_serves_no_tool() {
    let dir = scratch("pack");
    let cfg = dir.join("rascal.toml");
    std::fs::write(
        &cfg,
        format!(
            "config_version = 1\n[packs.fake]\ncommand = [{:?}, \"fake-pack\", \"exact\"]\ncall_timeout_s = 30\n",
            env!("CARGO_BIN_EXE_rascal")
        ),
    )
    .unwrap();
    let mut command = serve(&dir);
    command.args(["--config", cfg.to_str().unwrap()]);
    let mut client = Client::start(&mut command).unwrap();
    let names: Vec<String> = client
        .list_tools()
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert!(!names.contains(&"segment".to_string()), "{names:?}");
    assert!(names.contains(&"remove_prompt".to_string()), "{names:?}");
    match client.call("segment", serde_json::json!({})) {
        Err(Failure::Rpc { code, message, .. }) => {
            assert_eq!(code, -32602);
            assert_eq!(message, "no tool segment");
        }
        other => panic!("{other:?}"),
    }
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
