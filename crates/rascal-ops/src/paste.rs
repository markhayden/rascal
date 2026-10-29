//! `paste`: a filler's result composited onto the base through the mask (SPEC.md
//! "Operations", the paste contract). `out` starts as a copy of `base`; only pixels with
//! `w > 0` are written, so `out = base` wherever `w = 0`, protected pixels included. Modes:
//! `feather` blends by `w`, `hard` writes the fill where the core is set and not protected,
//! `poisson` solves the gradient-domain blend (`poisson`) over the unprotected `w > 0`
//! pixels and then blends the clamped solution by `w`, so the blend zone still fades. A wrap
//! frame has no adjacency the solve could use and is refused under `poisson`.

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, GenericImageView};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::frame::Topology;
use rascal_core::image::{Image, Role};
use rascal_core::mask::Mask;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};

use crate::poisson::{self, Settings, Solve};

/// The op.
pub struct Paste;

const NAME: &str = "paste";

/// The modes a manifest lists.
pub const MODES: [&str; 3] = ["feather", "hard", "poisson"];

impl Op for Paste {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.1.0",
            "A fill composited onto the base through the mask: feather blends by the effective weight, hard writes the core, poisson blends in the gradient domain.",
            "After a filler returns its result for the mask's frame, or after apply, to produce the composite; the last step of the heal graph before put. poisson when the fill's tone or level does not match the base at the mask's edge: it keeps the fill's gradients and takes its values from the base around the mask.",
            "When the fill is not the mask's frame and size (window or put first); poisson on a wrap frame (cut the band first; the seam canvas is bounded) or on a mask whose every w > 0 pixel is cut off from the base (nothing to anchor to).",
            "An Image in the mask's frame with the composite role. Pixels with effective weight 0 are copied from the base without arithmetic, protected pixels included; feather: (1 − w) · base + w · fill; hard: fill where the core is set and not protected; poisson: the Poisson solve over the unprotected w > 0 pixels with the fill's gradients and the base's values at the edge, clamped, then (1 − w) · base + w · solution. A base's alpha is kept; a fill's alpha is ignored and the summary says so. poisson reports the domain size and the iterations; a solve that does not converge within poisson-v1's ceiling is an operational error.",
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
                    many: false,
                },
                Port {
                    name: "fill".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The filler's result, in the same frame and size.".to_string(),
                    many: false,
                },
                Port {
                    name: "mask".to_string(),
                    artifact_type: ArtifactType::Mask,
                    required: true,
                    purpose: "The mask whose effective weight decides what is written.".to_string(),
                    many: false,
                },
            ],
            vec![Port {
                name: "image".to_string(),
                artifact_type: ArtifactType::Image,
                required: true,
                purpose: "The composite.".to_string(),
                many: false,
            }],
            vec![
                Param::optional(
                    "mode",
                    "feather: blend by the effective weight; hard: the fill where the core is set; poisson: the gradient-domain blend, then the feather of its solution.",
                    ParamKind::Choice,
                    serde_json::json!("feather"),
                )
                .choices(&MODES),
            ],
        )
        .with_constants(&["poisson-v1"])
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
            "poisson" => Mode::Poisson,
            other => return Ok(Output::invalid_args(format!("unknown mode {other}"))),
        };
        // poisson: solve first, then feather the clamped solution by w.
        let mut solved = None;
        let (pixels, written) = match mode {
            Mode::Poisson => {
                if frame.topology == Topology::Wrap {
                    return Ok(Output::invalid_args(format!(
                        "poisson paste refuses the wrap frame {}: cut the band first, the seam canvas is bounded",
                        frame.id
                    )));
                }
                match poisson::solve(
                    &base.pixels.to_rgb8(),
                    &fill.pixels.to_rgb8(),
                    mask,
                    &Settings::v1(),
                )? {
                    Solve::Unanchored { component_px } => {
                        return Ok(Output::not_applicable(format!(
                            "poisson paste has nothing to anchor to: {component_px} editable pixels form a component with no neighbour outside the mask's w > 0 pixels"
                        )));
                    }
                    Solve::Solved {
                        solution,
                        domain_px,
                        iterations,
                    } => {
                        solved = Some((domain_px, iterations));
                        paste_pixels(
                            &base.pixels,
                            &DynamicImage::ImageRgb8(solution),
                            mask,
                            Mode::Feather,
                        )
                    }
                }
            }
            Mode::Feather | Mode::Hard => paste_pixels(&base.pixels, &fill.pixels, mask, mode),
        };
        let out = Image::new("paste.image", pixels, frame, Role::Composite, None);
        let fill_alpha = if matches!(fill.pixels, DynamicImage::ImageRgba8(_)) {
            "; the fill's alpha was ignored"
        } else {
            ""
        };
        let solve_note = solved
            .map(|(domain_px, iterations)| {
                format!(" ({domain_px} pixels solved in {iterations} iterations)")
            })
            .unwrap_or_default();
        let mut outputs = BTreeMap::new();
        outputs.insert("image".to_string(), Artifact::Image(Arc::new(out)));
        let mut output = Output::ok(
            format!(
                "{} paste in {}: {written} pixels written{solve_note}, every w = 0 pixel copied from the base{fill_alpha}",
                mode.name(),
                frame.id
            ),
            outputs,
        )
        .with_measurement("written_px", written as f64, "px", Some(&frame.id));
        if let Some((domain_px, iterations)) = solved {
            output = output
                .with_measurement("domain_px", domain_px as f64, "px", Some(&frame.id))
                .with_measurement("iterations", iterations as f64, "iterations", None);
        }
        Ok(output)
    }
}

/// A paste mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Feather,
    Hard,
    Poisson,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Feather => "feather",
            Mode::Hard => "hard",
            Mode::Poisson => "poisson",
        }
    }
}

/// The contract on pixels: a copy of `base` with only `w > 0` pixels written (feather) or
/// only unprotected core pixels (hard), arithmetic per RGB channel in `f32`, rounded and
/// clamped; a base's alpha is kept, a fill's ignored. Returns the composite and how many
/// pixels were written. `Poisson` here is the feather of an already-solved fill: the op
/// solves first (`poisson::solve`) and passes the solution in as the fill.
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
            Mode::Feather | Mode::Poisson if weight > 0.0 => {
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
        DynamicImage::ImageLuma8(src) => {
            // Arithmetic per RGB channel on the grey value, written back as grey.
            let mut out = src.clone();
            for y in 0..h {
                for x in 0..w {
                    let v = out.get_pixel(x, y)[0];
                    let mut rgb = [v, v, v];
                    blend(x, y, &mut rgb);
                    out.get_pixel_mut(x, y).0 = [rgb[0]];
                }
            }
            DynamicImage::ImageLuma8(out)
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
