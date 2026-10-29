//! Task 1.8b: `rasterize` per kind, and the `line` and `stroke` finders.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use image::DynamicImage;
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, FrameTable, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::location::Location;
use rascal_core::op::{Context, Inputs, Op, Params, Status};
use rascal_ops::line::Line;
use rascal_ops::rasterize::{Rasterize, dilate_path, fill_polygon};
use rascal_ops::stroke::Stroke;

const W: u32 = 60;
const H: u32 = 40;

fn world() -> (Frame, FrameTable, Artifact) {
    let frame = Frame::raw(W, H, Projection::Flat);
    let image = Image::new(
        "i",
        DynamicImage::ImageRgb8(rascal_testkit::images::flat(W, H, 90)),
        &frame,
        Role::Canonical,
        None,
    );
    let table = FrameTable {
        frames: vec![frame.clone()],
    };
    (frame, table, Artifact::Image(Arc::new(image)))
}

fn rasterize(location: Location, table: &FrameTable, image: &Artifact) -> rascal_core::op::Output {
    let frame = table.frames[0].clone();
    let identity = location.identity(&frame.identity);
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("location".into(), Artifact::Location { location, identity });
    inputs.insert("image".into(), image.clone());
    let op = Rasterize;
    let params = op.manifest().resolve_params(&Params::new()).unwrap();
    op.run(&Context { frames: table }, &inputs, &params)
        .unwrap()
}

fn core_of(out: &rascal_core::op::Output) -> image::GrayImage {
    let Artifact::Mask(mask) = &out.outputs["mask"] else {
        panic!("{:?}", out.envelope)
    };
    mask.core.clone()
}

fn count(core: &image::GrayImage) -> usize {
    core.pixels().filter(|p| p[0] != 0).count()
}

#[test]
fn bbox_and_whole_image_as_before() {
    let (frame, table, image) = world();
    let out = rasterize(
        Location::bbox(&frame.id, [0.1, 0.25, 0.5, 0.75]).unwrap(),
        &table,
        &image,
    );
    assert_eq!(out.envelope.status, Status::Ok);
    assert_eq!(count(&core_of(&out)), 24 * 20);
    let out = rasterize(
        Location::WholeImage {
            frame: frame.id.clone(),
        },
        &table,
        &image,
    );
    assert_eq!(count(&core_of(&out)), (W * H) as usize);
}

#[test]
fn a_polygon_is_filled_as_drawn() {
    let (frame, table, image) = world();
    // A right triangle covering half of a 30×20 box: area 300 pixels, give or take the edge.
    let tri = Location::Polygon {
        frame: frame.id.clone(),
        polygon: vec![[0.0, 0.0], [0.5, 0.0], [0.0, 0.5]],
    };
    let out = rasterize(tri, &table, &image);
    assert_eq!(out.envelope.status, Status::Ok);
    let core = core_of(&out);
    let n = count(&core);
    assert!((285..=315).contains(&n), "{n}");
    assert_eq!(core.get_pixel(1, 1)[0], 255);
    assert_eq!(core.get_pixel(28, 18)[0], 0, "outside the hypotenuse");
    assert_eq!(core.get_pixel(40, 30)[0], 0);
    // The pure function agrees with brute-force point-in-polygon for a concave shape.
    // Nudged off the pixel grid so no pixel centre lies exactly on an edge, where the two tie
    // rules would legitimately differ.
    let star: Vec<[f64; 2]> = [
        [0.5, 0.05],
        [0.6, 0.4],
        [0.95, 0.4],
        [0.65, 0.6],
        [0.8, 0.95],
        [0.5, 0.7],
        [0.2, 0.95],
        [0.35, 0.6],
        [0.05, 0.4],
        [0.4, 0.4],
    ]
    .into_iter()
    .map(|[x, y]| [x + 0.00137, y + 0.00271])
    .collect();
    let filled = fill_polygon(&star, W, H);
    for y in 0..H {
        for x in 0..W {
            let (px, py) = (
                (f64::from(x) + 0.5) / f64::from(W),
                (f64::from(y) + 0.5) / f64::from(H),
            );
            let mut inside = false;
            let n = star.len();
            let mut j = n - 1;
            for i in 0..n {
                let (xi, yi) = (star[i][0], star[i][1]);
                let (xj, yj) = (star[j][0], star[j][1]);
                if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
                    inside = !inside;
                }
                j = i;
            }
            assert_eq!(filled.get_pixel(x, y)[0] != 0, inside, "({x},{y})");
        }
    }
}

