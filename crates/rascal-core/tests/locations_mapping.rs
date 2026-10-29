//! Task 1.2: the seven location kinds' validity rules and mapping toward the root.
#![allow(clippy::unwrap_used)]

use rascal_core::frame::{Frame, FrameTable, Plan, Projection};
use rascal_core::location::{Location, SEAM_RULE};
use rascal_core::mapping::{self, Mapping};

fn flat() -> Frame {
    Frame::raw(320, 200, Projection::Flat)
}
fn pano() -> Frame {
    Frame::raw(4000, 2000, Projection::Equirectangular)
}
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}
fn assert_bbox(location: &Location, expected: [f64; 4]) {
    let got = bbox_of(location);
    assert!(
        got.iter().zip(&expected).all(|(a, b)| close(*a, *b)),
        "got {got:?}, expected {expected:?}"
    );
}
/// Equal to within floating-point rounding: a mapped-and-unmapped coordinate differs from the
/// original by at most a few ulps.
fn assert_same(a: &Location, b: &Location) {
    assert_eq!(a.kind(), b.kind());
    assert_eq!(a.frame(), b.frame());
    let va = serde_json::to_value(a).unwrap();
    let vb = serde_json::to_value(b).unwrap();
    fn walk(x: &serde_json::Value, y: &serde_json::Value) {
        match (x, y) {
            (serde_json::Value::Number(p), serde_json::Value::Number(q)) => {
                assert!(
                    close(p.as_f64().unwrap(), q.as_f64().unwrap()),
                    "{p} vs {q}"
                )
            }
            (serde_json::Value::Array(p), serde_json::Value::Array(q)) => {
                assert_eq!(p.len(), q.len());
                p.iter().zip(q).for_each(|(i, j)| walk(i, j));
            }
            (serde_json::Value::Object(p), serde_json::Value::Object(q)) => {
                assert_eq!(p.keys().collect::<Vec<_>>(), q.keys().collect::<Vec<_>>());
                p.iter().for_each(|(k, i)| walk(i, &q[k]));
            }
            _ => assert_eq!(x, y),
        }
    }
    walk(&va, &vb);
}
fn bbox_of(location: &Location) -> [f64; 4] {
    match location {
        Location::Bbox { bbox, .. } => *bbox,
        other => panic!("not a bbox: {other:?}"),
    }
}

// --- validity -----------------------------------------------------------------------------------

