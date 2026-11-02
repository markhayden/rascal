//! Task 10.5 from the command line: `rascal workspace export-mask`, `import-mask` and
//! `import-location`. A seam mask made by `rascal run seam` leaves its run directory as a bundle
//! and enters a fresh workspace with the same identity; a PNG is refused with exit 64 and
//! nothing imported; a location file registers against a named image (ADR 0022). And task
//! 10.10's `rascal export --json` and `rascal import --json` (CONTRACT 5.1): the tools' documents
//! on stdout, exit 0, or 64 with the refusal JSON on stdout.
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

/// A contract location on `image` (the shape `--location` takes, 8.2).
fn contract_box(image: &Path, file_sha256: &str, bbox: [f64; 4]) -> String {
    let (w, h) = image::image_dimensions(image).unwrap();
    serde_json::json!({
        "image": { "file_sha256": file_sha256, "width": w, "height": h,
                   "projection": "flat", "topology": "bounded" },
        "kind": "bbox",
        "parts": [{ "bbox": bbox }],
        "straddles_wrap": false
    })
    .to_string()
}

#[test]
fn a_location_imports_from_json_against_a_named_image() {
    let dir = scratch("location");
    let ws = dir.join("ws");
    let floor = repo_root().join("fixtures/images/floor.png");
    let printed = ok(&["workspace", "import", s(&ws), s(&floor)]);
    let digest = printed
        .trim()
        .rsplit_once("file_sha256:")
        .unwrap()
        .1
        .to_string();
    let chairs = repo_root().join("fixtures/locations/chairs.json");

    // rascal's own location: registered in the image's frame; the same file twice, one id.
    let one = ok(&[
        "workspace",
        "import-location",
        s(&ws),
        s(&chairs),
        "--image",
        "img-1",
    ]);
    let (id, rest) = one.trim().split_once(' ').unwrap();
    assert!(
        id.starts_with("loc-") && rest.contains("raw:320x200:flat"),
        "{one}"
    );
    let two = ok(&[
        "workspace",
        "import-location",
        s(&ws),
        s(&chairs),
        "--image",
        "img-1",
    ]);
    assert_eq!(one, two);

    // A table-manners location on the same file registers too.
    let tm = dir.join("tm.json");
    std::fs::write(&tm, contract_box(&floor, &digest, [0.1, 0.1, 0.4, 0.5])).unwrap();
    let three = ok(&[
        "workspace",
        "import-location",
        s(&ws),
        s(&tm),
        "--image",
        "img-1",
    ]);
    assert!(three.starts_with("loc-") && three != one, "{three}");

    let before = std::fs::read_to_string(ws.join("CURRENT")).unwrap();
    let refuse = |file: &Path, says: &str| {
        let out = rascal(&[
            "workspace",
            "import-location",
            s(&ws),
            s(file),
            "--image",
            "img-1",
        ]);
        assert_eq!(out.status.code(), Some(64), "{says}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains(says), "{err}\nshould say: {says}");
    };
    // On another file.
    let other = dir.join("other.json");
    std::fs::write(
        &other,
        contract_box(&floor, &"0".repeat(64), [0.1, 0.1, 0.4, 0.5]),
    )
    .unwrap();
    refuse(&other, "on another image");
    // Naming another frame.
    let framed = dir.join("framed.json");
    std::fs::write(
        &framed,
        r#"{ "kind": "bbox", "frame": "raw:9x9:flat", "bbox": [0.1, 0.1, 0.2, 0.2] }"#,
    )
    .unwrap();
    refuse(&framed, "names frame raw:9x9:flat");
    // Bad geometry.
    let inverted = dir.join("inverted.json");
    std::fs::write(
        &inverted,
        r#"{ "kind": "bbox", "bbox": [0.5, 0.1, 0.2, 0.2] }"#,
    )
    .unwrap();
    refuse(&inverted, "need x0 < x1");
    assert_eq!(
        std::fs::read_to_string(ws.join("CURRENT")).unwrap(),
        before,
        "nothing published by a refusal"
    );
    // Straddling the wrap edge of a panorama (the seam rule).
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let pano_id = ok(&["workspace", "import", s(&ws), s(&pano)])
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    let before = std::fs::read_to_string(ws.join("CURRENT")).unwrap();
    let straddling = dir.join("straddling.json");
    std::fs::write(
        &straddling,
        r#"{ "kind": "bbox", "bbox": [-0.1, 0.1, 0.1, 0.2] }"#,
    )
    .unwrap();
    let out = rascal(&[
        "workspace",
        "import-location",
        s(&ws),
        s(&straddling),
        "--image",
        &pano_id,
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).contains("seam rule"));
    // A PNG is not a location.
    refuse(&floor, "not valid JSON");
    // An id that is not an image.
    let out = rascal(&[
        "workspace",
        "import-location",
        s(&ws),
        s(&chairs),
        "--image",
        id,
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert_eq!(std::fs::read_to_string(ws.join("CURRENT")).unwrap(), before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_refused_export_leaves_no_file_behind() {
    let dir = scratch("noout");
    let run = dir.join("run");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    ok(&["run", "seam", s(&pano), "--out", s(&run)]);
    let mask = last_mask(&run);
    let out_dir = dir.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    // Not a Mask: 64, and no --out appears.
    let bundle = out_dir.join("x.mask.tar");
    let out = rascal(&[
        "workspace",
        "export-mask",
        s(&run),
        "img-1",
        "--out",
        s(&bundle),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(!bundle.exists(), "nothing written for a refused export");
    // An --out that is a directory: 64, the directory as it was, no partial file beside it.
    let occupied = out_dir.join("occupied");
    std::fs::create_dir_all(&occupied).unwrap();
    let out = rascal(&[
        "workspace",
        "export-mask",
        s(&run),
        &mask,
        "--out",
        s(&occupied),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("is a directory"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read_dir(&occupied).unwrap().count(), 0);
    let names: Vec<String> = std::fs::read_dir(&out_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["occupied"], "nothing left beside it");
    // A written bundle replaces the file whole; no temporary is left beside it.
    ok(&[
        "workspace",
        "export-mask",
        s(&run),
        &mask,
        "--out",
        s(&bundle),
    ]);
    let mut names: Vec<String> = std::fs::read_dir(&out_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["occupied", "x.mask.tar"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_mask_whose_image_is_missing_names_it_and_works_once_it_is_imported() {
    let dir = scratch("noimage");
    let run = dir.join("run");
    let floor = repo_root().join("fixtures/images/floor.png");
    ok(&[
        "run",
        "rect",
        s(&floor),
        "--bbox",
        "0.2,0.2,0.5,0.6",
        "--out",
        s(&run),
    ]);
    let mask = last_mask(&run);
    let bundle = dir.join("box.mask.tar");
    ok(&[
        "workspace",
        "export-mask",
        s(&run),
        &mask,
        "--out",
        s(&bundle),
    ]);
    // The file digest `workspace import` prints for this image, from a throwaway workspace.
    let printed = ok(&["workspace", "import", s(&dir.join("probe")), s(&floor)]);
    let digest = printed
        .trim()
        .rsplit_once("file_sha256:")
        .unwrap()
        .1
        .to_string();

    // A fresh workspace gets the mask and nothing else.
    let fresh = dir.join("fresh");
    let imported = ok(&["workspace", "import-mask", s(&fresh), s(&bundle)]);
    let id = imported.split_whitespace().next().unwrap().to_string();
    let before = std::fs::read_to_string(fresh.join("CURRENT")).unwrap();

    // An op that needs the pixels: refused, naming the image by its file digest and the remedy.
    let out = rascal(&[
        "window",
        "--workspace",
        s(&fresh),
        "--image",
        "img-1",
        "--mask",
        &id,
    ]);
    assert_eq!(out.status.code(), Some(64));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains(&format!("sha256 {digest}")), "{err}");
    assert!(
        err.contains(&format!("rascal workspace import {} <path>", s(&fresh))),
        "{err}"
    );
    assert_eq!(
        std::fs::read_to_string(fresh.join("CURRENT")).unwrap(),
        before,
        "nothing published by the refusal"
    );

    // The image goes in by its own path; the mask's raw frame is the one it lands in.
    let printed = ok(&["workspace", "import", s(&fresh), s(&floor)]);
    let fields: Vec<&str> = printed.split_whitespace().collect();
    let (image, frame) = (fields[0].to_string(), fields[1].to_string());
    assert_eq!(
        frame,
        imported.split_whitespace().nth(1).unwrap(),
        "{printed}"
    );
    let listed = ok(&["workspace", "list", s(&fresh), "--type", "Frame", "--json"]);
    let frames: Vec<serde_json::Value> = serde_json::from_str(&listed).unwrap();
    assert_eq!(frames.len(), 1, "one raw frame, the mask's: {listed}");

    // The same mask id now applies, with no second import of the mask.
    let done = ok(&[
        "window",
        "--workspace",
        s(&fresh),
        "--image",
        &image,
        "--mask",
        &id,
    ]);
    assert!(done.contains("committed"), "{done}");
    let masks = ok(&["workspace", "list", s(&fresh), "--type", "Mask", "--json"]);
    let masks: Vec<serde_json::Value> = serde_json::from_str(&masks).unwrap();
    assert_eq!(masks.len(), 2, "the imported mask and window's");
    assert_eq!(masks[0]["id"], id.as_str());
    let _ = std::fs::remove_dir_all(&dir);
}

/// `rascal <args>` with `stdin` piped in and the data home `data`.
fn piped(args: &[&str], stdin: &str, data: &Path) -> Output {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .env("XDG_DATA_HOME", data)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn document(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

#[test]
fn export_and_import_json_move_a_mask_between_stores() {
    let dir = std::fs::canonicalize(scratch("transfer")).unwrap();
    let data = dir.join("data");
    let a = dir.join("a");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    ok(&["run", "seam", s(&pano), "--out", s(&a)]);
    let store = rascal_core::workspace::Workspace::open(&a).unwrap();
    let mask = last_mask(&a);
    let reference = rascal_core::contract::artifact_ref(&store, &mask).unwrap();
    drop(store);
    let request = serde_json::json!({ "artifact": reference }).to_string();

    let out = piped(
        &["export", "--json", "-", "--workspace", s(&a)],
        &request,
        &data,
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let exported = document(&out);
    let path = exported["file"]["path"].as_str().unwrap().to_string();
    assert!(path.starts_with(s(&data.join("rascal/exports"))), "{path}");
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(
        exported["file"]["sha256"],
        rascal_core::contract::file_sha256(&bytes)
    );
    // From a file, the same document.
    let file = dir.join("export.json");
    std::fs::write(&file, &request).unwrap();
    let again = rascal_with(&["export", "--json", s(&file), "--workspace", s(&a)], &data);
    assert_eq!(document(&again), exported);

    // Into a new store: new, then already present, the same reference.
    let b = dir.join("b");
    let import = serde_json::json!({ "file": { "path": path } }).to_string();
    let exports = data.join("rascal/exports");
    let args = [
        "import",
        "--json",
        "-",
        "--workspace",
        s(&b),
        "--allow-dir",
        s(&exports),
    ];
    let first = piped(&args, &import, &data);
    assert_eq!(
        first.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first = document(&first);
    assert_eq!(first["already_present"], false);
    assert_eq!(first["artifact"], exported["artifact"]);
    let second = document(&piped(&args, &import, &data));
    assert_eq!(second["already_present"], true);
    assert_eq!(second["artifact"], exported["artifact"]);

    // Without --allow-dir every path is refused: 64, the refusal on stdout.
    let refused = piped(
        &["import", "--json", "-", "--workspace", s(&b)],
        &import,
        &data,
    );
    assert_eq!(refused.status.code(), Some(64));
    let refusal = document(&refused);
    assert_eq!(refusal["reason"], "invalid_arguments");
    assert_eq!(refusal["error"]["code"], "input");

    // A PNG: 64, the refusal naming what it is, nothing recorded.
    let before = std::fs::read_to_string(b.join("CURRENT")).unwrap();
    let png = repo_root().join("fixtures/images/floor.png");
    let canonical_png = std::fs::canonicalize(&png).unwrap();
    let out = piped(
        &[
            "import",
            "--json",
            "-",
            "--workspace",
            s(&b),
            "--allow-dir",
            s(canonical_png.parent().unwrap()),
        ],
        &serde_json::json!({ "file": { "path": s(&canonical_png) } }).to_string(),
        &data,
    );
    assert_eq!(out.status.code(), Some(64));
    let refusal = document(&out);
    assert_eq!(refusal["reason"], "invalid_arguments");
    assert!(
        refusal["error"]["reason"].as_str().unwrap().contains("PNG"),
        "{refusal:#}"
    );
    assert_eq!(std::fs::read_to_string(b.join("CURRENT")).unwrap(), before);

    // An unresolved export: the binding refusal, 64.
    let empty = dir.join("empty");
    let out = piped(
        &["export", "--json", "-", "--workspace", s(&empty)],
        &request,
        &data,
    );
    assert_eq!(out.status.code(), Some(64));
    let refusal = document(&out);
    assert_eq!(refusal["reason"], "binding");
    assert_eq!(refusal["bindings"][0]["port"], "artifact");
    assert_eq!(refusal["bindings"][0]["problem"], "unresolved");

    // A request that is not JSON: 64, a usage refusal.
    let out = piped(&["export", "--json", "-", "--workspace", s(&a)], "{", &data);
    assert_eq!(out.status.code(), Some(64));
    assert_eq!(document(&out)["error"]["code"], "usage");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn import_json_names_the_image_a_location_needs() {
    let dir = std::fs::canonicalize(scratch("needs-image")).unwrap();
    let floor = repo_root().join("fixtures/images/floor.png");
    let digest = rascal_core::contract::file_sha256(&std::fs::read(&floor).unwrap());
    let location = contract_box(&floor, &digest, [0.1, 0.1, 0.4, 0.5]);
    let inline = serde_json::json!({ "inline": {
        "media_type": "application/json",
        "data": rascal_testkit::fake_pack::base64_encode(location.as_bytes()),
    }})
    .to_string();
    let ws = dir.join("ws");
    let args = ["import", "--json", "-", "--workspace", s(&ws)];
    let out = piped(&args, &inline, &dir.join("data"));
    assert_eq!(out.status.code(), Some(64));
    let reason = document(&out)["error"]["reason"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(reason.contains(&digest), "{reason}");
    assert!(
        reason.contains("pass that image to any action first"),
        "{reason}"
    );
    // Once the store holds the image, the same request records the location.
    ok(&["workspace", "import", s(&ws), s(&floor)]);
    let out = piped(&args, &inline, &dir.join("data"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let imported = document(&out);
    assert_eq!(imported["artifact"]["type"], "location");
    assert_eq!(imported["artifact"]["on"]["file_sha256"], digest.as_str());
    let _ = std::fs::remove_dir_all(&dir);
}

fn rascal_with(args: &[&str], data: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .env("XDG_DATA_HOME", data)
        .output()
        .unwrap()
}
