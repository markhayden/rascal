//! `rasterize`: a location's interior as a mask core, in the location's frame (SPEC.md
//! "Operations"): `bbox` and `polygon` as drawn, `whole_image` everything, a `line` in column
//! form a band `width_px` wide centred on the column and in path form the path dilated by
//! `width_px / 2`, a `stroke` the path dilated by `radius_px`; `view` and `absent` have no
//! interior and are `not_applicable`.

use std::collections::BTreeMap;

use image::GrayImage;
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::location::Location;
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};
use rascal_core::recipe::mask_artifact;

/// The op.
pub struct Rasterize;

const NAME: &str = "rasterize";

impl Op for Rasterize {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "0.1.0",
            "A location's interior as the core of a new mask in its frame: boxes and polygons as drawn, a line as a band of its width, a stroke as its path dilated by its radius.",
            "When a location (a box, a polygon, a line, a stroke, the whole image) should become a mask as drawn, before any refinement.",
            "When the mark is rough and should be tightened to the defect's edges (use tighten), or when the location is a view or absent, which have no interior.",
            "A mask whose core is the interior; blend and protect are empty. Not applicable for view and absent locations, or a location that covers no pixel.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.05,
            },
            vec![
                Port {
                    name: "location".to_string(),
                    artifact_type: ArtifactType::Location,
                    required: true,
                    purpose: "The location to rasterize.".to_string(),
                    many: false,
                },
                Port {
                    name: "image".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "An image in the location's frame, for the frame.".to_string(),
                    many: false,
                },
            ],
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask with the interior as its core.".to_string(),
                many: false,
            }],
            vec![
                Param::optional(
                    "use_case",
                    "The use case recorded on the mask.",
                    ParamKind::Choice,
                    serde_json::json!("custom"),
                )
                .choices(&["seam", "remove", "repair", "protect", "custom"]),
            ],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let (Some(Artifact::Location { location, identity }), Some(Artifact::Image(image))) =
            (inputs.get("location"), inputs.get("image"))
        else {
            return Err(Error::op(NAME, "inputs location and image are required"));
        };
        if location.frame() != image.record.frame {
            return Err(Error::op(
                NAME,
                format!(
                    "the location is in frame {} but the image is in {}",
                    location.frame(),
                    image.record.frame
                ),
            ));
        }
        let frame = context.frames.require(&image.record.frame)?;
        let use_case: UseCase = match params.get("use_case").and_then(|v| v.as_str()) {
            None => UseCase::Custom,
            Some(text) => match serde_json::from_value(serde_json::Value::String(text.to_string()))
            {
                Ok(use_case) => use_case,
                Err(_) => return Ok(Output::invalid_args(format!("unknown use_case {text}"))),
            },
        };
        let core = match location_core(location, frame) {
            Ok(Some(core)) => core,
            Ok(None) => {
                return Ok(Output::not_applicable(format!(
                    "a {} location has no interior to rasterize",
                    location.kind()
                )));
            }
            Err(reason) => return Ok(Output::invalid_args(reason)),
        };
        if core.pixels().all(|p| p[0] == 0) {
            return Ok(Output::not_applicable(format!(
                "the {} location covers no pixel of the frame",
                location.kind()
            )));
        }
        let source = Source {
            locations: vec![identity.as_str().to_string()],
            instances: Vec::new(),
        };
        let mask = Mask::from_core("mask", frame, use_case, core, source)?;
        let painted = mask.core.pixels().filter(|p| p[0] != 0).count();
        let mut outputs = BTreeMap::new();
        outputs.insert("mask".to_string(), mask_artifact(mask));
        Ok(Output::ok(
            format!(
                "core of {painted} pixels from a {} location",
                location.kind()
            ),
            outputs,
        ))
    }
}

/// The binary plane a location covers in its frame, by the rules of the catalogue: a bbox and
/// a polygon as drawn, `whole_image` everything, a column line as a band `width_px` wide, a
/// path line dilated by `width_px / 2`, a stroke by `radius_px`. `Ok(None)` for a kind with no
/// interior (`view`, `absent`); `Err` with the reason for a malformed line. A box that covers
/// no pixel centre is an empty plane.
pub fn location_core(
    location: &Location,
    frame: &rascal_core::frame::Frame,
) -> std::result::Result<Option<GrayImage>, String> {
    let (width, height) = (frame.width, frame.height);
    Ok(Some(match location {
        Location::Bbox { bbox, .. } => {
            let mut core = GrayImage::new(width, height);
            if let Some(rect) = bbox_pixels(*bbox, width, height) {
                for y in rect.1..rect.3 {
                    for x in rect.0..rect.2 {
                        core.put_pixel(x, y, image::Luma([255]));
                    }
                }
            }
            core
        }
        Location::WholeImage { .. } => GrayImage::from_pixel(width, height, image::Luma([255])),
        Location::Polygon { polygon, .. } => fill_polygon(polygon, width, height),
        Location::Line {
            column: Some(c),
            width_px,
            ..
        } => {
            let centre = c * f64::from(width);
            let half = width_px / 2.0;
            GrayImage::from_fn(width, height, |x, _| {
                let d = (f64::from(x) + 0.5 - centre).abs();
                image::Luma([if d <= half { 255 } else { 0 }])
            })
        }
        Location::Line {
            path: Some(path),
            width_px,
            ..
        } => dilate_path(path, width_px / 2.0, width, height),
        Location::Line { .. } => {
            return Err("line: give exactly one of column and path".to_string());
        }
        Location::Stroke {
            path, radius_px, ..
        } => dilate_path(path, *radius_px, width, height),
        Location::View { .. } | Location::Absent { .. } => return Ok(None),
    }))
}

