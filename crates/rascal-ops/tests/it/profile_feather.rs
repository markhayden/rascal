//! Task 2.2: `profile` and `feather`. Symmetry about the line; the core edge at `half`; the
//! 50% contour at `1.1774 σ` outside the core within half a pixel; zero beyond `3 σ`;
//! `feather` on a blob matches the distance-transform brute force; protect wins.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{GrayImage, Luma};
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, FrameTable, Projection};
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{Context, Inputs, Op, Output, Params, Status};
use rascal_core::profile::{BandProfile, Windows};
use rascal_ops::feather::{Feather, feather_blend, support};
use rascal_ops::profile::{Profile, band_core};

const W: u32 = 120;
const H: u32 = 40;
const C: u32 = 60;

fn frame() -> Frame {
    Frame::raw(W, H, Projection::Flat)
}

/// A band profile whose half-width ramps from 3 px at the top to 15 px at the bottom.
fn band(frame: &Frame) -> BandProfile {
    let half: Vec<f64> = (0..H)
        .map(|y| 3.0 + 12.0 * f64::from(y) / f64::from(H - 1))
        .collect();
    BandProfile::new(
        "b",
        &frame.id,
        &frame.identity,
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        C,
        3.0,
        half,
        Windows {
            gap_px: 1,
            step_cols: 2,
            texture_cols: 12,
        },
        "fan-v1",
    )
    .unwrap()
}

fn run(op: &dyn Op, table: &FrameTable, inputs: &Inputs, params: Params) -> Output {
    let manifest = op.manifest();
    let mut given = params;
    for (k, v) in op.constant_set(&manifest.constants[0]) {
        given.entry(k).or_insert(v);
    }
    let resolved = manifest.resolve_params(&given).unwrap();
    op.run(&Context { frames: table }, inputs, &resolved)
        .unwrap()
}

fn profile_mask(ratio: Option<f64>) -> (Mask, BandProfile) {
    let f = frame();
    let b = band(&f);
    let table = FrameTable {
        frames: vec![f.clone()],
    };
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("band".into(), Artifact::BandProfile(Arc::new(b.clone())));
    let mut params = Params::new();
    if let Some(r) = ratio {
        params.insert("feather_ratio".into(), toml::Value::Float(r));
    }
    let out = run(&Profile, &table, &inputs, params);
    assert_eq!(out.envelope.status, Status::Ok, "{}", out.envelope.summary);
    let Artifact::Mask(mask) = &out.outputs["mask"] else {
        panic!()
    };
    ((**mask).clone(), b)
}

/// The distance from a pixel's centre to the nearest core pixel's edge, by brute force.
fn edge_distance(core: &GrayImage, x: u32, y: u32) -> f64 {
    let mut best = f64::INFINITY;
    for (cx, cy, p) in core.enumerate_pixels() {
        if p[0] != 0 {
            let d = ((f64::from(cx) - f64::from(x)).powi(2)
                + (f64::from(cy) - f64::from(y)).powi(2))
            .sqrt();
            best = best.min(d);
        }
    }
    best - 0.5
}

#[test]
fn the_band_is_symmetric_about_the_line_with_its_core_edge_at_half() {
    let (mask, b) = profile_mask(None);
    assert_eq!(mask.record.use_case, UseCase::Seam);
    assert_eq!(mask.record.source.locations, vec![b.line.clone()]);
    let profile = mask.profile.as_ref().expect("the profile is stored");
    assert_eq!(profile.band, b);
    assert_eq!(profile.sigma_px.rows.len(), H as usize);
    for y in 0..H {
        let half = b.half[y as usize];
        for x in 0..W {
            // Symmetry: x and its mirror about the line (pixel centres at x + 0.5).
            let mirror = (2 * C) as i64 - 1 - i64::from(x);
            if (0..i64::from(W)).contains(&mirror) {
                let m = mirror as u32;
                assert_eq!(
                    mask.core.get_pixel(x, y),
                    mask.core.get_pixel(m, y),
                    "core {x},{y}"
                );
                assert_eq!(
                    mask.blend.get_pixel(x, y),
                    mask.blend.get_pixel(m, y),
                    "blend {x},{y}"
                );
            }
            // The core edge is exactly at half: set when |x + 0.5 − c| ≤ half.
            let inside = (f64::from(x) + 0.5 - f64::from(C)).abs() <= half;
            assert_eq!(
                mask.core.get_pixel(x, y)[0] != 0,
                inside,
                "{x},{y} half {half}"
            );
        }
        // sigma[y] = feather_ratio · half[y] with the default ratio.
        let sigma = profile.sigma_px.values[y as usize];
        assert!((sigma - 0.55 * half).abs() < 1e-9, "row {y}: {sigma}");
    }
}