#[test]
fn every_kind_validates_and_each_rule_refuses() {
    let f = flat();
    let p = pano();
    let ok =
        |l: Location, frame: &Frame| l.validate(frame).unwrap_or_else(|e| panic!("{l:?}: {e}"));
    let bad = |l: Location, frame: &Frame, needle: &str| {
        let e = l.validate(frame).expect_err(&format!("{l:?} should fail"));
        assert!(e.to_string().contains(needle), "{l:?}: {e}");
    };
    let fid = f.id.clone();
    let pid = p.id.clone();

    ok(
        Location::Bbox {
            frame: fid.clone(),
            bbox: [0.1, 0.1, 0.5, 0.5],
        },
        &f,
    );
    ok(
        Location::Bbox {
            frame: fid.clone(),
            bbox: [-0.1, 0.1, 0.5, 0.5],
        },
        &f,
    );
    bad(
        Location::Bbox {
            frame: fid.clone(),
            bbox: [0.5, 0.1, 0.5, 0.5],
        },
        &f,
        "x0 < x1",
    );
    bad(
        Location::Bbox {
            frame: fid.clone(),
            bbox: [0.1, 0.1, f64::NAN, 0.5],
        },
        &f,
        "finite",
    );
    bad(
        Location::Bbox {
            frame: pid.clone(),
            bbox: [-0.1, 0.1, 0.5, 0.5],
        },
        &p,
        SEAM_RULE,
    );
    bad(
        Location::Bbox {
            frame: pid.clone(),
            bbox: [0.9, 0.1, 1.1, 0.5],
        },
        &p,
        "seam rule",
    );
    bad(
        Location::Bbox {
            frame: fid.clone(),
            bbox: [0.1, 0.1, 0.5, 0.5],
        },
        &p,
        "names frame",
    );

    ok(
        Location::Polygon {
            frame: fid.clone(),
            polygon: vec![[0.1, 0.1], [0.5, 0.1], [0.3, 0.4]],
        },
        &f,
    );
    bad(
        Location::Polygon {
            frame: fid.clone(),
            polygon: vec![[0.1, 0.1], [0.5, 0.1]],
        },
        &f,
        "at least 3",
    );
    bad(
        Location::Polygon {
            frame: fid.clone(),
            polygon: vec![[0.0, 0.0], [1.0, 1.0], [1.0, 0.0], [0.0, 1.0]],
        },
        &f,
        "simple",
    );
    bad(
        Location::Polygon {
            frame: fid.clone(),
            polygon: (0..65)
                .map(|i| [i as f64 / 65.0, (i % 2) as f64 * 0.5])
                .collect(),
        },
        &f,
        "at most 64",
    );
    bad(
        Location::Polygon {
            frame: pid.clone(),
            polygon: vec![[-0.1, 0.1], [0.5, 0.1], [0.3, 0.4]],
        },
        &p,
        "seam rule",
    );

    ok(
        Location::View {
            frame: pid.clone(),
            yaw: 90.0,
            pitch: -10.0,
            fov: 60.0,
        },
        &p,
    );
    bad(
        Location::View {
            frame: fid.clone(),
            yaw: 90.0,
            pitch: -10.0,
            fov: 60.0,
        },
        &f,
        "does not wrap",
    );
    bad(
        Location::View {
            frame: pid.clone(),
            yaw: 190.0,
            pitch: 0.0,
            fov: 60.0,
        },
        &p,
        "yaw",
    );
    bad(
        Location::View {
            frame: pid.clone(),
            yaw: 0.0,
            pitch: 0.0,
            fov: 5.0,
        },
        &p,
        "fov",
    );

    ok(Location::WholeImage { frame: fid.clone() }, &f);
    ok(Location::Absent { frame: fid.clone() }, &f);

    ok(
        Location::Line {
            frame: fid.clone(),
            column: Some(0.5),
            path: None,
            width_px: 3.0,
            group: None,
        },
        &f,
    );
    ok(
        Location::Line {
            frame: fid.clone(),
            column: None,
            path: Some(vec![[0.1, 0.0], [0.2, 1.0]]),
            width_px: 3.0,
            group: None,
        },
        &f,
    );
    bad(
        Location::Line {
            frame: fid.clone(),
            column: Some(0.5),
            path: Some(vec![[0.1, 0.0], [0.2, 1.0]]),
            width_px: 3.0,
            group: None,
        },
        &f,
        "exactly one",
    );
    bad(
        Location::Line {
            frame: fid.clone(),
            column: None,
            path: None,
            width_px: 3.0,
            group: None,
        },
        &f,
        "exactly one",
    );
    bad(
        Location::Line {
            frame: fid.clone(),
            column: Some(1.5),
            path: None,
            width_px: 3.0,
            group: None,
        },
        &f,
        "[0, 1]",
    );
    bad(
        Location::Line {
            frame: fid.clone(),
            column: Some(0.5),
            path: None,
            width_px: 0.0,
            group: None,
        },
        &f,
        "positive",
    );
    bad(
        Location::Line {
            frame: fid.clone(),
            column: None,
            path: Some(vec![[0.1, 0.0]]),
            width_px: 3.0,
            group: None,
        },
        &f,
        "at least 2",
    );

    ok(
        Location::Stroke {
            frame: fid.clone(),
            path: vec![[0.1, 0.1], [0.2, 0.2]],
            radius_px: 4.0,
            group: None,
        },
        &f,
    );
    bad(
        Location::Stroke {
            frame: fid.clone(),
            path: vec![[0.1, 0.1], [0.2, 0.2]],
            radius_px: -1.0,
            group: None,
        },
        &f,
        "positive",
    );
    bad(
        Location::Stroke {
            frame: fid,
            path: vec![[0.1, 0.1]],
            radius_px: 4.0,
            group: None,
        },
        &f,
        "at least 2",
    );
}

// --- mapping --------------------------------------------------------------------------------------

fn crop_world() -> (FrameTable, Frame, Frame) {
    let parent = flat();
    let crop = Frame::crop(&parent, 1, [64, 40, 128, 100]).unwrap();
    let mut table = FrameTable::default();
    table.insert(parent.clone());
    table.insert(crop.clone());
    (table, parent, crop)
}

fn cut_world() -> (FrameTable, Frame, Frame) {
    let parent = pano();
    let plan = Plan::new(&parent, 0.07).unwrap(); // s = 280 of 4000
    let cut = Frame::cut(&parent, "img", &plan).unwrap();
    let mut table = FrameTable::default();
    table.insert(parent.clone());
    table.insert(cut.clone());
    (table, parent, cut)
}

#[test]
fn a_raw_location_maps_to_itself() {
    let (table, parent, _) = crop_world();
    let l = Location::bbox(&parent.id, [0.1, 0.1, 0.5, 0.5]).unwrap();
    let m = mapping::to_root(&l, &table, "loc-1").unwrap();
    assert_eq!(
        m,
        Mapping {
            parts: vec![l],
            partial: false,
            source: "loc-1".to_string()
        }
    );
}

