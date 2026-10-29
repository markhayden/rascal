//! Task 1.5: every rendering profile exhaustively on a synthetic mask with core, blend and
//! protect, through the pure functions and through the op.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, GrayImage, Luma, Rgb, RgbImage};
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, FrameTable, Projection};
use rascal_core::image::{Image, RenderingRole, Role};
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{Context, Op, Params, Status};
use rascal_core::rendering::{self, Profile};

/// 8 wide, 4 high. Core at x 2..4 on every row; blend 51 at x = 4 and 128 at x = 5; protect at
/// (3, 1) (inside the core) and (5, 2) (under a blend pixel). Everything else is outside.
fn world() -> (Frame, Mask, DynamicImage) {
    let frame = Frame::raw(8, 4, Projection::Flat);
    let core = GrayImage::from_fn(8, 4, |x, _| {
        Luma([if (2..4).contains(&x) { 255 } else { 0 }])
    });
    let mut mask = Mask::from_core("m", &frame, UseCase::Custom, core, Source::default()).unwrap();
    let blend = GrayImage::from_fn(8, 4, |x, _| {
        Luma([match x {
            4 => 51,
            5 => 128,
            _ => 0,
        }])
    });
    mask.set_blend(blend).unwrap();
    let mut protect = GrayImage::new(8, 4);
    protect.put_pixel(3, 1, Luma([255]));
    protect.put_pixel(5, 2, Luma([255]));
    mask.protect_union(&protect).unwrap();
    mask.check().unwrap();
    let image = DynamicImage::ImageRgb8(RgbImage::from_fn(8, 4, |x, y| {
        Rgb([100 + x as u8, 50 + y as u8, 20])
    }));
    (frame, mask, image)
}

fn weights(mask: &Mask) -> Vec<Vec<f32>> {
    (0..4)
        .map(|y| (0..8).map(|x| mask.weight(x, y)).collect())
        .collect()
}

#[test]
fn the_world_has_every_zone() {
    let (_, mask, _) = world();
    let w = weights(&mask);
    assert_eq!(w[0], vec![0.0, 0.0, 1.0, 1.0, 0.2, 128.0 / 255.0, 0.0, 0.0]);
    assert_eq!(w[1][3], 0.0, "protected core pixel");
    assert_eq!(w[2][5], 0.0, "protected blend pixel");
    assert_eq!(
        mask.blend.get_pixel(5, 2)[0],
        0,
        "blend cleared under protect"
    );
}

#[test]
fn grey_is_the_weight_times_255() {
    let (_, mask, _) = world();
    let g = rendering::grey(&mask);
    for y in 0..4 {
        for x in 0..8 {
            assert_eq!(
                g.get_pixel(x, y)[0],
                (255.0 * mask.weight(x, y)).round() as u8,
                "({x},{y})"
            );
        }
    }
    assert_eq!(g.get_pixel(4, 0)[0], 51);
    assert_eq!(g.get_pixel(3, 1)[0], 0);
}

#[test]
fn binary_thresholds_the_weight_and_protected_pixels_are_zero_at_every_threshold() {
    let (_, mask, _) = world();
    assert!(rendering::binary(&mask, 0).is_err());
    for t in 1..=255u8 {
        let b = rendering::binary(&mask, t).unwrap();
        for y in 0..4 {
            for x in 0..8 {
                let protected = mask.protect.get_pixel(x, y)[0] != 0;
                let expected = if !protected && mask.weight(x, y) >= f32::from(t) / 255.0 {
                    255
                } else {
                    0
                };
                assert_eq!(b.get_pixel(x, y)[0], expected, "t {t} ({x},{y})");
            }
        }
    }
    let b1 = rendering::binary(&mask, 1).unwrap();
    assert_eq!(b1.get_pixel(4, 0)[0], 255, "blend 51 is above 1/255");
    assert_eq!(
        b1.get_pixel(3, 1)[0],
        0,
        "protected pixels are 0 whatever t"
    );
    let b127 = rendering::binary(&mask, 127).unwrap();
    assert_eq!(b127.get_pixel(4, 0)[0], 0);
    assert_eq!(b127.get_pixel(5, 0)[0], 255, "128 >= 127");
    let b255 = rendering::binary(&mask, 255).unwrap();
    assert_eq!(b255.get_pixel(5, 0)[0], 0);
    assert_eq!(b255.get_pixel(2, 0)[0], 255);
}

