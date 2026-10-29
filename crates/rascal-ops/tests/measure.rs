//! Task 2.6: `measure`. Each measure on a constructed case with a known answer; empty-window
//! and zero-denominator rows skipped and counted; hue wraparound; drift outside is zero after
//! a correct paste; measured_nothing when every row is skipped; not_applicable per measure.
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
use rascal_ops::measure::{
    Measure, continuity, drift, hue_difference, summarise, texture_match, tone_step, transition,
};
use rascal_ops::paste::{Mode, paste_pixels};
use rascal_ops::primitives::{luminance, sobel_magnitude};
use rascal_ops::profile::band_core;

const W: u32 = 120;
const H: u32 = 20;
const C: u32 = 60;

fn frame() -> Frame {
    Frame::raw(W, H, Projection::Flat)
}

fn band(f: &Frame, half: f64) -> BandProfile {
    BandProfile::new(
        "b",
        &f.id,
        &f.identity,
        "sha256:3333333333333333333333333333333333333333333333333333333333333333",
        C,
        3.0,
        vec![half; H as usize],
        Windows {
            gap_px: 1,
            step_cols: 4,
            texture_cols: 10,
        },
        "fan-v1",
    )
    .unwrap()
}

fn band_mask(f: &Frame, half: f64) -> Mask {
    let b = band(f, half);
    let mut mask = Mask::from_core(
        "m",
        f,
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
        &f.id,
        &f.identity,
        1,
        vec![1.0; H as usize],
    )
    .unwrap();
    mask.set_profile(&b.line.clone(), b, sigma).unwrap();
    let mut blend = GrayImage::new(W, H);
    for y in 0..H {
        for x in 0..W {
            let d = (f64::from(x) + 0.5 - f64::from(C)).abs();
            if d > half && d <= half + 4.0 {
                blend.put_pixel(x, y, Luma([128]));
            }
        }
    }
    mask.set_blend(blend).unwrap();
    mask
}

/// Left and right halves of two flat colours, with optional texture everywhere: a
/// deterministic pattern of amplitude `amp` whose Sobel gradient is nowhere zero (a checker's
/// would be, since the 3x3 operator sees identical columns).
fn halves(left: [u8; 3], right: [u8; 3], amp: i16) -> RgbImage {
    RgbImage::from_fn(W, H, |x, y| {
        let base = if x < C { left } else { right };
        let t = (((x * 7 + y * 13) % 5) as i16 - 2) * amp;
        Rgb([
            (i16::from(base[0]) + t).clamp(0, 255) as u8,
            (i16::from(base[1]) + t).clamp(0, 255) as u8,
            (i16::from(base[2]) + t).clamp(0, 255) as u8,
        ])
    })
}

fn run(table: &FrameTable, inputs: &Inputs, set: &str) -> Output {
    let manifest = Measure.manifest();
    let mut params = Params::new();
    params.insert("set".into(), toml::Value::String(set.into()));
    let resolved = manifest.resolve_params(&params).unwrap();
    Measure
        .run(&Context { frames: table }, inputs, &resolved)
        .unwrap()
}

fn inputs(after: RgbImage, before: Option<RgbImage>, mask: &Mask) -> (FrameTable, Inputs) {
    let f = frame();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert(
        "after".into(),
        Artifact::Image(Arc::new(Image::new(
            "a",
            DynamicImage::ImageRgb8(after),
            &f,
            Role::Composite,
            None,
        ))),
    );
    if let Some(b) = before {
        inputs.insert(
            "before".into(),
            Artifact::Image(Arc::new(Image::new(
                "b",
                DynamicImage::ImageRgb8(b),
                &f,
                Role::Canonical,
                None,
            ))),
        );
    }
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(mask.clone())));
    (FrameTable { frames: vec![f] }, inputs)
}

fn measurement(out: &Output, name: &str) -> f64 {
    out.envelope
        .measurements
        .iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| {
            panic!(
                "{name} in {:?}",
                out.envelope
                    .measurements
                    .iter()
                    .map(|m| &m.name)
                    .collect::<Vec<_>>()
            )
        })
        .value
}

