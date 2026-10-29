//! `paste`: a filler's result composited onto the base through the mask (SPEC.md
//! "Operations", the paste contract). `out` starts as a copy of `base`; only pixels with
//! `w > 0` are written, so `out = base` wherever `w = 0`, protected pixels included. Modes:
//! `feather` blends by `w`, `hard` writes the fill where the core is set and not protected,
//! `poisson` is the M7 milestone and is not applicable until then.

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, GenericImageView};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::image::{Image, Role};
use rascal_core::mask::Mask;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};

/// The op.
pub struct Paste;

const NAME: &str = "paste";

/// The modes a manifest lists; `poisson` is accepted and answers `not_applicable` until M7.
pub const MODES: [&str; 3] = ["feather", "hard", "poisson"];

impl Op for Paste {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "A fill composited onto the base through the mask: feather blends by the effective weight, hard writes the core.",
            "After a filler returns its result for the mask's frame, or after apply, to produce the composite; the last step of the heal graph before put.",
            "When the fill is not the mask's frame and size (window or put first); for a seamless gradient-domain blend (poisson arrives with M7).",
            "An Image in the mask's frame with the composite role. Pixels with effective weight 0 are copied from the base without arithmetic, protected pixels included; feather: (1 − w) · base + w · fill; hard: fill where the core is set and not protected. A base's alpha is kept; a fill's alpha is ignored and the summary says so.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![
                Port {
                    name: "base".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The image to paste onto.".to_string(),
                },
                Port {
                    name: "fill".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The filler's result, in the same frame and size.".to_string(),
                },
                Port {
                    name: "mask".to_string(),
                    artifact_type: ArtifactType::Mask,
                    required: true,
                    purpose: "The mask whose effective weight decides what is written.".to_string(),
                },
            ],
            vec![Port {
                name: "image".to_string(),
                artifact_type: ArtifactType::Image,
                required: true,
                purpose: "The composite.".to_string(),
            }],
            vec![
                Param::optional(
                    "mode",
                    "feather: blend by the effective weight; hard: the fill where the core is set; poisson: the gradient-domain blend of M7.",
                    ParamKind::Choice,
                    serde_json::json!("feather"),
                )
                .choices(&MODES),
            ],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let (Some(Artifact::Image(base)), Some(Artifact::Image(fill)), Some(Artifact::Mask(mask))) =
            (inputs.get("base"), inputs.get("fill"), inputs.get("mask"))
        else {
            return Err(Error::op(NAME, "inputs base, fill and mask are required"));
        };
        let mode = params
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("feather");
        let frame = context.frames.require(&mask.record.frame)?;
        if base.pixels.dimensions() != mask.dimensions()
            || fill.pixels.dimensions() != mask.dimensions()
        {
            return Ok(Output::invalid_args(format!(
                "base is {}x{}, fill is {}x{}, the mask is {}x{}; paste needs one size",
                base.pixels.width(),
                base.pixels.height(),
                fill.pixels.width(),
                fill.pixels.height(),
                mask.dimensions().0,
                mask.dimensions().1
            )));
        }
        let mode = match mode {
            "feather" => Mode::Feather,
            "hard" => Mode::Hard,
            "poisson" => {
                return Ok(Output::not_applicable(
                    "poisson paste arrives with milestone M7; use feather or hard",
                ));
            }
            other => return Ok(Output::invalid_args(format!("unknown mode {other}"))),
        };
        let (pixels, written) = paste_pixels(&base.pixels, &fill.pixels, mask, mode);
        let out = Image::new("paste.image", pixels, frame, Role::Composite, None);
        let fill_alpha = if matches!(fill.pixels, DynamicImage::ImageRgba8(_)) {
            "; the fill's alpha was ignored"
        } else {
            ""
        };
        let mut outputs = BTreeMap::new();
        outputs.insert("image".to_string(), Artifact::Image(Arc::new(out)));
        Ok(Output::ok(
            format!(
                "{} paste in {}: {written} pixels written, every w = 0 pixel copied from the base{fill_alpha}",
                mode.name(),
                frame.id
            ),
            outputs,
        )
        .with_measurement("written_px", written as f64, "px", Some(&frame.id)))
    }
}

/// A paste mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Feather,
    Hard,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Feather => "feather",
            Mode::Hard => "hard",
        }
    }
}

/// The contract on pixels: a copy of `base` with only `w > 0` pixels written (feather) or
/// only unprotected core pixels (hard), arithmetic per RGB channel in `f32`, rounded and
/// clamped; a base's alpha is kept, a fill's ignored. Returns the composite and how many
/// pixels were written.
pub fn paste_pixels(
    base: &DynamicImage,
    fill: &DynamicImage,
    mask: &Mask,
    mode: Mode,
) -> (DynamicImage, u64) {
    let (w, h) = mask.dimensions();
    let fill_rgb = fill.to_rgb8();
    let mut written = 0u64;
    let mut blend = |x: u32, y: u32, dst: &mut [u8]| {
        let weight = mask.weight(x, y);
        let f = fill_rgb.get_pixel(x, y);
        match mode {
            Mode::Feather if weight > 0.0 => {
                for ch in 0..3 {
                    let v = (1.0 - weight) * f32::from(dst[ch]) + weight * f32::from(f[ch]);
                    dst[ch] = v.round().clamp(0.0, 255.0) as u8;
                }
                written += 1;
            }
            Mode::Hard
                if mask.core.get_pixel(x, y)[0] != 0 && mask.protect.get_pixel(x, y)[0] == 0 =>
            {
                dst[..3].copy_from_slice(&f.0);
                written += 1;
            }
            _ => {}
        }
    };
    let out = match base {
        DynamicImage::ImageRgba8(src) => {
            let mut out = src.clone();
            for y in 0..h {
                for x in 0..w {
                    blend(x, y, &mut out.get_pixel_mut(x, y).0);
                }
            }
            DynamicImage::ImageRgba8(out)
        }
        _ => {
            let mut out = base.to_rgb8();
            for y in 0..h {
                for x in 0..w {
                    blend(x, y, &mut out.get_pixel_mut(x, y).0);
                }
            }
            DynamicImage::ImageRgb8(out)
        }
    };
    (out, written)
}
