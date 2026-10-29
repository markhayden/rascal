//! Task 2.4: `window` and `unwindow`. The spec's three window tests plus fractional context
//! and a tied placement; `unwindow(window(x)) = x` where nothing changed; the crop frame's
//! identity derives from the parent and the bbox; the refusals.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, GrayImage, Luma};
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, FrameKind, FrameTable, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{Context, Inputs, Op, Output, Params, Status};
use rascal_ops::unwindow::{Unwindow, unwindow_pixels};
use rascal_ops::window::{TooLarge, Window, window_bbox};

#[test]
fn the_window_contract() {
    // Full-height support in a 400x100 image: 20x100 with context 0.5 wants 40x200, made
    // square at 200, capped to the image's height: a full-height 200x100 window, the context
    // sideways only, centred on the support.
    assert_eq!(
        window_bbox([190, 0, 210, 100], 400, 100, Some(1024), 0.5),
        Ok([100, 0, 300, 100])
    );
    // In a 200-wide image the same window is capped to the whole width as well.
    assert_eq!(
        window_bbox([90, 0, 110, 100], 200, 100, None, 0.5),
        Ok([0, 0, 200, 100])
    );
    // Support touching each border is kept inside the image by sliding, not by shrinking: a
    // 20x20 support with context 10 is a 40 square (max_side 60 is a cap, not a target).
    assert_eq!(
        window_bbox([0, 40, 20, 60], 200, 100, Some(60), 0.5),
        Ok([0, 30, 40, 70])
    );
    assert_eq!(
        window_bbox([180, 40, 200, 60], 200, 100, Some(60), 0.5),
        Ok([160, 30, 200, 70])
    );
    assert_eq!(
        window_bbox([90, 0, 110, 20], 200, 100, Some(60), 0.5),
        Ok([80, 0, 120, 40])
    );
    assert_eq!(
        window_bbox([90, 80, 110, 100], 200, 100, Some(60), 0.5),
        Ok([80, 60, 120, 100])
    );
    // Support that fits while its requested context does not: capped to max_side, the support
    // still inside.
    assert_eq!(
        window_bbox([80, 40, 120, 60], 200, 100, Some(44), 0.5),
        Ok([78, 28, 122, 72])
    );
    // Fractional context rounds half up in pixels: 0.3 * 15 = 4.5 -> 5 each side -> 25 square;
    // the odd vertical remainder (15) gives the smaller part (7) to the top offset.
    assert_eq!(
        window_bbox([50, 50, 65, 60], 200, 100, None, 0.3),
        Ok([45, 43, 70, 68])
    );
    // A tied placement: an odd remainder gives the smaller part to the near offset.
    assert_eq!(
        window_bbox([50, 50, 60, 60], 200, 100, Some(13), 0.5),
        Ok([49, 49, 62, 62])
    );
    // No context: the window is the support, made square.
    assert_eq!(
        window_bbox([10, 10, 20, 14], 200, 100, None, 0.0),
        Ok([10, 7, 20, 17])
    );
    // Too large on either side.
    assert_eq!(
        window_bbox([0, 0, 150, 10], 200, 100, Some(100), 0.5),
        Err(TooLarge {
            support: [0, 0, 150, 10],
            max_side: 100
        })
    );
    // Whatever the inputs, the support is inside the window and the window inside the image.
    for (s, m, c) in [
        ([3, 7, 9, 90], Some(95), 0.5),
        ([0, 0, 1, 1], Some(1), 2.0),
        ([100, 0, 200, 100], None, 1.0),
        ([17, 33, 18, 34], Some(7), 0.25),
    ] {
        let [x0, y0, x1, y1] = window_bbox(s, 200, 100, m, c).unwrap();
        assert!(
            x0 <= s[0] && y0 <= s[1] && x1 >= s[2] && y1 >= s[3],
            "{s:?} in {:?}",
            [x0, y0, x1, y1]
        );
        assert!(x1 <= 200 && y1 <= 100);
        if let Some(m) = m {
            assert!(x1 - x0 <= m && y1 - y0 <= m);
        }
    }
}

