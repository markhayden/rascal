//! Task 2.1: `fan`. Monotone in texture and in step; `raw_half = thin` at a closed gate; the
//! golden profile on the step-sky fixture to half a pixel; `not_applicable` when a window
//! clips; `fan-thin-v1` fixes every row at `thin`; the refusals.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use image::{DynamicImage, Rgb, RgbImage};
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, FrameTable, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::location::Location;
use rascal_core::op::{Context, Inputs, Op, Output, Params, Status};
use rascal_core::profile::BandProfile;
use rascal_ops::fan::{Fan, Settings, fan_profile};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A canvas: a flat tone with a step of `step` levels across the centre column and uniform
/// noise of amplitude `noise` (a deterministic pattern, not a random source).
fn canvas(w: u32, h: u32, step: f32, noise: f32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let base = 140.0 + if x < w / 2 { step } else { 0.0 };
        let n = ((x * 7 + y * 13) % 5) as f32 / 4.0 * 2.0 - 1.0;
        let v = (base + n * noise).round().clamp(0.0, 255.0) as u8;
        Rgb([v, v, v])
    })
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

#[test]
fn the_band_narrows_as_texture_grows_and_opens_as_the_step_grows() {
    let s = Settings::fan_v1();
    // Texture: the same step, more and more noise; the mean half-width never grows.
    let mut last = f64::INFINITY;
    for noise in [0.0, 2.0, 4.0, 8.0, 16.0] {
        let p = fan_profile(&canvas(400, 60, 12.0, noise), 0.5, &s).unwrap();
        let m = mean(&p.half);
        assert!(m <= last + 1e-9, "noise {noise}: {m} > {last}");
        last = m;
    }
    assert!(
        last <= s.thin_px * 400.0 / s.ref_width + 0.5,
        "heavy texture stays thin"
    );
    // Step: no texture, a growing step; the mean half-width never shrinks, and is thin with no
    // step at all (the gate is closed on every row).
    let mut last = 0.0;
    for step in [0.0, 1.0, 2.0, 4.0, 8.0] {
        let p = fan_profile(&canvas(400, 60, step, 0.0), 0.5, &s).unwrap();
        let m = mean(&p.half);
        assert!(m >= last - 1e-9, "step {step}: {m} < {last}");
        if step == 0.0 {
            assert!(
                p.raw_half.iter().all(|h| (h - p.thin).abs() < 1e-9),
                "closed gate: raw_half = thin"
            );
            assert!(p.half.iter().all(|h| (h - p.thin).abs() < 1e-9));
        }
        last = m;
    }
    let wide = s.wide_max * 400.0;
    assert!(
        (last - wide).abs() < 0.5,
        "a smooth canvas with a clear step opens to wide: {last} vs {wide}"
    );
}

#[test]
fn the_golden_profile_on_the_step_sky_fixture_holds_to_half_a_pixel() {
    let image = image::open(repo_root().join("fixtures/images/step-sky.png"))
        .unwrap()
        .to_rgb8();
    let (w, h) = image.dimensions();
    let s = Settings::fan_v1();
    let p = fan_profile(&image, 0.5, &s).unwrap();
    assert_eq!(p.half.len(), h as usize);
    // The sky rows open, the floor rows stay thin.
    let horizon = (h * 3 / 5) as usize;
    let sky = mean(&p.half[..horizon - 20]);
    let floor = mean(&p.half[horizon + 20..]);
    assert!(sky > p.thin + 5.0, "sky opens: {sky} vs thin {}", p.thin);
    assert!(
        floor < p.thin + 0.5,
        "the textured floor stays thin: {floor} vs {}",
        p.thin
    );
    // The committed golden, regenerated with UPDATE_FIXTURES=1.
    let frame = Frame::raw(w, h, Projection::Flat);
    let band = BandProfile::new(
        "golden",
        &frame.id,
        &frame.identity,
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        p.column_px,
        p.thin,
        p.half.clone(),
        p.windows.clone(),
        "fan-v1",
    )
    .unwrap();
    let path = repo_root().join("fixtures/masks/step-sky.profile.json");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        band.write(&path).unwrap();
    }
    let golden = BandProfile::read(&path).unwrap();
    assert_eq!(golden.column_px, band.column_px);
    assert_eq!(golden.windows, band.windows);
    assert!((golden.thin_px - band.thin_px).abs() < 1e-9);
    for (y, (a, b)) in golden.half.iter().zip(&band.half).enumerate() {
        assert!((a - b).abs() <= 0.5, "row {y}: golden {a}, computed {b}");
    }
}

fn world(image: RgbImage, projection: Projection) -> (Frame, FrameTable, Inputs) {
    let (w, h) = image.dimensions();
    let frame = Frame::raw(w, h, projection);
    let img = Image::new(
        "i",
        DynamicImage::ImageRgb8(image),
        &frame,
        Role::Canonical,
        None,
    );
    let line = Location::Line {
        frame: frame.id.clone(),
        column: Some(0.5),
        path: None,
        width_px: 2.0,
        group: None,
    };
    let identity = line.identity(&frame.identity);
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), Artifact::Image(Arc::new(img)));
    inputs.insert(
        "line".into(),
        Artifact::Location {
            location: line,
            identity,
        },
    );
    let table = FrameTable {
        frames: vec![frame.clone()],
    };
    (frame, table, inputs)
}

