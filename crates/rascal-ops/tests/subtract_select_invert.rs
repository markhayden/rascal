//! Task 4.2: `subtract` (protect from a mask or a location, grown; precedence over core and
//! blend; the profile kept), `select` (keep, drop and exclude over three planes; the role and
//! frame rules), `invert` (from a `cut:` mask into the raw frame: protect on the two edge
//! spans, core on the middle; from a `crop:` mask: protect on its bbox).
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, GrayImage, Luma};
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, FrameTable, Plan, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::location::Location;
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{Context, Inputs, Op, Output, Params, Status};
use rascal_core::profile::{BandProfile, Windows};
use rascal_core::series::Series;
use rascal_ops::invert::{Invert, map_plane_to_parent};
use rascal_ops::profile::band_core;
use rascal_ops::select::Select;
use rascal_ops::subtract::Subtract;

const W: u32 = 100;
const H: u32 = 60;

fn in_rect(x: u32, y: u32, r: [u32; 4]) -> bool {
    (r[0]..r[1]).contains(&x) && (r[2]..r[3]).contains(&y)
}

fn plane(w: u32, h: u32, set: impl Fn(u32, u32) -> bool) -> GrayImage {
    GrayImage::from_fn(w, h, |x, y| Luma([if set(x, y) { 255 } else { 0 }]))
}

fn run(op: &dyn Op, table: &FrameTable, inputs: &Inputs, params: Params) -> Output {
    let resolved = op.manifest().resolve_params(&params).unwrap();
    op.run(&Context { frames: table }, inputs, &resolved)
        .unwrap()
}

fn mask_out(output: &Output) -> Arc<Mask> {
    match output.outputs.get("mask") {
        Some(Artifact::Mask(m)) => m.clone(),
        other => panic!("{other:?}"),
    }
}

fn count(plane: &GrayImage) -> usize {
    plane.pixels().filter(|p| p[0] != 0).count()
}

/// A band mask with a profile and a tone-ramp prescription, so `subtract` can prove it keeps
/// them.
fn band_mask(frame: &Frame) -> Mask {
    let b = BandProfile::new(
        "b",
        &frame.id,
        &frame.identity,
        "sha256:5555555555555555555555555555555555555555555555555555555555555555",
        50,
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
        frame,
        UseCase::Seam,
        band_core(W, 50, &b.half),
        Source::default(),
    )
    .unwrap();
    let series = |name: &str, unit: &str, channels: u32, v: f64| {
        Series::new(
            name,
            name,
            unit,
            &frame.id,
            &frame.identity,
            channels,
            vec![v; (channels * H) as usize],
        )
        .unwrap()
    };
    mask.set_profile(&b.line.clone(), b, series("sigma_px", "px", 1, 1.0))
        .unwrap();
    mask.set_blend(plane(W, H, |x, _| {
        (36..40).contains(&x) || (60..64).contains(&x)
    }))
    .unwrap();
    mask.push_tone_ramp(series("step", "levels", 3, 4.0), 0.5)
        .unwrap();
    mask
}

