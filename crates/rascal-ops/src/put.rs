//! `put`: a cut canvas written back onto its parent: the canvas's left half onto the parent's
//! rightmost `s` columns and its right half onto the leftmost `s`; no other pixel changes. A
//! cut then put with an unchanged canvas is byte-identical.

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, GenericImageView};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::frame::Frame;
use rascal_core::image::{Image, Role};
use rascal_core::op::{Context, Cost, CostTier, Inputs, Manifest, Op, Output, Params, Port};

/// The op.
pub struct Put;

const NAME: &str = "put";

impl Op for Put {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "A cut canvas written back onto its parent image; nothing outside the two strips changes.",
            "After work on a cut canvas, to return to the full wrapped image.",
            "When the canvas was not made from this parent by this plan: the sizes will not agree and put refuses.",
            "An Image in the parent's raw frame with the composite role: the parent's pixels with the canvas's two halves written onto its edge strips. A cut then put with an unchanged canvas is byte-identical.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![
                Port {
                    name: "image".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The parent image, in the raw frame the plan was made for."
                        .to_string(),
                    many: false,
                },
                Port {
                    name: "canvas".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The canvas, in the cut frame.".to_string(),
                    many: false,
                },
                Port {
                    name: "plan".to_string(),
                    artifact_type: ArtifactType::Plan,
                    required: true,
                    purpose: "The plan cut produced.".to_string(),
                    many: false,
                },
            ],
            vec![Port {
                name: "image".to_string(),
                artifact_type: ArtifactType::Image,
                required: true,
                purpose: "The parent with the canvas put back.".to_string(),
                many: false,
            }],
            vec![],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, _params: &Params) -> Result<Output> {
        let (
            Some(Artifact::Image(parent)),
            Some(Artifact::Image(canvas)),
            Some(Artifact::Plan(plan)),
        ) = (
            inputs.get("image"),
            inputs.get("canvas"),
            inputs.get("plan"),
        )
        else {
            return Err(Error::op(
                NAME,
                "inputs image, canvas and plan are required",
            ));
        };
        let frame = context.frames.require(&parent.record.frame)?;
        if plan.parent != frame.id || plan.width != frame.width || plan.height != frame.height {
            return Ok(Output::invalid_args(format!(
                "the plan was made for {} ({}x{}), not {}",
                plan.parent, plan.width, plan.height, frame.id
            )));
        }
        // The cut frame's id names the image and the plan it was cut from; a canvas of the
        // right size from another panorama of the same geometry is in another frame.
        let expected = Frame::cut(frame, &parent.identity().hex()[..12], plan)?;
        if canvas.record.frame != expected.id {
            return Ok(Output::invalid_args(format!(
                "the canvas is in frame {}, not the cut frame of this image and plan ({})",
                canvas.record.frame, expected.id
            )));
        }
        if canvas.dimensions() != (2 * plan.strip, plan.height) {
            return Ok(Output::invalid_args(format!(
                "the canvas is {}x{} but the plan's canvas is {}x{}",
                canvas.dimensions().0,
                canvas.dimensions().1,
                2 * plan.strip,
                plan.height
            )));
        }
        let out = put_pixels(&parent.pixels, &canvas.pixels, plan.strip);
        let image = Image::new("put.image", out, frame, Role::Composite, None);
        let mut outputs = BTreeMap::new();
        outputs.insert("image".to_string(), Artifact::Image(Arc::new(image)));
        Ok(Output::ok(
            format!(
                "put a {}x{} canvas back onto {}",
                2 * plan.strip,
                plan.height,
                frame.id
            ),
            outputs,
        ))
    }
}

/// The parent with the canvas's halves written onto its edge strips.
pub fn put_pixels(parent: &DynamicImage, canvas: &DynamicImage, s: u32) -> DynamicImage {
    let (w, h) = parent.dimensions();
    let mut out = parent.clone();
    let canvas_rgba = canvas.to_rgba8();
    match &mut out {
        DynamicImage::ImageRgba8(dst) => {
            for y in 0..h {
                for x in 0..s {
                    dst.put_pixel(w - s + x, y, *canvas_rgba.get_pixel(x, y));
                    dst.put_pixel(x, y, *canvas_rgba.get_pixel(s + x, y));
                }
            }
        }
        DynamicImage::ImageLuma8(dst) if matches!(canvas, DynamicImage::ImageLuma8(_)) => {
            let canvas_luma = canvas.to_luma8();
            for y in 0..h {
                for x in 0..s {
                    dst.put_pixel(w - s + x, y, *canvas_luma.get_pixel(x, y));
                    dst.put_pixel(x, y, *canvas_luma.get_pixel(s + x, y));
                }
            }
        }
        _ => {
            let mut dst = parent.to_rgb8();
            let canvas_rgb = canvas.to_rgb8();
            for y in 0..h {
                for x in 0..s {
                    dst.put_pixel(w - s + x, y, *canvas_rgb.get_pixel(x, y));
                    dst.put_pixel(x, y, *canvas_rgb.get_pixel(s + x, y));
                }
            }
            out = DynamicImage::ImageRgb8(dst);
        }
    }
    out
}
