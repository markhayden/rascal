//! The focus of a pair: the columns and rows a result changed, joined over the two sides, as
//! a region with context around it that wraps across the edge of a wrap frame and clamps at
//! the edge of a bounded one; the crop takes its columns modulo the width.
#![allow(clippy::unwrap_used)]

use image::{DynamicImage, Rgb, RgbImage};
use rascal_core::frame::Topology;
use rascal_core::lab::focus::{CONTEXT_MAX, CONTEXT_MIN, Changes, Focus};

fn flat(width: u32, height: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([90, 120, 150])))
}

/// `base` with every pixel in `[x0, x1) × [y0, y1)` changed, columns modulo the width.
fn changed(base: &DynamicImage, x0: u32, x1: u32, y0: u32, y1: u32) -> DynamicImage {
    let mut out = base.to_rgb8();
    let width = out.width();
    for y in y0..y1 {
        for x in x0..x1 {
            out.put_pixel(x % width, y, Rgb([200, 10, 10]));
        }
    }
    DynamicImage::ImageRgb8(out)
}

#[test]
fn nothing_changed_is_no_focus() {
    let base = flat(300, 200);
    let changes = Changes::between(&base, &base).unwrap();
    assert!(!changes.any());
    assert!(changes.focus(Topology::Bounded).is_none());
    assert!(changes.focus(Topology::Wrap).is_none());
}

#[test]
fn sizes_must_agree() {
    let err = Changes::between(&flat(300, 200), &flat(301, 200)).unwrap_err();
    assert!(err.to_string().contains("300x200"), "{err}");
    assert!(err.to_string().contains("301x200"), "{err}");
}

#[test]
fn a_bounded_change_takes_context_clamped_at_the_edges() {
    let base = flat(2000, 1500);
    // A 100 × 60 change in the middle: context is half the extent, at least CONTEXT_MIN.
    let after = changed(&base, 1000, 1100, 700, 760);
    let focus = Changes::between(&base, &after)
        .unwrap()
        .focus(Topology::Bounded)
        .unwrap();
    assert_eq!(
        focus,
        Focus {
            x: 1000 - CONTEXT_MIN,
            y: 700 - CONTEXT_MIN,
            width: 100 + 2 * CONTEXT_MIN,
            height: 60 + 2 * CONTEXT_MIN
        }
    );
    // A change touching the left edge: context clamps at 0 on that side.
    let after = changed(&base, 0, 10, 0, 1500);
    let focus = Changes::between(&base, &after)
        .unwrap()
        .focus(Topology::Bounded)
        .unwrap();
    assert_eq!(
        focus,
        Focus {
            x: 0,
            y: 0,
            width: 10 + CONTEXT_MIN,
            height: 1500
        }
    );
    // A wide change caps its context at CONTEXT_MAX.
    let after = changed(&base, 200, 1800, 100, 1400);
    let focus = Changes::between(&base, &after)
        .unwrap()
        .focus(Topology::Bounded)
        .unwrap();
    assert_eq!(
        focus,
        Focus {
            x: 0,
            y: 0,
            width: 2000,
            height: 1500
        }
    );
    let after = changed(&base, 700, 1300, 700, 800);
    let focus = Changes::between(&base, &after)
        .unwrap()
        .focus(Topology::Bounded)
        .unwrap();
    assert_eq!(focus.x, 700 - 300);
    assert_eq!(focus.width, 600 + 600);
    let base = flat(4000, 3000);
    let after = changed(&base, 500, 3500, 1000, 1100);
    let focus = Changes::between(&base, &after)
        .unwrap()
        .focus(Topology::Bounded)
        .unwrap();
    // The context (CONTEXT_MAX each side) clamps at both edges: the whole width.
    assert_eq!(focus.x, 0);
    assert_eq!(focus.width, 4000);
    let after = changed(&base, 600, 3300, 1000, 1100);
    let focus = Changes::between(&base, &after)
        .unwrap()
        .focus(Topology::Bounded)
        .unwrap();
    assert_eq!(focus.x, 600 - CONTEXT_MAX);
    assert_eq!(focus.width, 2700 + 2 * CONTEXT_MAX);
}

