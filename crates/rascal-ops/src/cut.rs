//! `cut`: the two edge strips of a wrapped image side by side, so the wrap edge is the centre
//! column of a bounded canvas (SPEC.md "Frames and images"). No resampling, cropping or
//! padding; the plan records how `put` returns the canvas.

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, GenericImageView, RgbImage, RgbaImage};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::frame::{Frame, Plan, Topology};
use rascal_core::image::Image;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};

/// The op.
pub struct Cut;

const NAME: &str = "cut";

impl Op for Cut {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "The two edge strips of a wrapped image side by side, so its wrap edge is the centre column of a bounded canvas.",
            "Before any work on the wrap seam of an equirectangular image: the seam becomes an ordinary vertical line in the middle of a small canvas.",
            "On a flat image, or on a frame that is already a cut canvas; and when the edit is not near the wrap edge.",
            "An Image in a new cut: frame (2s wide, the parent's height; the canvas's left half is the parent's rightmost s columns and its right half the leftmost s) and the Plan that put reverses. Byte-identical pixels; nothing is resampled.",
            Cost { tier: CostTier::Cheap, seconds: 0.1 },
            vec![Port {
                name: "image".to_string(),
                artifact_type: ArtifactType::Image,
                required: true,
                purpose: "A canonical or working image in a wrapped raw frame, at least 4 columns wide.".to_string(),
            }],
            vec![
                Port {
                    name: "image".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The canvas, in the cut frame.".to_string(),
                },
                Port {
                    name: "plan".to_string(),
                    artifact_type: ArtifactType::Plan,
                    required: true,
                    purpose: "The strip width and the source spans, for put.".to_string(),
                },
            ],
            vec![Param::optional("strip", "The strip fraction f in (0, 0.5]: s = min(max(2, 2·round_half_up(f·W/2)), 2·floor(W/4)).", ParamKind::Number, serde_json::json!(0.07)).bounded(1e-9, 0.5)],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let Some(Artifact::Image(image)) = inputs.get("image") else {
            return Err(Error::op(NAME, "input image is required"));
        };
        let strip = params
            .get("strip")
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
            .unwrap_or(0.07);
        let parent = context.frames.require(&image.record.frame)?;
        if parent.topology != Topology::Wrap || !parent.is_raw() {
            return Ok(Output::not_applicable(format!(
                "frame {} does not wrap; cut needs an equirectangular raw frame",
                parent.id
            )));
        }
        if parent.width < 4 {
            return Ok(Output::not_applicable(format!(
                "the image is {} wide; cut needs at least 4 columns",
                parent.width
            )));
        }
        let plan = Plan::new(parent, strip)?;
        let frame = Frame::cut(parent, &image.identity().hex()[..12], &plan)?;
        let canvas = cut_pixels(&image.pixels, plan.strip);
        let out = Image::new("cut.image", canvas, &frame, image.record.role, None);
        let mut outputs = BTreeMap::new();
        outputs.insert("image".to_string(), Artifact::Image(Arc::new(out)));
        outputs.insert("plan".to_string(), Artifact::Plan(Arc::new(plan.clone())));
        Ok(Output::ok(
            format!(
                "cut {}x{} with s = {} into {}",
                parent.width, parent.height, plan.strip, frame.id
            ),
            outputs,
        )
        .with_frame(frame))
    }
}

/// The canvas: the rightmost `s` columns followed by the leftmost `s`.
pub fn cut_pixels(pixels: &DynamicImage, s: u32) -> DynamicImage {
    let (w, h) = pixels.dimensions();
    match pixels {
        DynamicImage::ImageRgba8(src) => {
            let mut out = RgbaImage::new(2 * s, h);
            for y in 0..h {
                for x in 0..s {
                    out.put_pixel(x, y, *src.get_pixel(w - s + x, y));
                    out.put_pixel(s + x, y, *src.get_pixel(x, y));
                }
            }
            DynamicImage::ImageRgba8(out)
        }
        other => {
            let src = other.to_rgb8();
            let mut out = RgbImage::new(2 * s, h);
            for y in 0..h {
                for x in 0..s {
                    out.put_pixel(x, y, *src.get_pixel(w - s + x, y));
                    out.put_pixel(s + x, y, *src.get_pixel(x, y));
                }
            }
            DynamicImage::ImageRgb8(out)
        }
    }
}
