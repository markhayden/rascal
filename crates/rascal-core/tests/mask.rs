//! Task 1.3: the mask record's invariants on all three planes, the effective weight
//! exhaustively, identity's inclusions and exclusions, the refinement postconditions, and the
//! round trip through a directory.
#![allow(clippy::unwrap_used)]

use image::{GrayImage, Luma};
use rascal_core::Error;
use rascal_core::frame::{Frame, Projection};
use rascal_core::mask::{self, LoadedPrescription, Mask, ProvenanceNode, Source, UseCase};
use rascal_core::profile::{BandProfile, Windows};
use rascal_core::series::Series;

fn frame() -> Frame {
    Frame::raw(40, 30, Projection::Flat)
}

fn rect_core(w: u32, h: u32, x0: u32, y0: u32, x1: u32, y1: u32) -> GrayImage {
    GrayImage::from_fn(w, h, |x, y| {
        if x >= x0 && x < x1 && y >= y0 && y < y1 {
            Luma([255])
        } else {
            Luma([0])
        }
    })
}

fn rect_mask() -> Mask {
    Mask::from_core(
        "m",
        &frame(),
        UseCase::Custom,
        rect_core(40, 30, 10, 10, 20, 20),
        Source::default(),
    )
    .unwrap()
}

fn band(frame: &Frame, line: &str, thin: f64) -> BandProfile {
    BandProfile::new(
        "band",
        &frame.id,
        &frame.identity,
        line,
        20,
        thin,
        vec![thin + 1.0; frame.height as usize],
        Windows {
            gap_px: 1,
            step_cols: 3,
            texture_cols: 8,
        },
        "fan-v1",
    )
    .unwrap()
}

fn sigma(frame: &Frame, value: f64) -> Series {
    Series::new(
        "sigma",
        "sigma_px",
        "px",
        &frame.id,
        &frame.identity,
        1,
        vec![value; frame.height as usize],
    )
    .unwrap()
}