#[test]
fn a_column_line_is_a_band_of_its_width_at_the_column() {
    let (frame, table, image) = world();
    let line = Location::Line {
        frame: frame.id.clone(),
        column: Some(0.5),
        path: None,
        width_px: 6.0,
        group: None,
    };
    let out = rasterize(line, &table, &image);
    assert_eq!(out.envelope.status, Status::Ok);
    let core = core_of(&out);
    // Centre x = 30: columns whose centres lie within 3 px: 27, 28, 29, 30, 31, 32.
    for x in 0..W {
        let expected = (27..=32).contains(&x);
        assert_eq!(core.get_pixel(x, 0)[0] != 0, expected, "x {x}");
    }
    assert_eq!(count(&core), 6 * H as usize);
    // The spec's rule is |x + 0.5 − c| ≤ half: at an integer centre a width of 1 ties on both
    // neighbours and paints two columns; a centre between pixels paints one.
    let thin = Location::Line {
        frame: frame.id.clone(),
        column: Some(0.5),
        path: None,
        width_px: 1.0,
        group: None,
    };
    assert_eq!(
        count(&core_of(&rasterize(thin, &table, &image))),
        2 * H as usize
    );
    let between = Location::Line {
        frame: frame.id.clone(),
        column: Some(30.5 / 60.0),
        path: None,
        width_px: 1.0,
        group: None,
    };
    assert_eq!(
        count(&core_of(&rasterize(between, &table, &image))),
        H as usize
    );
}

