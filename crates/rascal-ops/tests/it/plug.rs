//! Task 13.3: `plug` (SPEC.md "Self-correcting remove", part 1). Holes are 4-connected unset
//! regions touching no image edge; each is filled when its area is at most `max_share` of the
//! largest 8-connected blob beside it, with every label and area taken before anything fills.
//! Checked on the cases the spec names and against an independent flood-fill reference.
#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use image::{GrayImage, Luma};
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, FrameTable, Projection};
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{Context, Inputs, Op, Output, Params, Status};
use rascal_ops::plug::Plug;

const W: u32 = 80;
const H: u32 = 80;

fn frame() -> Frame {
    Frame::raw(W, H, Projection::Flat)
}

/// True inside the half-open rectangle `[x0, x1) × [y0, y1)`.
fn in_rect(x: u32, y: u32, r: [u32; 4]) -> bool {
    (r[0]..r[1]).contains(&x) && (r[2]..r[3]).contains(&y)
}

fn plane(set: impl Fn(u32, u32) -> bool) -> GrayImage {
    GrayImage::from_fn(W, H, |x, y| Luma([if set(x, y) { 255 } else { 0 }]))
}

fn mask_of(core: GrayImage) -> Mask {
    Mask::from_core("m", &frame(), UseCase::Remove, core, Source::default()).unwrap()
}

fn run(mask: Mask, max_share: Option<f64>) -> Output {
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("mask".into(), Artifact::Mask(Arc::new(mask)));
    let mut given = Params::new();
    if let Some(share) = max_share {
        given.insert("max_share".into(), toml::Value::Float(share));
    }
    let resolved = Plug.manifest().resolve_params(&given).unwrap();
    let table = FrameTable {
        frames: vec![frame()],
    };
    Plug.run(&Context { frames: &table }, &inputs, &resolved)
        .unwrap()
}

fn core_out(output: &Output) -> GrayImage {
    match output.outputs.get("mask") {
        Some(Artifact::Mask(m)) => m.core.clone(),
        other => panic!("{other:?}"),
    }
}

fn measure(output: &Output, name: &str) -> f64 {
    output
        .envelope
        .measurements
        .iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("no measurement {name}"))
        .value
}

fn count(p: &GrayImage) -> usize {
    p.pixels().filter(|p| p[0] != 0).count()
}

/// The spec's rule written out plainly, for comparison: flood-fill every component, decide
/// every hole from the input's labels, then fill.
fn reference(core: &GrayImage, max_share: f64) -> GrayImage {
    let (w, h) = (core.width() as i64, core.height() as i64);
    let on = |x: i64, y: i64| {
        x >= 0 && y >= 0 && x < w && y < h && core.get_pixel(x as u32, y as u32)[0] != 0
    };
    let flood = |want: bool, eight: bool| {
        let mut label = vec![0usize; (w * h) as usize];
        let mut areas = vec![0usize];
        for start in 0..(w * h) {
            let (sx, sy) = (start % w, start / w);
            if on(sx, sy) != want || label[start as usize] != 0 {
                continue;
            }
            let id = areas.len();
            areas.push(0);
            let mut queue = VecDeque::from([(sx, sy)]);
            label[start as usize] = id;
            while let Some((x, y)) = queue.pop_front() {
                areas[id] += 1;
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        if (dx == 0 && dy == 0) || (!eight && dx != 0 && dy != 0) {
                            continue;
                        }
                        let (nx, ny) = (x + dx, y + dy);
                        if nx < 0 || ny < 0 || nx >= w || ny >= h || on(nx, ny) != want {
                            continue;
                        }
                        let k = (ny * w + nx) as usize;
                        if label[k] == 0 {
                            label[k] = id;
                            queue.push_back((nx, ny));
                        }
                    }
                }
            }
        }
        (label, areas)
    };
    let (fg, fg_areas) = flood(true, true);
    let (bg, bg_areas) = flood(false, false);
    let mut out = core.clone();
    for hole in 1..bg_areas.len() {
        let pixels: Vec<(i64, i64)> = (0..w * h)
            .filter(|&k| bg[k as usize] == hole)
            .map(|k| (k % w, k / w))
            .collect();
        if pixels
            .iter()
            .any(|&(x, y)| x == 0 || y == 0 || x == w - 1 || y == h - 1)
        {
            continue;
        }
        let mut owner: Option<usize> = None;
        for &(x, y) in &pixels {
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (nx, ny) = (x + dx, y + dy);
                    if on(nx, ny) {
                        let id = fg[(ny * w + nx) as usize];
                        // Labels are numbered in row-major order of their first pixel, so a
                        // lower id wins a tie in area.
                        owner = Some(match owner {
                            Some(o)
                                if fg_areas[o] > fg_areas[id]
                                    || (fg_areas[o] == fg_areas[id] && o < id) =>
                            {
                                o
                            }
                            _ => id,
                        });
                    }
                }
            }
        }
        let owner = owner.unwrap();
        if pixels.len() as f64 <= max_share * fg_areas[owner] as f64 {
            for (x, y) in pixels {
                out.put_pixel(x as u32, y as u32, Luma([255]));
            }
        }
    }
    out
}

