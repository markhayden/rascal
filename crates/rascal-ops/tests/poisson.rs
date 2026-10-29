//! Task 7.1: the gradient-domain paste. A fill equal to base returns base; a constant fill on
//! a constant base of another value returns base exactly, at the image border too; the
//! single-editable-pixel case with a zero right-hand side and a wrong initial guess; a
//! linear-gradient fill on a matching linear base is reproduced to one level; an offset fill
//! loses its seam under poisson where feather keeps it; a component with no neighbour
//! outside the domain is not applicable; a tiny `max_iter` is an operational error; a wrap
//! frame is refused; an RGBA base keeps its alpha. The ignored benchmark records the solve
//! time for a 1000×1000 domain (`docs/results/poisson-bench.md`).
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, GrayImage, Luma, Rgb, RgbImage, Rgba, RgbaImage};
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, FrameTable, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{Context, Inputs, Op, Output, Params, Status};
use rascal_ops::paste::Paste;
use rascal_ops::poisson::{Settings, Solve, solve};

const W: u32 = 48;
const H: u32 = 32;

fn frame() -> Frame {
    Frame::raw(W, H, Projection::Flat)
}

/// A mask whose core is the given block, with a two-step blend ring around it.
fn block_mask(f: &Frame, x0: u32, y0: u32, x1: u32, y1: u32) -> Mask {
    let (w, h) = (f.width, f.height);
    let mut core = GrayImage::new(w, h);
    for y in y0..y1 {
        for x in x0..x1 {
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
    let mut blend = GrayImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let ring = (y0 as i32 - y as i32)
                .max(y as i32 - (y1 as i32 - 1))
                .max(x0 as i32 - x as i32)
                .max(x as i32 - (x1 as i32 - 1));
            let v = match ring {
                1 => 170,
                2 => 85,
                _ => 0,
            };
            if v > 0 {
                blend.put_pixel(x, y, Luma([v]));
            }
        }
    }
    m.set_blend(blend).unwrap();
    m
}

fn constant(w: u32, h: u32, v: u8) -> RgbImage {
    RgbImage::from_fn(w, h, |_, _| Rgb([v, v, v]))
}

fn linear() -> RgbImage {
    RgbImage::from_fn(W, H, |x, y| {
        let v = (2 * x + 2 * y) as u8;
        Rgb([v, v / 2 + 10, 200 - v])
    })
}

fn offset(image: &RgbImage, by: u8) -> RgbImage {
    RgbImage::from_fn(image.width(), image.height(), |x, y| {
        let p = image.get_pixel(x, y);
        Rgb([p[0] + by, p[1] + by, p[2] + by])
    })
}

fn run_op(f: &Frame, base: DynamicImage, fill: DynamicImage, m: &Mask, mode: &str) -> Output {
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert(
        "base".into(),
        Artifact::Image(Arc::new(Image::new("b", base, f, Role::Canonical, None))),
    );
    inputs.insert(
        "fill".into(),
        Artifact::Image(Arc::new(Image::new("f", fill, f, Role::Working, None))),
    );
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(m.clone())));
    let table = FrameTable {
        frames: vec![f.clone()],
    };
    let manifest = Paste.manifest();
    let mut params = Params::new();
    params.insert("mode".into(), toml::Value::String(mode.into()));
    let resolved = manifest.resolve_params(&params).unwrap();
    Paste
        .run(&Context { frames: &table }, &inputs, &resolved)
        .unwrap()
}

fn composite(out: &Output) -> RgbImage {
    let Artifact::Image(image) = &out.outputs["image"] else {
        panic!("{}", out.envelope.summary)
    };
    image.pixels.to_rgb8()
}

fn solved(base: &RgbImage, fill: &RgbImage, m: &Mask) -> RgbImage {
    match solve(base, fill, m, &Settings::v1()).unwrap() {
        Solve::Solved { solution, .. } => solution,
        Solve::Unanchored { .. } => panic!("unanchored"),
    }
}

#[test]
fn a_fill_equal_to_base_returns_base() {
    let f = frame();
    let m = block_mask(&f, 14, 10, 34, 22);
    let base = linear();
    let out = run_op(
        &f,
        DynamicImage::ImageRgb8(base.clone()),
        DynamicImage::ImageRgb8(base.clone()),
        &m,
        "poisson",
    );
    assert_eq!(out.envelope.status, Status::Ok, "{}", out.envelope.summary);
    assert_eq!(composite(&out), base);
    assert!(
        out.envelope.summary.contains("poisson"),
        "{}",
        out.envelope.summary
    );
    let iterations = out
        .envelope
        .measurements
        .iter()
        .find(|m| m.name == "iterations")
        .unwrap();
    assert_eq!(iterations.value, 0.0, "the initial guess is the solution");
}