/// A 400x100 floor with a 20 px band at the centre, a one-pixel blend on each side of it and
/// one protected pixel inside it.
fn world() -> (Frame, FrameTable, Image, Mask) {
    let f = Frame::raw(400, 100, Projection::Flat);
    let image = Image::new(
        "i",
        DynamicImage::ImageRgb8(rascal_testkit::images::floor(400, 100)),
        &f,
        Role::Canonical,
        None,
    );
    let mut core = GrayImage::new(400, 100);
    for y in 0..100 {
        for x in 190..210 {
            core.put_pixel(x, y, Luma([255]));
        }
    }
    let mut mask = Mask::from_core(
        "m",
        &f,
        UseCase::Seam,
        core,
        Source {
            locations: vec![],
            instances: vec![],
        },
    )
    .unwrap();
    let mut blend = GrayImage::new(400, 100);
    for y in 0..100 {
        blend.put_pixel(189, y, Luma([128]));
        blend.put_pixel(210, y, Luma([128]));
    }
    mask.set_blend(blend).unwrap();
    let mut protect = GrayImage::new(400, 100);
    protect.put_pixel(200, 50, Luma([255]));
    mask.protect_union(&protect).unwrap();
    let table = FrameTable {
        frames: vec![f.clone()],
    };
    (f, table, image, mask)
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

#[test]
fn window_then_unwindow_is_the_identity_and_the_crop_frame_derives_from_parent_and_bbox() {
    let (f, mut table, image, mask) = world();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), Artifact::Image(Arc::new(image.clone())));
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(mask.clone())));
    let mut params = Params::new();
    params.insert("max_side".into(), toml::Value::Integer(1024));
    let out = run(&Window, &table, &inputs, params.clone());
    assert_eq!(out.envelope.status, Status::Ok, "{}", out.envelope.summary);
    assert_eq!(out.frames.len(), 1);
    let crop_frame = out.frames[0].clone();
    assert_eq!(crop_frame.kind, FrameKind::Crop);
    assert_eq!(crop_frame.id, "crop:1");
    // The support is [189, 0, 211, 100) with the blend: 22 wide + 11 each side = 44, square
    // at 200, capped to the height 100: a 200x100 window centred on the support.
    assert_eq!(crop_frame.bbox, Some([100, 0, 300, 100]));
    assert_eq!(crop_frame.parent.as_deref(), Some(f.id.as_str()));
    assert_eq!(
        crop_frame.identity,
        Frame::crop(&f, 7, [100, 0, 300, 100]).unwrap().identity,
        "the identity ignores n"
    );
    let Artifact::Image(crop) = &out.outputs["image"] else {
        panic!()
    };
    let Artifact::Mask(cropped) = &out.outputs["mask"] else {
        panic!()
    };
    assert_eq!(crop.record.frame, crop_frame.id);
    assert_eq!(cropped.record.frame, crop_frame.id);
    assert_eq!(crop.pixels.width(), 200);
    assert_eq!(cropped.dimensions(), (200, 100));
    // The planes are cut by the same bbox: weights agree pixel for pixel.
    for y in 0..100 {
        for x in 0..200 {
            assert_eq!(cropped.weight(x, y), mask.weight(x + 100, y), "{x},{y}");
        }
    }
    assert_eq!(cropped.record.use_case, UseCase::Seam);
    // unwindow writes it back exactly.
    table.insert(crop_frame.clone());
    let mut back_inputs: Inputs = BTreeMap::new();
    back_inputs.insert("image".into(), Artifact::Image(Arc::new(image.clone())));
    back_inputs.insert("crop".into(), out.outputs["image"].clone());
    let back = run(&Unwindow, &table, &back_inputs, Params::new());
    assert_eq!(
        back.envelope.status,
        Status::Ok,
        "{}",
        back.envelope.summary
    );
    let Artifact::Image(restored) = &back.outputs["image"] else {
        panic!()
    };
    assert_eq!(restored.pixels.as_bytes(), image.pixels.as_bytes());
    assert_eq!(restored.record.frame, f.id);
    assert_eq!(restored.record.role, Role::Composite);
    // A changed crop lands in exactly the bbox and nowhere else.
    let mut edited = crop.pixels.to_rgb8();
    edited.put_pixel(0, 0, image::Rgb([1, 2, 3]));
    edited.put_pixel(199, 99, image::Rgb([4, 5, 6]));
    let merged = unwindow_pixels(
        &image.pixels,
        &DynamicImage::ImageRgb8(edited),
        [100, 0, 300, 100],
    )
    .to_rgb8();
    assert_eq!(*merged.get_pixel(100, 0), image::Rgb([1, 2, 3]));
    assert_eq!(*merged.get_pixel(299, 99), image::Rgb([4, 5, 6]));
    let original = image.pixels.to_rgb8();
    for (x, y, p) in merged.enumerate_pixels() {
        if !(100..300).contains(&x) {
            assert_eq!(p, original.get_pixel(x, y));
        }
    }
    // The same window again reuses the frame id, so a repeat does not multiply crop frames.
    let again = run(&Window, &table, &inputs, params);
    assert_eq!(again.frames[0].id, "crop:1");
    // Not applicable: an empty support, and a support over max_side with the sizes named.
    let empty = Mask::from_core(
        "e",
        &f,
        UseCase::Remove,
        GrayImage::new(400, 100),
        Source {
            locations: vec![],
            instances: vec![],
        },
    )
    .unwrap();
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(empty)));
    let out = run(&Window, &table, &inputs, Params::new());
    assert_eq!(out.envelope.status, Status::NotApplicable);
    assert!(out.envelope.reason.as_deref().unwrap().contains("empty"));
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(mask)));
    let mut small = Params::new();
    small.insert("max_side".into(), toml::Value::Integer(50));
    let out = run(&Window, &table, &inputs, small);
    assert_eq!(out.envelope.status, Status::NotApplicable);
    let reason = out.envelope.reason.as_deref().unwrap();
    assert!(
        reason.contains("22x100") && reason.contains("50"),
        "{reason}"
    );
    // unwindow refuses a crop that is not in a crop frame of this parent.
    let mut bad: Inputs = BTreeMap::new();
    bad.insert("image".into(), Artifact::Image(Arc::new(image.clone())));
    bad.insert("crop".into(), Artifact::Image(Arc::new(image.clone())));
    let out = run(&Unwindow, &table, &bad, Params::new());
    assert_eq!(out.envelope.status, Status::InvalidArgs);
    assert!(
        out.envelope
            .reason
            .as_deref()
            .unwrap()
            .contains("not a crop: frame")
    );
}
