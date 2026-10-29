//! Task 4.1: `grow`, `shrink`, `close`. Geometry on a point and a blob against brute force;
//! the shadow extension; the refinement postconditions on all three planes; a core shrunk to
//! nothing is not applicable; `ramp → shrink → feather → apply`: the refinement removed the
//! prescription, so `apply` has nothing to do and no pixel changes, which is the postcondition
//! the sequence proves.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, GrayImage, Luma, Rgb, RgbImage};
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, FrameTable, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{Context, Inputs, Op, Output, Params, Status};
use rascal_core::profile::{BandProfile, Windows};
use rascal_core::series::Series;
use rascal_ops::apply::Apply;
use rascal_ops::close::Close;
use rascal_ops::feather::Feather;
use rascal_ops::grow::{Grow, grow_core};
use rascal_ops::primitives::{dilate, disc, erode};
use rascal_ops::profile::band_core;
use rascal_ops::ramp::Ramp;
use rascal_ops::shrink::Shrink;

const W: u32 = 120;
const H: u32 = 80;

fn frame() -> Frame {
    Frame::raw(W, H, Projection::Flat)
}

/// True inside the half-open rectangle `[x0, x1) × [y0, y1)`.
fn in_rect(x: u32, y: u32, r: [u32; 4]) -> bool {
    (r[0]..r[1]).contains(&x) && (r[2]..r[3]).contains(&y)
}

fn plane(set: impl Fn(u32, u32) -> bool) -> GrayImage {
    GrayImage::from_fn(W, H, |x, y| Luma([if set(x, y) { 255 } else { 0 }]))
}

fn mask_of(core: GrayImage) -> Mask {
    Mask::from_core("m", &frame(), UseCase::Remove, core, Source::default()).unwrap()
}

fn run(op: &dyn Op, mask: Mask, params: &[(&str, f64)]) -> Output {
    let f = frame();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(mask)));
    let mut given = Params::new();
    for (k, v) in params {
        given.insert(k.to_string(), toml::Value::Float(*v));
    }
    let resolved = op.manifest().resolve_params(&given).unwrap();
    let table = FrameTable { frames: vec![f] };
    op.run(&Context { frames: &table }, &inputs, &resolved)
        .unwrap()
}

fn mask_out(output: &Output) -> Arc<Mask> {
    match output.outputs.get("mask") {
        Some(Artifact::Mask(m)) => m.clone(),
        other => panic!("{other:?}"),
    }
}

fn set(plane: &GrayImage) -> Vec<(u32, u32)> {
    plane
        .enumerate_pixels()
        .filter(|(_, _, p)| p[0] != 0)
        .map(|(x, y, _)| (x, y))
        .collect()
}

/// Brute-force dilation: every pixel within `r` of a set pixel.
fn brute_dilate(plane: &GrayImage, r: f64) -> GrayImage {
    let on = set(plane);
    GrayImage::from_fn(W, H, |x, y| {
        let hit = on.iter().any(|(sx, sy)| {
            let (dx, dy) = (f64::from(x) - f64::from(*sx), f64::from(y) - f64::from(*sy));
            dx * dx + dy * dy <= r * r + 1e-9
        });
        Luma([if hit { 255 } else { 0 }])
    })
}

#[test]
fn grow_on_a_point_is_the_disc_and_on_a_blob_matches_brute_force() {
    let point = plane(|x, y| x == 60 && y == 40);
    let out = run(&Grow, mask_of(point.clone()), &[("px", 4.0)]);
    let grown = mask_out(&out);
    let expected: std::collections::BTreeSet<(u32, u32)> = disc(4.0)
        .into_iter()
        .map(|(dx, dy)| ((60 + dx) as u32, (40 + dy) as u32))
        .collect();
    let got: std::collections::BTreeSet<(u32, u32)> = set(&grown.core).into_iter().collect();
    assert_eq!(got, expected, "a point grows into the disc");
    assert_eq!(out.envelope.status, Status::Ok);
    let blob = plane(|x, y| {
        (x as i64 - 50).pow(2) + (y as i64 - 40).pow(2) <= 15 * 15
            || in_rect(x, y, [71, 90, 21, 25])
    });
    let out = run(&Grow, mask_of(blob.clone()), &[("px", 3.5)]);
    assert_eq!(set(&mask_out(&out).core), set(&brute_dilate(&blob, 3.5)));
    assert_eq!(set(&dilate(&blob, 3.5)), set(&brute_dilate(&blob, 3.5)));
}

