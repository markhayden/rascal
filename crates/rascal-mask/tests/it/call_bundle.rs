//! Task 10.13 (CONTRACT 4.9): `rascal call --bundle <path>` replays an invocation bundle a host
//! built (here the testkit's writer, from the results and `export`). Line → Rasterize → Feather
//! replays from another directory into a fresh store with the original workspace deleted: each
//! replayed invocation equals the bundle's and the final Mask has the original identity. A
//! tampered bundle is refused with nothing imported and nothing run, as is one with
//! `unportable` or `reference_only` entries; two replays of one file are idempotent in the store.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rascal_testkit::invocation_bundle::{self as writer, ArtifactFile, InputFile, Step};
use rascal_testkit::mcp_client::Client;
use serde_json::{Value, json};

fn floor() -> PathBuf {
    std::fs::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/images/floor.png"),
    )
    .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-bundle-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn completed(client: &mut Client, name: &str, args: Value) -> Value {
    let r = client.call(name, args).unwrap().structured.unwrap();
    assert_eq!(r["status"], "completed", "{name}: {r:#}");
    r
}

fn export(client: &mut Client, artifact: &Value) -> Vec<u8> {
    let exported = client
        .call("export", json!({ "artifact": artifact }))
        .unwrap()
        .structured
        .unwrap();
    std::fs::read(exported["file"]["path"].as_str().unwrap()).unwrap()
}

