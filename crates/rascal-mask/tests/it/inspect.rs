//! Task 1.9 from the command line: the overlay and chart pages, the export of a step and a
//! location, replay on an untouched, a relocated and a tampered run.
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
    let dir = std::env::temp_dir().join(format!("rascal-inspect2-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rect_run(dir: &Path) -> (PathBuf, PathBuf) {
    let floor = repo_root().join("fixtures/images/floor.png");
    let run = dir.join("run");
    let out = rascal(&[
        "run",
        "rect",
        floor.to_str().unwrap(),
        "--bbox",
        "0.2,0.2,0.4,0.5",
        "--out",
        run.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mask = std::fs::read_dir(run.join("artifacts"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "mask"))
        .unwrap();
    (run, mask)
}

#[test]
fn inspect_out_writes_the_overlay_the_chart_and_the_json() {
    let dir = scratch("pages");
    let (_, mask) = rect_run(&dir);
    let pages = dir.join("pages");
    let out = rascal(&[
        "inspect",
        mask.to_str().unwrap(),
        "--out",
        pages.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let overlay = image::open(pages.join("overlay.jpg")).unwrap().to_rgb8();
    assert_eq!(overlay.dimensions(), (320, 200));
    let svg = std::fs::read_to_string(pages.join("chart.svg")).unwrap();
    assert!(svg.starts_with("<svg") && svg.contains("core width per row"));
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(pages.join("inspect.json")).unwrap())
            .unwrap();
    assert_eq!(json["zones"]["core"], 64 * 60);
    // A second time into the same directory is refused: a run directory must be new or empty.
    let out = rascal(&[
        "inspect",
        mask.to_str().unwrap(),
        "--out",
        pages.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    // No mode at all is a usage error that says which to give.
    let out = rascal(&["inspect", mask.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--json, --out <dir> or --replay"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_compat_validates_a_step_and_a_location_and_refuses_a_line() {
    let dir = scratch("export");
    let (run, _) = rect_run(&dir);
    let out_file = dir.join("e.json");
    let out = rascal(&[
        "export",
        "--compat",
        run.join("journal.jsonl").to_str().unwrap(),
        "--step",
        "rasterize",
        "--out",
        out_file.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let exported: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_file).unwrap()).unwrap();
    assert_eq!(exported["status"], "ok");
    assert!(exported.get("artifacts").is_none());
    assert_eq!(exported["provenance"]["tool"], "rasterize");
    let out = rascal(&[
        "export",
        "--compat",
        run.join("journal.jsonl").to_str().unwrap(),
        "--step",
        "nope",
        "--out",
        out_file.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    let loc = dir.join("box.json");
    std::fs::write(
        &loc,
        r#"{"kind":"bbox","frame":"raw:320x200:flat","bbox":[0.2,0.2,0.4,0.5]}"#,
    )
    .unwrap();
    let out = rascal(&[
        "export",
        "--compat",
        "--location",
        loc.to_str().unwrap(),
        "--out",
        out_file.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let exported: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_file).unwrap()).unwrap();
    assert_eq!(exported["kind"], "bbox");
    std::fs::write(
        &loc,
        r#"{"kind":"line","frame":"raw:320x200:flat","column":0.5,"width_px":3}"#,
    )
    .unwrap();
    let out = rascal(&[
        "export",
        "--compat",
        "--location",
        loc.to_str().unwrap(),
        "--out",
        out_file.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).contains("a line location has no copied form"));
    let out = rascal(&[
        "export",
        run.join("journal.jsonl").to_str().unwrap(),
        "--step",
        "rasterize",
        "--out",
        out_file.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64), "without --compat");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn replay_is_empty_untouched_and_relocated_and_names_a_tampered_plane() {
    let dir = scratch("replay");
    let (run, mask) = rect_run(&dir);
    let out = rascal(&["inspect", mask.to_str().unwrap(), "--replay"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["differing_planes"], serde_json::json!({}));
    assert_eq!(report["op"], "rasterize");
    // cp -r run/ elsewhere/ and replay there, the original gone.
    let elsewhere = dir.join("elsewhere");
    let status = Command::new("cp")
        .args(["-R", run.to_str().unwrap(), elsewhere.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::remove_dir_all(&run).unwrap();
    let moved_mask = elsewhere.join("artifacts").join(mask.file_name().unwrap());
    let out = rascal(&["inspect", moved_mask.to_str().unwrap(), "--replay"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Tamper with the protect plane: the replay names it and exits 3.
    let protect = moved_mask.join("protect.png");
    let mut plane = image::open(&protect).unwrap().to_luma8();
    plane.put_pixel(5, 5, image::Luma([255]));
    plane.save(&protect).unwrap();
    let out = rascal(&["inspect", moved_mask.to_str().unwrap(), "--replay"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["differing_planes"]["protect"], 1);
    assert!(String::from_utf8_lossy(&out.stderr).contains("protect"));
    let _ = std::fs::remove_dir_all(&dir);
}