#[test]
fn continuity_is_the_step_over_the_rows_mean_difference_and_nothing_on_a_flat_image() {
    // Flat halves 120 and 132 with no texture: the only neighbouring difference on a row is
    // the 12-level step between columns 59 and 60; the row mean is 12 / 119, so the ratio is 119.
    let after = halves([120; 3], [132; 3], 0);
    let rows = continuity(&after, C);
    assert_eq!(rows.skipped, 0);
    assert_eq!(rows.rows.len(), H as usize);
    for v in &rows.values {
        assert!((v - 119.0).abs() < 1e-9, "{v}");
    }
    // With a checker of ±2 everywhere, every neighbour differs by 4 and the step adds to it.
    let textured = halves([120; 3], [132; 3], 2);
    let rows = continuity(&textured, C);
    for v in &rows.values {
        assert!(*v > 1.0 && *v < 119.0, "{v}");
    }
    // A flat image: zero denominators, every row skipped.
    let flat = halves([120; 3], [120; 3], 0);
    let rows = continuity(&flat, C);
    assert_eq!(rows.skipped, H);
    assert!(rows.values.is_empty());
    // Through the op: the summaries, the series, and measured_nothing on the flat image.
    let f = frame();
    let mask = band_mask(&f, 6.0);
    let (table, ins) = inputs(after, None, &mask);
    let out = run(&table, &ins, "continuity");
    assert_eq!(out.envelope.status, Status::Ok, "{}", out.envelope.summary);
    assert!((measurement(&out, "continuity.mean") - 119.0).abs() < 1e-9);
    assert!((measurement(&out, "continuity.p95") - 119.0).abs() < 1e-9);
    assert_eq!(measurement(&out, "continuity.rows_skipped"), 0.0);
    let Artifact::Series(series) = &out.outputs["continuity"] else {
        panic!()
    };
    assert_eq!(series.rows, (0..H).collect::<Vec<_>>());
    assert_eq!(series.unit, "ratio");
    let (table, ins) = inputs(flat, None, &mask);
    let out = run(&table, &ins, "continuity");
    assert_eq!(
        out.envelope.status,
        Status::MeasuredNothing,
        "{}",
        out.envelope.summary
    );
    assert!(
        out.outputs.is_empty(),
        "no series when nothing was measured"
    );
    assert_eq!(measurement(&out, "continuity.rows_skipped"), f64::from(H));
    // Summaries: nearest-rank p95 and the even-count median.
    let s = summarise(&[1.0, 2.0, 3.0, 4.0], 95.0);
    assert_eq!((s.mean, s.median, s.p95, s.max), (2.5, 2.5, 4.0, 4.0));
}

#[test]
fn tone_step_reports_lab_differences_with_circular_hue_and_omits_hue_below_the_chroma_floor() {
    let f = frame();
    let b = band(&f, 6.0);
    // Grey halves: a lightness step, no chroma, so the hue is omitted on every row.
    let [dl, dc, dh] = tone_step(&halves([100; 3], [140; 3], 0), &b, 1.0);
    assert_eq!(dl.skipped, 0);
    for v in &dl.values {
        assert!(*v > 10.0 && *v < 25.0, "L 100 to 140 grey: {v}");
    }
    for v in &dc.values {
        assert!(v.abs() < 0.5, "{v}");
    }
    assert_eq!(
        dh.skipped, H,
        "the hue is omitted where chroma is below the floor"
    );
    // Two saturated colours straddling the ±180° hue seam in Lab: the signed difference is
    // small and crosses the seam, not ~360.
    assert_eq!(hue_difference(179.0, -179.0), 2.0);
    assert_eq!(hue_difference(-179.0, 179.0), -2.0);
    assert_eq!(hue_difference(10.0, 190.0), 180.0);
    assert_eq!(hue_difference(0.0, 90.0), 90.0);
    let left = [0, 200, 190];
    let right = [0, 190, 200];
    let [_, _, dh] = tone_step(&halves(left, right, 0), &b, 1.0);
    assert_eq!(dh.skipped, 0);
    for v in &dh.values {
        assert!(v.abs() < 30.0, "a small hue turn across the cyan seam: {v}");
        assert!(*v != 0.0);
    }
    // The op: three series and their summaries; a mask without a profile is not applicable.
    let mask = band_mask(&f, 6.0);
    let (table, ins) = inputs(halves(left, right, 0), None, &mask);
    let out = run(&table, &ins, "tone_step");
    assert_eq!(out.envelope.status, Status::Ok, "{}", out.envelope.summary);
    for port in ["tone_step_dl", "tone_step_dchroma", "tone_step_dhue"] {
        assert!(out.outputs.contains_key(port), "{port}");
    }
    assert_eq!(measurement(&out, "tone_step.dhue.rows_skipped"), 0.0);
    let plain = Mask::from_core(
        "p",
        &f,
        UseCase::Remove,
        band_core(W, C, &vec![6.0; H as usize]),
        Source {
            locations: vec![],
            instances: vec![],
        },
    )
    .unwrap();
    let (table, ins) = inputs(halves(left, right, 0), None, &plain);
    let out = run(&table, &ins, "tone_step");
    assert_eq!(out.envelope.status, Status::NotApplicable);
    assert!(
        out.envelope.summary.contains("no profile"),
        "{}",
        out.envelope.summary
    );
}