#[test]
fn a_path_line_and_a_stroke_are_the_path_dilated() {
    let (frame, table, image) = world();
    let path = vec![[0.1, 0.5], [0.9, 0.5]];
    let line = Location::Line {
        frame: frame.id.clone(),
        column: None,
        path: Some(path.clone()),
        width_px: 8.0,
        group: None,
    };
    let out = rasterize(line, &table, &image);
    let core = core_of(&out);
    // A horizontal segment from x = 6 to x = 54 at y = 20, dilated by 4: rows with centres within 4.
    assert_eq!(core.get_pixel(30, 20)[0], 255);
    assert_eq!(core.get_pixel(30, 16)[0], 255, "3.5 px away");
    assert_eq!(core.get_pixel(30, 23)[0], 255, "3.5 px away");
    assert_eq!(core.get_pixel(30, 15)[0], 0, "4.5 px away");
    assert_eq!(core.get_pixel(30, 24)[0], 0, "4.5 px away");
    assert_eq!(
        core.get_pixel(2, 20)[0],
        255,
        "the round cap reaches 4 px past the end"
    );
    assert_eq!(core.get_pixel(1, 20)[0], 0);
    // The stroke with the same radius is the same mask.
    let stroke = Location::Stroke {
        frame: frame.id.clone(),
        path,
        radius_px: 4.0,
        group: None,
    };
    let s = core_of(&rasterize(stroke, &table, &image));
    assert_eq!(s.as_raw(), core.as_raw());
    // The pure function against brute force on a bent path.
    let bent = [[0.2, 0.2], [0.5, 0.8], [0.8, 0.3]];
    let d = dilate_path(&bent, 3.0, W, H);
    let pts: Vec<(f64, f64)> = bent
        .iter()
        .map(|p| (p[0] * f64::from(W), p[1] * f64::from(H)))
        .collect();
    for y in 0..H {
        for x in 0..W {
            let (cx, cy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let dist = pts
                .windows(2)
                .map(|s| {
                    let (ax, ay) = s[0];
                    let (bx, by) = s[1];
                    let (dx, dy) = (bx - ax, by - ay);
                    let t =
                        (((cx - ax) * dx + (cy - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                    ((cx - ax - t * dx).powi(2) + (cy - ay - t * dy).powi(2)).sqrt()
                })
                .fold(f64::INFINITY, f64::min);
            assert_eq!(
                d.get_pixel(x, y)[0] != 0,
                dist <= 3.0 + 1e-9,
                "({x},{y}) dist {dist}"
            );
        }
    }
}

#[test]
fn view_and_absent_are_not_applicable_and_publish_nothing() {
    let (frame, table, image) = world();
    let pano = Frame::raw(80, 40, Projection::Equirectangular);
    let ptable = FrameTable {
        frames: vec![pano.clone()],
    };
    let pimage = Artifact::Image(Arc::new(Image::new(
        "p",
        DynamicImage::ImageRgb8(rascal_testkit::images::flat(80, 40, 90)),
        &pano,
        Role::Canonical,
        None,
    )));
    let view = Location::View {
        frame: pano.id.clone(),
        yaw: 0.0,
        pitch: 0.0,
        fov: 60.0,
    };
    let out = rasterize(view, &ptable, &pimage);
    assert_eq!(out.envelope.status, Status::NotApplicable);
    assert!(out.outputs.is_empty() && out.frames.is_empty());
    let absent = Location::Absent {
        frame: frame.id.clone(),
    };
    let out = rasterize(absent, &table, &image);
    assert_eq!(out.envelope.status, Status::NotApplicable);
    assert!(out.outputs.is_empty());
    // A location that covers no pixel is not applicable either.
    let off = Location::bbox(&frame.id, [1.5, 1.5, 2.0, 2.0]).unwrap();
    assert_eq!(
        rasterize(off, &table, &image).envelope.status,
        Status::NotApplicable
    );
}

#[test]
fn the_line_and_stroke_ops_make_validated_locations() {
    let (frame, table, image) = world();
    let line = Line;
    let manifest = line.manifest();
    manifest.check().unwrap();
    let run = |text: &str| {
        let mut inputs: Inputs = BTreeMap::new();
        inputs.insert("image".into(), image.clone());
        let params: Params = toml::from_str(text).unwrap();
        let params = manifest.resolve_params(&params).unwrap();
        line.run(&Context { frames: &table }, &inputs, &params)
            .unwrap()
    };
    let out = run("column = 0.5\nwidth_px = 3");
    assert_eq!(out.envelope.status, Status::Ok);
    let Artifact::Location { location, .. } = &out.outputs["location"] else {
        panic!()
    };
    assert!(
        matches!(location, Location::Line { column: Some(c), path: None, width_px, .. } if *c == 0.5 && *width_px == 3.0)
    );
    assert_eq!(location.frame(), frame.id);
    let out = run("path = [[0.1, 0.1], [0.5, 0.9]]\nwidth_px = 2");
    assert_eq!(out.envelope.status, Status::Ok);
    let out = run("width_px = 2");
    assert_eq!(out.envelope.status, Status::InvalidArgs, "neither form");
    assert!(
        out.envelope
            .reason
            .as_deref()
            .unwrap()
            .contains("exactly one of column and path")
    );
    let out = run("column = 0.5\npath = [[0.1, 0.1], [0.5, 0.9]]\nwidth_px = 2");
    assert_eq!(out.envelope.status, Status::InvalidArgs, "both forms");
    assert!(
        manifest
            .resolve_params(&toml::from_str("column = 1.5\nwidth_px = 2").unwrap())
            .unwrap_err()
            .contains("column must be in [0, 1]")
    );
    assert!(
        manifest
            .resolve_params(&toml::from_str("path = [[0.1, 0.1]]\nwidth_px = 2").unwrap())
            .unwrap_err()
            .contains("at least 2 points")
    );

    let stroke = Stroke;
    let smanifest = stroke.manifest();
    smanifest.check().unwrap();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), image.clone());
    let params = smanifest
        .resolve_params(&toml::from_str("path = [[0.1, 0.1], [0.3, 0.3]]\nradius_px = 4").unwrap())
        .unwrap();
    let out = stroke
        .run(&Context { frames: &table }, &inputs, &params)
        .unwrap();
    assert_eq!(out.envelope.status, Status::Ok);
    let Artifact::Location { location, .. } = &out.outputs["location"] else {
        panic!()
    };
    assert!(matches!(location, Location::Stroke { radius_px, .. } if *radius_px == 4.0));
    assert!(
        smanifest
            .resolve_params(&toml::from_str("path = [[0.1, 0.1], [0.3, 0.3]]").unwrap())
            .unwrap_err()
            .contains("radius_px is required")
    );
    // Through rasterize, the stroke draws.
    let out = rasterize(location.clone(), &table, &image);
    assert_eq!(out.envelope.status, Status::Ok);
    assert!(count(&core_of(&out)) > 50);
}
