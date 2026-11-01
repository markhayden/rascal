//! Task 4.3: `tighten`. Edges within one pixel on the smooth and the textured scratch; an
//! edgeless stroke is untightened throughout and equals `rasterize` exactly; the reversing
//! path takes the previous direction; the endpoint direction is one-sided; ties break by
//! midpoint, width, then the lower index; a crossing after smoothing reverts.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, RgbImage};
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, FrameTable, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::location::Location;
use rascal_core::mask::Mask;
use rascal_core::op::{Context, Inputs, Op, Output, Params, Status};
use rascal_core::series::Series;
use rascal_ops::rasterize::dilate_path;
use rascal_ops::tighten::{
    Extent, Settings, Tighten, best_pair, directions, resample, smooth_extents,
};
use rascal_testkit::images::{SCRATCH_WIDTH_PX, scratch_path, scratch_smooth, scratch_textured};

const W: u32 = 400;
const H: u32 = 300;

fn run_tighten(image: RgbImage, path: Vec<[f64; 2]>, radius: f64) -> Output {
    let f = Frame::raw(W, H, Projection::Flat);
    let table = FrameTable {
        frames: vec![f.clone()],
    };
    let location = Location::Stroke {
        frame: f.id.clone(),
        path,
        radius_px: radius,
        group: None,
    };
    let identity = location.identity(&f.identity);
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("stroke".into(), Artifact::Location { location, identity });
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
    let op = Tighten;
    let manifest = op.manifest();
    let mut given = Params::new();
    for (k, v) in op.constant_set("tighten-v1") {
        given.insert(k, v);
    }
    let resolved = manifest.resolve_params(&given).unwrap();
    op.run(&Context { frames: &table }, &inputs, &resolved)
        .unwrap()
}

fn outputs(out: &Output) -> (Arc<Mask>, Arc<Series>) {
    let mask = match out.outputs.get("mask") {
        Some(Artifact::Mask(m)) => m.clone(),
        other => panic!("{other:?}"),
    };
    let series = match out.outputs.get("tightened") {
        Some(Artifact::Series(s)) => s.clone(),
        other => panic!("{other:?}"),
    };
    (mask, series)
}

/// Per sample `(tightened, a, b)` from the series.
fn rows(series: &Series) -> Vec<(bool, f64, f64)> {
    series
        .values
        .chunks(3)
        .map(|c| (c[0] > 0.5, c[1], c[2]))
        .collect()
}