#[test]
fn alpha_is_transparent_only_on_the_unprotected_core() {
    let (_, mask, image) = world();
    let a = rendering::alpha(&mask, &image).unwrap();
    let rgb = image.to_rgb8();
    for y in 0..4 {
        for x in 0..8 {
            let p = a.get_pixel(x, y);
            let src = rgb.get_pixel(x, y);
            assert_eq!([p[0], p[1], p[2]], src.0, "colour untouched at ({x},{y})");
            let paint = mask.core.get_pixel(x, y)[0] != 0 && mask.protect.get_pixel(x, y)[0] == 0;
            assert_eq!(p[3], if paint { 0 } else { 255 }, "({x},{y})");
        }
    }
    assert_eq!(
        a.get_pixel(3, 1)[3],
        255,
        "the protected core pixel is left alone"
    );
    assert_eq!(
        a.get_pixel(4, 0)[3],
        255,
        "blend pixels are not transparent"
    );
    let wrong = DynamicImage::ImageRgb8(RgbImage::new(9, 4));
    assert!(rendering::alpha(&mask, &wrong).is_err());
}

#[test]
fn tinted_moves_only_the_unprotected_core_toward_the_colour() {
    let (_, mask, image) = world();
    let t = rendering::tinted(&mask, &image, [255, 0, 255], 0.5).unwrap();
    let rgb = image.to_rgb8();
    for y in 0..4 {
        for x in 0..8 {
            let p = t.get_pixel(x, y);
            let src = rgb.get_pixel(x, y);
            let paint = mask.core.get_pixel(x, y)[0] != 0 && mask.protect.get_pixel(x, y)[0] == 0;
            if paint {
                let expected = [
                    ((src[0] as f32 + 255.0) / 2.0).round() as u8,
                    (src[1] as f32 / 2.0).round() as u8,
                    ((src[2] as f32 + 255.0) / 2.0).round() as u8,
                ];
                assert_eq!(p.0, expected, "({x},{y})");
            } else {
                assert_eq!(p.0, src.0, "untouched at ({x},{y})");
            }
        }
    }
    assert_eq!(
        t.get_pixel(3, 1).0,
        rgb.get_pixel(3, 1).0,
        "the protected core pixel is left alone"
    );
    let full = rendering::tinted(&mask, &image, [0, 0, 0], 1.0).unwrap();
    assert_eq!(full.get_pixel(2, 0).0, [0, 0, 0]);
    let none = rendering::tinted(&mask, &image, [0, 0, 0], 0.0).unwrap();
    assert_eq!(none.get_pixel(2, 0).0, rgb.get_pixel(2, 0).0);
    assert!(rendering::tinted(&mask, &image, [0, 0, 0], 1.5).is_err());
}

#[test]
fn overlay_tints_by_weight_and_draws_the_core_edge_white() {
    let (_, mask, image) = world();
    let o = rendering::overlay(&mask, &image, [255, 0, 255]).unwrap();
    let rgb = image.to_rgb8();
    // Every core pixel in this world is on the core's edge (the core is two columns wide and
    // touches the top and bottom rows), so every core pixel is white. The protected pixel at
    // (3, 1) left the core when it was protected, so it is untouched like any w = 0 pixel.
    for y in 0..4 {
        for x in 2..4 {
            if (x, y) == (3, 1) {
                assert_eq!(
                    o.get_pixel(x, y).0,
                    rgb.get_pixel(x, y).0,
                    "protected: untouched"
                );
            } else {
                assert_eq!(o.get_pixel(x, y).0, [255, 255, 255], "({x},{y})");
            }
        }
    }
    // Blend pixels are tinted by half their weight; outside pixels are untouched.
    let src = rgb.get_pixel(4, 0);
    let amount = 0.5 * 0.2;
    let expected = [
        ((1.0 - amount) * src[0] as f32 + amount * 255.0).round() as u8,
        ((1.0 - amount) * src[1] as f32).round() as u8,
        ((1.0 - amount) * src[2] as f32 + amount * 255.0).round() as u8,
    ];
    assert_eq!(o.get_pixel(4, 0).0, expected);
    assert_eq!(o.get_pixel(0, 0).0, rgb.get_pixel(0, 0).0);
    assert_eq!(
        o.get_pixel(5, 2).0,
        rgb.get_pixel(5, 2).0,
        "protected: w = 0, untouched"
    );
    // A wider core has an interior that is not white.
    let frame = Frame::raw(8, 4, Projection::Flat);
    let big = Mask::from_core(
        "b",
        &frame,
        UseCase::Custom,
        GrayImage::from_pixel(8, 4, Luma([255])),
        Source::default(),
    )
    .unwrap();
    let o = rendering::overlay(&big, &image, [255, 0, 255]).unwrap();
    assert_eq!(o.get_pixel(0, 0).0, [255, 255, 255]);
    assert_ne!(o.get_pixel(3, 1).0, [255, 255, 255]);
}