#[test]
fn crop_mappings_scale_and_offset_and_round_trip_exactly_when_unclipped() {
    let (table, parent, crop) = crop_world();
    // The crop is columns 64..128 (of 320) and rows 40..100 (of 200): powers of two, so the
    // arithmetic is exact.
    let l = Location::bbox(&crop.id, [0.25, 0.5, 0.75, 1.0]).unwrap();
    let up = mapping::to_root(&l, &table, "l").unwrap();
    assert!(!up.partial);
    assert_bbox(&up.parts[0], [0.25, 0.35, 0.35, 0.5]);
    assert_eq!(up.parts[0].frame(), parent.id);
    let down = mapping::from_parent(&up.parts[0], &crop, &parent, "l").unwrap();
    assert!(!down.partial);
    assert_eq!(down.parts.len(), 1);
    assert_same(&down.parts[0], &l);

    // A polygon and a stroke go the same way.
    let poly = Location::Polygon {
        frame: crop.id.clone(),
        polygon: vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
    };
    let up = mapping::to_root(&poly, &table, "p").unwrap();
    let down = mapping::from_parent(&up.parts[0], &crop, &parent, "p").unwrap();
    assert_same(&down.parts[0], &poly);
    let stroke = Location::Stroke {
        frame: crop.id.clone(),
        path: vec![[0.0, 0.0], [0.5, 0.5], [1.0, 0.25]],
        radius_px: 3.0,
        group: None,
    };
    let up = mapping::to_root(&stroke, &table, "s").unwrap();
    assert!(
        matches!(&up.parts[0], Location::Stroke { radius_px, .. } if *radius_px == 3.0),
        "pixel radii are unchanged"
    );
    let down = mapping::from_parent(&up.parts[0], &crop, &parent, "s").unwrap();
    assert_same(&down.parts[0], &stroke);

    // The whole crop is a bbox of the parent.
    let whole = Location::WholeImage {
        frame: crop.id.clone(),
    };
    let up = mapping::to_root(&whole, &table, "w").unwrap();
    assert_bbox(&up.parts[0], [0.2, 0.2, 0.4, 0.5]);
}

#[test]
fn geometry_partly_outside_is_clipped_and_flagged_and_wholly_outside_is_an_error() {
    let (_, parent, crop) = crop_world();
    // Parent bbox 0.1..0.3 × 0.1..0.3 overlaps the crop (0.2..0.4 × 0.2..0.5) in part.
    let l = Location::bbox(&parent.id, [0.1, 0.1, 0.3, 0.3]).unwrap();
    let down = mapping::from_parent(&l, &crop, &parent, "l").unwrap();
    assert!(down.partial);
    let b = bbox_of(&down.parts[0]);
    assert!(
        close(b[0], 0.0) && close(b[1], 0.0) && close(b[2], 0.5) && close(b[3], 1.0 / 3.0),
        "{b:?}"
    );
    // Wholly outside.
    let l = Location::bbox(&parent.id, [0.5, 0.6, 0.7, 0.8]).unwrap();
    let e = mapping::from_parent(&l, &crop, &parent, "l").unwrap_err();
    assert!(e.to_string().contains("wholly outside"), "{e}");
    // A stroke crossing the crop edge is clipped to the inside and flagged.
    let s = Location::Stroke {
        frame: parent.id.clone(),
        path: vec![[0.1, 0.3], [0.3, 0.3]],
        radius_px: 2.0,
        group: None,
    };
    let down = mapping::from_parent(&s, &crop, &parent, "s").unwrap();
    assert!(down.partial);
    let Location::Stroke { path, .. } = &down.parts[0] else {
        panic!()
    };
    assert_eq!(path.len(), 2);
    assert!(close(path[0][0], 0.0) && close(path[1][0], 0.5), "{path:?}");
    // A polygon straddling the edge is clipped to the inside.
    let p = Location::Polygon {
        frame: parent.id.clone(),
        polygon: vec![[0.1, 0.25], [0.3, 0.25], [0.3, 0.45], [0.1, 0.45]],
    };
    let down = mapping::from_parent(&p, &crop, &parent, "p").unwrap();
    assert!(down.partial);
    let Location::Polygon { polygon, .. } = &down.parts[0] else {
        panic!()
    };
    assert!(
        polygon.iter().all(|q| q[0] >= -1e-9 && q[0] <= 0.5 + 1e-9),
        "{polygon:?}"
    );
}

