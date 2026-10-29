//! `unwindow`: a crop written back into exactly its bbox of the parent (SPEC.md "Operations").
//! The inverse of `window` for the image; `unwindow(window(x)) = x` when nothing changed.

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, GenericImage, GenericImageView};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::frame::FrameKind;
use rascal_core::image::{Image, Role};
use rascal_core::op::{Context, Cost, CostTier, Inputs, Manifest, Op, Output, Params, Port};

/// The op.
pub struct Unwindow;

const NAME: &str = "unwindow";

impl Op for Unwindow {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "A crop written back into exactly its bbox of the parent image.",
            "After work on a windowed crop (a filler's composite, an apply), to return to the full image.",
            "When the crop is not in a crop: frame of this parent: the frames will not agree and unwindow refuses.",
            "An Image in the parent's frame with the composite role: the parent's pixels with the crop's written into the bbox the crop frame records; nothing outside the bbox changes.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.05,
            },
            vec![
                Port {
                    name: "image".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The parent image.".to_string(),
                    many: false,
                },
                Port {
                    name: "crop".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The crop, in a crop: frame of the parent.".to_string(),
                    many: false,
                },
            ],
            vec![Port {
                name: "image".to_string(),
                artifact_type: ArtifactType::Image,
                required: true,
                purpose: "The parent with the crop written back.".to_string(),
                many: false,
            }],
            vec![],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, _: &Params) -> Result<Output> {
        let (Some(Artifact::Image(parent)), Some(Artifact::Image(crop))) =
            (inputs.get("image"), inputs.get("crop"))
        else {
            return Err(Error::op(NAME, "inputs image and crop are required"));
        };
        let parent_frame = context.frames.require(&parent.record.frame)?;
        let crop_frame = context.frames.require(&crop.record.frame)?;
        let (Some(bbox), FrameKind::Crop) = (crop_frame.bbox, crop_frame.kind) else {
            return Ok(Output::invalid_args(format!(
                "the crop is in frame {}, which is not a crop: frame",
                crop_frame.id
            )));
        };
        if crop_frame.parent.as_deref() != Some(parent_frame.id.as_str()) {
            return Ok(Output::invalid_args(format!(
                "the crop frame {} was cut from {}, not from {}",
                crop_frame.id,
                crop_frame.parent.as_deref().unwrap_or("?"),
                parent_frame.id
            )));
        }
        let [l, t, r, b] = bbox;
        if crop.pixels.dimensions() != (r - l, b - t) {
            return Ok(Output::invalid_args(format!(
                "the crop is {}x{} but its frame's bbox is {}x{}",
                crop.pixels.width(),
                crop.pixels.height(),
                r - l,
                b - t
            )));
        }
        let pixels = unwindow_pixels(&parent.pixels, &crop.pixels, bbox);
        let out = Image::new(
            "unwindow.image",
            pixels,
            parent_frame,
            Role::Composite,
            None,
        );
        let mut outputs = BTreeMap::new();
        outputs.insert("image".to_string(), Artifact::Image(Arc::new(out)));
        Ok(Output::ok(
            format!(
                "wrote the {}x{} crop back into [{l}, {t}, {r}, {b}) of {}",
                r - l,
                b - t,
                parent_frame.id
            ),
            outputs,
        ))
    }
}

/// The parent with the crop's pixels in the bbox, in the parent's sample format.
pub fn unwindow_pixels(parent: &DynamicImage, crop: &DynamicImage, bbox: [u32; 4]) -> DynamicImage {
    let [l, t, _, _] = bbox;
    let mut out = parent.clone();
    match &mut out {
        DynamicImage::ImageRgba8(dst) => {
            let src = crop.to_rgba8();
            let _ = dst.copy_from(&src, l, t);
        }
        DynamicImage::ImageLuma8(dst) => {
            let src = crop.to_luma8();
            let _ = dst.copy_from(&src, l, t);
        }
        _ => {
            let mut dst = parent.to_rgb8();
            let src = crop.to_rgb8();
            let _ = dst.copy_from(&src, l, t);
            out = DynamicImage::ImageRgb8(dst);
        }
    }
    out
}