#[test]
fn the_profile_table_matches_the_spec() {
    assert_eq!(
        Profile::ALL.map(Profile::name),
        ["grey", "binary", "alpha", "tinted", "overlay"]
    );
    assert_eq!(Profile::Grey.role(), Some(RenderingRole::MaskInput));
    assert_eq!(Profile::Binary.role(), Some(RenderingRole::MaskInput));
    assert_eq!(Profile::Alpha.role(), Some(RenderingRole::ImageInput));
    assert_eq!(Profile::Tinted.role(), Some(RenderingRole::ImageInput));
    assert_eq!(Profile::Overlay.role(), None);
    assert!(!Profile::Grey.needs_image() && !Profile::Binary.needs_image());
    assert!(
        Profile::Alpha.needs_image()
            && Profile::Tinted.needs_image()
            && Profile::Overlay.needs_image()
    );
    assert_eq!(rendering::parse_colour("#FF00ff").unwrap(), [255, 0, 255]);
    assert_eq!(rendering::parse_colour("102030").unwrap(), [16, 32, 48]);
    assert!(rendering::parse_colour("#ff00").is_err());
    assert!(Profile::parse("sepia").is_none());
}

#[test]
fn the_op_routes_every_profile_and_refuses_what_it_must() {
    let (frame, mask, image) = world();
    let table = FrameTable {
        frames: vec![frame.clone()],
    };
    let context = Context { frames: &table };
    let op = rascal_ops::render::Render;
    let manifest = op.manifest();
    manifest.check().unwrap();
    let image_artifact = Artifact::Image(Arc::new(Image::new(
        "i",
        image.clone(),
        &frame,
        Role::Canonical,
        None,
    )));
    let run = |profile: &str, with_image: bool, extra: &[(&str, toml::Value)]| {
        let mut inputs = BTreeMap::new();
        inputs.insert("mask".to_string(), Artifact::Mask(Arc::new(mask.clone())));
        if with_image {
            inputs.insert("image".to_string(), image_artifact.clone());
        }
        let mut params = Params::new();
        params.insert("for".to_string(), toml::Value::String(profile.to_string()));
        for (k, v) in extra {
            params.insert(k.to_string(), v.clone());
        }
        let params = manifest.resolve_params(&params).unwrap();
        op.run(&context, &inputs, &params).unwrap()
    };
    for profile in Profile::ALL {
        let out = run(profile.name(), true, &[]);
        assert_eq!(out.envelope.status, Status::Ok, "{}", profile.name());
        let Artifact::Image(rendered) = &out.outputs["image"] else {
            panic!()
        };
        assert_eq!(
            rendered.record.role,
            if profile == Profile::Overlay {
                Role::Inspection
            } else {
                Role::Rendering
            }
        );
        assert_eq!(rendered.record.rendering_role, profile.role());
        assert_eq!(rendered.record.frame, frame.id);
        assert_eq!(rendered.dimensions(), (8, 4));
    }
    for profile in ["alpha", "tinted", "overlay"] {
        let out = run(profile, false, &[]);
        assert_eq!(
            out.envelope.status,
            Status::InvalidArgs,
            "{profile} without an image"
        );
        assert!(
            out.envelope
                .reason
                .as_deref()
                .unwrap()
                .contains("give input image")
        );
        assert!(out.outputs.is_empty());
    }
    let out = run(
        "tinted",
        true,
        &[("colour", toml::Value::String("#zz0000".into()))],
    );
    assert_eq!(out.envelope.status, Status::InvalidArgs);
    // Threshold 0 is refused by the manifest's bounds before the op runs.
    let mut params = Params::new();
    params.insert("for".to_string(), toml::Value::String("binary".into()));
    params.insert("threshold".to_string(), toml::Value::Integer(0));
    assert!(
        manifest
            .resolve_params(&params)
            .unwrap_err()
            .contains("threshold must be in [1, 255]")
    );
    // The binary rendering through the op uses the threshold.
    let out = run("binary", false, &[("threshold", toml::Value::Integer(255))]);
    let Artifact::Image(rendered) = &out.outputs["image"] else {
        panic!()
    };
    assert_eq!(rendered.pixels.to_luma8().get_pixel(5, 0)[0], 0);
}