#[test]
fn the_half_contour_is_at_1_1774_sigma_outside_the_core_and_zero_beyond_three_sigma() {
    let (mask, b) = profile_mask(Some(1.0));
    let profile = mask.profile.as_ref().unwrap();
    for y in 0..H {
        let sigma = profile.sigma_px.values[y as usize];
        assert!((sigma - b.half[y as usize]).abs() < 1e-9);
        // Walk right from the line. Pixels sample the feather one pixel apart, so the 50%
        // contour is located by interpolating the blend between the last pixel at or above
        // 128 and the first below it: that estimate is within half a pixel of 1.1774 σ.
        // Nothing past support · sigma is painted; everything inside it has the formula's value.
        let mut last_above: Option<(f64, f64)> = None;
        let mut first_below: Option<(f64, f64)> = None;
        for x in C..W {
            let d = edge_distance(&mask.core, x, y);
            let v = mask.blend.get_pixel(x, y)[0];
            if mask.core.get_pixel(x, y)[0] != 0 {
                assert_eq!(v, 0, "zero inside the core");
                continue;
            }
            if v >= 128 {
                last_above = Some((d, f64::from(v)));
            } else if first_below.is_none() {
                first_below = Some((d, f64::from(v)));
            }
            if d > support() * sigma {
                assert_eq!(v, 0, "row {y} x {x}: d {d} beyond {} sigma", support());
            } else if d > 0.0 {
                let expected = (255.0 * (-d * d / (2.0 * sigma * sigma)).exp()).round() as u8;
                assert_eq!(v, expected, "row {y} x {x} d {d}");
            }
        }
        let (da, va) = last_above.expect("a pixel at or above half");
        let (db, vb) = first_below.expect("the blend falls below half within the row");
        let d50 = da + (va - 127.5) / (va - vb) * (db - da);
        let contour = sigma * (2.0f64.ln() * 2.0).sqrt();
        assert!(
            (d50 - contour).abs() <= 0.5,
            "row {y}: 50% at {d50} ({da}:{va} to {db}:{vb}), contour {contour}"
        );
    }
}

#[test]
fn feather_on_a_blob_matches_the_brute_force_and_keeps_protect_and_the_profile() {
    let f = Frame::raw(40, 30, Projection::Flat);
    let mut core = GrayImage::new(40, 30);
    for (x, y, p) in core.enumerate_pixels_mut() {
        let dx = f64::from(x) - 19.0;
        let dy = f64::from(y) - 14.0;
        if dx * dx / 64.0 + dy * dy / 25.0 <= 1.0 || (x > 30 && y < 6) {
            *p = Luma([255]);
        }
    }
    let mut mask = Mask::from_core(
        "blob",
        &f,
        UseCase::Remove,
        core.clone(),
        Source {
            locations: vec![],
            instances: vec![],
        },
    )
    .unwrap();
    let mut protect = GrayImage::new(40, 30);
    for x in 0..40 {
        protect.put_pixel(x, 29, Luma([255]));
        protect.put_pixel(x, 28, Luma([255]));
    }
    mask.protect_union(&protect).unwrap();
    let table = FrameTable {
        frames: vec![f.clone()],
    };
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(mask.clone())));
    let mut params = Params::new();
    params.insert("sigma_px".into(), toml::Value::Float(2.5));
    let out = run(&Feather, &table, &inputs, params);
    assert_eq!(out.envelope.status, Status::Ok);
    let Artifact::Mask(feathered) = &out.outputs["mask"] else {
        panic!()
    };
    assert_eq!(feathered.core, mask.core, "the core is untouched");
    assert_eq!(feathered.protect, mask.protect, "protect is preserved");
    let mut painted = 0;
    for y in 0..30 {
        for x in 0..40 {
            let v = feathered.blend.get_pixel(x, y)[0];
            if feathered.protect.get_pixel(x, y)[0] != 0 || feathered.core.get_pixel(x, y)[0] != 0 {
                assert_eq!(v, 0, "{x},{y}: zero under protect and in the core");
                continue;
            }
            let d = edge_distance(&feathered.core, x, y);
            let expected = if d > 0.0 && d <= 3.0 * 2.5 {
                (255.0 * (-d * d / (2.0 * 2.5 * 2.5)).exp()).round() as u8
            } else {
                0
            };
            assert_eq!(v, expected, "{x},{y} at d {d}");
            painted += u32::from(v != 0);
        }
    }
    assert!(painted > 100, "the feather painted {painted} pixels");
    // The pure function agrees with the op.
    let direct = feather_blend(&core, |_| 2.5, 3.0);
    for (x, y, p) in direct.enumerate_pixels() {
        if feathered.protect.get_pixel(x, y)[0] == 0 {
            assert_eq!(p[0], feathered.blend.get_pixel(x, y)[0]);
        }
    }
    // A band mask keeps its profile through feather, while profile's own blend is replaced.
    let (band_mask, _) = profile_mask(None);
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(band_mask.clone())));
    let mut params = Params::new();
    params.insert("sigma_px".into(), toml::Value::Float(1.0));
    let table = FrameTable {
        frames: vec![frame()],
    };
    let out = run(&Feather, &table, &inputs, params);
    let Artifact::Mask(refeathered) = &out.outputs["mask"] else {
        panic!()
    };
    assert_eq!(refeathered.profile, band_mask.profile);
    assert_ne!(refeathered.blend, band_mask.blend);
    assert_ne!(refeathered.record.identity, band_mask.record.identity);
    // band_core is what profile set.
    assert_eq!(band_core(W, C, &band(&frame()).half), band_mask.core);
}