#[test]
fn subtract_protects_from_a_mask_or_a_location_grown_and_keeps_the_profile() {
    let f = Frame::raw(W, H, Projection::Flat);
    let table = FrameTable {
        frames: vec![f.clone()],
    };
    let mask = band_mask(&f);
    let before_protect = count(&mask.protect);
    // From a mask: its core and its blend both count as support; grown by 2.
    let other_core = plane(W, H, |x, y| in_rect(x, y, [45, 52, 10, 20]));
    let mut other =
        Mask::from_core("o", &f, UseCase::Custom, other_core, Source::default()).unwrap();
    other
        .set_blend(plane(W, H, |x, y| in_rect(x, y, [52, 55, 10, 20])))
        .unwrap();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(mask.clone())));
    inputs.insert("subtract".into(), Artifact::Mask(Arc::new(other)));
    let mut params = Params::new();
    params.insert("grow_px".into(), toml::Value::Float(2.0));
    let out = run(&Subtract, &table, &inputs, params);
    assert_eq!(out.envelope.status, Status::Ok, "{:?}", out.envelope.reason);
    let m = mask_out(&out);
    assert!(count(&m.protect) > before_protect);
    // Protect covers the support dilated by 2 (a point 2 px right of the blend's edge is in,
    // 3 px is out), and wins: core and blend are zero there.
    assert!(m.protect.get_pixel(56, 15)[0] != 0 && m.protect.get_pixel(58, 15)[0] == 0);
    assert!(m.protect.get_pixel(43, 15)[0] != 0 && m.protect.get_pixel(42, 15)[0] == 0);
    for (x, y, p) in m.protect.enumerate_pixels() {
        if p[0] != 0 {
            assert_eq!(m.core.get_pixel(x, y)[0], 0);
            assert_eq!(m.blend.get_pixel(x, y)[0], 0);
        }
    }
    // Outside the new protect, core and blend are untouched; the profile and the prescription
    // stay; no refinement report.
    assert_eq!(m.core.get_pixel(50, 40)[0], 255);
    assert_eq!(
        m.blend.get_pixel(37, 40)[0],
        mask.blend.get_pixel(37, 40)[0]
    );
    assert!(m.profile.is_some() && !m.prescriptions.is_empty());
    assert!(out.envelope.refinement.is_none());
    m.check().unwrap();
    // From a location (a box), ungrown: exactly the box's pixels.
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(mask.clone())));
    let location = Location::bbox(&f.id, [0.4, 0.5, 0.6, 0.6]).unwrap();
    let identity = location.identity(&f.identity);
    inputs.insert("location".into(), Artifact::Location { location, identity });
    let out = run(&Subtract, &table, &inputs, Params::new());
    let m = mask_out(&out);
    assert_eq!(count(&m.protect), 20 * 6, "the box is 20 by 6 pixels");
    assert!(m.protect.get_pixel(40, 30)[0] != 0 && m.protect.get_pixel(39, 30)[0] == 0);
    // Both or neither second input is invalid.
    let mut both: Inputs = inputs.clone();
    both.insert("subtract".into(), Artifact::Mask(Arc::new(mask.clone())));
    assert_eq!(
        run(&Subtract, &table, &both, Params::new()).envelope.status,
        Status::InvalidArgs
    );
    let mut neither: Inputs = BTreeMap::new();
    neither.insert("mask".into(), Artifact::Mask(Arc::new(mask)));
    assert_eq!(
        run(&Subtract, &table, &neither, Params::new())
            .envelope
            .status,
        Status::InvalidArgs
    );
}

fn plane_image(f: &Frame, id: &str, role: Role, set: impl Fn(u32, u32) -> bool) -> Artifact {
    Artifact::Image(Arc::new(Image::new(
        id,
        DynamicImage::ImageLuma8(plane(f.width, f.height, set)),
        f,
        role,
        None,
    )))
}

