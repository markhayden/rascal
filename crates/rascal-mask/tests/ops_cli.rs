//! Task 1.4: `rascal ops list` and `rascal ops describe`, and the exit codes for `invalid_args`
//! and `not_applicable` with and without `--strict`, driven through recipe files.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn rascal(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .output()
        .unwrap()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-opscli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn journal(dir: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir.join("journal.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn ops_list_names_the_catalogue_and_json_gives_every_manifest() {
    let out = rascal(&["ops", "list"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for op in [
        "rect",
        "rasterize",
        "render",
        "cut",
        "put",
        "line",
        "stroke",
    ] {
        assert!(text.lines().any(|l| l.starts_with(op)), "{text}");
    }
    let out = rascal(&["ops", "list", "--json"]);
    let manifests: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    // Every op listed in text is a manifest in JSON, once each, and nothing else.
    let listed: std::collections::BTreeSet<&str> = text
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    let names: std::collections::BTreeSet<&str> = manifests
        .iter()
        .filter_map(|m| m["name"].as_str())
        .collect();
    assert_eq!(listed, names);
    assert_eq!(names.len(), manifests.len(), "no duplicate manifests");
    for m in &manifests {
        for field in [
            "name",
            "version",
            "purpose",
            "when_to_use",
            "when_not_to_use",
            "output_guidance",
            "cost",
            "inputs",
            "outputs",
            "params",
            "returns",
            "input_schema",
            "output_schema",
        ] {
            assert!(!m[field].is_null(), "{} lacks {field}", m["name"]);
        }
    }
}

#[test]
fn ops_describe_prints_the_full_manifest() {
    let out = rascal(&["ops", "describe", "rasterize", "--json"]);
    assert!(out.status.success());
    let m: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(m["name"], "rasterize");
    assert_eq!(m["inputs"][0]["name"], "location");
    assert_eq!(m["inputs"][0]["artifact_type"], "Location");
    assert_eq!(m["outputs"][0]["artifact_type"], "Mask");
    assert_eq!(m["params"][0]["name"], "use_case");
    assert_eq!(m["params"][0]["choices"][0], "seam");
    assert_eq!(
        m["input_schema"]["properties"]["use_case"]["enum"][4],
        "custom"
    );
    assert_eq!(m["returns"]["max_regions"], 64);
    let out = rascal(&["ops", "describe", "rasterize"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("when to use:") && text.contains("location (Location, required)"),
        "{text}"
    );
    let out = rascal(&["ops", "describe", "nope"]);
    assert_eq!(out.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown op nope"));
}

#[test]
fn invalid_args_exits_64_with_and_without_strict_and_is_in_the_journal() {
    let dir = scratch("invalid");
    let floor = repo_root().join("fixtures/images/floor.png");
    let recipe = repo_root().join("fixtures/recipes/bad-box.toml");
    for (name, strict) in [("a", false), ("b", true)] {
        let out_dir = dir.join(name);
        let mut args = vec![
            "run",
            recipe.to_str().unwrap(),
            floor.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
        ];
        if strict {
            args.push("--strict");
        }
        let out = rascal(&args);
        assert_eq!(
            out.status.code(),
            Some(64),
            "strict {strict}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stderr).contains("x0 < x1"));
        let lines = journal(&out_dir);
        assert_eq!(lines.len(), 2, "import, find: {lines:?}");
        assert_eq!(lines[1]["envelope"]["status"], "invalid_args");
        assert_eq!(lines[1]["step"], "find");
        assert!(
            !out_dir.join("artifacts").exists()
                || std::fs::read_dir(out_dir.join("artifacts"))
                    .unwrap()
                    .count()
                    == 0
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn not_applicable_exits_1_under_strict_and_0_without_and_names_the_requirement() {
    let dir = scratch("na");
    let floor = repo_root().join("fixtures/images/floor.png");
    let recipe = repo_root().join("fixtures/recipes/outside-box.toml");
    for (name, strict, code) in [("a", false, 0), ("b", true, 1)] {
        let out_dir = dir.join(name);
        let mut args = vec![
            "run",
            recipe.to_str().unwrap(),
            floor.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
        ];
        if strict {
            args.push("--strict");
        }
        let out = rascal(&args);
        assert_eq!(
            out.status.code(),
            Some(code),
            "strict {strict}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("step find reported not_applicable"), "{err}");
        assert!(
            err.contains("step mask requires find.location, but step find did not produce it"),
            "{err}"
        );
        let lines = journal(&out_dir);
        // The import, one op line for find, one note for the blocked requirement, nothing for mask.
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert_eq!(lines[1]["envelope"]["status"], "not_applicable");
        assert_eq!(lines[2]["event"], "note");
        assert!(
            lines[2]["note"]
                .as_str()
                .unwrap()
                .contains("(not_applicable: "),
            "{}",
            lines[2]["note"]
        );
        assert!(
            !out_dir.join("artifacts").exists()
                || std::fs::read_dir(out_dir.join("artifacts"))
                    .unwrap()
                    .count()
                    == 0
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Task 2.1's CLI scenario: `rascal fan --workspace ws/ --image <canvas> --line loc --json`
/// prints the profile summary; on the step-sky canvas the sky rows widen and the floor stays
/// at thin.
#[test]
fn fan_on_the_step_sky_canvas_prints_its_summary_and_opens_only_on_the_sky() {
    let dir = scratch("fan");
    let ws = dir.join("ws");
    let sky = repo_root().join("fixtures/images/step-sky.png");
    let out = rascal(&[
        "workspace",
        "import",
        ws.to_str().unwrap(),
        sky.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = rascal(&[
        "line",
        "--workspace",
        ws.to_str().unwrap(),
        "--image",
        "img-1",
        "--column",
        "0.5",
        "--width_px",
        "2",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let line_id = stdout
        .lines()
        .find_map(|l| l.strip_prefix("location: "))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    let out = rascal(&[
        "fan",
        "--workspace",
        ws.to_str().unwrap(),
        "--image",
        "img-1",
        "--line",
        &line_id,
        "--json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(stdout.lines().any(|l| l.starts_with("band: ")), "{stdout}");
    let json = &stdout[stdout.find('{').unwrap()..=stdout.rfind('}').unwrap()];
    let envelope: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(envelope["status"], "ok");
    let summary = envelope["summary"].as_str().unwrap();
    assert!(
        summary.contains("thin")
            && summary.contains("widest half")
            && summary.contains("rows widened"),
        "{summary}"
    );
    let measure = |name: &str| {
        envelope["measurements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["name"] == name)
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    let thin = measure("thin_px");
    assert!(measure("half_max_px") > thin + 5.0, "the sky opens");
    let widened = measure("rows_widened");
    // 300 rows, the top three fifths sky: the widened rows are a sky-sized share, not all.
    assert!(
        widened > 120.0 && widened < 230.0,
        "rows widened: {widened}"
    );
    // The journal records the resolved tunables and the set.
    let lines = journal(&ws);
    let fan = lines.iter().find(|l| l["op"] == "fan").unwrap();
    assert_eq!(fan["params"]["constants"], "fan-v1");
    assert_eq!(fan["params"]["wide_max"], 0.15);
    assert_eq!(fan["params"]["gate"], true);
    let _ = std::fs::remove_dir_all(&dir);
}
