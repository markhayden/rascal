//! A filler entry's `overrides` (docs/fillers.md): `heal` applies them to the recipe before
//! it runs, so a filler gets the band it can carry; the journal notes each one and records
//! the resolved step. `rascal run` applies none, and a bad override is refused naming the
//! filler entry before anything runs.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rascal-heal-overrides-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rascal(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .env_remove("RASCAL_LOG")
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn config(dir: &Path, overrides: &str) -> PathBuf {
    let path = dir.join("rascal.toml");
    let bin = env!("CARGO_BIN_EXE_rascal");
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\nrevision = \"fake exact\"\n{overrides}"
        ),
    )
    .unwrap();
    path
}

fn journal(dir: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir.join("journal.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn fan_step(lines: &[serde_json::Value]) -> &serde_json::Value {
    lines
        .iter()
        .find(|l| l["step"] == "fan" && l["event"] == "op")
        .unwrap()
}

#[test]
fn the_filler_entry_chooses_the_band_and_the_journal_says_so() {
    let dir = scratch("band");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let plain = config(&dir, "");
    std::fs::create_dir_all(dir.join("thin")).unwrap();
    let thin = config(
        &dir.join("thin"),
        "overrides = { \"fan.constants\" = \"fan-thin-v1\", \"fan.wide_max\" = 0.02 }\n",
    );
    for (name, cfg) in [("plain", &plain), ("thin", &thin)] {
        let out = dir.join(format!("run-{name}"));
        let result = rascal(&[
            "heal",
            "seam",
            pano.to_str().unwrap(),
            "--filler",
            "fake",
            "--config",
            cfg.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--no-dash",
        ]);
        assert_eq!(
            result.status.code(),
            Some(0),
            "{name}: {}",
            text(&result.stderr)
        );
    }
    let plain_lines = journal(&dir.join("run-plain"));
    let thin_lines = journal(&dir.join("run-thin"));
    assert_eq!(fan_step(&plain_lines)["params"]["constants"], "fan-v3");
    assert_eq!(fan_step(&thin_lines)["params"]["constants"], "fan-thin-v1");
    assert_eq!(fan_step(&thin_lines)["params"]["wide_max"], 0.02);
    let notes: Vec<String> = thin_lines
        .iter()
        .filter(|l| l["event"] == "note")
        .filter_map(|l| l["note"].as_str().map(String::from))
        .collect();
    assert!(
        notes
            .iter()
            .any(|n| n == "override from fillers.fake: fan.constants = \"fan-thin-v1\""),
        "{notes:?}"
    );
    assert!(
        notes
            .iter()
            .any(|n| n == "override from fillers.fake: fan.wide_max = 0.02"),
        "{notes:?}"
    );
    assert!(
        !plain_lines.iter().any(|l| l["note"]
            .as_str()
            .is_some_and(|n| n.starts_with("override from"))),
        "no override note without overrides"
    );
    // --set wins over the entry's overrides, and both are journaled.
    let out = dir.join("run-set");
    let result = rascal(&[
        "heal",
        "seam",
        pano.to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        thin.to_str().unwrap(),
        "--set",
        "fan.constants=fan-v1",
        "--out",
        out.to_str().unwrap(),
        "--no-dash",
    ]);
    assert_eq!(result.status.code(), Some(0), "{}", text(&result.stderr));
    let set_lines = journal(&out);
    assert_eq!(fan_step(&set_lines)["params"]["constants"], "fan-v1");
    assert_eq!(
        fan_step(&set_lines)["params"]["wide_max"],
        0.02,
        "the entry's other override stays"
    );
    let notes: Vec<String> = set_lines
        .iter()
        .filter(|l| l["event"] == "note")
        .filter_map(|l| l["note"].as_str().map(String::from))
        .collect();
    assert!(
        notes
            .iter()
            .any(|n| n == "override from --set: fan.constants = \"fan-v1\""),
        "{notes:?}"
    );
    assert!(
        notes
            .iter()
            .any(|n| n.starts_with("override from fillers.fake: fan.constants")),
        "{notes:?}"
    );
    // `rascal run` with the same config applies nothing: the fan runs the recipe's fan-v3.
    let out = dir.join("run-run");
    let result = rascal(&[
        "run",
        "seam",
        pano.to_str().unwrap(),
        "--config",
        thin.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0), "{}", text(&result.stderr));
    assert_eq!(fan_step(&journal(&out))["params"]["constants"], "fan-v3");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bad_override_is_refused_naming_the_entry_before_anything_runs() {
    let dir = scratch("bad");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    for (overrides, expect) in [
        (
            "overrides = { \"fan.bogus\" = 1 }\n",
            "fillers.fake: override fan.bogus: fan has no parameter bogus",
        ),
        (
            "overrides = { \"nope.px\" = 1 }\n",
            "fillers.fake: override nope.px: no step nope in the recipe",
        ),
        (
            "overrides = { \"fan.constants\" = \"fan-v9\" }\n",
            "fillers.fake: override fan.constants: fan has no constant set fan-v9",
        ),
    ] {
        let cfg = config(&dir, overrides);
        let out = dir.join("run");
        let _ = std::fs::remove_dir_all(&out);
        let result = rascal(&[
            "heal",
            "seam",
            pano.to_str().unwrap(),
            "--filler",
            "fake",
            "--config",
            cfg.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--no-dash",
        ]);
        assert_eq!(
            result.status.code(),
            Some(64),
            "{overrides}: {}",
            text(&result.stderr)
        );
        assert!(
            text(&result.stderr).contains(expect),
            "{overrides}: {}",
            text(&result.stderr)
        );
        assert!(
            !out.join("result.png").exists(),
            "{overrides}: nothing was healed"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