#[test]
fn select_keeps_drops_and_excludes_over_three_planes() {
    let f = Frame::raw(W, H, Projection::Flat);
    let table = FrameTable {
        frames: vec![f.clone()],
    };
    let rects = [[10, 30, 10, 30], [40, 60, 10, 30], [70, 90, 10, 30]];
    let mut inputs: Inputs = BTreeMap::new();
    for (i, r) in rects.iter().enumerate() {
        inputs.insert(
            format!("plane_{i}"),
            plane_image(&f, &format!("p{i}"), Role::Plane, move |x, y| {
                in_rect(x, y, *r)
            }),
        );
    }
    // All three by default.
    let out = run(&Select, &table, &inputs, Params::new());
    assert_eq!(out.envelope.status, Status::Ok, "{:?}", out.envelope.reason);
    let m = mask_out(&out);
    assert_eq!(count(&m.core), 3 * 400);
    assert_eq!(m.record.source.instances.len(), 3);
    assert_eq!(m.record.use_case, UseCase::Remove);
    assert!(m.blend.pixels().all(|p| p[0] == 0) && m.protect.pixels().all(|p| p[0] == 0));
    // keep [0, 2]: the middle one is out.
    let mut params = Params::new();
    params.insert(
        "keep".into(),
        toml::Value::Array(vec![toml::Value::Integer(0), toml::Value::Integer(2)]),
    );
    let m = mask_out(&run(&Select, &table, &inputs, params));
    assert_eq!(count(&m.core), 800);
    assert_eq!(m.core.get_pixel(50, 20)[0], 0);
    assert_eq!(m.record.source.instances.len(), 2);
    // drop [1]: the same.
    let mut params = Params::new();
    params.insert(
        "drop".into(),
        toml::Value::Array(vec![toml::Value::Integer(1)]),
    );
    let m = mask_out(&run(&Select, &table, &inputs, params));
    assert_eq!(count(&m.core), 800);
    // exclude a box over the right half: the third instance goes, and half of nothing else.
    let mut params = Params::new();
    params.insert(
        "exclude".into(),
        toml::Value::Array(vec![toml::Value::Array(
            [0.65, 0.0, 1.0, 1.0]
                .iter()
                .map(|v| toml::Value::Float(*v))
                .collect(),
        )]),
    );
    let out = run(&Select, &table, &inputs, params);
    let m = mask_out(&out);
    assert_eq!(count(&m.core), 800);
    assert_eq!(m.core.get_pixel(80, 20)[0], 0);
    // keep and drop together, an unknown index, a non-plane role, a second frame: invalid.
    let mut params = Params::new();
    params.insert(
        "keep".into(),
        toml::Value::Array(vec![toml::Value::Integer(0)]),
    );
    params.insert(
        "drop".into(),
        toml::Value::Array(vec![toml::Value::Integer(1)]),
    );
    assert_eq!(
        run(&Select, &table, &inputs, params).envelope.status,
        Status::InvalidArgs
    );
    let mut params = Params::new();
    params.insert(
        "keep".into(),
        toml::Value::Array(vec![toml::Value::Integer(7)]),
    );
    let out = run(&Select, &table, &inputs, params);
    assert_eq!(out.envelope.status, Status::InvalidArgs);
    assert!(
        out.envelope
            .reason
            .as_deref()
            .unwrap()
            .contains("no instance 7")
    );
    let mut wrong_role = inputs.clone();
    wrong_role.insert(
        "plane_1".into(),
        plane_image(&f, "c", Role::Canonical, |_, _| true),
    );
    let out = run(&Select, &table, &wrong_role, Params::new());
    assert_eq!(out.envelope.status, Status::InvalidArgs);
    assert!(
        out.envelope
            .reason
            .as_deref()
            .unwrap()
            .contains("Canonical role")
    );
    let g = Frame::raw(W, H, Projection::Equirectangular);
    let table2 = FrameTable {
        frames: vec![f.clone(), g.clone()],
    };
    let mut other_frame = inputs.clone();
    other_frame.insert(
        "plane_2".into(),
        plane_image(&g, "q", Role::Plane, |_, _| true),
    );
    let out = run(&Select, &table2, &other_frame, Params::new());
    assert_eq!(out.envelope.status, Status::InvalidArgs);
    // Everything excluded: not applicable, nothing published.
    let mut params = Params::new();
    params.insert(
        "exclude".into(),
        toml::Value::Array(vec![toml::Value::Array(
            [0.0, 0.0, 1.0, 1.0]
                .iter()
                .map(|v| toml::Value::Float(*v))
                .collect(),
        )]),
    );
    let out = run(&Select, &table, &inputs, params);
    assert_eq!(out.envelope.status, Status::NotApplicable);
    assert!(out.outputs.is_empty());
}