fn edges_within_a_pixel(image: RgbImage, label: &str) {
    let out = run_tighten(image, scratch_path(), 8.0);
    assert_eq!(out.envelope.status, Status::Ok, "{:?}", out.envelope.reason);
    let (mask, series) = outputs(&out);
    let rows = rows(&series);
    let tightened: Vec<&(bool, f64, f64)> = rows.iter().filter(|r| r.0).collect();
    assert!(
        tightened.len() * 10 >= rows.len() * 8,
        "{label}: {} of {} samples tightened",
        tightened.len(),
        rows.len()
    );
    // The drawn scratch is 3 px wide about the path: its edges are at ±1.5 px. Each tightened
    // extent lies within a pixel of them (the stroke itself was 8 px each side).
    let half = SCRATCH_WIDTH_PX / 2.0;
    let off = tightened
        .iter()
        .filter(|(_, a, b)| (a + half).abs() > 1.0 || (b - half).abs() > 1.0)
        .count();
    assert!(
        off * 20 <= tightened.len(),
        "{label}: {off} of {} tightened extents are more than a pixel off the edges",
        tightened.len()
    );
    let median = |pick: fn(&(bool, f64, f64)) -> f64| {
        let mut v: Vec<f64> = tightened.iter().map(|r| pick(r)).collect();
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    assert!(
        (median(|r| r.1) + half).abs() <= 1.0,
        "{label}: median a {}",
        median(|r| r.1)
    );
    assert!(
        (median(|r| r.2) - half).abs() <= 1.0,
        "{label}: median b {}",
        median(|r| r.2)
    );
    // The core is far narrower than the stroke and covers the scratch's centre line.
    let stroke = dilate_path(&scratch_path(), 8.0, W, H);
    let core_px = mask.core.pixels().filter(|p| p[0] != 0).count();
    let stroke_px = stroke.pixels().filter(|p| p[0] != 0).count();
    assert!(
        core_px * 2 < stroke_px,
        "{label}: core {core_px} vs stroke {stroke_px}"
    );
    for p in scratch_path().iter().skip(2).take(20) {
        let (x, y) = ((p[0] * f64::from(W)) as u32, (p[1] * f64::from(H)) as u32);
        assert_eq!(
            mask.core.get_pixel(x, y)[0],
            255,
            "{label}: centre line at {x},{y}"
        );
    }
    mask.check().unwrap();
    assert_eq!(mask.record.use_case, rascal_core::mask::UseCase::Repair);
    assert_eq!(mask.record.source.locations.len(), 1);
}

#[test]
fn edges_within_a_pixel_on_the_smooth_scratch() {
    edges_within_a_pixel(scratch_smooth(W, H), "smooth");
}

#[test]
fn edges_within_a_pixel_on_the_textured_scratch() {
    edges_within_a_pixel(scratch_textured(W, H), "textured");
}

#[test]
fn an_edgeless_stroke_is_untightened_throughout_and_equals_rasterize_exactly() {
    // A stroke across the smooth wall, well away from the scratch.
    let path = vec![[0.2, 0.15], [0.5, 0.22], [0.8, 0.12]];
    let out = run_tighten(scratch_smooth(W, H), path.clone(), 6.0);
    assert_eq!(out.envelope.status, Status::Ok, "{:?}", out.envelope.reason);
    let (mask, series) = outputs(&out);
    assert!(
        rows(&series)
            .iter()
            .all(|(t, a, b)| !t && *a == -6.0 && *b == 6.0)
    );
    let drawn = dilate_path(&path, 6.0, W, H);
    assert_eq!(
        mask.core.as_raw(),
        drawn.as_raw(),
        "caps and joins make it the drawn stroke"
    );
    let m = |n: &str| {
        out.envelope
            .measurements
            .iter()
            .find(|m| m.name == n)
            .unwrap()
            .value
    };
    assert_eq!(m("tightened_samples"), 0.0);
}

#[test]
fn a_reversing_path_keeps_its_direction_and_a_still_path_is_not_applicable() {
    // Out and back along one line: the turning sample's central difference is zero and takes
    // the previous direction; the first sample's is one-sided.
    let samples = vec![
        [10.0, 10.0],
        [20.0, 10.0],
        [30.0, 10.0],
        [20.0, 10.0],
        [10.0, 10.0],
    ];
    let d = directions(&samples).unwrap();
    assert_eq!(d[0], [1.0, 0.0], "one-sided at the start");
    assert_eq!(d[2], d[1], "the turn takes the previous direction");
    assert_eq!(d[4], [-1.0, 0.0], "one-sided at the end");
    // The zero difference first: it takes the next nonzero one.
    let samples = vec![[10.0, 10.0], [10.0, 10.0], [20.0, 10.0]];
    let d = directions(&samples).unwrap();
    assert_eq!(d[0], [1.0, 0.0]);
    // No direction at all.
    assert!(directions(&[[5.0, 5.0], [5.0, 5.0], [5.0, 5.0]]).is_none());
    let out = run_tighten(scratch_smooth(W, H), vec![[0.3, 0.3], [0.3, 0.3]], 5.0);
    assert_eq!(out.envelope.status, Status::NotApplicable);
    assert!(
        out.envelope
            .reason
            .as_deref()
            .unwrap()
            .contains("no nonzero direction")
    );
    // Resampling: every multiple of the spacing along the length, and the end point.
    let r = resample(&[[0.0, 0.0], [10.0, 0.0]], 4.0);
    assert_eq!(r, vec![[0.0, 0.0], [4.0, 0.0], [8.0, 0.0], [10.0, 0.0]]);
}

#[test]
fn ties_break_by_midpoint_then_width_then_the_lower_index() {
    let settings = Settings {
        radius: 5.0,
        min_contrast: 10.0,
        min_defect: 1.0,
        max_defect: 10.0,
        path_sigma: 0.0,
        spacing: 1.0,
    };
    let offsets: Vec<f64> = (-5..=5).map(f64::from).collect(); // 11 samples, index 5 at t = 0
    // Equal scores: (3, 4) at −1.5, (3, 6) at −0.5, (4, 8) at +1 and (6, 8) at +2 are the
    // admissible pairs; the midpoint nearest the path wins.
    let mut g = vec![0.0; 11];
    g[3] = -20.0;
    g[4] = 20.0;
    g[6] = 20.0;
    g[8] = -20.0;
    assert_eq!(best_pair(&g, &offsets, &settings), Some((3, 6)));
    // Equal score and midpoint: the narrower pair wins: (4, 6) over (3, 7).
    let mut g = vec![0.0; 11];
    g[3] = -20.0;
    g[7] = 20.0;
    g[4] = -20.0;
    g[6] = 20.0;
    assert_eq!(best_pair(&g, &offsets, &settings), Some((4, 6)));
    // Equal score, midpoint size and width: (3, 5) at −1 and (5, 7) at +1; the lower first
    // index wins.
    let mut g = vec![0.0; 11];
    g[3] = -20.0;
    g[5] = 20.0;
    g[7] = -20.0;
    assert_eq!(
        best_pair(&g, &offsets, &settings),
        Some((3, 5)),
        "the lower index wins"
    );
    // Below the contrast, same sign, too narrow or too wide: no pair.
    let mut g = vec![0.0; 11];
    g[3] = -5.0;
    g[6] = 20.0;
    assert_eq!(best_pair(&g, &offsets, &settings), None);
    let mut g = vec![0.0; 11];
    g[3] = 20.0;
    g[6] = 20.0;
    assert_eq!(best_pair(&g, &offsets, &settings), None);
    let narrow = Settings {
        min_defect: 4.0,
        ..settings
    };
    let mut g = vec![0.0; 11];
    g[3] = -20.0;
    g[6] = 20.0;
    assert_eq!(best_pair(&g, &offsets, &narrow), None);
    let wide = Settings {
        max_defect: 2.0,
        ..settings
    };
    assert_eq!(best_pair(&g, &offsets, &wide), None);
}

#[test]
fn smoothing_runs_separately_and_the_crossing_guard_reverts() {
    let t = |a: f64, b: f64| Extent {
        a,
        b,
        tightened: true,
    };
    // Runs are smoothed separately: the untightened sample splits them, so the lone sample
    // after it is untouched by the first run's smoothing.
    let mut extents = vec![
        t(-1.0, 1.0),
        t(-1.0, 1.0),
        t(2.5, 2.6),
        t(-1.0, 1.0),
        t(-1.0, 1.0),
        Extent::full(5.0),
        t(-2.0, 2.0),
    ];
    smooth_extents(&mut extents, 1.0, 5.0);
    assert!(extents[..5].iter().all(|e| e.tightened && e.a < e.b));
    assert!(
        extents[2].a < 2.5,
        "the outlier was pulled toward its neighbours"
    );
    assert!(!extents[5].tightened);
    assert_eq!(extents[6], t(-2.0, 2.0));
    // One kernel smooths a and b, so a run of proper extents (a < b everywhere) can never
    // cross: a convex combination keeps the order. The guard still reverts a sample that
    // arrives crossed, which only a caller can produce.
    let mut crossed = vec![t(-1.0, 1.0), t(2.0, 1.0), t(-1.0, 1.0)];
    smooth_extents(&mut crossed, 0.2, 5.0);
    assert_eq!(crossed[1], Extent::full(5.0));
    assert!(crossed[0].tightened && crossed[2].tightened);
    // Sigma below half a sample is the identity.
    let mut same = vec![t(-1.0, 1.0), t(-3.0, 0.5)];
    smooth_extents(&mut same, 0.2, 5.0);
    assert_eq!(same, vec![t(-1.0, 1.0), t(-3.0, 0.5)]);
}

#[test]
fn the_worst_tightened_extent_is_within_two_pixels_and_equals_the_rasterize_op_when_untightened() {
    use rascal_ops::rasterize::Rasterize;
    let out = run_tighten(scratch_smooth(W, H), scratch_path(), 8.0);
    let (_, series) = outputs(&out);
    let half = SCRATCH_WIDTH_PX / 2.0;
    let worst = rows(&series)
        .iter()
        .filter(|r| r.0)
        .map(|(_, a, b)| (a + half).abs().max((b - half).abs()))
        .fold(0.0_f64, f64::max);
    assert!(
        worst <= 2.0,
        "the worst tightened extent is {worst} px off the edge"
    );
    // An untightened stroke through the op equals the rasterize op's mask, not only the
    // dilation function's.
    let path = vec![[0.2, 0.15], [0.5, 0.22], [0.8, 0.12]];
    let out = run_tighten(scratch_smooth(W, H), path.clone(), 6.0);
    let (tightened, _) = outputs(&out);
    let f = Frame::raw(W, H, Projection::Flat);
    let table = FrameTable {
        frames: vec![f.clone()],
    };
    let location = Location::Stroke {
        frame: f.id.clone(),
        path,
        radius_px: 6.0,
        group: None,
    };
    let identity = location.identity(&f.identity);
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("location".into(), Artifact::Location { location, identity });
    inputs.insert(
        "image".into(),
        Artifact::Image(Arc::new(Image::new(
            "i",
            DynamicImage::ImageRgb8(scratch_smooth(W, H)),
            &f,
            Role::Canonical,
            None,
        ))),
    );
    let mut params = Params::new();
    params.insert("use_case".into(), toml::Value::String("repair".into()));
    let resolved = Rasterize.manifest().resolve_params(&params).unwrap();
    let drawn = Rasterize
        .run(&Context { frames: &table }, &inputs, &resolved)
        .unwrap();
    let Some(Artifact::Mask(drawn)) = drawn.outputs.get("mask") else {
        panic!()
    };
    assert_eq!(tightened.core.as_raw(), drawn.core.as_raw());
}

#[test]
fn a_stroke_partly_outside_the_frame_and_an_oversized_radius_are_bounded() {
    // Half the stroke is off the left edge: the reads clamp, the core is what is inside.
    let path = vec![[-0.3, 0.5], [0.3, 0.5]];
    let out = run_tighten(scratch_smooth(W, H), path, 5.0);
    assert_eq!(out.envelope.status, Status::Ok, "{:?}", out.envelope.reason);
    let (mask, _) = outputs(&out);
    assert!(mask.core.get_pixel(0, 150)[0] != 0 && mask.core.get_pixel(119, 150)[0] != 0);
    assert_eq!(mask.core.get_pixel(130, 150)[0], 0);
    // Wholly outside: nothing to paint.
    let out = run_tighten(scratch_smooth(W, H), vec![[-0.5, 0.5], [-0.2, 0.5]], 5.0);
    assert_eq!(out.envelope.status, Status::NotApplicable);
    // A radius far beyond the frame is clamped to the diagonal by the op, and refused at the
    // front door by the location's validation against the frame.
    let started = std::time::Instant::now();
    let out = run_tighten(scratch_smooth(W, H), vec![[0.3, 0.5], [0.7, 0.5]], 1e9);
    assert_eq!(out.envelope.status, Status::Ok, "{:?}", out.envelope.reason);
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    let m = |n: &str| {
        out.envelope
            .measurements
            .iter()
            .find(|m| m.name == n)
            .unwrap()
            .value
    };
    assert!(
        m("radius_px") < 1000.0,
        "clamped to the diagonal: {}",
        m("radius_px")
    );
    let f = Frame::raw(W, H, Projection::Flat);
    let huge = Location::Stroke {
        frame: f.id.clone(),
        path: vec![[0.3, 0.5], [0.7, 0.5]],
        radius_px: 1e9,
        group: None,
    };
    let e = huge.validate(&f).unwrap_err().to_string();
    assert!(e.contains("larger than the frame's diagonal"), "{e}");
}

#[test]
fn the_tighten_v1_constants_are_pinned() {
    let set: toml::Table = rascal_ops::tighten::TIGHTEN_V1.parse().unwrap();
    let v = |k: &str| set[k].as_float().unwrap();
    assert_eq!(
        (
            v("min_contrast"),
            v("min_defect_px"),
            v("max_defect_factor"),
            v("path_sigma"),
            v("sample_spacing_px")
        ),
        (12.0, 1.0, 2.0, 4.0, 1.0),
        "a changed value is a new set version and a golden update in the same commit"
    );
    assert_eq!(Tighten.manifest().constants, vec!["tighten-v1".to_string()]);
}