#[test]
fn a_change_across_the_wrap_edge_is_one_region_through_the_edge() {
    let base = flat(1000, 500);
    // Columns 990..1000 and 0..10 changed: on a wrap frame one 20-wide region through the
    // edge; on a bounded frame the whole width.
    let after = changed(&base, 990, 1010, 100, 200);
    let changes = Changes::between(&base, &after).unwrap();
    let focus = changes.focus(Topology::Wrap).unwrap();
    assert_eq!(
        focus,
        Focus {
            x: 990 - CONTEXT_MIN,
            y: 100 - CONTEXT_MIN,
            width: 20 + 2 * CONTEXT_MIN,
            height: 100 + 2 * CONTEXT_MIN
        }
    );
    let focus = changes.focus(Topology::Bounded).unwrap();
    assert_eq!((focus.x, focus.width), (0, 1000));
    // The crop takes columns modulo the width: its middle columns are the changed ones.
    let crop = focus_crop(&changes, &after);
    assert_eq!(
        (crop.width(), crop.height()),
        (20 + 2 * CONTEXT_MIN, 100 + 2 * CONTEXT_MIN)
    );
    assert_eq!(
        crop.get_pixel(CONTEXT_MIN - 1, CONTEXT_MIN),
        &Rgb([90, 120, 150])
    );
    assert_eq!(
        crop.get_pixel(CONTEXT_MIN, CONTEXT_MIN),
        &Rgb([200, 10, 10])
    );
    assert_eq!(
        crop.get_pixel(CONTEXT_MIN + 19, CONTEXT_MIN),
        &Rgb([200, 10, 10])
    );
    assert_eq!(
        crop.get_pixel(CONTEXT_MIN + 20, CONTEXT_MIN),
        &Rgb([90, 120, 150])
    );
    // A change so wide its context meets itself around the frame is the whole width.
    let after = changed(&base, 100, 900, 0, 500);
    let focus = Changes::between(&base, &after)
        .unwrap()
        .focus(Topology::Wrap)
        .unwrap();
    assert_eq!(
        focus,
        Focus {
            x: 0,
            y: 0,
            width: 1000,
            height: 500
        }
    );
}

fn focus_crop(changes: &Changes, image: &DynamicImage) -> RgbImage {
    changes.focus(Topology::Wrap).unwrap().crop(image)
}

#[test]
fn joined_changes_frame_both_sides() {
    let base = flat(1000, 500);
    let left = changed(&base, 100, 120, 100, 120);
    let right = changed(&base, 300, 320, 300, 320);
    let mut changes = Changes::between(&base, &left).unwrap();
    changes
        .join(&Changes::between(&base, &right).unwrap())
        .unwrap();
    let focus = changes.focus(Topology::Bounded).unwrap();
    // The joined extent is 100..320 on both axes, with 110 of context clamped at the edge.
    assert_eq!(focus.x, 0);
    assert_eq!(focus.width, 320 + 110);
    assert_eq!(focus.y, 0);
    assert_eq!(focus.height, 320 + 110);
    let err = changes
        .join(&Changes::between(&flat(10, 10), &flat(10, 10)).unwrap())
        .unwrap_err();
    assert!(err.to_string().contains("10x10"), "{err}");
}

#[test]
fn the_centre_is_the_middle_of_the_region_modulo_the_width() {
    let focus = Focus {
        x: 990 - CONTEXT_MIN,
        y: 100,
        width: 20 + 2 * CONTEXT_MIN,
        height: 100,
    };
    assert_eq!(focus.centre(1000), (0.0, 150.0));
    let focus = Focus {
        x: 10,
        y: 20,
        width: 30,
        height: 40,
    };
    assert_eq!(focus.centre(1000), (25.0, 40.0));
}