#[test]
fn cut_mappings_split_at_the_centre_and_land_on_both_edges() {
    let (table, parent, cut) = cut_world();
    // s = 280, W = 4000: canvas x 0..0.5 is parent 0.93..1.0, 0.5..1 is parent 0..0.07.
    let left = Location::bbox(&cut.id, [0.0, 0.2, 0.25, 0.4]).unwrap();
    let up = mapping::to_root(&left, &table, "a").unwrap();
    assert!(!up.partial && up.parts.len() == 1);
    assert_bbox(&up.parts[0], [0.93, 0.2, 0.965, 0.4]);
    assert_eq!(up.parts[0].frame(), parent.id);
    let across = Location::bbox(&cut.id, [0.25, 0.2, 0.75, 0.4]).unwrap();
    let up = mapping::to_root(&across, &table, "b").unwrap();
    assert!(!up.partial, "a split at the seam is not clipping");
    assert_eq!(up.parts.len(), 2);
    assert_bbox(&up.parts[0], [0.965, 0.2, 1.0, 0.4]);
    assert_bbox(&up.parts[1], [0.0, 0.2, 0.035, 0.4]);
    // Each part maps back to its own half; the split is not inverted by one step.
    for part in &up.parts {
        let down = mapping::from_parent(part, &cut, &parent, "b").unwrap();
        assert!(!down.partial && down.parts.len() == 1, "{down:?}");
    }
    // The centre column is the wrap edge: it lands on the parent's column 0.
    let seam = Location::Line {
        frame: cut.id.clone(),
        column: Some(0.5),
        path: None,
        width_px: 3.0,
        group: None,
    };
    let up = mapping::to_root(&seam, &table, "c").unwrap();
    assert!(
        matches!(&up.parts[0], Location::Line { column: Some(c), width_px, .. } if *c == 0.0 && *width_px == 3.0),
        "{up:?}"
    );
    // A path crossing the centre splits into two parts sharing one group.
    let path = Location::Line {
        frame: cut.id.clone(),
        column: None,
        path: Some(vec![[0.4, 0.1], [0.6, 0.9]]),
        width_px: 2.0,
        group: None,
    };
    let up = mapping::to_root(&path, &table, "d").unwrap();
    assert_eq!(up.parts.len(), 2);
    assert!(!up.partial);
    let groups: Vec<Option<String>> = up
        .parts
        .iter()
        .map(|p| match p {
            Location::Line { group, .. } => group.clone(),
            _ => None,
        })
        .collect();
    assert_eq!(groups[0], groups[1]);
    assert!(groups[0].is_some());
    // The whole canvas is the two edge spans of the panorama.
    let up = mapping::to_root(
        &Location::WholeImage {
            frame: cut.id.clone(),
        },
        &table,
        "e",
    )
    .unwrap();
    assert_eq!(up.parts.len(), 2);
    assert_bbox(&up.parts[0], [0.93, 0.0, 1.0, 1.0]);
    assert_bbox(&up.parts[1], [0.0, 0.0, 0.07, 1.0]);
    // Parent geometry in the middle is outside the canvas: clipped, or wholly outside.
    let middle = Location::bbox(&parent.id, [0.4, 0.1, 0.6, 0.2]).unwrap();
    assert!(
        mapping::from_parent(&middle, &cut, &parent, "f")
            .unwrap_err()
            .to_string()
            .contains("wholly outside")
    );
    let spanning = Location::bbox(&parent.id, [0.0, 0.1, 1.0, 0.2]).unwrap();
    let down = mapping::from_parent(&spanning, &cut, &parent, "g").unwrap();
    assert!(down.partial && down.parts.len() == 2, "{down:?}");
    // A view does not map; an absent location becomes an absent location.
    let view = Location::View {
        frame: parent.id.clone(),
        yaw: 0.0,
        pitch: 0.0,
        fov: 60.0,
    };
    assert!(mapping::from_parent(&view, &cut, &parent, "v").is_err());
    let absent = Location::Absent {
        frame: cut.id.clone(),
    };
    assert_eq!(
        mapping::to_root(&absent, &table, "x").unwrap().parts,
        vec![Location::Absent {
            frame: parent.id.clone()
        }]
    );
}

#[test]
fn a_chain_of_two_steps_maps_through_both() {
    let (mut table, parent, cut) = cut_world();
    let crop = Frame::crop(&cut, 1, [140, 0, 420, 2000]).unwrap(); // the middle half of the canvas, across the seam
    table.insert(crop.clone());
    let l = Location::bbox(&crop.id, [0.0, 0.0, 1.0, 1.0]).unwrap();
    let up = mapping::to_root(&l, &table, "l").unwrap();
    assert_eq!(up.parts.len(), 2, "{up:?}");
    assert!(up.parts.iter().all(|p| p.frame() == parent.id));
    assert!(!up.partial);
    let a = bbox_of(&up.parts[0]);
    let b = bbox_of(&up.parts[1]);
    assert!(close(a[0], 0.965) && close(a[2], 1.0), "{a:?}");
    assert!(close(b[0], 0.0) && close(b[2], 0.035), "{b:?}");
}
