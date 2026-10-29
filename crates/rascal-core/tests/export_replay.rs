//! Task 1.9: the compatibility export's projection and its two refusals, and replay.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use rascal_core::export;
use rascal_core::frame::{Frame, FrameTable, Plan, Projection};
use rascal_core::location::Location;
use rascal_core::op::{Envelope, EnvelopeImage, Inputs, Params, Region};
use rascal_core::replay;
use rascal_core::schemas::SchemaDir;
use rascal_core::workspace::Workspace;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn schemas() -> SchemaDir {
    SchemaDir::new(repo_root().join("schemas"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-exp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A workspace with the floor imported and the rect recipe's two steps run.
fn rect_run(name: &str) -> (PathBuf, Workspace, String) {
    let dir = scratch(name);
    let ws = Workspace::open(&dir).unwrap();
    let (image_id, frame, _) = ws
        .import(&repo_root().join("fixtures/images/floor.png"), None)
        .unwrap();
    let registry = rascal_ops::registry().unwrap();
    let (loc_id, _) = ws
        .add_location(Location::bbox(&frame.id, [0.2, 0.2, 0.4, 0.5]).unwrap())
        .unwrap();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), ws.load(&image_id).unwrap());
    inputs.insert("location".into(), ws.load(&loc_id).unwrap());
    let invocation = ws
        .invoke("mask", "rasterize", &registry, &inputs, &Params::new())
        .unwrap();
    let mask_id = invocation.ids["mask"].clone();
    (dir, ws, mask_id)
}

#[test]
fn a_real_rect_envelope_projects_and_validates() {
    let (dir, ws, _) = rect_run("export-ok");
    let records = ws.records().unwrap();
    let rasterize = records.iter().find(|r| r.op == "rasterize").unwrap();
    let projected = export::project_envelope(&rasterize.envelope, &ws.frames().unwrap()).unwrap();
    export::validate_envelope(&schemas(), &projected).unwrap();
    assert_eq!(projected["status"], "ok");
    assert!(
        projected.get("artifacts").is_none(),
        "native bookkeeping is dropped"
    );
    assert_eq!(projected["provenance"]["tool"], "rasterize");
    assert!(projected["regions"].as_array().unwrap().is_empty());
    // The import envelope too.
    let import = records.iter().find(|r| r.op == "import").unwrap();
    let projected = export::project_envelope(&import.envelope, &ws.frames().unwrap()).unwrap();
    export::validate_envelope(&schemas(), &projected).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bbox_location_exports_standalone_and_line_and_stroke_are_refused() {
    let frame = Frame::raw(320, 200, Projection::Flat);
    let bbox = Location::bbox(&frame.id, [0.2, 0.2, 0.4, 0.5]).unwrap();
    let projected = export::project_location(&bbox).unwrap();
    export::validate_location(&schemas(), &projected).unwrap();
    assert_eq!(projected["kind"], "bbox");
    for location in [
        Location::Polygon {
            frame: frame.id.clone(),
            polygon: vec![[0.1, 0.1], [0.5, 0.1], [0.3, 0.4]],
        },
        Location::WholeImage {
            frame: frame.id.clone(),
        },
        Location::Absent {
            frame: frame.id.clone(),
        },
    ] {
        export::validate_location(&schemas(), &export::project_location(&location).unwrap())
            .unwrap();
    }
    let line = Location::Line {
        frame: frame.id.clone(),
        column: Some(0.5),
        path: None,
        width_px: 3.0,
        group: None,
    };
    let e = export::project_location(&line).unwrap_err().to_string();
    assert!(e.contains("a line location has no copied form"), "{e}");
    let stroke = Location::Stroke {
        frame: frame.id.clone(),
        path: vec![[0.1, 0.1], [0.2, 0.2]],
        radius_px: 2.0,
        group: None,
    };
    assert!(
        export::project_location(&stroke)
            .unwrap_err()
            .to_string()
            .contains("stroke")
    );
}

#[test]
fn a_line_region_and_a_cut_image_are_refused_naming_them() {
    let frame = Frame::raw(320, 200, Projection::Flat);
    let frames = FrameTable {
        frames: vec![frame.clone()],
    };
    let mut envelope = Envelope::ok("with a line region");
    envelope.provenance = rascal_core::op::Provenance {
        tool: "t".into(),
        version: "1.0.0".into(),
        duration_ms: 1,
    };
    envelope.regions.push(Region {
        id: "r1".into(),
        location: Location::Line {
            frame: frame.id.clone(),
            column: Some(0.5),
            path: None,
            width_px: 3.0,
            group: None,
        },
        label: "seam".into(),
        score: 1.0,
    });
    let e = export::project_envelope(&envelope, &frames)
        .unwrap_err()
        .to_string();
    assert!(e.contains("region r1 is a line location"), "{e}");
    // A bbox region projects fine.
    envelope.regions[0].location = Location::bbox(&frame.id, [0.1, 0.1, 0.2, 0.2]).unwrap();
    let projected = export::project_envelope(&envelope, &frames).unwrap();
    export::validate_envelope(&schemas(), &projected).unwrap();
    assert_eq!(projected["regions"][0]["location"]["kind"], "bbox");
    // An image in a cut frame cannot map to its parent as a bbox.
    let pano = Frame::raw(4000, 2000, Projection::Equirectangular);
    let plan = Plan::new(&pano, 0.07).unwrap();
    let cut = Frame::cut(&pano, "img", &plan).unwrap();
    let frames = FrameTable {
        frames: vec![pano.clone(), cut.clone()],
    };
    let mut envelope = Envelope::ok("with a cut image");
    envelope.provenance = rascal_core::op::Provenance {
        tool: "t".into(),
        version: "1.0.0".into(),
        duration_ms: 1,
    };
    envelope.images.push(EnvelopeImage {
        frame: cut.id.clone(),
        parent: pano.id.clone(),
        width: 560,
        height: 2000,
        media_type: "image/png".into(),
        identity: "sha256:0000000000000000000000000000000000000000000000000000000000000001".into(),
    });
    let e = export::project_envelope(&envelope, &frames)
        .unwrap_err()
        .to_string();
    assert!(e.contains("a cut frame whose mapping"), "{e}");
    // A crop image maps to its bbox and validates.
    let flat = Frame::raw(320, 200, Projection::Flat);
    let crop = Frame::crop(&flat, 1, [64, 40, 128, 100]).unwrap();
    let frames = FrameTable {
        frames: vec![flat.clone(), crop.clone()],
    };
    envelope.images[0] = EnvelopeImage {
        frame: crop.id.clone(),
        parent: flat.id.clone(),
        width: 64,
        height: 60,
        media_type: "image/png".into(),
        identity: "sha256:0000000000000000000000000000000000000000000000000000000000000001".into(),
    };
    let projected = export::project_envelope(&envelope, &frames).unwrap();
    export::validate_envelope(&schemas(), &projected).unwrap();
    assert_eq!(
        projected["images"][0]["mapping"]["bbox"],
        serde_json::json!([0.2, 0.2, 0.4, 0.5])
    );
}

#[test]
fn replay_is_empty_on_an_untouched_run_names_a_tampered_plane_and_survives_relocation() {
    let (dir, ws, mask_id) = rect_run("replay");
    let registry = rascal_ops::registry().unwrap();
    let entry = ws.snapshot().unwrap().artifact(&mask_id).unwrap().clone();
    let mask_dir = dir.join(&entry.path);
    let stored = replay::read_untrusted(&mask_dir).unwrap();
    let report = replay::replay_mask(&ws, &registry, &stored).unwrap();
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(report.op, "rasterize");
    assert_eq!(report.stored_identity, report.rebuilt_identity);
    drop(ws);
    // Copied elsewhere, with the original inputs gone, it replays the same.
    let elsewhere = scratch("relocated");
    let _ = std::fs::remove_dir_all(&elsewhere);
    copy_tree(&dir, &elsewhere);
    std::fs::remove_dir_all(&dir).unwrap();
    let moved = Workspace::open(&elsewhere).unwrap();
    let moved_mask = replay::read_untrusted(&elsewhere.join(&entry.path)).unwrap();
    let report = replay::replay_mask(&moved, &registry, &moved_mask).unwrap();
    assert!(report.is_clean(), "{report:?}");
    // A tampered core plane is named with the count of differing pixels.
    let core_path = elsewhere.join(&entry.path).join("core.png");
    let mut core = image::open(&core_path).unwrap().to_luma8();
    core.put_pixel(10, 10, image::Luma([255]));
    core.put_pixel(11, 10, image::Luma([255]));
    core.save(&core_path).unwrap();
    let tampered = replay::read_untrusted(&elsewhere.join(&entry.path)).unwrap();
    let report = replay::replay_mask(&moved, &registry, &tampered).unwrap();
    assert!(!report.is_clean());
    assert_eq!(report.differing_planes.get("core"), Some(&2));
    assert!(!report.differing_planes.contains_key("blend"));
    assert_eq!(report.status_summary(), "core differs on 2 pixels");
    // The planes restored and the record edited instead: the identity string in mask.toml is
    // untouched, so only recomputing it tells, and the report says the record differs.
    stored.core.save(&core_path).unwrap();
    let toml_path = elsewhere.join(&entry.path).join("mask.toml");
    let text = std::fs::read_to_string(&toml_path).unwrap();
    assert!(text.contains("use_case = \"custom\""), "{text}");
    std::fs::write(
        &toml_path,
        text.replace("use_case = \"custom\"", "use_case = \"seam\""),
    )
    .unwrap();
    let edited = replay::read_untrusted(&elsewhere.join(&entry.path)).unwrap();
    let report = replay::replay_mask(&moved, &registry, &edited).unwrap();
    assert!(report.record_differs, "{report:?}");
    assert!(report.differing_planes.is_empty(), "{report:?}");
    assert!(!report.is_clean());
    let _ = std::fs::remove_dir_all(&elsewhere);
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

trait Summary {
    fn status_summary(&self) -> String;
}
impl Summary for replay::ReplayReport {
    fn status_summary(&self) -> String {
        self.differing_planes
            .iter()
            .map(|(p, n)| format!("{p} differs on {n} pixels"))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

#[test]
fn a_mask_nobody_produced_has_no_record() {
    let (dir, ws, _) = rect_run("norecord");
    let e = replay::record_for(
        &ws,
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("no committed record produced it"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}
