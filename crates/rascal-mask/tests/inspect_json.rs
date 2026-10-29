//! Task 1.3: `rascal inspect <mask> --json` prints the record, zone statistics and identity; a
//! hand-edited plane is refused on load with the invariant named.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn rascal(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .output()
        .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-inspect-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_mask(dir: &Path) -> PathBuf {
    let floor = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/images/floor.png");
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
    std::fs::read_dir(run.join("artifacts"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "mask"))
        .unwrap()
}

#[test]
fn inspect_json_prints_record_statistics_and_identity() {
    let dir = scratch("json");
    let mask = make_mask(&dir);
    let out = rascal(&["inspect", mask.to_str().unwrap(), "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["record"]["frame"], "raw:320x200:flat");
    assert_eq!(value["record"]["width"], 320);
    assert_eq!(value["zones"]["core"], 64 * 60);
    assert_eq!(value["zones"]["blend"], 0);
    assert_eq!(value["zones"]["protect"], 0);
    assert_eq!(value["zones"]["painted"], 64 * 60);
    assert_eq!(
        value["zones"]["support"],
        serde_json::json!([64, 40, 128, 100])
    );
    assert_eq!(value["identity"], value["record"]["identity"]);
    assert!(value["identity"].as_str().unwrap().starts_with("sha256:"));
    assert!(
        mask.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with(&value["identity"].as_str().unwrap()[7..])
    );
    assert!(value["profile"].is_null());
    // Without a mode, a usage error names the three.
    let out = rascal(&["inspect", mask.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--json, --out <dir> or --replay"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hand_edited_plane_is_refused_with_the_invariant_named() {
    let dir = scratch("edited");
    let mask = make_mask(&dir);
    // A nonzero blend inside the core.
    let mut blend = image::open(mask.join("blend.png")).unwrap().to_luma8();
    blend.put_pixel(70, 50, image::Luma([128]));
    blend.save(mask.join("blend.png")).unwrap();
    let out = rascal(&["inspect", mask.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("invariant broken: blend is zero inside the core"),
        "{err}"
    );
    // A non-binary core.
    blend.put_pixel(70, 50, image::Luma([0]));
    blend.save(mask.join("blend.png")).unwrap();
    let mut core = image::open(mask.join("core.png")).unwrap().to_luma8();
    core.put_pixel(10, 10, image::Luma([7]));
    core.save(mask.join("core.png")).unwrap();
    let out = rascal(&["inspect", mask.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("core is binary"));
    // A core edited back to binary but different from what the identity says.
    core.put_pixel(10, 10, image::Luma([255]));
    core.save(mask.join("core.png")).unwrap();
    let out = rascal(&["inspect", mask.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("stored identity matches the content"));
    let _ = std::fs::remove_dir_all(&dir);
}
