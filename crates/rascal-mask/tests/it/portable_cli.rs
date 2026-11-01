//! Task 10.5 from the command line: `rascal workspace export-mask`, `import-mask` and
//! `import-location`. A seam mask made by `rascal run seam` leaves its run directory as a bundle
//! and enters a fresh workspace with the same identity; a PNG is refused with exit 64 and
//! nothing imported; a location file registers against a named image (ADR 0022).
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn rascal(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .output()
        .unwrap()
}

fn ok(args: &[&str]) -> String {
    let out = rascal(args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-portable-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// The id of the one Mask `workspace list --type Mask` shows last.
fn last_mask(ws: &Path) -> String {
    let listed = ok(&["workspace", "list", s(ws), "--type", "Mask", "--json"]);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&listed).unwrap();
    rows.last().unwrap()["id"].as_str().unwrap().to_string()
}

fn inspect_identity(mask_dir: &Path) -> String {
    let report: serde_json::Value =
        serde_json::from_str(&ok(&["inspect", s(mask_dir), "--json"])).unwrap();
    report["identity"].as_str().unwrap().to_string()
}

fn mask_dir(ws: &Path, id: &str) -> PathBuf {
    let listed = ok(&["workspace", "list", s(ws), "--json"]);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&listed).unwrap();
    let identity = rows.iter().find(|r| r["id"] == id).unwrap()["identity"]
        .as_str()
        .unwrap()
        .to_string();
    ws.join("artifacts")
        .join(format!("{}.mask", identity.trim_start_matches("sha256:")))
}

#[test]
fn a_seam_mask_moves_to_a_fresh_workspace_with_its_identity() {
    let dir = scratch("seam");
    let run = dir.join("run");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    ok(&["run", "seam", s(&pano), "--out", s(&run)]);
    let mask = last_mask(&run);
    let bundle = dir.join("seam.mask.tar");
    let exported = ok(&[
        "workspace",
        "export-mask",
        s(&run),
        &mask,
        "--out",
        s(&bundle),
    ]);
    let (identity, digest) = exported.trim().split_once(' ').unwrap();
    assert!(
        identity.starts_with("sha256:") && digest.starts_with("sha256:"),
        "{exported}"
    );
    // An --out that exists is written over, as `cp` would.
    let again = ok(&[
        "workspace",
        "export-mask",
        s(&run),
        &mask,
        "--out",
        s(&bundle),
    ]);
    assert_eq!(again, exported, "the same mask, the same bundle");

    let fresh = dir.join("fresh");
    let imported = ok(&[
        "workspace",
        "import-mask",
        s(&fresh),
        s(&bundle),
        "--sha256",
        digest,
    ]);
    let fields: Vec<&str> = imported.split_whitespace().collect();
    assert_eq!(fields.len(), 3, "{imported}");
    assert!(fields[1].starts_with("cut:"), "{imported}");
    assert_eq!(fields[2], identity);
    assert_eq!(inspect_identity(&mask_dir(&fresh, fields[0])), identity);
    assert_eq!(inspect_identity(&mask_dir(&run, &mask)), identity);
    // Twice: the same id, already present.
    let twice = ok(&["workspace", "import-mask", s(&fresh), s(&bundle)]);
    assert!(
        twice.starts_with(fields[0]) && twice.contains("already present"),
        "{twice}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_png_or_a_damaged_bundle_is_refused_with_64_and_nothing_imported() {
    let dir = scratch("refused");
    let run = dir.join("run");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    ok(&["run", "seam", s(&pano), "--out", s(&run)]);
    let fresh = dir.join("fresh");
    let floor = repo_root().join("fixtures/images/floor.png");
    ok(&["workspace", "import", s(&fresh), s(&floor)]);
    let before = std::fs::read_to_string(fresh.join("CURRENT")).unwrap();

    let grey = run.join("seam-grey.png");
    assert!(grey.exists());
    let out = rascal(&["workspace", "import-mask", s(&fresh), s(&grey)]);
    assert_eq!(out.status.code(), Some(64));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("not a mask bundle: it is a PNG image"),
        "{err}"
    );

    let bundle = dir.join("seam.mask.tar");
    ok(&[
        "workspace",
        "export-mask",
        s(&run),
        &last_mask(&run),
        "--out",
        s(&bundle),
    ]);
    let mut bytes = std::fs::read(&bundle).unwrap();
    let mid = bytes.len() / 3;
    bytes[mid] ^= 0x10;
    std::fs::write(&bundle, &bytes).unwrap();
    let out = rascal(&["workspace", "import-mask", s(&fresh), s(&bundle)]);
    assert_eq!(out.status.code(), Some(64));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("refused") && err.contains("nothing was imported"),
        "{err}"
    );

    let out = rascal(&[
        "workspace",
        "export-mask",
        s(&fresh),
        "img-1",
        "--out",
        s(&bundle),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).contains("only a Mask"));

    assert_eq!(
        std::fs::read_to_string(fresh.join("CURRENT")).unwrap(),
        before
    );
    let _ = std::fs::remove_dir_all(&dir);
}
