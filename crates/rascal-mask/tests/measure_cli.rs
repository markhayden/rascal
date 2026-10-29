//! Task 2.6's CLI scenario: `rascal measure --before a.png --after b.png --mask m --set
//! continuity,drift --json` prints summaries; on a flat image `continuity` reports
//! measured_nothing with rows_skipped = rows.
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
    let dir = std::env::temp_dir().join(format!("rascal-measure-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn measure_prints_summaries_and_reports_measured_nothing_on_a_flat_image() {
    let dir = scratch("cli");
    let floor = repo_root().join("fixtures/images/floor.png");
    let run = dir.join("run");
    let out = rascal(&[
        "run",
        "rect",
        floor.to_str().unwrap(),
        "--bbox",
        "0.4,0.1,0.6,0.9",
        "--out",
        run.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mask_dir = String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("mask: ").map(PathBuf::from))
        .unwrap();
    // An "after" that differs from the floor inside the box only.
    let base = image::open(&floor).unwrap().to_rgb8();
    let core = image::open(mask_dir.join("core.png")).unwrap().to_luma8();
    let after = image::RgbImage::from_fn(base.width(), base.height(), |x, y| {
        let p = *base.get_pixel(x, y);
        if core.get_pixel(x, y)[0] != 0 {
            image::Rgb([p[0] / 2, p[1] / 2, p[2] / 2])
        } else {
            p
        }
    });
    let after_path = dir.join("after.png");
    after.save(&after_path).unwrap();
    let out = rascal(&[
        "measure",
        "--before",
        floor.to_str().unwrap(),
        "--after",
        after_path.to_str().unwrap(),
        "--mask",
        mask_dir.to_str().unwrap(),
        "--set",
        "drift",
        "--json",
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(envelope["status"], "ok");
    let measurement = |name: &str| {
        envelope["measurements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["name"] == name)
            .unwrap_or_else(|| panic!("{name}"))["value"]
            .as_f64()
            .unwrap()
    };
    assert_eq!(measurement("drift.outside.r"), 0.0);
    assert!(measurement("drift.core.r") > 20.0);
    // A rect mask has no profile and no line: continuity is not applicable; on a flat image
    // with a band mask, continuity measures nothing with rows_skipped = rows.
    let flat_path = dir.join("flat.png");
    image::RgbImage::from_pixel(320, 200, image::Rgb([90, 90, 90]))
        .save(&flat_path)
        .unwrap();
    let out = rascal(&[
        "measure",
        "--after",
        flat_path.to_str().unwrap(),
        "--mask",
        mask_dir.to_str().unwrap(),
        "--set",
        "continuity",
        "--json",
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(envelope["status"], "not_applicable", "{envelope}");
    // Through the band: import the flat image, make the line, the band, then measure.
    let ws = dir.join("ws");
    let out = rascal(&[
        "workspace",
        "import",
        ws.to_str().unwrap(),
        flat_path.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    let port = |out: &Output, name: &str| -> String {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|l| {
                l.strip_prefix(&format!("{name}: "))
                    .map(|r| r.split_whitespace().next().unwrap().to_string())
            })
            .unwrap()
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
    // The band mask's directory, by its identity, measured through the file form.
    let out = rascal(&["workspace", "list", w, "--type", "Mask", "--json"]);
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    let identity = rows.iter().find(|r| r["id"] == mask_id).unwrap()["identity"]
        .as_str()
        .unwrap()
        .to_string();
    let band_dir = ws
        .join("artifacts")
        .join(format!("{}.mask", identity.trim_start_matches("sha256:")));
    let out = rascal(&[
        "measure",
        "--after",
        flat_path.to_str().unwrap(),
        "--mask",
        band_dir.to_str().unwrap(),
        "--set",
        "continuity",
        "--json",
        "--strict",
    ]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(envelope["status"], "measured_nothing", "{envelope}");
    let skipped = envelope["measurements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == "continuity.rows_skipped")
        .unwrap()["value"]
        .as_f64()
        .unwrap();
    assert_eq!(skipped, 200.0, "rows_skipped = rows");
    // --list names the measures, in the documented order.
    let out = rascal(&["measure", "--list"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "continuity\ntone_step\ntransition\ntexture_match\ndrift"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