#[test]
fn a_small_hole_in_a_ring_is_filled() {
    let core = plane(|x, y| in_rect(x, y, [20, 60, 20, 60]) && !in_rect(x, y, [38, 42, 38, 42]));
    let out = run(mask_of(core), None);
    assert_eq!(out.envelope.status, Status::Ok);
    assert_eq!(count(&core_out(&out)), 1600);
    assert_eq!(measure(&out, "holes_found"), 1.0);
    assert_eq!(measure(&out, "holes_filled"), 1.0);
    assert_eq!(measure(&out, "plugged_px"), 16.0);
}

#[test]
fn a_frame_keeps_its_large_picture_hole() {
    let core = plane(|x, y| in_rect(x, y, [10, 70, 10, 70]) && !in_rect(x, y, [14, 66, 14, 66]));
    let before = count(&core);
    let out = run(mask_of(core), None);
    assert_eq!(count(&core_out(&out)), before);
    assert_eq!(measure(&out, "holes_found"), 1.0);
    assert_eq!(measure(&out, "holes_filled"), 0.0);
}

#[test]
fn a_hole_touching_the_image_edge_is_not_a_hole() {
    let core = plane(|x, y| in_rect(x, y, [0, 30, 0, 30]) && !in_rect(x, y, [0, 5, 10, 15]));
    let before = count(&core);
    let out = run(mask_of(core), None);
    assert_eq!(count(&core_out(&out)), before);
    assert_eq!(measure(&out, "holes_found"), 0.0);
}

#[test]
fn a_pocket_open_only_diagonally_is_a_hole_and_an_orthogonal_opening_is_not() {
    // The block's corner pixel is cleared, so the pocket next to it reaches the outside only
    // across a corner: holes are 4-connected, so it is enclosed.
    let diagonal =
        plane(|x, y| in_rect(x, y, [5, 25, 5, 25]) && (x, y) != (5, 5) && (x, y) != (6, 6));
    let out = run(mask_of(diagonal), None);
    assert_eq!(measure(&out, "holes_filled"), 1.0);
    assert_eq!(core_out(&out).get_pixel(6, 6)[0], 255);
    assert_eq!(
        core_out(&out).get_pixel(5, 5)[0],
        0,
        "the corner is outside, not a hole"
    );
    // Clearing (6, 5) as well opens the pocket to the outside along an edge.
    let open = plane(|x, y| {
        in_rect(x, y, [5, 25, 5, 25]) && (x, y) != (5, 5) && (x, y) != (6, 6) && (x, y) != (6, 5)
    });
    let out = run(mask_of(open), None);
    assert_eq!(measure(&out, "holes_found"), 0.0);
    assert_eq!(core_out(&out).get_pixel(6, 6)[0], 0);
}

#[test]
fn merged_blobs_with_several_holes_follow_the_reference_whatever_the_order() {
    // Two overlapping squares (one blob) with holes of several sizes, and a separate small
    // blob whose hole is judged against its own area, not the big blob's.
    let core = plane(|x, y| {
        let big = in_rect(x, y, [5, 45, 5, 45]) || in_rect(x, y, [30, 70, 30, 70]);
        let small = in_rect(x, y, [55, 75, 2, 22]);
        let holes = in_rect(x, y, [10, 12, 10, 12])
            || in_rect(x, y, [20, 40, 20, 26])
            || in_rect(x, y, [50, 62, 50, 62])
            || in_rect(x, y, [62, 66, 8, 14]);
        (big || small) && !holes
    });
    for share in [0.0, 0.05, 0.1, 0.3, 1.0] {
        let out = run(mask_of(core.clone()), Some(share));
        assert_eq!(
            core_out(&out).as_raw(),
            reference(&core, share).as_raw(),
            "max_share {share}"
        );
    }
}

#[test]
fn a_protected_hole_stays_empty() {
    let core = plane(|x, y| in_rect(x, y, [20, 60, 20, 60]) && !in_rect(x, y, [38, 42, 38, 42]));
    let mut mask = mask_of(core);
    mask.protect_union(&plane(|x, y| in_rect(x, y, [38, 42, 38, 42])))
        .unwrap();
    let out = run(mask, None);
    let plugged = core_out(&out);
    assert_eq!(
        plugged.get_pixel(39, 39)[0],
        0,
        "protect wins over the fill"
    );
    assert_eq!(count(&plugged), 1600 - 16);
    assert_eq!(
        measure(&out, "plugged_px"),
        0.0,
        "a protected pixel is not counted as plugged"
    );
}

#[test]
fn max_share_zero_fills_nothing_and_one_fills_any_hole_up_to_its_blob() {
    let ring = plane(|x, y| in_rect(x, y, [20, 60, 20, 60]) && !in_rect(x, y, [38, 42, 38, 42]));
    assert_eq!(
        count(&core_out(&run(mask_of(ring.clone()), Some(0.0)))),
        1600 - 16
    );
    let frame_shape =
        plane(|x, y| in_rect(x, y, [10, 70, 10, 70]) && !in_rect(x, y, [14, 66, 14, 66]));
    // The frame's hole (2704 px) is larger than the frame itself (896 px): even 1.0 keeps it.
    let before = count(&frame_shape);
    assert_eq!(
        count(&core_out(&run(mask_of(frame_shape), Some(1.0)))),
        before
    );
    assert_eq!(count(&core_out(&run(mask_of(ring), Some(1.0)))), 1600);
}