/// The integer pixel rectangle `[x0, x1) × [y0, y1)` a normalised box covers, clipped to the
/// frame, by the same pixel-centre rule as [`fill_polygon`]: pixel `i` is covered when its
/// centre `(i + 0.5) / size` lies in `[v0, v1)`. The rule is exact at multiples of a pixel and
/// immune to the floating noise of `v * size` (`0.55 * 100` is not `55`). `None` when the box
/// covers no pixel centre.
pub fn bbox_pixels(bbox: [f64; 4], width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let [x0, y0, x1, y1] = bbox;
    // The first pixel whose centre is at or past `v`.
    let first_at = |v: f64, size: u32| {
        ((v.clamp(0.0, 1.0) * f64::from(size) - 0.5).ceil().max(0.0) as u32).min(size)
    };
    let px0 = first_at(x0, width);
    let px1 = first_at(x1, width);
    let py0 = first_at(y0, height);
    let py1 = first_at(y1, height);
    (px1 > px0 && py1 > py0).then_some((px0, py0, px1, py1))
}

/// A simple polygon filled by scanline with the even-odd rule, vertices in fractions of the
/// frame, sampled at pixel centres.
pub fn fill_polygon(polygon: &[[f64; 2]], width: u32, height: u32) -> GrayImage {
    let pts: Vec<(f64, f64)> = polygon
        .iter()
        .map(|p| (p[0] * f64::from(width), p[1] * f64::from(height)))
        .collect();
    let mut core = GrayImage::new(width, height);
    let n = pts.len();
    for y in 0..height {
        let cy = f64::from(y) + 0.5;
        let mut crossings: Vec<f64> = Vec::new();
        for i in 0..n {
            let (x0, y0) = pts[i];
            let (x1, y1) = pts[(i + 1) % n];
            if (y0 <= cy && y1 > cy) || (y1 <= cy && y0 > cy) {
                crossings.push(x0 + (cy - y0) / (y1 - y0) * (x1 - x0));
            }
        }
        crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in crossings.chunks(2) {
            if let [a, b] = pair {
                let start = (a - 0.5).ceil().max(0.0) as u32;
                // Pixel centres cx with a <= cx < b: half-open, so a shared edge is painted once.
                let end = ((b - 0.5).ceil() - 1.0).min(f64::from(width) - 1.0);
                if end >= 0.0 {
                    for x in start..=end as u32 {
                        core.put_pixel(x, y, image::Luma([255]));
                    }
                }
            }
        }
    }
    core
}

/// The pixels whose centres lie within `radius` of a polyline (fractions of the frame): the
/// union of round-capped segments.
pub fn dilate_path(path: &[[f64; 2]], radius: f64, width: u32, height: u32) -> GrayImage {
    let pts: Vec<(f64, f64)> = path
        .iter()
        .map(|p| (p[0] * f64::from(width), p[1] * f64::from(height)))
        .collect();
    let r2 = radius * radius;
    let (min_x, max_x, min_y, max_y) = pts.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |a, p| (a.0.min(p.0), a.1.max(p.0), a.2.min(p.1), a.3.max(p.1)),
    );
    let x_lo = (min_x - radius - 1.0).floor().max(0.0) as u32;
    let x_hi = ((max_x + radius + 1.0).ceil().max(0.0) as u32).min(width);
    let y_lo = (min_y - radius - 1.0).floor().max(0.0) as u32;
    let y_hi = ((max_y + radius + 1.0).ceil().max(0.0) as u32).min(height);
    let mut core = GrayImage::new(width, height);
    for y in y_lo..y_hi {
        let cy = f64::from(y) + 0.5;
        for x in x_lo..x_hi {
            let cx = f64::from(x) + 0.5;
            let hit = pts.windows(2).any(|seg| {
                let (ax, ay) = seg[0];
                let (bx, by) = seg[1];
                let (dx, dy) = (bx - ax, by - ay);
                let len2 = dx * dx + dy * dy;
                let t = if len2 == 0.0 {
                    0.0
                } else {
                    (((cx - ax) * dx + (cy - ay) * dy) / len2).clamp(0.0, 1.0)
                };
                let (px, py) = (ax + t * dx, ay + t * dy);
                (cx - px).powi(2) + (cy - py).powi(2) <= r2 + 1e-9
            });
            if hit {
                core.put_pixel(x, y, image::Luma([255]));
            }
        }
    }
    core
}
