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
    assert_eq!(
        names,
        ["thin_px", "half_max_px", "rows_widened", "rows_capped"]
    );
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

/// A smooth canvas with a clear step across the centre (the band would open wide on every
/// row) and a dark disc of radius `r` centred at (`cx`, `cy`): a solid object.
fn canvas_with_disc(w: u32, h: u32, cx: f32, cy: f32, r: f32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let base = 140.0 + if x < w / 2 { 12.0 } else { 0.0 };
        let dx = x as f32 + 0.5 - cx;
        let dy = y as f32 + 0.5 - cy;
        let v = if dx * dx + dy * dy <= r * r {
            40.0
        } else {
            base
        };
        Rgb([v as u8, v as u8, v as u8])
    })
}

#[test]
fn the_object_cap_stops_the_band_short_of_an_object_and_keeps_a_straddling_one_thin() {
    // At the reference width the windows are the set's own: thin 15, a 160-column texture
    // window, so a 20-pixel disc raises the texture ruler a little and closes nothing.
    let (w, h) = (1620, 240);
    let c = 810.0;
    // One disc beside the line (its near edge 20 px right of it), one straddling it.
    let image = {
        let mut img = canvas_with_disc(w, h, c + 30.0, 60.0, 10.0);
        let straddling = canvas_with_disc(w, h, c, 180.0, 10.0);
        for (x, y, p) in straddling.enumerate_pixels() {
            if p[0] == 40 {
                img.put_pixel(x, y, *p);
            }
        }
        img
    };
    let v1 = fan_profile(&image, 0.5, &Settings::fan_v1()).unwrap();
    let v2 = fan_profile(&image, 0.5, &Settings::fan_v2()).unwrap();
    let wide = Settings::fan_v1().wide_max * f64::from(w);
    let thin = v2.thin;
    // fan-v1 ignores the objects: every row uncapped, and the band through both discs well
    // over the cap (the disc's own edges raise the texture ruler a little, no more).
    assert!(v1.cap.iter().all(|c| c.is_infinite()));
    assert!(
        v1.half[60] > 20.0 && v1.half[180] > 20.0,
        "{} {}",
        v1.half[60],
        v1.half[180]
    );
    assert!(v1.half[20] > wide - 1.0, "{} vs {wide}", v1.half[20]);
    // fan-v2, rows through the disc beside the line: the band stops short of its near edge
    // (20 px less the margin, 3 px at ref_width).
    let margin = Settings::fan_v2().object_margin_px * f64::from(w) / Settings::fan_v2().ref_width;
    for y in 52..68 {
        // The disc's near edge on this row: 30 less its half-chord.
        let dy = y as f64 + 0.5 - 60.0;
        let edge = 30.0 - (100.0 - dy * dy).max(0.0).sqrt();
        assert!(v2.cap[y].is_finite(), "row {y} is capped");
        assert!(
            v2.half[y] <= edge - margin + 0.75 && v2.half[y] >= thin,
            "row {y}: half {} against the edge at {edge:.1} px",
            v2.half[y]
        );
    }
    // Rows through the straddling disc: edges on both sides, so thin.
    for y in 172..188 {
        assert!(
            (v2.half[y] - thin).abs() < 1e-9,
            "row {y}: half {} is not thin {thin}",
            v2.half[y]
        );
    }
    // Rows well away from both discs are as wide as fan-v1's.
    for y in [20usize, 120, 230] {
        assert!(v2.cap[y].is_infinite(), "row {y} is free");
        assert!(
            (v2.half[y] - v1.half[y]).abs() < 1e-9,
            "row {y}: {} vs {}",
            v2.half[y],
            v1.half[y]
        );
    }
    // The cap reaches a little beyond the disc's rows (the running minimum), never further
    // than object_sigma of the height.
    let reach = (Settings::fan_v2().object_sigma * f64::from(h)).round() as usize;
    assert!(v2.cap[70 + reach - 1].is_finite() && v2.cap[70 + reach + 2].is_infinite());
    // The cap off (object_edge 0) reproduces fan-v1 exactly.
    let mut off = Settings::fan_v2();
    off.object_edge = 0.0;
    let same = fan_profile(&image, 0.5, &off).unwrap();
    assert_eq!(same.half, v1.half);
}