#[test]
fn a_constant_fill_on_a_constant_base_returns_base_exactly_including_at_the_border() {
    let f = frame();
    let base = constant(W, H, 50);
    let fill = constant(W, H, 200);
    // Interior, and a block in the image corner whose border pixels have missing neighbours.
    for m in [block_mask(&f, 14, 10, 34, 22), block_mask(&f, 0, 0, 12, 9)] {
        let out = run_op(
            &f,
            DynamicImage::ImageRgb8(base.clone()),
            DynamicImage::ImageRgb8(fill.clone()),
            &m,
            "poisson",
        );
        assert_eq!(out.envelope.status, Status::Ok, "{}", out.envelope.summary);
        assert_eq!(composite(&out), base);
    }
}

#[test]
fn the_single_editable_pixel_with_a_zero_right_hand_side_converges_from_a_wrong_guess() {
    let f = frame();
    // One core pixel, no blend: b = 0 there, and the initial guess (the fill) is 200.
    let mut m = block_mask(&f, 20, 15, 21, 16);
    m.set_blend(GrayImage::new(W, H)).unwrap();
    let base = constant(W, H, 0);
    let fill = constant(W, H, 200);
    let out = run_op(
        &f,
        DynamicImage::ImageRgb8(base.clone()),
        DynamicImage::ImageRgb8(fill),
        &m,
        "poisson",
    );
    assert_eq!(out.envelope.status, Status::Ok, "{}", out.envelope.summary);
    assert_eq!(composite(&out), base);
}

#[test]
fn a_linear_gradient_fill_on_a_matching_linear_base_is_reproduced_to_one_level() {
    let f = frame();
    let m = block_mask(&f, 14, 10, 34, 22);
    let base = linear();
    let fill = offset(&base, 40);
    let solution = solved(&base, &fill, &m);
    for (x, y, p) in solution.enumerate_pixels() {
        if m.weight(x, y) > 0.0 {
            let b = base.get_pixel(x, y);
            for ch in 0..3 {
                assert!(
                    (i32::from(p[ch]) - i32::from(b[ch])).abs() <= 1,
                    "{x},{y} channel {ch}: {} vs {}",
                    p[ch],
                    b[ch]
                );
            }
        } else {
            assert_eq!(
                p,
                fill.get_pixel(x, y),
                "outside the domain the fill is kept"
            );
        }
    }
}

#[test]
fn an_offset_fill_loses_its_seam_under_poisson_where_feather_keeps_it() {
    let f = frame();
    let m = block_mask(&f, 14, 10, 34, 22);
    let base = linear();
    let fill = offset(&base, 40);
    let poisson = run_op(
        &f,
        DynamicImage::ImageRgb8(base.clone()),
        DynamicImage::ImageRgb8(fill.clone()),
        &m,
        "poisson",
    );
    let feather = run_op(
        &f,
        DynamicImage::ImageRgb8(base.clone()),
        DynamicImage::ImageRgb8(fill.clone()),
        &m,
        "feather",
    );
    assert_eq!(
        poisson.envelope.status,
        Status::Ok,
        "{}",
        poisson.envelope.summary
    );
    let (p, fe) = (composite(&poisson), composite(&feather));
    for (x, y, b) in base.enumerate_pixels() {
        let (g, h) = (p.get_pixel(x, y), fe.get_pixel(x, y));
        if m.weight(x, y) <= 0.0 {
            assert_eq!(g, b, "{x},{y}: w = 0 pixels are the base");
            assert_eq!(h, b);
        } else {
            for ch in 0..3 {
                assert!((i32::from(g[ch]) - i32::from(b[ch])).abs() <= 1, "{x},{y}");
            }
        }
        if m.core.get_pixel(x, y)[0] != 0 {
            assert_eq!(
                h[0],
                b[0] + 40,
                "{x},{y}: feather keeps the offset in the core"
            );
        }
    }
}