fn run(table: &FrameTable, inputs: &Inputs, params: Params) -> Output {
    let manifest = Fan.manifest();
    let mut given = params;
    let constants = given.remove("constants");
    let name = constants
        .as_ref()
        .and_then(|c| c.as_str())
        .unwrap_or("fan-v1")
        .to_string();
    for (k, v) in Fan.constant_set(&name) {
        given.entry(k).or_insert(v);
    }
    let mut resolved = manifest.resolve_params(&given).unwrap();
    resolved.insert("constants".into(), toml::Value::String(name));
    Fan.run(&Context { frames: table }, inputs, &resolved)
        .unwrap()
}

#[test]
fn the_op_emits_a_band_profile_and_fan_thin_fixes_every_row_at_thin() {
    let (frame, table, inputs) = world(canvas(400, 60, 12.0, 0.0), Projection::Flat);
    let out = run(&table, &inputs, Params::new());
    assert_eq!(out.envelope.status, Status::Ok, "{}", out.envelope.summary);
    let Artifact::BandProfile(band) = &out.outputs["band"] else {
        panic!()
    };
    assert_eq!(band.frame, frame.id);
    assert_eq!(band.constants, "fan-v1");
    assert_eq!(band.column_px, 200);
    assert_eq!(band.half.len(), 60);
    assert!(band.half.iter().all(|h| *h > band.thin_px + 5.0), "opens");
    assert!(
        out.envelope.summary.contains("60 of 60 rows widened"),
        "{}",
        out.envelope.summary
    );
    let names: Vec<&str> = out
        .envelope
        .measurements
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(names, ["thin_px", "half_max_px", "rows_widened"]);
    // fan-thin-v1: the same canvas, every row at thin.
    let mut params = Params::new();
    params.insert(
        "constants".into(),
        toml::Value::String("fan-thin-v1".into()),
    );
    let out = run(&table, &inputs, params);
    let Artifact::BandProfile(thin) = &out.outputs["band"] else {
        panic!()
    };
    assert_eq!(thin.constants, "fan-thin-v1");
    assert!(thin.half.iter().all(|h| (h - thin.thin_px).abs() < 1e-9));
    assert!(
        out.envelope.summary.contains("0 of 60 rows widened"),
        "{}",
        out.envelope.summary
    );
    // The same effect by overriding the tunable on the default set.
    let mut params = Params::new();
    params.insert("wide_max".into(), toml::Value::Float(0.0));
    let out = run(&table, &inputs, params);
    let Artifact::BandProfile(fixed) = &out.outputs["band"] else {
        panic!()
    };
    assert_eq!(fixed.half, thin.half);
}

#[test]
fn a_clipped_window_is_not_applicable_and_the_refusals_name_their_reason() {
    let s = Settings::fan_v1();
    // A line near the left edge: the left texture window has no columns.
    let err = fan_profile(&canvas(200, 20, 12.0, 0.0), 0.01, &s).unwrap_err();
    assert_eq!(err.0, "left step", "the step windows are checked first");
    let (_, table, mut inputs) = world(canvas(200, 20, 12.0, 0.0), Projection::Flat);
    let frame_id = "raw:200x20:flat".to_string();
    let near = Location::Line {
        frame: frame_id.clone(),
        column: Some(0.01),
        path: None,
        width_px: 2.0,
        group: None,
    };
    let identity = near.identity(&table.frames[0].identity);
    inputs.insert(
        "line".into(),
        Artifact::Location {
            location: near,
            identity,
        },
    );
    let out = run(&table, &inputs, Params::new());
    assert_eq!(out.envelope.status, Status::NotApplicable);
    assert!(
        out.envelope
            .reason
            .as_deref()
            .unwrap()
            .contains("left step window clips"),
        "{:?}",
        out.envelope.reason
    );
    // A path line is invalid_args; a wrapping frame is invalid_args.
    let path = Location::Line {
        frame: frame_id,
        column: None,
        path: Some(vec![[0.5, 0.0], [0.5, 1.0]]),
        width_px: 2.0,
        group: None,
    };
    let identity = path.identity(&table.frames[0].identity);
    inputs.insert(
        "line".into(),
        Artifact::Location {
            location: path,
            identity,
        },
    );
    let out = run(&table, &inputs, Params::new());
    assert_eq!(out.envelope.status, Status::InvalidArgs);
    assert!(
        out.envelope
            .reason
            .as_deref()
            .unwrap()
            .contains("column form")
    );
    let (_, table, inputs) = world(canvas(400, 200, 12.0, 0.0), Projection::Equirectangular);
    let out = run(&table, &inputs, Params::new());
    assert_eq!(out.envelope.status, Status::InvalidArgs);
    assert!(out.envelope.reason.as_deref().unwrap().contains("wraps"));
}