#[test]
fn fan_v3_removes_a_straddling_object_whole_and_eases_the_cap_in_and_out() {
    let (w, h) = (1620, 240);
    let c = 810.0;
    let image = {
        let mut img = canvas_with_disc(w, h, c + 30.0, 60.0, 10.0);
        let straddling = canvas_with_disc(w, h, c, 180.0, 10.0);
        for (x, y, p) in straddling.enumerate_pixels() {
            if p[0] == 40 {
                img.put_pixel(x, y, *p);
            }
        }
        img
    };
    let v1 = fan_profile(&image, 0.5, &Settings::fan_v1()).unwrap();
    let v2 = fan_profile(&image, 0.5, &Settings::fan_v2()).unwrap();
    let v3 = fan_profile(&image, 0.5, &Settings::fan_v3()).unwrap();
    // Rows through the straddling disc: fan-v2 striped them at thin; fan-v3 leaves them to
    // the rulers, as wide as fan-v1 (the object is removed whole).
    for y in 172..188 {
        assert!((v2.half[y] - v2.thin).abs() < 1e-9);
        assert!(v3.cap[y].is_infinite(), "row {y} is free under fan-v3");
        assert!(
            (v3.half[y] - v1.half[y]).abs() < 1e-9,
            "row {y}: {} vs fan-v1's {}",
            v3.half[y],
            v1.half[y]
        );
    }
    // Rows through the disc beside the line keep fan-v2's cap exactly: a capped row never
    // widens over the object.
    for y in 52..68 {
        assert!(v3.cap[y].is_finite());
        assert!(
            v3.half[y] <= v2.half[y] + 1e-9,
            "row {y}: {} over fan-v2's {}",
            v3.half[y],
            v2.half[y]
        );
    }
    // Beyond the running minimum's reach, fan-v2 steps back to the full width in one row;
    // fan-v3 eases back over the taper: narrower than fan-v1 there, widening row by row,
    // and within half a pixel of the full width again four tapers on.
    let reach = (Settings::fan_v2().object_sigma * f64::from(h)).round() as usize;
    let taper = (Settings::fan_v3().object_taper * f64::from(h)).round() as usize;
    let first_free = 70 + reach + 1;
    assert!(v2.cap[first_free].is_infinite());
    assert!(
        v3.half[first_free] < v1.half[first_free] - 1.0,
        "{} vs {}",
        v3.half[first_free],
        v1.half[first_free]
    );
    for y in first_free..first_free + 2 * taper {
        assert!(
            v3.half[y + 1] >= v3.half[y] - 1e-9,
            "row {y}: {} then {}",
            v3.half[y],
            v3.half[y + 1]
        );
    }
    let settled = first_free + 4 * taper + 2;
    assert!(
        (v3.half[settled] - v1.half[settled]).abs() < 0.5,
        "row {settled}: {} vs {}",
        v3.half[settled],
        v1.half[settled]
    );
    // The embedded set round-trips through the schema and parses as the op's defaults do.
    assert!(Settings::fan_v3().object_straddle_whole);
    assert!(!Settings::fan_v2().object_straddle_whole);
}

/// A smooth canvas with the centre step and rows of short dashes across the opening: strong
/// horizontal steps one to three rows tall, the way cloud texture is. Each block of dashes is
/// offset from the one above by more than the chain's wander, so no edge runs down the frame.
fn canvas_with_dashes(w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let base = 140 + if x < w / 2 { 12 } else { 0 };
        let block = y / 6;
        let rows = 1 + block % 3;
        let in_rows = y % 6 < rows;
        let offset = (block % 4) * 7;
        let left = w / 2 - 120;
        let dash = x >= left && x < w / 2 + 120 && (x - left + offset) % 28 < 10;
        let v = if in_rows && dash { 70 } else { base };
        Rgb([v, v, v])
    })
}

#[test]
fn fan_v3_ignores_texture_whose_edges_do_not_run_down_the_frame() {
    let (w, h) = (1620, 400);
    let image = canvas_with_dashes(w, h);
    let v1 = fan_profile(&image, 0.5, &Settings::fan_v1()).unwrap();
    let v2 = fan_profile(&image, 0.5, &Settings::fan_v2()).unwrap();
    let v3 = fan_profile(&image, 0.5, &Settings::fan_v3()).unwrap();
    let capped = |p: &rascal_ops::fan::Profile| p.cap.iter().filter(|c| c.is_finite()).count();
    // fan-v2 takes every strong step as an object's edge and caps most of the frame.
    assert!(
        capped(&v2) > h as usize / 2,
        "fan-v2 capped {} rows",
        capped(&v2)
    );
    // fan-v3 asks for a vertical run of object_run · h (4 rows here); the dashes are one to
    // three rows tall, so none qualifies and the band is fan-v1's.
    assert_eq!(capped(&v3), 0, "fan-v3 capped {} rows", capped(&v3));
    assert_eq!(v3.half, v1.half);
}
