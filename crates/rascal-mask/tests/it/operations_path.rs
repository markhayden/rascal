//! Task 10.14 (CONTRACT 4.7 to 4.9, 5.1): the operations path end to end through the
//! independent client, nothing imported by hand. `line` on an image path, `rasterize` on its
//! location's reference, the Mask exported; `serve` restarted on the workspace moved elsewhere,
//! the Mask's reference read back and `feather` with a new sigma bound to it; that result's
//! invocation run again by `rascal call`; the chain replayed from a bundle in another directory
//! with the store gone; and `remove`'s Mask bound by a later `feather`.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rascal_testkit::invocation_bundle::{self as writer, ArtifactFile, InputFile, Step};
use rascal_testkit::mcp_client::Client;
use serde_json::{Value, json};

fn image(name: &str) -> PathBuf {
    std::fs::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/images")
            .join(name),
    )
    .unwrap()
}

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-operations-path-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn serve(dir: &Path, ws: &Path) -> Client {
    let mut command = rascal_testkit::mcp_client::serve_command(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "--workspace",
            ws.to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env("XDG_DATA_HOME", dir.join("data"))
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .env_remove("RASCAL_LOG");
    Client::start(&mut command).unwrap()
}

fn rascal(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .env("XDG_DATA_HOME", dir.join("data"))
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .env_remove("RASCAL_LOG")
        .output()
        .unwrap()
}

fn completed(client: &mut Client, name: &str, args: Value) -> Value {
    let r = client.call(name, args).unwrap().structured.unwrap();
    assert_eq!(r["status"], "completed", "{name}: {r:#}");
    r
}

fn made(result: &Value, port: &str) -> Value {
    result["artifacts"]["out"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["port"] == port)
        .unwrap()["artifact"]
        .clone()
}

fn export(client: &mut Client, artifact: &Value) -> Vec<u8> {
    let exported = client
        .call("export", json!({ "artifact": artifact }))
        .unwrap()
        .structured
        .unwrap();
    std::fs::read(exported["file"]["path"].as_str().unwrap()).unwrap()
}

#[test]
fn line_rasterize_feather_end_to_end() {
    let dir = scratch();
    let floor = image("floor.png");
    let ws = dir.join("ws");
    let mut client = serve(&dir, &ws);
    let line = completed(
        &mut client,
        "line",
        json!({ "image": { "path": floor.to_str().unwrap() }, "column": 0.5, "width_px": 24 }),
    );
    let location = made(&line, "location");
    let rasterized = completed(
        &mut client,
        "rasterize",
        json!({ "location": location, "image": line["artifacts"]["in"][0]["artifact"] }),
    );
    let mask = made(&rasterized, "mask");
    let mask_file = export(&mut client, &mask);
    let location_file = export(&mut client, &location);
    client.close().unwrap();

    // `serve` restarted on the workspace moved elsewhere: the reference reads back and binds.
    let moved = dir.join("moved/ws");
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::rename(&ws, &moved).unwrap();
    let mut client = serve(&dir, &moved);
    let text = client
        .request("resources/read", json!({ "uri": mask["resource"] }))
        .unwrap()["contents"][0]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let held: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(held["artifact"], mask);
    let feathered = completed(
        &mut client,
        "feather",
        json!({ "mask": mask, "sigma_px": 9 }),
    );
    let feathered_mask = made(&feathered, "mask");

    // A task's Mask, bound by a later operation.
    let removed = completed(
        &mut client,
        "remove",
        json!({ "image": { "path": image("chair-blob.png").to_str().unwrap() },
                "location": { "kind": "bbox", "bbox": [0.27, 0.21, 0.53, 0.79] } }),
    );
    completed(
        &mut client,
        "feather",
        json!({ "mask": made(&removed, "mask"), "sigma_px": 3 }),
    );
    client.close().unwrap();

    // The Feather's invocation, run again by `rascal call`.
    let args = dir.join("feather.json");
    std::fs::write(&args, feathered["invocation"]["arguments"].to_string()).unwrap();
    let called = rascal(
        &dir,
        &[
            "call",
            "feather",
            "--arguments",
            args.to_str().unwrap(),
            "--workspace",
            moved.to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ],
    );
    assert_eq!(called.status.code(), Some(0));
    let called: Value = serde_json::from_slice(&called.stdout).unwrap();
    assert_eq!(called["invocation"], feathered["invocation"]);
    assert_eq!(made(&called, "mask"), feathered_mask);

    // The chain as a bundle, replayed in another directory with the store gone.
    let floor_bytes = std::fs::read(&floor).unwrap();
    let inputs = |result: &Value| -> Vec<InputFile> {
        result["images"]["in"]
            .as_array()
            .unwrap()
            .iter()
            .map(|taken| InputFile {
                slot: None,
                port: Some(taken["slot"].as_str().unwrap().to_string()),
                image: taken["image"].clone(),
                bytes: floor_bytes.clone(),
                ext: "png".to_string(),
            })
            .collect()
    };
    let steps = [
        Step {
            result: line.clone(),
            inputs: inputs(&line),
            artifacts: vec![],
            reference_only: vec![],
            unportable: vec![],
        },
        Step {
            result: rasterized.clone(),
            inputs: inputs(&rasterized),
            artifacts: vec![ArtifactFile {
                port: "location".to_string(),
                artifact: location,
                bytes: location_file,
                ext: "location.json".to_string(),
            }],
            reference_only: vec![],
            unportable: vec![],
        },
        Step {
            result: feathered.clone(),
            inputs: vec![],
            artifacts: vec![ArtifactFile {
                port: "mask".to_string(),
                artifact: mask,
                bytes: mask_file,
                ext: "mask.tar".to_string(),
            }],
            reference_only: vec![],
            unportable: vec![],
        },
    ];
    let bundle = dir.join("bundles/chain.tar");
    std::fs::create_dir_all(bundle.parent().unwrap()).unwrap();
    std::fs::write(&bundle, writer::write(&steps)).unwrap();
    std::fs::remove_dir_all(dir.join("moved")).unwrap();
    let elsewhere = dir.join("elsewhere");
    let replayed = rascal(
        &dir,
        &[
            "call",
            "--bundle",
            bundle.to_str().unwrap(),
            "--workspace",
            elsewhere.join("ws").to_str().unwrap(),
            "--runs",
            elsewhere.join("runs").to_str().unwrap(),
            "--allow-dir",
            bundle.parent().unwrap().to_str().unwrap(),
        ],
    );
    assert_eq!(
        replayed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    let lines: Vec<Value> = String::from_utf8(replayed.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    for (replayed, original) in lines.iter().zip([&line, &rasterized, &feathered]) {
        assert_eq!(replayed["invocation"], original["invocation"]);
    }
    assert_eq!(made(&lines[2], "mask"), feathered_mask);
    let _ = std::fs::remove_dir_all(&dir);
}
