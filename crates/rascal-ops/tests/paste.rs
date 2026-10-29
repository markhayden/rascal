//! Task 2.5: `paste`. Each mode writes only `w > 0` unprotected pixels; a fill equal to base
//! returns base; a fill that differs from base at every pixel still leaves `w = 0` pixels
//! equal to base; an RGBA base keeps its alpha; poisson is not applicable until M7.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, GrayImage, Luma, Rgb, RgbImage, Rgba, RgbaImage};
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, FrameTable, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{Context, Inputs, Op, Output, Params, Status};
use rascal_ops::paste::{Mode, Paste, paste_pixels};

const W: u32 = 48;
const H: u32 = 32;

fn frame() -> Frame {
    Frame::raw(W, H, Projection::Flat)
}

/// A mask with a core block, a feathered ring of three weights around it, and a protected
/// cross through the middle.
fn mask(f: &Frame) -> Mask {
    let mut core = GrayImage::new(W, H);
    for y in 10..22 {
        for x in 14..34 {
            core.put_pixel(x, y, Luma([255]));
        }
    }
    let mut m = Mask::from_core(
        "m",
        f,
        UseCase::Remove,
        core,
        Source {
            locations: vec![],
            instances: vec![],
        },
    )
    .unwrap();
    let mut blend = GrayImage::new(W, H);
    for y in 7..25 {
        for x in 11..37 {
            let ring = (10i32 - y as i32)
                .max(y as i32 - 21)
                .max(14 - x as i32)
                .max(x as i32 - 33);
            let v = match ring {
                1 => 192,
                2 => 96,
                3 => 24,
                _ => 0,
            };
            if v > 0 {
                blend.put_pixel(x, y, Luma([v]));
            }
        }
    }
    m.set_blend(blend).unwrap();
    let mut protect = GrayImage::new(W, H);
    for y in 0..H {
        protect.put_pixel(24, y, Luma([255]));
    }
    for x in 0..W {
        protect.put_pixel(x, 16, Luma([255]));
    }
    m.protect_union(&protect).unwrap();
    m
}

fn base_rgb() -> RgbImage {
    RgbImage::from_fn(W, H, |x, y| Rgb([(x * 5) as u8, (y * 7) as u8, 100]))
}

/// A fill that differs from the base at every pixel and channel.
fn fill_rgb() -> RgbImage {
    RgbImage::from_fn(W, H, |x, y| {
        Rgb([255 - (x * 5) as u8, 200u8.wrapping_sub((y * 7) as u8), 30])
    })
}

fn run(table: &FrameTable, inputs: &Inputs, mode: &str) -> Output {
    let manifest = Paste.manifest();
    let mut params = Params::new();
    params.insert("mode".into(), toml::Value::String(mode.into()));
    let resolved = manifest.resolve_params(&params).unwrap();
    Paste
        .run(&Context { frames: table }, inputs, &resolved)
        .unwrap()
}

fn inputs(base: DynamicImage, fill: DynamicImage, m: &Mask) -> (FrameTable, Inputs) {
    let f = frame();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert(
        "base".into(),
        Artifact::Image(Arc::new(Image::new("b", base, &f, Role::Canonical, None))),
    );
    inputs.insert(
        "fill".into(),
        Artifact::Image(Arc::new(Image::new("f", fill, &f, Role::Working, None))),
    );
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(m.clone())));
    (FrameTable { frames: vec![f] }, inputs)
}

