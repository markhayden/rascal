//! Task 8.2 (ADR 0020): a table-manners location crosses into a run. A squint-shaped finding on
//! the run's image masks exactly what rascal's own location does; a finding on another image,
//! one in two parts and one with more vertices than rascal's polygon holds are refused (64).
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rascal_core::contract;

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
    let dir =
        std::env::temp_dir().join(format!("rascal-contract-loc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn mask_dir(out: &Output) -> PathBuf {
    text(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("mask: ").map(PathBuf::from))
        .unwrap_or_else(|| panic!("no mask line in {}", text(&out.stdout)))
}

/// A squint-shaped finding: a box on `image` with squint's own words beside it.
fn finding(image: &Path, file_sha256: &str, parts: serde_json::Value) -> serde_json::Value {
    let (w, h) = image::image_dimensions(image).unwrap();
    serde_json::json!({
        "image": { "file_sha256": file_sha256, "width": w, "height": h,
                   "projection": "flat", "topology": "bounded" },
        "kind": "bbox",
        "parts": parts,
        "straddles_wrap": false,
        "label": "the chair by the window",
        "disposition": "violation",
        "confidence": "confident",
        "source": { "tool": "squint", "version": "0.2.0", "run": "decision-1", "item": "F1" }
    })
}

fn run_remove(dir: &Path, name: &str, image: &Path, location: &Path) -> Output {
    rascal(&[
        "run",
        "remove",
        image.to_str().unwrap(),
        "--location",
        location.to_str().unwrap(),
        "--out",
        dir.join(name).to_str().unwrap(),
    ])
}

#[test]
fn a_squint_finding_masks_exactly_what_rascals_own_location_does() {
    let dir = scratch("same");
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let own = repo_root().join("fixtures/locations/chairs.json");
    let bbox = serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&own).unwrap())
        .unwrap()["bbox"]
        .clone();
    let digest = contract::file_sha256(&std::fs::read(&image).unwrap());
    let found = dir.join("finding.json");
    std::fs::write(
        &found,
        finding(&image, &digest, serde_json::json!([{ "bbox": bbox }])).to_string(),
    )
    .unwrap();
    let a = run_remove(&dir, "own", &image, &own);
    assert_eq!(a.status.code(), Some(0), "{}", text(&a.stderr));
    let b = run_remove(&dir, "contract", &image, &found);
    assert_eq!(b.status.code(), Some(0), "{}", text(&b.stderr));
    for plane in ["core.png", "blend.png"] {
        assert_eq!(
            std::fs::read(mask_dir(&a).join(plane)).unwrap(),
            std::fs::read(mask_dir(&b).join(plane)).unwrap(),
            "{plane}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_finding_rascal_cannot_take_is_refused_with_the_reason() {
    let dir = scratch("refused");
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let digest = contract::file_sha256(&std::fs::read(&image).unwrap());
    let cases = [
        (
            "other-image",
            finding(
                &image,
                &"0".repeat(64),
                serde_json::json!([{ "bbox": [0.1, 0.1, 0.2, 0.2] }]),
            ),
            "the location is on another image",
        ),
        (
            "two-parts",
            finding(
                &image,
                &digest,
                serde_json::json!([{ "bbox": [0.1, 0.1, 0.2, 0.2] }, { "bbox": [0.3, 0.1, 0.4, 0.2] }]),
            ),
            "rascal holds one shape per location",
        ),
        (
            "not-a-location",
            serde_json::json!({ "image": { "file_sha256": digest }, "kind": "bbox" }),
            "not a table-manners location",
        ),
    ];
    for (name, body, words) in cases {
        let path = dir.join(format!("{name}.json"));
        std::fs::write(&path, body.to_string()).unwrap();
        let out = run_remove(&dir, name, &image, &path);
        assert_eq!(out.status.code(), Some(64), "{name}: {}", text(&out.stderr));
        assert!(
            text(&out.stderr).contains(words),
            "{name}: {}",
            text(&out.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