/// Line → Rasterize → Feather served in `dir`, and the steps of their bundle.
fn original(dir: &Path) -> (Vec<Value>, Vec<Step>) {
    let mut command = rascal_testkit::mcp_client::serve_command(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "--workspace",
            dir.join("ws").to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env("XDG_DATA_HOME", dir.join("data"))
        .env_remove("RASCAL_LOG");
    let mut client = Client::start(&mut command).unwrap();
    let line = completed(
        &mut client,
        "line",
        json!({ "image": { "path": floor().to_str().unwrap() }, "column": 0.5, "width_px": 24 }),
    );
    let location = line["artifacts"]["out"][0]["artifact"].clone();
    let rasterized = completed(
        &mut client,
        "rasterize",
        json!({ "location": location, "image": line["artifacts"]["in"][0]["artifact"] }),
    );
    let mask = rasterized["artifacts"]["out"][0]["artifact"].clone();
    let feathered = completed(
        &mut client,
        "feather",
        json!({ "mask": mask, "sigma_px": 6 }),
    );
    let location_file = export(&mut client, &location);
    let mask_file = export(&mut client, &mask);
    client.close().unwrap();
    let floor_bytes = std::fs::read(floor()).unwrap();
    let image_input = |result: &Value| -> Vec<InputFile> {
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
    let steps = vec![
        Step {
            result: line.clone(),
            inputs: image_input(&line),
            artifacts: vec![],
            reference_only: vec![],
            unportable: vec![],
        },
        Step {
            result: rasterized.clone(),
            inputs: image_input(&rasterized),
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
    (vec![line, rasterized, feathered], steps)
}

/// `rascal call --bundle <bundle>` into `into`'s store and runs.
fn replay(into: &Path, bundle: &Path) -> Output {
    let allow = bundle.parent().unwrap();
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args([
            "call",
            "--bundle",
            bundle.to_str().unwrap(),
            "--workspace",
            into.join("ws").to_str().unwrap(),
            "--runs",
            into.join("runs").to_str().unwrap(),
            "--allow-dir",
            allow.to_str().unwrap(),
        ])
        .env("XDG_DATA_HOME", into.join("data"))
        .env("XDG_CACHE_HOME", into.join("cache"))
        .env_remove("RASCAL_LOG")
        .output()
        .unwrap()
}

/// What a replay printed: one result per line.
fn lines(out: &Output) -> Vec<Value> {
    String::from_utf8(out.stdout.clone())
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn save(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join("bundles").join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

/// The operations a store's journal records, by op name.
fn ops(ws: &Path) -> Vec<String> {
    std::fs::read_to_string(ws.join("journal.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|l| l["event"] == "op")
        .map(|l| l["op"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_chain_replays_into_a_fresh_store_with_the_original_gone() {
    let dir = scratch("chain");
    let (results, steps) = original(&dir);
    let bytes = writer::write(&steps);
    assert_eq!(bytes, writer::write(&steps), "the writer is byte-stable");
    let bundle = save(&dir, "chain.tar", &bytes);
    std::fs::remove_dir_all(dir.join("ws")).unwrap();
    let into = dir.join("elsewhere");
    let out = replay(&into, &bundle);
    let printed = lines(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{printed:#?}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(printed.len(), 3);
    for (replayed, original) in printed.iter().zip(&results) {
        assert_eq!(replayed["status"], "completed", "{replayed:#}");
        assert_eq!(replayed["invocation"], original["invocation"]);
        assert_eq!(replayed["run"]["session_id"], Value::Null);
    }
    assert_eq!(
        printed[2]["artifacts"]["out"][0]["artifact"]["identity"],
        results[2]["artifacts"]["out"][0]["artifact"]["identity"]
    );
    // Again on the same file: idempotent in the store. Every artifact import is already
    // present, so the second replay records only its steps' own runs (each operation run, and
    // the image the first step takes by path, as any call records them).
    let before = ops(&into.join("ws")).len();
    let again = replay(&into, &bundle);
    assert_eq!(again.status.code(), Some(0));
    let added: Vec<String> = ops(&into.join("ws")).split_off(before);
    assert_eq!(
        added,
        ["import", "line", "rasterize", "feather"],
        "{added:?}"
    );
    let printed_again = lines(&again);
    assert_eq!(
        printed_again[2]["artifacts"]["out"][0]["artifact"],
        printed[2]["artifacts"]["out"][0]["artifact"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Asserts a replay was refused before anything ran: exit 64, no result, nothing in the store
/// and no run; the reason on stderr names `what`.
fn refused_whole(dir: &Path, name: &str, bytes: &[u8], what: &str) {
    let bundle = save(dir, name, bytes);
    let into = dir.join(format!("into-{name}"));
    let out = replay(&into, &bundle);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(64), "{name}: {stderr}");
    assert!(out.stdout.is_empty(), "{name}");
    assert!(stderr.contains(what), "{name}: {what} not in {stderr}");
    assert!(
        ops(&into.join("ws")).is_empty(),
        "{name}: something was imported"
    );
    assert!(
        std::fs::read_dir(into.join("runs")).map_or(true, |mut d| d.next().is_none()),
        "{name}: something ran"
    );
}

#[test]
fn a_tampered_or_incomplete_bundle_is_refused_whole() {
    let dir = scratch("tampered");
    let (_, steps) = original(&dir);
    let (manifest, members) = writer::parts(&steps);
    let flip = |members: &BTreeMap<String, Vec<u8>>, prefix: &str| {
        let mut changed = members.clone();
        let (_, bytes) = changed
            .iter_mut()
            .find(|(name, _)| name.starts_with(prefix))
            .unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        changed
    };
    let m = writer::manifest_bytes(&manifest);
    refused_whole(
        &dir,
        "artifact-byte.tar",
        &writer::pack(&m, &flip(&members, "artifacts/")),
        "has sha256",
    );
    refused_whole(
        &dir,
        "input-byte.tar",
        &writer::pack(&m, &flip(&members, "inputs/")),
        "file_sha256",
    );
    let mut missing = members.clone();
    let gone = missing.keys().next().unwrap().clone();
    missing.remove(&gone);
    refused_whole(
        &dir,
        "missing.tar",
        &writer::pack(&m, &missing),
        "is missing",
    );
    let mut extra = members.clone();
    extra.insert(format!("inputs/{}.png", "0".repeat(64)), b"x".to_vec());
    refused_whole(
        &dir,
        "extra.tar",
        &writer::pack(&m, &extra),
        "is not one invocation.json names",
    );
    let mut big = m.clone();
    big.extend(std::iter::repeat_n(b' ', 8 << 20));
    refused_whole(
        &dir,
        "oversized.tar",
        &writer::pack(&big, &members),
        "over the limit",
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unportable_or_reference_only_entries_are_refused_naming_them() {
    let dir = scratch("unportable");
    let (_, mut steps) = original(&dir);
    let mask = steps[2].artifacts[0].artifact.clone();
    steps[2].unportable =
        vec![json!({ "port": "mask", "artifact": mask, "reason": "the export failed" })];
    refused_whole(&dir, "unportable.tar", &writer::write(&steps), "unportable");
    steps[2].unportable = vec![];
    steps[0].reference_only = vec![json!({ "port": "image", "why": "a private image" })];
    refused_whole(
        &dir,
        "reference-only.tar",
        &writer::write(&steps),
        "reference_only",
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A one-step bundle (Feather on an exported Mask) into a fresh store imports the Mask; a second
/// replay finds it already present and records no import.
#[test]
fn a_bundle_artifact_is_imported_once() {
    let dir = scratch("import-once");
    let (_, steps) = original(&dir);
    let only = steps.into_iter().nth(2).unwrap();
    let bundle = save(&dir, "feather.tar", &writer::write(&[only]));
    let into = dir.join("fresh");
    for pass in 0..2 {
        let out = replay(&into, &bundle);
        assert_eq!(
            out.status.code(),
            Some(0),
            "pass {pass}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let imports = ops(&into.join("ws"))
            .iter()
            .filter(|op| op.as_str() == "import_mask")
            .count();
        assert_eq!(imports, 1, "pass {pass}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A later step naming an earlier step's output binds to that step's replayed output by port,
/// whatever its identity, and its result says what changed, in its run.json as printed.
#[test]
fn a_later_step_binds_to_the_replayed_output_by_port() {
    let dir = scratch("rebind");
    let (results, mut steps) = original(&dir);
    let real = results[1]["artifacts"]["out"][0]["artifact"]["identity"]
        .as_str()
        .unwrap()
        .to_string();
    let named = format!("sha256:{}", "a".repeat(64));
    // As if the original Rasterize had made another Mask: the bundle names it in Rasterize's
    // result and Feather's invocation, and carries no export of it.
    let rename = |value: &mut Value| {
        let text = value.to_string().replace(&real, &named);
        *value = serde_json::from_str(&text).unwrap();
    };
    rename(&mut steps[1].result);
    rename(&mut steps[2].result);
    steps[2].artifacts.clear();
    let bundle = save(&dir, "rebind.tar", &writer::write(&steps));
    let into = dir.join("fresh");
    let out = replay(&into, &bundle);
    let printed = lines(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{printed:#?}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let feather = &printed[2];
    assert_eq!(feather["invocation"]["arguments"]["mask"]["identity"], real);
    let notes: Vec<&str> = feather["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    assert!(
        notes
            .iter()
            .any(|n| n.contains("mask") && n.contains(&named) && n.contains(&real)),
        "{notes:?}"
    );
    // The report is in the run's own run.json: every printed line is its run's file.
    for line in &printed {
        let file = PathBuf::from(line["run"]["dir"].as_str().unwrap()).join("run.json");
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(&written, line, "{}", file.display());
    }
    let _ = std::fs::remove_dir_all(&dir);
}