#[test]
fn each_mode_writes_only_where_it_may_and_leaves_w_zero_pixels_equal_to_base() {
    let f = frame();
    let m = mask(&f);
    let base = base_rgb();
    let fill = fill_rgb();
    for (mode, mode_enum) in [("feather", Mode::Feather), ("hard", Mode::Hard)] {
        let (table, ins) = inputs(
            DynamicImage::ImageRgb8(base.clone()),
            DynamicImage::ImageRgb8(fill.clone()),
            &m,
        );
        let out = run(&table, &ins, mode);
        assert_eq!(out.envelope.status, Status::Ok, "{}", out.envelope.summary);
        let Artifact::Image(composite) = &out.outputs["image"] else {
            panic!()
        };
        assert_eq!(composite.record.role, Role::Composite);
        let got = composite.pixels.to_rgb8();
        let mut written = 0u64;
        for y in 0..H {
            for x in 0..W {
                let w = m.weight(x, y);
                let b = base.get_pixel(x, y);
                let fl = fill.get_pixel(x, y);
                let g = got.get_pixel(x, y);
                let core = m.core.get_pixel(x, y)[0] != 0;
                let protected = m.protect.get_pixel(x, y)[0] != 0;
                match mode_enum {
                    Mode::Feather => {
                        if w <= 0.0 {
                            assert_eq!(
                                g, b,
                                "{mode} {x},{y}: w = 0 is the base (protected: {protected})"
                            );
                        } else {
                            written += 1;
                            for ch in 0..3 {
                                let expected = ((1.0 - w) * f32::from(b[ch])
                                    + w * f32::from(fl[ch]))
                                .round() as u8;
                                assert_eq!(g[ch], expected, "{mode} {x},{y} ch {ch} w {w}");
                            }
                        }
                    }
                    Mode::Hard => {
                        if core && !protected {
                            assert_eq!(g, fl, "{mode} {x},{y}: the core takes the fill");
                            written += 1;
                        } else {
                            assert_eq!(g, b, "{mode} {x},{y}: outside the core is the base");
                        }
                    }
                }
            }
        }
        let measured = out
            .envelope
            .measurements
            .iter()
            .find(|m| m.name == "written_px")
            .unwrap()
            .value;
        assert_eq!(measured as u64, written, "{mode}");
        assert!(written > 0);
        // The pure function agrees.
        let (direct, n) = paste_pixels(
            &DynamicImage::ImageRgb8(base.clone()),
            &DynamicImage::ImageRgb8(fill.clone()),
            &m,
            mode_enum,
        );
        assert_eq!(direct.to_rgb8(), got);
        assert_eq!(n, written);
        // A fill equal to the base returns the base.
        let (table, ins) = inputs(
            DynamicImage::ImageRgb8(base.clone()),
            DynamicImage::ImageRgb8(base.clone()),
            &m,
        );
        let out = run(&table, &ins, mode);
        let Artifact::Image(same) = &out.outputs["image"] else {
            panic!()
        };
        assert_eq!(
            same.pixels.to_rgb8(),
            base,
            "{mode}: fill = base returns base"
        );
    }
}

#[test]
fn an_rgba_base_keeps_its_alpha_a_fill_alpha_is_ignored_and_poisson_waits_for_m7() {
    let f = frame();
    let m = mask(&f);
    let base = RgbaImage::from_fn(W, H, |x, y| {
        Rgba([(x * 5) as u8, (y * 7) as u8, 100, (x + y) as u8])
    });
    let fill = RgbaImage::from_fn(W, H, |_, _| Rgba([10, 20, 30, 0]));
    let (table, ins) = inputs(
        DynamicImage::ImageRgba8(base.clone()),
        DynamicImage::ImageRgba8(fill),
        &m,
    );
    let out = run(&table, &ins, "feather");
    assert_eq!(out.envelope.status, Status::Ok);
    assert!(
        out.envelope.summary.contains("alpha was ignored"),
        "{}",
        out.envelope.summary
    );
    let Artifact::Image(composite) = &out.outputs["image"] else {
        panic!()
    };
    let DynamicImage::ImageRgba8(got) = &composite.pixels else {
        panic!("an RGBA base stays RGBA")
    };
    for (x, y, p) in got.enumerate_pixels() {
        let b = base.get_pixel(x, y);
        assert_eq!(p[3], b[3], "{x},{y}: the base's alpha is kept");
        let w = m.weight(x, y);
        if w <= 0.0 {
            assert_eq!(p, b);
        } else {
            let expected = ((1.0 - w) * f32::from(b[0]) + w * 10.0).round() as u8;
            assert_eq!(p[0], expected, "{x},{y}");
        }
    }
    let out = run(&table, &ins, "poisson");
    assert_eq!(out.envelope.status, Status::NotApplicable);
    assert!(out.envelope.reason.as_deref().unwrap().contains("M7"));
    // Sizes must agree.
    let small = RgbImage::new(W / 2, H);
    let (table, ins) = inputs(
        DynamicImage::ImageRgb8(base_rgb()),
        DynamicImage::ImageRgb8(small),
        &m,
    );
    let out = run(&table, &ins, "hard");
    assert_eq!(out.envelope.status, Status::InvalidArgs);
    assert!(out.envelope.reason.as_deref().unwrap().contains("one size"));
}
