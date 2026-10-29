//! Task 5.1: the real Python pack through rascal's client. `rascal tools list` shows
//! `echo_region` with its two `_meta` documents accepted by the Rust side; `rascal tools call
//! rascal-tools echo_region --image i --bbox …` round-trips the box as one region and one
//! plane; the pack sees the allowlisted environment only (its `echo_env` debug tool says so).
//!
//! The pack is launched from its own virtual environment (`tools/python/.venv`, made by `uv
//! sync --frozen`). When that environment is absent the tests print why and pass, so
//! `cargo test` stays green on a machine without the pack; CI syncs it first.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pack_binary() -> Option<PathBuf> {
    let path = repo_root().join("tools/python/.venv/bin/rascal-tools");
    if path.is_file() {
        Some(path)
    } else {
        assert!(
            std::env::var_os("CI").is_none(),
            "{} is absent in CI; uv sync --frozen --project tools/python first",
            path.display()
        );
        eprintln!(
            "packs_real: {} is absent (run `uv sync --frozen` in tools/python); skipping",
            path.display()
        );
        None
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-packs-real-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rascal_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command.args(args).env_remove("RASCAL_LOG");
    for (k, v) in env {
        command.env(k, v);
    }
    command.output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn first_json(stdout: &str) -> serde_json::Value {
    let from = stdout.find('{').unwrap();
    serde_json::Deserializer::from_str(&stdout[from..])
        .into_iter::<serde_json::Value>()
        .next()
        .unwrap()
        .unwrap()
}

fn config(dir: &Path, binary: &Path, debug: bool) -> PathBuf {
    let path = dir.join("rascal.toml");
    let flag = if debug { ", \"--debug-tools\"" } else { "" };
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[fillers.hosted]\nurl = \"https://example.invalid/x\"\nprofile = \"binary\"\nkey_env = \"HOSTED_FILLER_KEY\"\n[packs.rascal-tools]\ncommand = [{:?}, \"serve\"{flag}]\nenv = [\"RASCAL_TOOLS_MODELS\", \"HOSTED_FILLER_KEY\"]\nrevision = \"rascal-tools 0.1.0\"\nstartup_timeout_s = 60\ncall_timeout_s = 60\n",
            binary.display().to_string()
        ),
    )
    .unwrap();
    path
}

#[test]
fn the_real_pack_lists_echo_region_and_round_trips_a_box() {
    let Some(binary) = pack_binary() else { return };
    let dir = scratch("echo");
    let cfg = config(&dir, &binary, false);
    let out = rascal_env(
        &["tools", "list", "--config", cfg.to_str().unwrap(), "--json"],
        &[],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let listed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let tools = listed["rascal-tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1, "{listed}");
    assert_eq!(tools[0]["name"], "echo_region");
    assert_eq!(tools[0]["outputs"][0]["many"], true);
    assert_eq!(tools[0]["params"][0]["kind"], "number_array");
    let ws = dir.join("ws");
    let floor = repo_root().join("fixtures/images/floor.png");
    let out = rascal_env(
        &[
            "tools",
            "call",
            "rascal-tools",
            "echo_region",
            "--config",
            cfg.to_str().unwrap(),
            "--workspace",
            ws.to_str().unwrap(),
            "--image",
            floor.to_str().unwrap(),
            "--bbox",
            "0.25,0.5,0.75,1",
            "--json",
        ],
        &[],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains("region_0:") && stdout.contains("plane_0:"),
        "{stdout}"
    );
    let envelope = first_json(&stdout);
    assert_eq!(envelope["status"], "ok");
    assert_eq!(
        envelope["regions"][0]["location"]["bbox"],
        serde_json::json!([0.25, 0.5, 0.75, 1.0])
    );
    assert_eq!(envelope["regions"][0]["label"], "echo");
    assert_eq!(envelope["images"].as_array().unwrap().len(), 1);
    assert!(
        envelope["images"][0]["identity"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    // The plane is the box: a pixel inside is set, one outside is not (320x200 floor).
    let rows: Vec<serde_json::Value> = serde_json::from_slice(
        &rascal_env(
            &[
                "workspace",
                "list",
                ws.to_str().unwrap(),
                "--type",
                "Image",
                "--json",
            ],
            &[],
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(rows.len(), 2, "the canonical image and the plane");
    let plane_id = stdout
        .lines()
        .find_map(|l| l.strip_prefix("plane_0: "))
        .unwrap()
        .split(' ')
        .next()
        .unwrap();
    let identity = rows.iter().find(|r| r["id"] == plane_id).unwrap()["identity"]
        .as_str()
        .unwrap()
        .to_string();
    let plane = image::open(
        ws.join("artifacts")
            .join(format!("{}.png", identity.trim_start_matches("sha256:"))),
    )
    .unwrap()
    .to_luma8();
    assert_eq!(plane.get_pixel(160, 150)[0], 255);
    assert_eq!(plane.get_pixel(40, 150)[0], 0);
    // A bad box is invalid_args from the pack, as an envelope.
    let out = rascal_env(
        &[
            "tools",
            "call",
            "rascal-tools",
            "echo_region",
            "--config",
            cfg.to_str().unwrap(),
            "--workspace",
            ws.to_str().unwrap(),
            "--image",
            "img-1",
            "--bbox",
            "0.9,0.9,0.1,0.1",
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(64), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("x0 < x1"),
        "{}",
        text(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_real_pack_sees_the_allowlisted_environment_only() {
    let Some(binary) = pack_binary() else { return };
    let dir = scratch("env");
    let cfg = config(&dir, &binary, true);
    let ws = dir.join("ws");
    let floor = repo_root().join("fixtures/images/floor.png");
    let out = rascal_env(
        &[
            "tools",
            "call",
            "rascal-tools",
            "echo_env",
            "--config",
            cfg.to_str().unwrap(),
            "--workspace",
            ws.to_str().unwrap(),
            "--image",
            floor.to_str().unwrap(),
            "--json",
        ],
        &[
            ("RASCAL_TOOLS_MODELS", "/models"),
            ("HOSTED_FILLER_KEY", "a-fillers-key"),
            ("UNDECLARED", "x"),
            ("SOME_SECRET", "y"),
        ],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let envelope = first_json(&text(&out.stdout));
    let summary = envelope["summary"].as_str().unwrap();
    let names: Vec<&str> = summary.strip_prefix("env: ").unwrap().split(',').collect();
    assert!(
        names.contains(&"RASCAL_TOOLS_MODELS") && names.contains(&"PATH"),
        "{names:?}"
    );
    for gone in ["HOSTED_FILLER_KEY", "UNDECLARED", "SOME_SECRET"] {
        assert!(!names.contains(&gone), "{gone} reached the pack: {names:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