#[test]
fn down_px_extends_the_grown_core_downward_only() {
    let blob = plane(|x, y| in_rect(x, y, [40, 60, 30, 40]));
    let out = run(
        &Grow,
        mask_of(blob.clone()),
        &[("px", 2.0), ("down_px", 20.0)],
    );
    let grown = mask_out(&out);
    let dilated = dilate(&blob, 2.0);
    for (x, y, p) in grown.core.enumerate_pixels() {
        // Set exactly where the dilated blob is, or where it is within 20 rows above.
        let expected = (0..=20.min(y)).any(|dy| dilated.get_pixel(x, y - dy)[0] != 0);
        assert_eq!(p[0] != 0, expected, "at {x},{y}");
    }
    // Rows 28..=61 are covered at x = 50 (dilated 28..41, shadow to 61); 27 and 62 are not.
    assert!(grown.core.get_pixel(50, 28)[0] != 0 && grown.core.get_pixel(50, 61)[0] != 0);
    assert!(grown.core.get_pixel(50, 27)[0] == 0 && grown.core.get_pixel(50, 62)[0] == 0);
    assert_eq!(set(&grow_core(&blob, 2.0, 20.0)), set(&grown.core));
    let m = |n: &str| {
        out.envelope
            .measurements
            .iter()
            .find(|m| m.name == n)
            .unwrap()
            .value
    };
    assert_eq!(m("down_px"), 20.0);
    assert!(m("core_px_after") > m("core_px_before"));
}

#[test]
fn shrink_matches_brute_force_and_a_core_eroded_to_nothing_is_not_applicable() {
    let blob = plane(|x, y| (x as i64 - 50).pow(2) + (y as i64 - 40).pow(2) <= 12 * 12);
    let out = run(&Shrink, mask_of(blob.clone()), &[("px", 4.0)]);
    let shrunk = mask_out(&out);
    // Brute force: a pixel survives when every pixel within the disc was set.
    let on = set(&blob);
    let on_set: std::collections::BTreeSet<(u32, u32)> = on.iter().copied().collect();
    let expected: Vec<(u32, u32)> = on
        .iter()
        .copied()
        .filter(|(x, y)| {
            disc(4.0).into_iter().all(|(dx, dy)| {
                let (px, py) = (*x as i64 + dx, *y as i64 + dy);
                px >= 0 && py >= 0 && on_set.contains(&(px as u32, py as u32))
            })
        })
        .collect();
    assert_eq!(set(&shrunk.core), expected);
    assert_eq!(set(&erode(&blob, 4.0)), expected);
    let out = run(&Shrink, mask_of(blob), &[("px", 13.0)]);
    assert_eq!(out.envelope.status, Status::NotApplicable);
    assert!(
        out.envelope
            .reason
            .as_deref()
            .unwrap()
            .contains("leaves no core")
    );
    assert!(out.outputs.is_empty());
}

#[test]
fn close_fills_a_gap_narrower_than_the_disc_and_keeps_the_extent() {
    // Two blocks 4 px apart, and a 2 px notch bitten from one of them.
    let blob = plane(|x, y| {
        let left = in_rect(x, y, [30, 50, 20, 60]);
        let right = in_rect(x, y, [54, 74, 20, 60]);
        let notch = in_rect(x, y, [38, 40, 20, 24]);
        (left || right) && !notch
    });
    let out = run(&Close, mask_of(blob.clone()), &[("radius_px", 3.0)]);
    let closed = mask_out(&out);
    assert_eq!(set(&closed.core), set(&erode(&dilate(&blob, 3.0), 3.0)));
    assert!(closed.core.get_pixel(51, 40)[0] != 0, "the gap filled");
    assert!(closed.core.get_pixel(39, 21)[0] != 0, "the notch filled");
    assert!(
        closed.core.get_pixel(28, 40)[0] == 0 && closed.core.get_pixel(76, 40)[0] == 0,
        "the extent kept"
    );
    // The gap is wider than the disc: it stays.
    let out = run(&Close, mask_of(blob), &[("radius_px", 1.5)]);
    assert!(mask_out(&out).core.get_pixel(52, 40)[0] == 0);
}

