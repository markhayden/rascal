//! Task 8.4 (ADR 0020): every `run` and `heal` writes `run.json`, the contract's result, last
//! and atomically: completed, invalid and error runs, each valid against table-manners' result
//! schema; a failed heal keeps the mask it made; an `--out` that held something is not touched.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rascal_core::contract::table_manners;

fn rascal(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .env_remove("RASCAL_LOG")
        .output()
        .unwrap()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-runjson-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
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

/// `run.json` from a run directory, validated against the contract's result schema.
fn run_json(dir: &Path) -> serde_json::Value {
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("run.json")).unwrap()).unwrap();
    let (_, text) = table_manners::SCHEMAS
        .iter()
        .find(|(i, _)| *i == "urn:table-manners:core:result")
        .unwrap();
    let schema: serde_json::Value = serde_json::from_str(text).unwrap();
    let validator = jsonschema::options()
        .with_retriever(Embedded)
        .build(&schema)
        .unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&value)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:?}\n{value:#}");
    assert!(!dir.join(".run.json.tmp").exists());
    value
}

fn config(dir: &Path, behaviour: &str) -> PathBuf {
    let path = dir.join(format!("{behaviour}.toml"));
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[fillers.fake]\ncommand = [{:?}, \"fake-filler\", \"{behaviour}\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\npaste = \"feather\"\ntimeout_s = 30\nrevision = \"fake\"\n",
            env!("CARGO_BIN_EXE_rascal")
        ),
    )
    .unwrap();
    path
}

