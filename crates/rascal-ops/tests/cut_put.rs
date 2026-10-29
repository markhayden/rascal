//! Task 1.8a: `cut` then `put` is byte-identical at the spec's widths, through the pure
//! functions and through the ops with their plans.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, Rgb, RgbImage, Rgba, RgbaImage};
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, FrameTable, Plan, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::op::{Context, Inputs, Op, Params, Status};
use rascal_ops::cut::{Cut, cut_pixels};
use rascal_ops::put::{Put, put_pixels};

fn pano(width: u32, height: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
        Rgb([
            (x % 256) as u8,
            (y % 256) as u8,
            ((x * 7 + y * 3) % 256) as u8,
        ])
    }))
}

#[test]
fn cut_then_put_is_byte_identical_at_the_spec_widths() {
    for (w, f, s) in [
        (4u32, 0.07, 2u32),
        (5, 0.07, 2),
        (10, 0.07, 2),
        (10, 0.3, 4),
        (5760, 0.07, 404),
    ] {
        let h = 6;
        let image = pano(w, h);
        assert_eq!(Plan::strip_width(w, f), s, "W = {w}, f = {f}");
        let canvas = cut_pixels(&image, s);
        assert_eq!(canvas.width(), 2 * s);
        assert_eq!(canvas.height(), h);
        let src = image.to_rgb8();
        let cv = canvas.to_rgb8();
        // The canvas's left half is the parent's rightmost s columns; its right half the leftmost.
        for y in 0..h {
            for x in 0..s {
                assert_eq!(
                    cv.get_pixel(x, y),
                    src.get_pixel(w - s + x, y),
                    "W {w} left half ({x},{y})"
                );
                assert_eq!(
                    cv.get_pixel(s + x, y),
                    src.get_pixel(x, y),
                    "W {w} right half ({x},{y})"
                );
            }
        }
        let back = put_pixels(&image, &canvas, s);
        assert_eq!(
            back.to_rgb8().as_raw(),
            src.as_raw(),
            "W = {w}: cut then put is byte-identical"
        );
        // A changed canvas lands on the two strips and nowhere else.
        let mut edited = cv.clone();
        for y in 0..h {
            for x in 0..2 * s {
                edited.put_pixel(x, y, Rgb([9, 9, 9]));
            }
        }
        let back = put_pixels(&image, &DynamicImage::ImageRgb8(edited), s).to_rgb8();
        for y in 0..h {
            for x in 0..w {
                let on_strip = x < s || x >= w - s;
                assert_eq!(
                    back.get_pixel(x, y).0 == [9, 9, 9],
                    on_strip,
                    "W {w} ({x},{y})"
                );
            }
        }
    }
    // RGBA keeps its alpha through both.
    let rgba = DynamicImage::ImageRgba8(RgbaImage::from_fn(10, 3, |x, y| {
        Rgba([x as u8, y as u8, 1, 200 - x as u8])
    }));
    let canvas = cut_pixels(&rgba, 2);
    assert!(matches!(canvas, DynamicImage::ImageRgba8(_)));
    let back = put_pixels(&rgba, &canvas, 2);
    assert_eq!(back.as_bytes(), rgba.as_bytes());
}

