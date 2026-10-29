//! Task 2.3: `ramp` and `apply`. The ramp is zero at and beyond `half`; `share · Δ` at the
//! line; protected pixels untouched; a hand-built mask whose support is smaller than the
//! prescription's band proves `apply` changes no `w = 0` pixel; the two sides of the step-sky
//! canvas meet in colour at the line.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, GrayImage, Luma, Rgb, RgbImage};
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, FrameTable, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::mask::{LoadedPrescription, Mask, Source, UseCase};
use rascal_core::op::{Context, Inputs, Op, Output, Params, Status};
use rascal_core::profile::{BandProfile, Windows};
use rascal_core::series::Series;
use rascal_ops::apply::{Apply, apply_prescriptions};
use rascal_ops::profile::band_core;
use rascal_ops::ramp::{Ramp, tone_step};

const W: u32 = 200;
const H: u32 = 30;
const C: u32 = 100;

fn frame() -> Frame {
    Frame::raw(W, H, Projection::Flat)
}

/// A flat canvas with a step of `delta` levels per channel across the centre: the left side
/// darker.
fn stepped(delta: [f64; 3]) -> RgbImage {
    RgbImage::from_fn(W, H, |x, _| {
        let left = x < C;
        let v = |ch: usize| {
            let base = [120.0, 130.0, 140.0][ch];
            (if left { base } else { base + delta[ch] }).round() as u8
        };
        Rgb([v(0), v(1), v(2)])
    })
}

fn band(frame: &Frame, half: f64) -> BandProfile {
    BandProfile::new(
        "b",
        &frame.id,
        &frame.identity,
        "sha256:2222222222222222222222222222222222222222222222222222222222222222",
        C,
        3.0,
        vec![half; H as usize],
        Windows {
            gap_px: 1,
            step_cols: 3,
            texture_cols: 20,
        },
        "fan-v1",
    )
    .unwrap()
}

fn band_mask(frame: &Frame, half: f64) -> Mask {
    let b = band(frame, half);
    let mut mask = Mask::from_core(
        "m",
        frame,
        UseCase::Seam,
        band_core(W, C, &b.half),
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
        &frame.id,
        &frame.identity,
        1,
        vec![1.0; H as usize],
    )
    .unwrap();
    mask.set_profile(&b.line.clone(), b, sigma).unwrap();
    mask
}

fn run(op: &dyn Op, table: &FrameTable, inputs: &Inputs, params: Params) -> Output {
    let manifest = op.manifest();
    let mut given = params;
    if let Some(first) = manifest.constants.first() {
        for (k, v) in op.constant_set(first) {
            given.entry(k).or_insert(v);
        }
    }
    let resolved = manifest.resolve_params(&given).unwrap();
    op.run(&Context { frames: table }, inputs, &resolved)
        .unwrap()
}

fn artifacts(image: RgbImage, mask: Mask) -> (FrameTable, Inputs) {
    let f = frame();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert(
        "image".into(),
        Artifact::Image(Arc::new(Image::new(
            "i",
            DynamicImage::ImageRgb8(image),
            &f,
            Role::Canonical,
            None,
        ))),
    );
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(mask)));
    (FrameTable { frames: vec![f] }, inputs)
}