fn step(frame: &Frame, value: f64) -> Series {
    Series::new(
        "step",
        "tone_ramp.step",
        "levels",
        &frame.id,
        &frame.identity,
        3,
        vec![value; 3 * frame.height as usize],
    )
    .unwrap()
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-mask-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

// --- effective weight -----------------------------------------------------------------------------

#[test]
fn effective_weight_exhaustively() {
    for blend in 0..=255u8 {
        for core in [0u8, 255] {
            for protect in [0u8, 255] {
                let w = mask::effective_weight(core, blend, protect);
                let expected = if protect != 0 {
                    0.0
                } else if core != 0 {
                    1.0
                } else {
                    f32::from(blend) / 255.0
                };
                assert_eq!(w, expected, "core {core} blend {blend} protect {protect}");
            }
        }
    }
    let f = frame();
    let mut m = rect_mask();
    let mut blend = GrayImage::new(f.width, f.height);
    blend.put_pixel(5, 5, Luma([51]));
    blend.put_pixel(15, 15, Luma([200])); // inside the core: forced to zero
    m.set_blend(blend).unwrap();
    assert_eq!(m.weight(15, 15), 1.0);
    assert_eq!(m.weight(5, 5), 0.2);
    assert_eq!(m.weight(0, 0), 0.0);
    assert_eq!(m.blend.get_pixel(15, 15)[0], 0);
    let mut protect = GrayImage::new(f.width, f.height);
    protect.put_pixel(15, 15, Luma([255]));
    protect.put_pixel(5, 5, Luma([255]));
    m.protect_union(&protect).unwrap();
    assert_eq!(m.weight(15, 15), 0.0);
    assert_eq!(m.weight(5, 5), 0.0);
    assert_eq!(m.core.get_pixel(15, 15)[0], 0, "core cleared under protect");
    assert_eq!(m.blend.get_pixel(5, 5)[0], 0, "blend cleared under protect");
    assert_eq!(m.statistics().protect, 2);
    assert_eq!(m.support(), Some([10, 10, 20, 20]));
}

// --- invariants --------------------------------------------------------------------------------------

fn invariant(result: Result<(), Error>) -> String {
    match result {
        Err(Error::MaskInvariant { invariant, .. }) => invariant,
        other => panic!("expected a mask invariant error, got {other:?}"),
    }
}

#[test]
fn invariants_on_all_three_planes() {
    let mut m = rect_mask();
    m.check().unwrap();
    m.core.put_pixel(1, 1, Luma([9]));
    assert!(invariant(m.check()).starts_with("core is binary"));
    m.core.put_pixel(1, 1, Luma([0]));
    m.protect.put_pixel(1, 1, Luma([1]));
    assert!(invariant(m.check()).starts_with("protect is binary"));
    m.protect.put_pixel(1, 1, Luma([0]));
    m.blend.put_pixel(15, 15, Luma([3]));
    assert!(invariant(m.check()).starts_with("blend is zero inside the core"));
    m.blend.put_pixel(15, 15, Luma([0]));
    m.protect.put_pixel(2, 2, Luma([255]));
    m.blend.put_pixel(2, 2, Luma([3]));
    assert!(invariant(m.check()).starts_with("blend is zero under protect"));
    m.blend.put_pixel(2, 2, Luma([0]));
    m.protect.put_pixel(2, 2, Luma([0]));
    // The content now equals the original, so the identity matches again.
    m.check().unwrap();
    m.core.put_pixel(1, 1, Luma([255]));
    assert_eq!(
        invariant(m.check()),
        "the stored identity matches the content"
    );
    let mut n = rect_mask();
    n.blend = GrayImage::new(40, 29);
    assert_eq!(invariant(n.check()), "the three planes differ in size");
    let mut n = rect_mask();
    n.record.width = 41;
    assert_eq!(invariant(n.check()), "the planes are not the record's size");
    // A core plane of the wrong size is refused at construction.
    assert!(
        Mask::from_core(
            "m",
            &frame(),
            UseCase::Custom,
            GrayImage::new(41, 30),
            Source::default()
        )
        .is_err()
    );
    // Any nonzero core sample becomes 255 at construction.
    let mut soft = GrayImage::new(40, 30);
    soft.put_pixel(3, 3, Luma([17]));
    let m = Mask::from_core("m", &frame(), UseCase::Custom, soft, Source::default()).unwrap();
    assert_eq!(m.core.get_pixel(3, 3)[0], 255);
}

#[test]
fn profile_and_prescription_invariants() {
    let f = frame();
    let other = Frame::raw(40, 31, Projection::Flat);
    let mut m = rect_mask();
    assert!(
        m.set_profile("line-id", band(&other, "line-id", 4.0), sigma(&f, 2.0))
            .is_err(),
        "wrong frame"
    );
    assert!(
        m.set_profile(
            "line-id",
            band(&f, "line-id", 4.0),
            sigma(&Frame::raw(40, 30, Projection::Equirectangular), 2.0)
        )
        .is_err()
    );
    m.set_profile("line-id", band(&f, "line-id", 4.0), sigma(&f, 2.0))
        .unwrap();
    m.check().unwrap();
    assert!(
        m.push_tone_ramp(sigma(&f, 1.0), 0.5).is_err(),
        "needs 3 channels"
    );
    assert!(
        m.push_tone_ramp(step(&f, 1.0), 1.5).is_err(),
        "share in [0, 1]"
    );
    m.push_tone_ramp(step(&f, 1.0), 0.5).unwrap();
    m.check().unwrap();
    assert_eq!(m.prescriptions.len(), 1);
    assert!(
        matches!(&m.prescriptions[0], LoadedPrescription::ToneRamp { share, .. } if *share == 0.5)
    );
    // The series' own invariants.
    assert!(Series::new("s", "x", "px", &f.id, &f.identity, 0, vec![]).is_err());
    assert!(Series::new("s", "x", "px", &f.id, &f.identity, 2, vec![1.0; 3]).is_err());
    assert!(Series::new("s", "x", "px", &f.id, &f.identity, 1, vec![f64::NAN]).is_err());
    assert!(
        Series::with_rows(
            "s",
            "x",
            "px",
            &f.id,
            &f.identity,
            1,
            vec![2, 1],
            vec![1.0, 2.0]
        )
        .is_err()
    );
    assert!(
        BandProfile::new(
            "b",
            &f.id,
            &f.identity,
            "l",
            20,
            0.0,
            vec![1.0; 30],
            Windows {
                gap_px: 1,
                step_cols: 1,
                texture_cols: 1
            },
            "fan-v1"
        )
        .is_err()
    );
    assert!(
        BandProfile::new(
            "b",
            &f.id,
            &f.identity,
            "l",
            20,
            4.0,
            vec![3.0; 30],
            Windows {
                gap_px: 1,
                step_cols: 1,
                texture_cols: 1
            },
            "fan-v1"
        )
        .is_err(),
        "half below thin"
    );
}

// --- identity -----------------------------------------------------------------------------------------

#[test]
fn identity_excludes_ids_filenames_and_provenance_and_includes_references() {
    let f = frame();
    let a = rect_mask();
    let b = rect_mask().with_id("another");
    assert_eq!(
        a.record.identity, b.record.identity,
        "the id is not part of identity"
    );
    let mut c = rect_mask();
    c.append_provenance(ProvenanceNode {
        step: "rasterize".into(),
        op: "rasterize".into(),
        version: "0.1.0".into(),
        params: Default::default(),
        inputs: Default::default(),
        outputs: Default::default(),
        constants: None,
    });
    assert_eq!(
        a.record.identity, c.record.identity,
        "provenance is not part of identity"
    );
    let mut d = rect_mask();
    d.record.zones.core = "elsewhere.png".into();
    assert_eq!(
        a.compute_identity().as_str(),
        d.compute_identity().as_str(),
        "file names are not part of identity"
    );
    // Referenced identities are.
    let mut e = rect_mask();
    e.set_profile("line-1", band(&f, "line-1", 4.0), sigma(&f, 2.0))
        .unwrap();
    let mut g = rect_mask();
    g.set_profile("line-1", band(&f, "line-1", 4.0), sigma(&f, 2.5))
        .unwrap();
    let mut h = rect_mask();
    h.set_profile("line-2", band(&f, "line-1", 4.0), sigma(&f, 2.0))
        .unwrap();
    assert_ne!(a.record.identity, e.record.identity);
    assert_ne!(
        e.record.identity, g.record.identity,
        "a changed sigma series under the same local name changes the mask"
    );
    assert_ne!(
        e.record.identity, h.record.identity,
        "the line identity is part of the profile"
    );
    let mut i = e.clone();
    i.push_tone_ramp(step(&f, 1.0), 0.5).unwrap();
    let mut j = e.clone();
    j.push_tone_ramp(step(&f, 1.1), 0.5).unwrap();
    let mut k = e.clone();
    k.push_tone_ramp(step(&f, 1.0), 0.4).unwrap();
    assert_ne!(i.record.identity, e.record.identity);
    assert_ne!(i.record.identity, j.record.identity, "the series' identity");
    assert_ne!(
        i.record.identity, k.record.identity,
        "the prescription's fields"
    );
    // The source, the use case and the frame.
    let s = Mask::from_core(
        "m",
        &f,
        UseCase::Custom,
        rect_core(40, 30, 10, 10, 20, 20),
        Source {
            locations: vec!["sha256:x".into()],
            instances: vec![],
        },
    )
    .unwrap();
    assert_ne!(a.record.identity, s.record.identity);
    let u = Mask::from_core(
        "m",
        &f,
        UseCase::Repair,
        rect_core(40, 30, 10, 10, 20, 20),
        Source::default(),
    )
    .unwrap();
    assert_ne!(a.record.identity, u.record.identity);
    let v = Mask::from_core(
        "m",
        &Frame::raw(40, 30, Projection::Equirectangular),
        UseCase::Custom,
        rect_core(40, 30, 10, 10, 20, 20),
        Source::default(),
    )
    .unwrap();
    assert_ne!(a.record.identity, v.record.identity);
}

// --- refinement postconditions -----------------------------------------------------------------------------

#[test]
fn replacing_the_core_clears_blend_profile_and_prescriptions_and_keeps_protect() {
    let f = frame();
    let mut m = rect_mask();
    let mut blend = GrayImage::new(40, 30);
    blend.put_pixel(5, 5, Luma([100]));
    m.set_blend(blend).unwrap();
    m.set_profile("line-1", band(&f, "line-1", 4.0), sigma(&f, 2.0))
        .unwrap();
    m.push_tone_ramp(step(&f, 1.0), 0.5).unwrap();
    let mut protect = GrayImage::new(40, 30);
    protect.put_pixel(30, 5, Luma([255]));
    m.protect_union(&protect).unwrap();
    assert!(m.profile.is_some(), "subtract keeps the profile");
    assert_eq!(m.prescriptions.len(), 1, "subtract keeps the prescriptions");

    let mut new_core = rect_core(40, 30, 0, 0, 40, 30);
    new_core.put_pixel(30, 5, Luma([255]));
    let report = m.replace_core(new_core).unwrap();
    assert!(report.blend_cleared && report.profile_cleared);
    assert!(m.blend.pixels().all(|p| p[0] == 0));
    assert!(m.profile.is_none() && m.record.profile.is_none());
    assert!(m.prescriptions.is_empty() && m.record.prescriptions.is_empty());
    assert_eq!(m.protect.get_pixel(30, 5)[0], 255, "protect preserved");
    assert_eq!(m.weight(30, 5), 0.0, "and still winning over the new core");
    m.check().unwrap();
    let report = m.replace_core(rect_core(40, 30, 1, 1, 2, 2)).unwrap();
    assert!(report.blend_cleared && !report.profile_cleared);
}

// --- the directory ---------------------------------------------------------------------------------------

#[test]
fn a_mask_round_trips_through_its_directory_with_profile_and_prescription() {
    let f = frame();
    let mut m = rect_mask();
    let mut blend = GrayImage::new(40, 30);
    blend.put_pixel(5, 5, Luma([100]));
    m.set_blend(blend).unwrap();
    m.set_profile("line-1", band(&f, "line-1", 4.0), sigma(&f, 2.0))
        .unwrap();
    m.push_tone_ramp(step(&f, 1.0), 0.5).unwrap();
    m.append_provenance(ProvenanceNode {
        step: "ramp".into(),
        op: "ramp".into(),
        version: "1.0.0".into(),
        params: [("share".to_string(), serde_json::json!(0.5))]
            .into_iter()
            .collect(),
        inputs: Default::default(),
        outputs: Default::default(),
        constants: Some("ramp-v1".into()),
    });
    let dir = scratch("roundtrip");
    let path = dir.join(m.dir_name());
    m.write(&path).unwrap();
    for name in [
        "mask.toml",
        "core.png",
        "blend.png",
        "protect.png",
        "band.profile.json",
        "sigma.series.json",
        "prescription-0.series.json",
    ] {
        assert!(path.join(name).is_file(), "{name}");
    }
    let again = Mask::read(&path).unwrap();
    assert_eq!(again.record, m.record);
    assert_eq!(again.core.as_raw(), m.core.as_raw());
    assert_eq!(again.blend.as_raw(), m.blend.as_raw());
    assert_eq!(again.profile, m.profile);
    assert_eq!(again.prescriptions, m.prescriptions);
    // A hand-edited series is refused by its own identity check.
    let series_path = path.join("sigma.series.json");
    let mut text = std::fs::read_to_string(&series_path).unwrap();
    text = text.replace("2.0", "2.5");
    std::fs::write(&series_path, text).unwrap();
    let e = Mask::read(&path).unwrap_err().to_string();
    assert!(e.contains("stored identity does not match"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A file name in `mask.toml` that leaves the directory is refused before anything is read,
/// by the checked and the unchecked reader alike.
#[test]
fn a_record_naming_a_file_outside_the_mask_is_refused() {
    let dir = scratch("escape");
    let m = rect_mask();
    m.write(&dir).unwrap();
    let toml_path = dir.join("mask.toml");
    let text = std::fs::read_to_string(&toml_path).unwrap();
    for (bad, role) in [
        ("../outside.png", "zones.core"),
        ("/etc/passwd", "zones.core"),
        ("..", "zones.core"),
        (".hidden.png", "zones.core"),
    ] {
        std::fs::write(
            &toml_path,
            text.replace("core = \"core.png\"", &format!("core = \"{bad}\"")),
        )
        .unwrap();
        for result in [Mask::read(&dir), Mask::read_unchecked(&dir)] {
            let err = result.expect_err(bad).to_string();
            assert!(
                err.contains(role) && err.contains("plain file name"),
                "{bad}: {err}"
            );
        }
    }
    std::fs::write(&toml_path, &text).unwrap();
    Mask::read(&dir).unwrap();
    // The writer refuses the same record.
    let mut tampered = Mask::read(&dir).unwrap();
    tampered.record.zones.blend = "sub/blend.png".to_string();
    let err = tampered.write(&dir).unwrap_err().to_string();
    assert!(
        err.contains("zones.blend") && err.contains("plain file name"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
