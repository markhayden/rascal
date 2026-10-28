//! `render`: a mask as one consumer wants to see it, typed as an `Image` with the
//! `rendering` role.
//!
//! Task 1.1 renders `grey`. REPLACED BY 1.5 (`binary`, `alpha`, `tinted`, `overlay`; the
//! profile table).

use std::collections::BTreeMap;

use image::{DynamicImage, GrayImage, Luma};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::image::{Image, RenderingRole, Role};
use rascal_core::mask::Mask;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};
use rascal_core::recipe::image_artifact;

/// The op.
pub struct Render;

const NAME: &str = "render";

impl Op for Render {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "0.1.0",
            "A mask rendered for one consumer; the record stays the truth.",
            "When a filler, a viewer or a test needs to see a mask as pixels: a grey weight map, a binary at a threshold, the photo tinted or cut out.",
            "When an op needs the mask itself: a rendering is an Image and is never accepted where a Mask is declared.",
            "An Image with the rendering role. grey is 8-bit round(255 · w), 255 = paint. Task 1.5 adds binary, alpha, tinted and overlay.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.05,
            },
            vec![
                Port {
                    name: "mask".to_string(),
                    artifact_type: ArtifactType::Mask,
                    required: true,
                    purpose: "The mask to render.".to_string(),
                },
                Port {
                    name: "image".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: false,
                    purpose: "The image to draw on, for image_input profiles; in the mask's frame."
                        .to_string(),
                },
            ],
            vec![Port {
                name: "image".to_string(),
                artifact_type: ArtifactType::Image,
                required: true,
                purpose: "The rendering.".to_string(),
            }],
            vec![Param::required("for", "The profile.", ParamKind::Choice).choices(&["grey"])],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let Some(Artifact::Mask(mask)) = inputs.get("mask") else {
            return Err(Error::op(NAME, "input mask is required"));
        };
        let profile = params
            .get("for")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::op(NAME, "parameter for is required"))?;
        let frame = context.frames.require(&mask.record.frame)?;
        let pixels = match profile {
            "grey" => DynamicImage::ImageLuma8(grey(mask)),
            // REPLACED BY 1.5.
            other => {
                return Err(Error::op(
                    NAME,
                    format!("profile {other} arrives in task 1.5"),
                ));
            }
        };
        let image = Image::new(
            &format!("render.{profile}"),
            pixels,
            frame,
            Role::Rendering,
            Some(RenderingRole::MaskInput),
        );
        let mut outputs = BTreeMap::new();
        outputs.insert("image".to_string(), image_artifact(image));
        Ok(Output::ok(
            format!("{profile} rendering in {}", frame.id),
            outputs,
        ))
    }
}

/// `grey`: 8-bit `round(255 · w)`, 255 = paint.
pub fn grey(mask: &Mask) -> GrayImage {
    let (width, height) = mask.dimensions();
    let mut out = GrayImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let value = (255.0 * mask.weight(x, y)).round().clamp(0.0, 255.0) as u8;
            out.put_pixel(x, y, Luma([value]));
        }
    }
    out
}