#[test]
fn ramp_measures_the_step_on_the_profiles_windows_and_apply_meets_the_sides_at_the_line() {
    let f = frame();
    let delta = [10.0, -6.0, 4.0];
    let canvas = stepped(delta);
    let (table, inputs) = artifacts(canvas.clone(), band_mask(&f, 20.0));
    let out = run(&Ramp, &table, &inputs, Params::new());
    assert_eq!(out.envelope.status, Status::Ok, "{}", out.envelope.summary);
    let Artifact::Mask(ramped) = &out.outputs["mask"] else {
        panic!()
    };
    assert_eq!(ramped.prescriptions.len(), 1);
    let LoadedPrescription::ToneRamp { step, share, .. } = &ramped.prescriptions[0];
    assert_eq!(*share, 0.5, "ramp-v1's share");
    assert_eq!(step.channels, 3);
    for y in 0..H as usize {
        for (ch, expected) in delta.iter().enumerate() {
            assert!(
                (step.values[y * 3 + ch] - expected).abs() < 1e-9,
                "row {y} ch {ch}"
            );
        }
    }
    // The pure function agrees, and smoothing a constant changes nothing.
    assert_eq!(tone_step(&canvas, &band(&f, 20.0), 0.6), step.values);
    // apply: the formula exactly, per pixel; w = 0 pixels byte-identical; the sides meet.
    let (table, inputs) = artifacts(canvas.clone(), (**ramped).clone());
    let out = run(&Apply, &table, &inputs, Params::new());
    assert_eq!(out.envelope.status, Status::Ok, "{}", out.envelope.summary);
    let Artifact::Image(applied) = &out.outputs["image"] else {
        panic!()
    };
    assert_eq!(applied.record.role, Role::Working);
    let after = applied.pixels.to_rgb8();
    for y in 0..H {
        let half = 20.0;
        for x in 0..W {
            let before = canvas.get_pixel(x, y);
            let got = after.get_pixel(x, y);
            let offset = f64::from(x) + 0.5 - f64::from(C);
            let d = offset.abs();
            if ramped.weight(x, y) <= 0.0 || d >= half {
                assert_eq!(
                    got, before,
                    "{x},{y}: zero at and beyond half, and where w = 0"
                );
                continue;
            }
            let sign = if offset < 0.0 { 1.0 } else { -1.0 };
            for (ch, step_ch) in delta.iter().enumerate() {
                let expected =
                    (f64::from(before[ch]) + sign * 0.5 * step_ch * (1.0 - d / half)).round() as u8;
                assert_eq!(got[ch], expected, "{x},{y} ch {ch}");
            }
        }
        // share · Δ at the line: the pixels either side of the line each moved by half the
        // step (less the half-pixel offset), so the two sides now differ by well under a level
        // per unit of step.
        let l = after.get_pixel(C - 1, y);
        let r = after.get_pixel(C, y);
        for ch in 0..3 {
            assert!(
                i32::from(l[ch]).abs_diff(i32::from(r[ch])) <= 1,
                "row {y} ch {ch}: {l:?} {r:?}"
            );
        }
    }
    // The pure function agrees with the op.
    let (direct, changed, largest) =
        apply_prescriptions(&DynamicImage::ImageRgb8(canvas.clone()), ramped);
    assert_eq!(direct.to_rgb8(), after);
    assert!(
        changed > 0 && largest >= 4,
        "{changed} changed, up to {largest}"
    );
}

#[test]
fn protected_pixels_and_pixels_outside_a_smaller_support_are_untouched() {
    let f = frame();
    let canvas = stepped([12.0, 12.0, 12.0]);
    // The prescription's band reaches 20 px; the mask's own core is a 4 px band with no blend,
    // and a protected block inside it.
    let wide = band(&f, 20.0);
    let mut mask = Mask::from_core(
        "narrow",
        &f,
        UseCase::Seam,
        band_core(W, C, &vec![4.0; H as usize]),
        Source {
            locations: vec![wide.line.clone()],
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
    mask.set_profile(&wide.line.clone(), wide, sigma).unwrap();
    let step = Series::new(
        "step",
        "tone_ramp.step",
        "levels",
        &f.id,
        &f.identity,
        3,
        vec![12.0; 3 * H as usize],
    )
    .unwrap();
    mask.push_tone_ramp(step, 0.5).unwrap();
    let mut protect = GrayImage::new(W, H);
    for y in 10..15 {
        for x in (C - 4)..(C + 4) {
            protect.put_pixel(x, y, Luma([255]));
        }
    }
    mask.protect_union(&protect).unwrap();
    let (table, inputs) = artifacts(canvas.clone(), mask.clone());
    let out = run(&Apply, &table, &inputs, Params::new());
    assert_eq!(out.envelope.status, Status::Ok, "{}", out.envelope.summary);
    let Artifact::Image(applied) = &out.outputs["image"] else {
        panic!()
    };
    let after = applied.pixels.to_rgb8();
    let mut moved = 0;
    for y in 0..H {
        for x in 0..W {
            let same = after.get_pixel(x, y) == canvas.get_pixel(x, y);
            if mask.weight(x, y) <= 0.0 {
                assert!(
                    same,
                    "{x},{y}: w = 0 (protected or outside the 4 px core) is untouched"
                );
            } else {
                assert!(!same, "{x},{y}: a core pixel near the line moves");
                moved += 1;
            }
        }
    }
    assert_eq!(
        moved,
        (H as usize - 5) * 8,
        "the 8 core columns on the unprotected rows"
    );
    // A mask with no prescription is not applicable; one with a prescription but no profile
    // cannot be (the constructors keep that from happening) and ramp refuses a profile-less
    // mask.
    let plain = Mask::from_core(
        "p",
        &f,
        UseCase::Remove,
        band_core(W, C, &vec![4.0; H as usize]),
        Source {
            locations: vec![],
            instances: vec![],
        },
    )
    .unwrap();
    let (table, inputs) = artifacts(canvas.clone(), plain);
    let out = run(&Apply, &table, &inputs, Params::new());
    assert_eq!(out.envelope.status, Status::NotApplicable);
    let out = run(&Ramp, &table, &inputs, Params::new());
    assert_eq!(out.envelope.status, Status::InvalidArgs);
    assert!(
        out.envelope
            .reason
            .as_deref()
            .unwrap()
            .contains("no band profile")
    );
}
