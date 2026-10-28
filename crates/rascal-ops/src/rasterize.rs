//! `rasterize`: a location's interior as a mask core, in the location's frame.
//!
//! Task 1.1 handles `bbox` and `whole_image`; `view` and `absent` are `not_applicable` as the
//! spec says. REPLACED BY 1.8b (`polygon`, `line`, `stroke`).

use std::collections::BTreeMap;

use image::GrayImage;
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::location::Location;
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{Context, Inputs, Manifest, Op, Output, Param, Params, Port};
use rascal_core::recipe::mask_artifact;

/// The op.
pub struct Rasterize;

const NAME: &str = "rasterize";

impl Op for Rasterize {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: NAME.to_string(),
            version: "0.1.0".to_string(),
            purpose: "A location's interior as the core of a new mask in its frame.".to_string(),
            inputs: vec![
                Port {
                    name: "location".to_string(),
                    artifact_type: ArtifactType::Location,
                    required: true,
                },
                Port {
                    name: "image".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                },
            ],
            outputs: vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
            }],
            params: vec![Param {
                name: "use_case".to_string(),
                purpose: "seam | remove | repair | protect | custom (default custom)".to_string(),
                required: false,
            }],
        }
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
        let frame = context
            .frames
            .get(&image.record.frame)
            .ok_or_else(|| Error::op(NAME, format!("frame {} is unknown", image.record.frame)))?;
        let use_case = match params.get("use_case").and_then(|v| v.as_str()) {
            None => UseCase::Custom,
            Some(text) => serde_json::from_value(serde_json::Value::String(text.to_string()))
                .map_err(|_| Error::op(NAME, format!("unknown use_case {text}")))?,
        };
        let (width, height) = (frame.width, frame.height);
        let core = match location {
            Location::Bbox { bbox, .. } => {
                let Some(rect) = bbox_pixels(*bbox, width, height) else {
                    return Ok(Output::not_applicable(
                        "the box covers no pixel of the frame",
                    ));
                };
                let mut core = GrayImage::new(width, height);
                for y in rect.1..rect.3 {
                    for x in rect.0..rect.2 {
                        core.put_pixel(x, y, image::Luma([255]));
                    }
                }
                core
            }
            Location::WholeImage { .. } => GrayImage::from_pixel(width, height, image::Luma([255])),
            Location::View { .. } | Location::Absent { .. } => {
                return Ok(Output::not_applicable(format!(
                    "a {} location has no interior to rasterize",
                    location.kind()
                )));
            }
            // REPLACED BY 1.8b: polygon fill, the line band and the stroke dilation.
            Location::Polygon { .. } | Location::Line { .. } | Location::Stroke { .. } => {
                return Err(Error::op(
                    NAME,
                    format!(
                        "rasterizing a {} location arrives in task 1.8b",
                        location.kind()
                    ),
                ));
            }
        };
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
            &BTreeMap::new(),
        ))
    }
}

/// The integer pixel rectangle `[x0, x1) × [y0, y1)` a normalised box covers, clipped to the
/// frame: the start column is the floor and the end column the ceiling of the fraction times
/// the size, so a box always covers the pixels it touches. `None` when it covers nothing.
pub fn bbox_pixels(bbox: [f64; 4], width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let [x0, y0, x1, y1] = bbox;
    let clamp = |v: f64, size: u32| v.clamp(0.0, 1.0) * f64::from(size);
    let px0 = clamp(x0, width).floor() as u32;
    let px1 = clamp(x1, width).ceil() as u32;
    let py0 = clamp(y0, height).floor() as u32;
    let py1 = clamp(y1, height).ceil() as u32;
    (px1 > px0 && py1 > py0).then_some((px0, py0, px1.min(width), py1.min(height)))
}