#[test]
fn transition_and_texture_match_follow_their_definitions_and_skip_empty_or_zero_rows() {
    let f = frame();
    let b = band(&f, 6.0);
    let mask = band_mask(&f, 6.0);
    // Texture everywhere (a checker) with a step at the line: the ratios are what the
    // definitions say when recomputed from the Sobel plane in the test.
    let after = halves([120; 3], [140; 3], 3);
    let sobel = sobel_magnitude(&luminance(&after), W, H);
    let at = |x: u32, y: u32| f64::from(sobel[(y * W + x) as usize]);
    let tr = transition(&after, &b, &sobel);
    let tm = texture_match(&after, &mask, &b, &sobel);
    assert_eq!((tr.skipped, tm.skipped), (0, 0));
    for (i, &y) in tr.rows.iter().enumerate() {
        let c = f64::from(C);
        let inside: Vec<f64> = (0..W)
            .filter(|x| {
                let d = f64::from(*x) - c;
                (-6.0..6.0).contains(&d)
            })
            .map(|x| at(x, y))
            .collect();
        let outside: Vec<f64> = (0..W)
            .filter(|x| {
                let d = f64::from(*x) - c;
                d < -12.0 || d >= 12.0
            })
            .map(|x| at(x, y))
            .collect();
        let expected = (inside.iter().sum::<f64>() / inside.len() as f64)
            / (outside.iter().sum::<f64>() / outside.len() as f64);
        assert!(
            (tr.values[i] - expected).abs() < 1e-9,
            "row {y}: {} vs {expected}",
            tr.values[i]
        );
        assert!(
            tr.values[i] > 1.0,
            "the step adds gradient inside the band: {}",
            tr.values[i]
        );
        let core: Vec<f64> = (0..W)
            .filter(|x| mask.core.get_pixel(*x, y)[0] != 0)
            .map(|x| at(x, y))
            .collect();
        let beyond: Vec<f64> = (0..W)
            .filter(|x| {
                let d = f64::from(*x) - c;
                (-12.0..-6.0).contains(&d) || (6.0..12.0).contains(&d)
            })
            .map(|x| at(x, y))
            .collect();
        let expected = (core.iter().sum::<f64>() / core.len() as f64)
            / (beyond.iter().sum::<f64>() / beyond.len() as f64);
        assert!((tm.values[i] - expected).abs() < 1e-9, "row {y}");
    }
    // A flat image: zero denominators, every row skipped, measured nothing for both.
    let flat = halves([120; 3], [120; 3], 0);
    let flat_sobel = sobel_magnitude(&luminance(&flat), W, H);
    assert_eq!(transition(&flat, &b, &flat_sobel).skipped, H);
    assert_eq!(texture_match(&flat, &mask, &b, &flat_sobel).skipped, H);
    let (table, ins) = inputs(flat, None, &mask);
    let out = run(&table, &ins, "transition,texture_match");
    assert_eq!(out.envelope.status, Status::MeasuredNothing);
    assert_eq!(measurement(&out, "transition.rows_skipped"), f64::from(H));
    // A band wider than the frame: the outside window is empty, every row skipped.
    let wide = band(&f, 40.0);
    assert_eq!(transition(&after, &wide, &sobel).skipped, H);
    // An unknown measure in the set is invalid_args.
    let (table, ins) = inputs(after, None, &mask);
    let out = run(&table, &ins, "continuity,sharpness");
    assert_eq!(out.envelope.status, Status::InvalidArgs);
}

#[test]
fn drift_is_zero_outside_the_mask_after_a_correct_paste_and_needs_a_before_image() {
    let f = frame();
    let mask = band_mask(&f, 6.0);
    let before = halves([120; 3], [140; 3], 2);
    let fill = RgbImage::from_pixel(W, H, Rgb([10, 20, 30]));
    let (pasted, _) = paste_pixels(
        &DynamicImage::ImageRgb8(before.clone()),
        &DynamicImage::ImageRgb8(fill),
        &mask,
        Mode::Feather,
    );
    let d = drift(&before, &pasted.to_rgb8(), &mask);
    assert_eq!(
        d.outside,
        [0.0, 0.0, 0.0],
        "a correct paste leaves w = 0 pixels alone"
    );
    assert!(d.core.iter().all(|v| *v > 50.0), "{:?}", d.core);
    assert!(
        d.blend.iter().all(|v| *v > 10.0 && *v < d.core[0]),
        "{:?}",
        d.blend
    );
    let (table, ins) = inputs(pasted.to_rgb8(), Some(before.clone()), &mask);
    let out = run(&table, &ins, "drift");
    assert_eq!(out.envelope.status, Status::Ok, "{}", out.envelope.summary);
    assert_eq!(measurement(&out, "drift.outside.r"), 0.0);
    assert!(measurement(&out, "drift.core.g") > 50.0);
    assert!(out.outputs.is_empty(), "drift has no series");
    // No before: not applicable for drift alone; with continuity in the set the op is ok and
    // the summary names drift as not applicable.
    let (table, ins) = inputs(pasted.to_rgb8(), None, &mask);
    let out = run(&table, &ins, "drift");
    assert_eq!(out.envelope.status, Status::NotApplicable);
    let out = run(&table, &ins, "continuity,drift");
    assert_eq!(out.envelope.status, Status::Ok);
    assert!(
        out.envelope.summary.contains("drift: not_applicable"),
        "{}",
        out.envelope.summary
    );
    assert!(out.outputs.contains_key("continuity"));
}
