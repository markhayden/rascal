//! `render`: a mask as one consumer wants to see it, typed as an `Image` with the `rendering`
//! role (or `inspection` for `overlay`). The five profiles are `rascal_core::rendering`.

use std::collections::BTreeMap;

use image::DynamicImage;
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::image::{Image, Role};
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};
use rascal_core::recipe::image_artifact;
use rascal_core::rendering::{self, Profile, RenderParams};

/// The op.
pub struct Render;

const NAME: &str = "render";

impl Op for Render {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "A mask rendered for one consumer; the record stays the truth.",
            "When a filler, a viewer or a test needs to see a mask as pixels: a grey weight map, a binary at a threshold, the photo tinted or cut out, or an overlay for the eye.",
            "When an op needs the mask itself: a rendering is an Image and is never accepted where a Mask is declared. There is no protect rendering; a second pass uses invert.",
            "An Image with the rendering role (mask_input for grey and binary, image_input for alpha and tinted) or the inspection role (overlay). grey: round(255 · w), 255 = paint. binary: 255 where w ≥ threshold/255, protected 0. alpha: transparent where the unprotected core is. tinted: the unprotected core toward the colour. overlay: the image, the tint by w, the core's edge in white.",
            Cost { tier: CostTier::Cheap, seconds: 0.05 },
            vec![
                Port {
                    name: "mask".to_string(),
                    artifact_type: ArtifactType::Mask,
                    required: true,
                    purpose: "The mask to render.".to_string(),
                    many: false,
                },
                Port {
                    name: "image".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: false,
                    purpose: "The image to draw on, required by alpha, tinted and overlay; in the mask's frame.".to_string(),
                    many: false,
                },
            ],
            vec![Port {
                name: "image".to_string(),
                artifact_type: ArtifactType::Image,
                required: true,
                purpose: "The rendering.".to_string(),
                many: false,
            }],
            vec![
                Param::required("for", "The profile.", ParamKind::Choice).choices(&["grey", "binary", "alpha", "tinted", "overlay"]),
                Param::optional("threshold", "binary: 255 where w ≥ threshold / 255; protected pixels are 0 whatever it is.", ParamKind::Integer, serde_json::json!(127)).bounded(1.0, 255.0),
                Param::optional("colour", "tinted and overlay: the tint as #rrggbb; the default is a magenta no photograph has.", ParamKind::String, serde_json::json!("#ff00ff")),
                Param::optional("strength", "tinted: how far the core moves toward the colour.", ParamKind::Number, serde_json::json!(0.7)).bounded(0.0, 1.0),
            ],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let Some(Artifact::Mask(mask)) = inputs.get("mask") else {
            return Err(Error::op(NAME, "input mask is required"));
        };
        let image = match inputs.get("image") {
            Some(Artifact::Image(image)) => Some(image.clone()),
            _ => None,
        };
        let profile_name = params.get("for").and_then(|v| v.as_str()).unwrap_or("grey");
        let Some(profile) = Profile::parse(profile_name) else {
            return Ok(Output::invalid_args(format!(
                "unknown profile {profile_name}"
            )));
        };
        let mut render_params = RenderParams::default();
        if let Some(t) = params.get("threshold").and_then(|v| v.as_integer()) {
            render_params.threshold = t.clamp(1, 255) as u8;
        }
        if let Some(c) = params.get("colour").and_then(|v| v.as_str()) {
            render_params.colour = match rendering::parse_colour(c) {
                Ok(colour) => colour,
                Err(e) => return Ok(Output::invalid_args(e.to_string())),
            };
        }
        if let Some(s) = params
            .get("strength")
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        {
            render_params.strength = s as f32;
        }
        let frame = context.frames.require(&mask.record.frame)?;
        if profile.needs_image() && image.is_none() {
            return Ok(Output::invalid_args(format!(
                "{} draws on the image: give input image",
                profile.name()
            )));
        }
        let pixels = match (profile, image.as_deref()) {
            (Profile::Grey, _) => DynamicImage::ImageLuma8(rendering::grey(mask)),
            (Profile::Binary, _) => {
                DynamicImage::ImageLuma8(rendering::binary(mask, render_params.threshold)?)
            }
            (Profile::Alpha, Some(img)) => {
                DynamicImage::ImageRgba8(rendering::alpha(mask, &img.pixels)?)
            }
            (Profile::Tinted, Some(img)) => DynamicImage::ImageRgb8(rendering::tinted(
                mask,
                &img.pixels,
                render_params.colour,
                render_params.strength,
            )?),
            (Profile::Overlay, Some(img)) => DynamicImage::ImageRgb8(rendering::overlay(
                mask,
                &img.pixels,
                render_params.colour,
            )?),
            (_, None) => unreachable!("checked above"),
        };
        let role = if profile == Profile::Overlay {
            Role::Inspection
        } else {
            Role::Rendering
        };
        let rendered = Image::new(
            &format!("render.{}", profile.name()),
            pixels,
            frame,
            role,
            profile.role(),
        );
        let mut outputs = BTreeMap::new();
        outputs.insert("image".to_string(), image_artifact(rendered));
        Ok(Output::ok(
            format!(
                "{} rendering in {} ({})",
                profile.name(),
                frame.id,
                profile.polarity()
            ),
            outputs,
        ))
    }
}