#[test]
fn a_component_with_no_neighbour_outside_the_domain_is_not_applicable() {
    let f = frame();
    let base = linear();
    let fill = offset(&base, 40);
    // The whole image: nothing to anchor to.
    let whole = block_mask(&f, 0, 0, W, H);
    let out = run_op(
        &f,
        DynamicImage::ImageRgb8(base.clone()),
        DynamicImage::ImageRgb8(fill.clone()),
        &whole,
        "poisson",
    );
    assert_eq!(out.envelope.status, Status::NotApplicable);
    let reason = out.envelope.reason.unwrap();
    assert!(reason.contains("anchor"), "{reason}");
    assert!(
        reason.contains(&(u64::from(W) * u64::from(H)).to_string()),
        "{reason}"
    );
    // Anything less than the whole image borders a pixel outside the domain: the top rows
    // alone are anchored by the row beneath them, through the solver directly.
    let mut core = GrayImage::new(W, H);
    for y in 0..6 {
        for x in 0..W {
            core.put_pixel(x, y, Luma([255]));
        }
    }
    let top = Mask::from_core(
        "m",
        &f,
        UseCase::Remove,
        core,
        Source {
            locations: vec![],
            instances: vec![],
        },
    )
    .unwrap();
    match solve(&base, &fill, &top, &Settings::v1()).unwrap() {
        Solve::Solved { domain_px, .. } => assert_eq!(domain_px, u64::from(W) * 6),
        Solve::Unanchored { .. } => panic!("the row beneath anchors the top rows"),
    }
}

#[test]
fn reaching_max_iter_is_an_operational_error() {
    let f = frame();
    let m = block_mask(&f, 14, 10, 34, 22);
    let base = linear();
    let fill = constant(W, H, 0);
    let Err(err) = solve(
        &base,
        &fill,
        &m,
        &Settings {
            tol: 1e-4,
            max_iter: 1,
        },
    ) else {
        panic!("one iteration cannot solve a 2-D domain")
    };
    let text = err.to_string();
    assert!(
        text.contains("paste") && text.contains("1 iteration"),
        "{text}"
    );
    // And the same problem converges under v1.
    let solution = solved(&base, &fill, &m);
    assert_eq!(solution.dimensions(), (W, H));
}

#[test]
fn a_wrap_frame_is_refused() {
    let f = Frame::raw(64, 32, Projection::Equirectangular);
    let m = block_mask(&f, 20, 10, 40, 22);
    let base = constant(64, 32, 50);
    let fill = constant(64, 32, 200);
    let out = run_op(
        &f,
        DynamicImage::ImageRgb8(base),
        DynamicImage::ImageRgb8(fill),
        &m,
        "poisson",
    );
    assert_eq!(out.envelope.status, Status::InvalidArgs);
    let reason = out.envelope.reason.unwrap();
    assert!(reason.contains("wrap"), "{reason}");
}

#[test]
fn an_rgba_base_keeps_its_alpha_under_poisson() {
    let f = frame();
    let m = block_mask(&f, 14, 10, 34, 22);
    let base = RgbaImage::from_fn(W, H, |x, y| Rgba([50, 60, 70, (x + y) as u8]));
    let fill = RgbaImage::from_fn(W, H, |_, _| Rgba([200, 200, 200, 0]));
    let out = run_op(
        &f,
        DynamicImage::ImageRgba8(base.clone()),
        DynamicImage::ImageRgba8(fill),
        &m,
        "poisson",
    );
    assert_eq!(out.envelope.status, Status::Ok, "{}", out.envelope.summary);
    assert!(out.envelope.summary.contains("alpha was ignored"));
    let Artifact::Image(image) = &out.outputs["image"] else {
        panic!()
    };
    let DynamicImage::ImageRgba8(got) = &image.pixels else {
        panic!("an RGBA base stays RGBA")
    };
    assert_eq!(got, &base);
}

/// Not a gate: `cargo test --release -p rascal-ops --test poisson -- --ignored --nocapture bench`
/// prints the solve time for a 1000×1000 domain (`docs/results/poisson-bench.md`).
#[test]
#[ignore = "benchmark; run in release and record the time"]
fn bench_a_1000_by_1000_domain() {
    let f = Frame::raw(1024, 1024, Projection::Flat);
    let m = block_mask(&f, 12, 12, 1012, 1012);
    let base = RgbImage::from_fn(1024, 1024, |x, y| {
        Rgb([(x / 4) as u8, (y / 4) as u8, ((x + y) / 8) as u8])
    });
    let fill = RgbImage::from_fn(1024, 1024, |x, y| {
        let p = base.get_pixel(x, y);
        let texture = (((x * 7 + y * 3) % 23) as i32 - 11) as u8;
        Rgb([
            p[0].wrapping_add(30).wrapping_add(texture),
            p[1].wrapping_add(30),
            p[2].wrapping_add(30),
        ])
    });
    let started = std::time::Instant::now();
    let solve = solve(&base, &fill, &m, &Settings::v1());
    let elapsed = started.elapsed();
    match solve {
        Ok(Solve::Solved {
            domain_px,
            iterations,
            ..
        }) => {
            println!("poisson bench: {domain_px} px solved in {iterations} iterations, {elapsed:?}")
        }
        Ok(Solve::Unanchored { .. }) => panic!("anchored by construction"),
        Err(e) => println!("poisson bench: {e} after {elapsed:?}"),
    }
}