#[test]
fn a_completed_heal_names_its_input_its_mask_and_its_result() {
    let dir = scratch("heal");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let out = rascal(&[
        "heal",
        "seam",
        pano.to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        config(&dir, "exact").to_str().unwrap(),
        "--out",
        dir.join("run").to_str().unwrap(),
        "--no-dash",
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r = run_json(&dir.join("run"));
    assert_eq!(r["status"], "completed");
    assert_eq!(r["action"], "seam");
    assert_eq!(r["error"], serde_json::Value::Null);
    assert_eq!(r["tool"]["name"], "rascal");
    let digest = rascal_core::contract::file_sha256(&std::fs::read(&pano).unwrap());
    assert_eq!(
        r["images"]["in"][0]["image"]["file_sha256"],
        digest.as_str()
    );
    assert_eq!(r["images"]["in"][0]["image"]["topology"], "wrap");
    let roles: Vec<&str> = r["images"]["out"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["mask", "result"]);
    let result = &r["images"]["out"][1];
    assert_eq!(result["image"]["width"], 800);
    assert_eq!(
        result["image"]["topology"], "wrap",
        "the result is the panorama"
    );
    assert!(Path::new(result["path"].as_str().unwrap()).is_file());
    assert_eq!(r["body"]["filler"], "fake");
    assert_eq!(r["body"]["outcome"], "ok");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_filler_is_an_error_that_keeps_the_mask() {
    let dir = scratch("failing");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let out = rascal(&[
        "heal",
        "seam",
        pano.to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        config(&dir, "failing").to_str().unwrap(),
        "--out",
        dir.join("run").to_str().unwrap(),
        "--no-dash",
    ]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r = run_json(&dir.join("run"));
    assert_eq!(r["status"], "error");
    assert!(r["error"]["reason"].as_str().unwrap().len() > 3);
    let mask = r["images"]["out"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["role"] == "mask");
    assert!(mask.is_some(), "the mask survives the failed fill: {r:#}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_invalid_request_is_invalid_and_a_used_location_is_listed() {
    let dir = scratch("invalid");
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let chairs = repo_root().join("fixtures/locations/chairs.json");
    let ok = rascal(&[
        "run",
        "remove",
        image.to_str().unwrap(),
        "--location",
        chairs.to_str().unwrap(),
        "--out",
        dir.join("ok").to_str().unwrap(),
    ]);
    assert_eq!(ok.status.code(), Some(0));
    let r = run_json(&dir.join("ok"));
    assert_eq!(r["locations"]["used"].as_array().unwrap().len(), 1);
    assert_eq!(
        r["locations"]["used"][0]["parts"][0]["bbox"],
        serde_json::json!([0.27, 0.21, 0.53, 0.79])
    );
    let bad = rascal(&[
        "run",
        "remove",
        image.to_str().unwrap(),
        "--location",
        repo_root()
            .join("fixtures/contract/locations/bbox-squint-finding.json")
            .to_str()
            .unwrap(),
        "--out",
        dir.join("bad").to_str().unwrap(),
    ]);
    assert_eq!(bad.status.code(), Some(64));
    let r = run_json(&dir.join("bad"));
    assert_eq!(r["status"], "invalid");
    assert_eq!(r["error"]["code"], "input");
    assert_eq!(r["error"]["retryable"], false);
    // Equal requests have equal identities; a different location does not.
    let again = rascal(&[
        "run",
        "remove",
        image.to_str().unwrap(),
        "--location",
        chairs.to_str().unwrap(),
        "--out",
        dir.join("again").to_str().unwrap(),
    ]);
    assert_eq!(again.status.code(), Some(0));
    assert_eq!(
        run_json(&dir.join("again"))["input_identity"],
        run_json(&dir.join("ok"))["input_identity"]
    );
    assert_ne!(
        r["input_identity"],
        run_json(&dir.join("ok"))["input_identity"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_out_that_held_something_is_left_untouched() {
    let dir = scratch("occupied");
    let occupied = dir.join("occupied");
    std::fs::create_dir_all(&occupied).unwrap();
    std::fs::write(occupied.join("keep.txt"), "mine").unwrap();
    let out = rascal(&[
        "run",
        "seam",
        repo_root()
            .join("fixtures/images/step-pano.png")
            .to_str()
            .unwrap(),
        "--out",
        occupied.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(!occupied.join("run.json").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A signal mid-fill (the slow fake sleeps three seconds): the run is `cancelled`, the exit
/// code is the shell's (143 for SIGTERM, 130 for SIGINT), and `run.json` names what it reached.
#[test]
fn a_signal_cancels_the_run_and_says_so() {
    let dir = scratch("signal");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let slow = config(&dir, "slow");
    for (signal, code) in [("TERM", 143), ("INT", 130)] {
        let run = dir.join(format!("run-{signal}"));
        let mut child = Command::new(env!("CARGO_BIN_EXE_rascal"))
            .args([
                "heal",
                "seam",
                pano.to_str().unwrap(),
                "--filler",
                "fake",
                "--config",
                slow.to_str().unwrap(),
                "--out",
                run.to_str().unwrap(),
                "--no-dash",
            ])
            .env_remove("RASCAL_LOG")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        // Wait until the fill is under way: the mask has been rendered.
        let started = std::time::Instant::now();
        while !run.join("seam-grey.png").exists() {
            assert!(
                started.elapsed().as_secs() < 30,
                "the run never reached the fill"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let kill = Command::new("kill")
            .args([format!("-{signal}"), child.id().to_string()])
            .status()
            .unwrap();
        assert!(kill.success());
        let status = child.wait().unwrap();
        assert_eq!(status.code(), Some(code), "SIG{signal}");
        let r = run_json(&run);
        assert_eq!(r["status"], "cancelled", "SIG{signal}");
        assert_eq!(r["error"], serde_json::Value::Null);
        assert_eq!(r["body"]["signal"], format!("SIG{signal}"));
        assert_eq!(r["images"]["in"].as_array().unwrap().len(), 1);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Task 8.6: with `[cache] dir`, a repeated heal is answered from the cache: `cache: hit`,
/// no spend, the filler not called; the first paid the filler's price (`cache: miss`).
#[test]
fn a_repeated_heal_is_a_cache_hit_that_spends_nothing() {
    let dir = scratch("cache");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let config = dir.join("priced.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[cache]\ndir = {:?}\n[fillers.fake]\ncommand = [{:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\nrevision = \"fake\"\nspend_usd_per_call = 0.04\n",
            dir.join("fills").to_str().unwrap(),
            env!("CARGO_BIN_EXE_rascal")
        ),
    )
    .unwrap();
    let heal = |name: &str| {
        let out = rascal(&[
            "heal",
            "seam",
            pano.to_str().unwrap(),
            "--filler",
            "fake",
            "--config",
            config.to_str().unwrap(),
            "--out",
            dir.join(name).to_str().unwrap(),
            "--no-dash",
        ]);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        run_json(&dir.join(name))
    };
    let first = heal("first");
    assert_eq!(first["cache"], "miss");
    assert_eq!(first["spend"]["usd"], 0.04);
    let second = heal("second");
    assert_eq!(second["cache"], "hit");
    assert_eq!(second["spend"]["usd"], 0.0);
    assert_eq!(second["input_identity"], first["input_identity"]);
    let journal = std::fs::read_to_string(dir.join("second/journal.jsonl")).unwrap();
    assert!(journal.contains("fill from the cache"), "{journal}");
    assert!(
        !journal.contains("filler call: "),
        "the filler was not called"
    );
    // The healed images are the same pixels.
    assert_eq!(
        second["images"]["out"][1]["image"]["file_sha256"],
        first["images"]["out"][1]["image"]["file_sha256"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}