#[test]
fn the_ops_carry_the_plan_and_the_frame_and_refuse_what_they_must() {
    let parent = Frame::raw(5760, 6, Projection::Equirectangular);
    let image = Image::new("i", pano(5760, 6), &parent, Role::Canonical, None);
    let mut table = FrameTable::default();
    table.insert(parent.clone());
    let context = Context { frames: &table };
    let cut = Cut;
    let manifest = cut.manifest();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), Artifact::Image(Arc::new(image.clone())));
    let params = manifest.resolve_params(&Params::new()).unwrap();
    let out = cut.run(&context, &inputs, &params).unwrap();
    assert_eq!(out.envelope.status, Status::Ok);
    assert_eq!(out.frames.len(), 1, "cut introduces the canvas frame");
    let canvas_frame = out.frames[0].clone();
    assert!(canvas_frame.id.starts_with("cut:"));
    assert_eq!((canvas_frame.width, canvas_frame.height), (808, 6));
    let Artifact::Image(canvas) = &out.outputs["image"] else {
        panic!()
    };
    let Artifact::Plan(plan) = &out.outputs["plan"] else {
        panic!()
    };
    assert_eq!(plan.strip, 404);
    assert_eq!(plan.parent, parent.id);
    assert_eq!(canvas.record.frame, canvas_frame.id);
    // put, with the canvas frame known.
    table.insert(canvas_frame.clone());
    let context = Context { frames: &table };
    let put = Put;
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), Artifact::Image(Arc::new(image.clone())));
    inputs.insert("canvas".into(), out.outputs["image"].clone());
    inputs.insert("plan".into(), out.outputs["plan"].clone());
    // The runner's frame rule says a mask and an image share a frame; two images in two frames
    // are what put takes, so it is exercised directly here.
    let back = put.run(&context, &inputs, &Params::new()).unwrap();
    assert_eq!(back.envelope.status, Status::Ok);
    let Artifact::Image(result) = &back.outputs["image"] else {
        panic!()
    };
    assert_eq!(result.pixels.as_bytes(), image.pixels.as_bytes());
    assert_eq!(result.record.role, Role::Composite);
    assert_eq!(result.record.frame, parent.id);
    // A canvas of the right size from another panorama's cut (another plan, another frame)
    // is refused by its frame, before its size is even looked at.
    let twin = Frame::raw(5760, 6, Projection::Equirectangular);
    let mut twin_inputs: Inputs = BTreeMap::new();
    // Another panorama of the same size: other pixels, so another image identity and another
    // cut frame.
    let twin_pixels = DynamicImage::ImageRgb8(RgbImage::from_fn(5760, 6, |x, y| {
        Rgb([((x + 1) % 256) as u8, (y % 256) as u8, 0])
    }));
    let twin_image = Image::new("t", twin_pixels, &twin, Role::Canonical, None);
    twin_inputs.insert("image".into(), Artifact::Image(Arc::new(twin_image)));
    let twin_cut = cut.run(&context, &twin_inputs, &params).unwrap();
    assert_ne!(
        twin_cut.frames[0].id, canvas_frame.id,
        "a different image, a different cut frame"
    );
    let mut table_with_twin = table.clone();
    table_with_twin.insert(twin_cut.frames[0].clone());
    let context_with_twin = Context {
        frames: &table_with_twin,
    };
    inputs.insert("canvas".into(), twin_cut.outputs["image"].clone());
    let refused = put
        .run(&context_with_twin, &inputs, &Params::new())
        .unwrap();
    assert_eq!(refused.envelope.status, Status::InvalidArgs);
    assert!(
        refused
            .envelope
            .reason
            .as_deref()
            .unwrap()
            .contains("not the cut frame of this image and plan"),
        "{:?}",
        refused.envelope.reason
    );
    inputs.insert("canvas".into(), out.outputs["image"].clone());
    // A plan for another frame is refused; a canvas of the wrong size is refused.
    let other = Frame::raw(4000, 6, Projection::Equirectangular);
    let other_plan = Plan::new(&other, 0.07).unwrap();
    inputs.insert("plan".into(), Artifact::Plan(Arc::new(other_plan)));
    let refused = put.run(&context, &inputs, &Params::new()).unwrap();
    assert_eq!(refused.envelope.status, Status::InvalidArgs);
    inputs.insert("plan".into(), out.outputs["plan"].clone());
    inputs.insert(
        "canvas".into(),
        Artifact::Image(Arc::new(Image::new(
            "c",
            pano(10, 6),
            &canvas_frame,
            Role::Working,
            None,
        ))),
    );
    let refused = put.run(&context, &inputs, &Params::new()).unwrap();
    assert_eq!(refused.envelope.status, Status::InvalidArgs);
    assert!(
        refused
            .envelope
            .reason
            .as_deref()
            .unwrap()
            .contains("canvas is 10x6")
    );
    // cut on a flat frame or a tiny one is not applicable; a strip out of range is refused by
    // the manifest's bounds.
    let flat = Frame::raw(320, 200, Projection::Flat);
    let mut table = FrameTable::default();
    table.insert(flat.clone());
    let context = Context { frames: &table };
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert(
        "image".into(),
        Artifact::Image(Arc::new(Image::new(
            "f",
            pano(320, 200),
            &flat,
            Role::Canonical,
            None,
        ))),
    );
    let out = cut.run(&context, &inputs, &params).unwrap();
    assert_eq!(out.envelope.status, Status::NotApplicable);
    assert!(out.outputs.is_empty() && out.frames.is_empty());
    let mut bad = Params::new();
    bad.insert("strip".into(), toml::Value::Float(0.6));
    assert!(
        manifest
            .resolve_params(&bad)
            .unwrap_err()
            .contains("strip must be in")
    );
}

/// A grey image is cut and put as grey: no silent conversion to colour.
#[test]
fn a_grey_image_stays_grey_through_cut_and_put() {
    let grey = DynamicImage::ImageLuma8(image::GrayImage::from_fn(20, 4, |x, y| {
        image::Luma([(x * 11 + y * 3) as u8])
    }));
    let canvas = cut_pixels(&grey, 2);
    assert!(matches!(canvas, DynamicImage::ImageLuma8(_)), "{canvas:?}");
    let back = put_pixels(&grey, &canvas, 2);
    assert!(matches!(back, DynamicImage::ImageLuma8(_)), "{back:?}");
    assert_eq!(back.as_bytes(), grey.as_bytes());
}