#[test]
fn invert_from_a_cut_mask_lands_protect_on_the_edge_spans_and_the_core_on_the_middle() {
    let raw = Frame::raw(W, H, Projection::Equirectangular);
    let plan = Plan::new(&raw, 0.1).unwrap();
    let s = plan.strip;
    assert_eq!(s, 10);
    let cut = Frame::cut(&raw, "img-1", &plan).unwrap();
    assert_eq!(cut.width, 2 * s);
    let table = FrameTable {
        frames: vec![raw.clone(), cut.clone()],
    };
    // The canvas mask: a band 6 px wide about the canvas centre on the upper half, plus a
    // protect patch on the canvas's far left.
    let mut mask = Mask::from_core(
        "m",
        &cut,
        UseCase::Seam,
        plane(cut.width, H, |x, y| (s - 3..s + 3).contains(&x) && y < 30),
        Source::default(),
    )
    .unwrap();
    mask.set_blend(plane(cut.width, H, |x, y| {
        (s - 5..s - 3).contains(&x) && y < 30
    }))
    .unwrap();
    mask.protect_union(&plane(cut.width, H, |x, y| x < 2 && y >= 40))
        .unwrap();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(mask.clone())));
    let out = run(&Invert, &table, &inputs, Params::new());
    assert_eq!(out.envelope.status, Status::Ok, "{:?}", out.envelope.reason);
    let m = mask_out(&out);
    assert_eq!(m.record.frame, raw.id);
    assert_eq!(m.record.use_case, UseCase::Protect);
    // Canvas x in [s-5, s+3) with y < 30: x < s maps to W - s + x (the right edge span),
    // x >= s to x - s (the left edge span).
    for y in 0..H {
        for x in 0..W {
            let canvas_x = if x >= W - s {
                Some(x - (W - s))
            } else if x < s {
                Some(x + s)
            } else {
                None
            };
            let reached = canvas_x.is_some_and(|cx| (s - 5..s + 3).contains(&cx) && y < 30);
            let protected = canvas_x.is_some_and(|cx| cx < 2 && y >= 40);
            let expect = reached || protected;
            assert_eq!(
                m.protect.get_pixel(x, y)[0] != 0,
                expect,
                "protect at {x},{y}"
            );
            assert_eq!(m.core.get_pixel(x, y)[0] != 0, !expect, "core at {x},{y}");
        }
    }
    assert!(m.blend.pixels().all(|p| p[0] == 0));
    // The middle, which the canvas never covered, is editable.
    assert_eq!(m.core.get_pixel(50, 10)[0], 255);
    m.check().unwrap();
    // `to` names the mask's own frame: the inversion stays in the canvas.
    let mut params = Params::new();
    params.insert("to".into(), toml::Value::String(cut.id.clone()));
    let same = mask_out(&run(&Invert, &table, &inputs, params));
    assert_eq!(same.record.frame, cut.id);
    assert_eq!(
        count(&same.protect),
        count(&mask.core) + count(&mask.blend) + count(&mask.protect)
    );
    // A frame off the chain is invalid.
    let mut params = Params::new();
    params.insert("to".into(), toml::Value::String("raw:1x1:flat".into()));
    assert_eq!(
        run(&Invert, &table, &inputs, params).envelope.status,
        Status::InvalidArgs
    );
    // The plane map itself: a canvas pixel at the left edge lands at W - s.
    let one = plane(cut.width, H, |x, y| x == 0 && y == 0);
    let mapped = map_plane_to_parent(&one, &cut, &raw).unwrap();
    assert_eq!(count(&mapped), 1);
    assert_eq!(mapped.get_pixel(W - s, 0)[0], 255);
}

#[test]
fn invert_from_a_crop_mask_lands_protect_on_its_bbox() {
    let raw = Frame::raw(W, H, Projection::Flat);
    let crop = Frame::crop(&raw, 1, [20, 10, 50, 40]).unwrap();
    assert_eq!((crop.width, crop.height), (30, 30));
    let table = FrameTable {
        frames: vec![raw.clone(), crop.clone()],
    };
    let mask = Mask::from_core(
        "m",
        &crop,
        UseCase::Remove,
        plane(30, 30, |x, y| in_rect(x, y, [5, 25, 5, 25])),
        Source::default(),
    )
    .unwrap();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(mask)));
    let m = mask_out(&run(&Invert, &table, &inputs, Params::new()));
    assert_eq!(m.record.frame, raw.id);
    for (x, y, p) in m.protect.enumerate_pixels() {
        let expect = in_rect(x, y, [25, 45, 15, 35]);
        assert_eq!(p[0] != 0, expect, "{x},{y}");
        assert_eq!(m.core.get_pixel(x, y)[0] != 0, !expect);
    }
    // A mask that reaches everything leaves nothing to edit.
    let full = Mask::from_core(
        "f",
        &raw,
        UseCase::Remove,
        plane(W, H, |_, _| true),
        Source::default(),
    )
    .unwrap();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(full)));
    let out = run(
        &Invert,
        &FrameTable { frames: vec![raw] },
        &inputs,
        Params::new(),
    );
    assert_eq!(out.envelope.status, Status::NotApplicable);
}
