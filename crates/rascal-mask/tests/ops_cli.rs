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

/// Task 2.2's CLI scenario: `rascal profile --band <profile> --workspace ws/` then
/// `rascal inspect` shows the core at the half-width and the blend fading outside it.
#[test]
fn profile_after_fan_writes_the_band_mask_that_inspect_describes() {
    let dir = scratch("profile");
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
    let port = |out: &std::process::Output, name: &str| -> String {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|l| {
                l.strip_prefix(&format!("{name}: "))
                    .map(|r| r.split_whitespace().next().unwrap().to_string())
            })
            .unwrap_or_else(|| panic!("no {name} in {}", String::from_utf8_lossy(&out.stdout)))
    };
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
    let line_id = port(&out, "location");
    let out = rascal(&[
        "fan",
        "--workspace",
        ws.to_str().unwrap(),
        "--image",
        "img-1",
        "--line",
        &line_id,
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let band_id = port(&out, "band");
    let out = rascal(&[
        "profile",
        "--workspace",
        ws.to_str().unwrap(),
        "--band",
        &band_id,
        "--json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mask_id = port(&out, "mask");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let json = &stdout[stdout.find('{').unwrap()..=stdout.rfind('}').unwrap()];
    let envelope: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(envelope["status"], "ok");
    // The mask directory, by its id through workspace list.
    let out = rascal(&[
        "workspace",
        "list",
        ws.to_str().unwrap(),
        "--type",
        "Mask",
        "--json",
    ]);
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    let identity = rows.iter().find(|r| r["id"] == mask_id).unwrap()["identity"]
        .as_str()
        .unwrap()
        .to_string();
    let mask_dir = ws
        .join("artifacts")
        .join(format!("{}.mask", identity.trim_start_matches("sha256:")));
    let out = rascal(&["inspect", mask_dir.to_str().unwrap(), "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["record"]["use_case"], "seam");
    assert!(
        report["profile"].is_object(),
        "the profile is stored: {}",
        report["profile"]
    );
    assert_eq!(report["profile"]["rows"], 300);
    let core = report["zones"]["core"].as_u64().unwrap();
    let blend = report["zones"]["blend"].as_u64().unwrap();
    assert!(
        core > 300 * 6,
        "the core is at least thin wide on every row: {core}"
    );
    assert!(
        blend > core / 4,
        "the blend fades outside the core: {blend} vs {core}"
    );
    // The overlay and chart pages render.
    let pages = dir.join("pages");
    let out = rascal(&[
        "inspect",
        mask_dir.to_str().unwrap(),
        "--out",
        pages.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(pages.join("overlay.jpg").exists() && pages.join("chart.svg").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Task 2.3's CLI scenario: `rascal ramp --image <canvas> --mask <band>` then `rascal apply`
/// yields a canvas whose two sides meet in colour at the line on the step-sky fixture;
/// pixels beyond the half-width and protected pixels are byte-identical.
#[test]
fn ramp_then_apply_meets_the_sides_of_the_step_sky_canvas_at_the_line() {
    let dir = scratch("ramp");
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
    let port = |out: &std::process::Output, name: &str| -> String {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|l| {
                l.strip_prefix(&format!("{name}: "))
                    .map(|r| r.split_whitespace().next().unwrap().to_string())
            })
            .unwrap_or_else(|| panic!("no {name} in {}", String::from_utf8_lossy(&out.stdout)))
    };
    let w = ws.to_str().unwrap();
    let out = rascal(&[
        "line",
        "--workspace",
        w,
        "--image",
        "img-1",
        "--column",
        "0.5",
        "--width_px",
        "2",
    ]);
    let line_id = port(&out, "location");
    let out = rascal(&[
        "fan",
        "--workspace",
        w,
        "--image",
        "img-1",
        "--line",
        &line_id,
    ]);
    let band_id = port(&out, "band");
    let out = rascal(&["profile", "--workspace", w, "--band", &band_id]);
    let mask_id = port(&out, "mask");
    let out = rascal(&[
        "ramp",
        "--workspace",
        w,
        "--image",
        "img-1",
        "--mask",
        &mask_id,
        "--json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ramped = port(&out, "mask");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let json = &stdout[stdout.find('{').unwrap()..=stdout.rfind('}').unwrap()];
    let envelope: serde_json::Value = serde_json::from_str(json).unwrap();
    let step_max = envelope["measurements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == "step_max_levels")
        .unwrap()["value"]
        .as_f64()
        .unwrap();
    // The sky rows carry the fixture's 10-level step; the floor rows' planks give the step
    // windows larger local differences, so the maximum over rows is at least the sky's.
    assert!(step_max >= 9.0, "the fixture's 10-level step: {step_max}");
    let canvas = dir.join("canvas.png");
    let out = rascal(&[
        "apply",
        "--workspace",
        w,
        "--image",
        "img-1",
        "--mask",
        &ramped,
        "--out",
        canvas.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let before = image::open(&sky).unwrap().to_rgb8();
    let after = image::open(&canvas).unwrap().to_rgb8();
    let (width, height) = before.dimensions();
    assert_eq!(after.dimensions(), (width, height));
    // On the sky rows the two sides meet at the line (the fixture's step is 10 levels).
    for y in [20, 60, 100, 140] {
        let l = after.get_pixel(width / 2 - 1, y);
        let r = after.get_pixel(width / 2, y);
        let bl = before.get_pixel(width / 2 - 1, y);
        let br = before.get_pixel(width / 2, y);
        for ch in 0..3 {
            assert!(
                bl[ch].abs_diff(br[ch]) >= 9,
                "the step is there before: {bl:?} {br:?}"
            );
            assert!(
                l[ch].abs_diff(r[ch]) <= 2,
                "row {y} ch {ch}: {l:?} vs {r:?}"
            );
        }
    }
    // Beyond the band (the widest half is 0.15 · width = 60 px) every pixel is byte-identical.
    let reach = (0.15 * f64::from(width)).ceil() as u32 + 1;
    for y in 0..height {
        for x in (0..width / 2 - reach).chain(width / 2 + reach..width) {
            assert_eq!(
                after.get_pixel(x, y),
                before.get_pixel(x, y),
                "{x},{y} beyond the band"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