#[test]
fn every_refinement_clears_blend_and_profile_keeps_protect_and_reports_it() {
    let f = frame();
    // A band mask with a profile, a blend, a tone-ramp prescription and a protect patch.
    let b = BandProfile::new(
        "b",
        &f.id,
        &f.identity,
        "sha256:3333333333333333333333333333333333333333333333333333333333333333",
        60,
        3.0,
        vec![8.0; H as usize],
        Windows {
            gap_px: 1,
            step_cols: 3,
            texture_cols: 20,
        },
        "fan-v1",
    )
    .unwrap();
    let mut mask = Mask::from_core(
        "m",
        &f,
        UseCase::Seam,
        band_core(W, 60, &b.half),
        Source {
            locations: vec![b.line.clone()],
            instances: vec![],
        },
    )
    .unwrap();
    let series = |name: &str, unit: &str, channels: u32, v: f64| {
        Series::new(
            name,
            name,
            unit,
            &f.id,
            &f.identity,
            channels,
            vec![v; (channels * H) as usize],
        )
        .unwrap()
    };
    mask.set_profile(&b.line.clone(), b, series("sigma_px", "px", 1, 1.0))
        .unwrap();
    mask.set_blend(plane(|x, _| (40..52).contains(&x) || (68..80).contains(&x)))
        .unwrap();
    mask.push_tone_ramp(series("step", "levels", 3, 4.0), 0.5)
        .unwrap();
    mask.protect_union(&plane(|x, y| in_rect(x, y, [55, 65, 0, 10])))
        .unwrap();
    let protect_before = mask.protect.clone();
    assert!(mask.profile.is_some() && !mask.prescriptions.is_empty());
    for (op, params) in [
        (&Grow as &dyn Op, vec![("px", 3.0)]),
        (&Shrink, vec![("px", 2.0)]),
        (&Close, vec![("radius_px", 2.0)]),
    ] {
        let out = run(op, mask.clone(), &params);
        let m = mask_out(&out);
        let report = out.envelope.refinement.as_ref().unwrap();
        assert!(
            report.blend_cleared && report.profile_cleared,
            "{}",
            op.manifest().name
        );
        assert!(m.blend.pixels().all(|p| p[0] == 0), "blend cleared");
        assert!(
            m.profile.is_none() && m.record.profile.is_none(),
            "profile removed"
        );
        assert!(m.prescriptions.is_empty() && m.record.prescriptions.is_empty());
        assert_eq!(
            m.protect.as_raw(),
            protect_before.as_raw(),
            "protect preserved"
        );
        for (x, y, p) in m.protect.enumerate_pixels() {
            if p[0] != 0 {
                assert_eq!(
                    m.core.get_pixel(x, y)[0],
                    0,
                    "core cleared under protect at {x},{y}"
                );
            }
        }
        assert!(m.core.pixels().all(|p| p[0] == 0 || p[0] == 255));
        m.check().unwrap();
        assert_ne!(m.record.identity, mask.record.identity);
        assert_eq!(m.record.id, format!("{}.mask", op.manifest().name));
    }
}

#[test]
fn ramp_shrink_feather_apply_changes_no_pixel_because_the_refinement_dropped_the_ramp() {
    let f = frame();
    let image = RgbImage::from_fn(W, H, |x, _| {
        let v = if x < 60 { 120 } else { 132 };
        Rgb([v, v, v])
    });
    let b = BandProfile::new(
        "b",
        &f.id,
        &f.identity,
        "sha256:4444444444444444444444444444444444444444444444444444444444444444",
        60,
        3.0,
        vec![10.0; H as usize],
        Windows {
            gap_px: 1,
            step_cols: 3,
            texture_cols: 20,
        },
        "fan-v1",
    )
    .unwrap();
    let mut mask = Mask::from_core(
        "m",
        &f,
        UseCase::Seam,
        band_core(W, 60, &b.half),
        Source {
            locations: vec![b.line.clone()],
            instances: vec![],
        },
    )
    .unwrap();
    let sigma = Series::new(
        "s",
        "sigma_px",
        "px",
        &f.id,
        &f.identity,
        1,
        vec![1.0; H as usize],
    )
    .unwrap();
    mask.set_profile(&b.line.clone(), b, sigma).unwrap();
    let table = FrameTable {
        frames: vec![f.clone()],
    };
    let image_artifact = Artifact::Image(Arc::new(Image::new(
        "i",
        DynamicImage::ImageRgb8(image.clone()),
        &f,
        Role::Canonical,
        None,
    )));
    let run_with = |op: &dyn Op, inputs: &Inputs, params: &[(&str, f64)]| {
        let manifest = op.manifest();
        let mut given = Params::new();
        for (k, v) in params {
            given.insert(k.to_string(), toml::Value::Float(*v));
        }
        if let Some(first) = manifest.constants.first() {
            for (k, v) in op.constant_set(first) {
                given.entry(k).or_insert(v);
            }
        }
        let resolved = manifest.resolve_params(&given).unwrap();
        op.run(&Context { frames: &table }, inputs, &resolved)
            .unwrap()
    };
    // ramp: the prescription is recorded.
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), image_artifact.clone());
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(mask)));
    let ramped = mask_out(&run_with(&Ramp, &inputs, &[]));
    assert!(!ramped.prescriptions.is_empty());
    // shrink: the prescription and the profile go; feather gives a new blend.
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("mask".into(), Artifact::Mask(ramped));
    let shrunk = mask_out(&run_with(&Shrink, &inputs, &[("px", 3.0)]));
    assert!(shrunk.prescriptions.is_empty() && shrunk.profile.is_none());
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("mask".into(), Artifact::Mask(shrunk));
    let feathered = mask_out(&run_with(&Feather, &inputs, &[("sigma_px", 2.0)]));
    assert!(feathered.blend.pixels().any(|p| p[0] != 0));
    // apply: nothing to apply, so it is not applicable and the image is byte-identical.
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), image_artifact);
    inputs.insert("mask".into(), Artifact::Mask(feathered.clone()));
    let out = run_with(&Apply, &inputs, &[]);
    assert_eq!(
        out.envelope.status,
        Status::NotApplicable,
        "{:?}",
        out.envelope.reason
    );
    assert!(out.outputs.is_empty());
    // The new core is strictly inside the old band (half-width 10 about column 60, eroded
    // by 3), so the old prescription's reach and the new support never disagree.
    for (x, y, p) in feathered.core.enumerate_pixels() {
        if p[0] != 0 {
            assert!((52..=67).contains(&x), "{x},{y}");
        }
    }
}
