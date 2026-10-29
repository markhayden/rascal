//! `apply`: every prescription on a mask performed once, in order, on the canvas (SPEC.md
//! "Operations"). A tone ramp moves each pixel with `w > 0` toward the other side of the line
//! by `share · Δ[y] · max(0, 1 − d / half[y])`; pixels with `w = 0` are untouched, so a
//! protected pixel or one beyond the mask's support is byte-identical.

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, RgbImage};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::image::{Image, Role};
use rascal_core::mask::{LoadedPrescription, Mask};
use rascal_core::op::{Context, Cost, CostTier, Inputs, Manifest, Op, Output, Params, Port};

/// The op.
pub struct Apply;

const NAME: &str = "apply";

impl Op for Apply {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "The mask's prescriptions performed on the canvas, once each, in order.",
            "After ramp, to meet the two sides of a seam in colour before or instead of a filler; before paste in the heal graph.",
            "On a mask without prescriptions (nothing to do; not applicable), or to paint a filler's result (use paste).",
            "An Image in the mask's frame with the working role: the canvas with every prescription performed. Pixels with effective weight 0 are byte-identical to the input.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![
                Port {
                    name: "image".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The canvas.".to_string(),
                },
                Port {
                    name: "mask".to_string(),
                    artifact_type: ArtifactType::Mask,
                    required: true,
                    purpose: "The mask carrying the prescriptions.".to_string(),
                },
            ],
            vec![Port {
                name: "image".to_string(),
                artifact_type: ArtifactType::Image,
                required: true,
                purpose: "The canvas with the prescriptions performed.".to_string(),
            }],
            vec![],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, _: &Params) -> Result<Output> {
        let (Some(Artifact::Image(image)), Some(Artifact::Mask(mask))) =
            (inputs.get("image"), inputs.get("mask"))
        else {
            return Err(Error::op(NAME, "inputs image and mask are required"));
        };
        let frame = context.frames.require(&mask.record.frame)?;
        if mask.prescriptions.is_empty() {
            return Ok(Output::not_applicable("the mask carries no prescription"));
        }
        let Some(profile) = &mask.profile else {
            return Ok(Output::invalid_args(
                "the mask's prescriptions need its band profile for the line and the half-widths",
            ));
        };
        let (pixels, changed, largest) = apply_prescriptions(&image.pixels, mask, &profile.band);
        let out = Image::new("apply.image", pixels, frame, Role::Working, None);
        let mut outputs = BTreeMap::new();
        outputs.insert("image".to_string(), Artifact::Image(Arc::new(out)));
        Ok(Output::ok(
            format!(
                "{} prescription(s) performed in {}: {changed} pixels moved, by up to {largest} levels",
                mask.prescriptions.len(),
                frame.id
            ),
            outputs,
        )
        .with_measurement("pixels_changed", changed as f64, "px", Some(&frame.id))
        .with_measurement("shift_max_levels", f64::from(largest), "levels", Some(&frame.id)))
    }
}

/// The prescriptions performed in order on a copy of the canvas: the result, how many pixels
/// changed and the largest change in levels. The image's own sample format is kept.
pub fn apply_prescriptions(
    canvas: &DynamicImage,
    mask: &Mask,
    band: &rascal_core::profile::BandProfile,
) -> (DynamicImage, u64, u8) {
    let c = f64::from(band.column_px);
    let mut rgb = canvas.to_rgb8();
    let (w, h) = rgb.dimensions();
    let mut changed = 0u64;
    let mut largest = 0u8;
    for prescription in &mask.prescriptions {
        let LoadedPrescription::ToneRamp { step, share, .. } = prescription;
        for y in 0..h {
            let half = band.half[y as usize];
            for x in 0..w {
                if mask.weight(x, y) <= 0.0 {
                    continue;
                }
                let offset = f64::from(x) + 0.5 - c;
                let d = offset.abs();
                let reach = (1.0 - d / half).max(0.0);
                if reach <= 0.0 {
                    continue;
                }
                // The left side moves toward the right (+Δ), the right toward the left (−Δ).
                let sign = if offset < 0.0 { 1.0 } else { -1.0 };
                let p = rgb.get_pixel_mut(x, y);
                for ch in 0..3 {
                    let delta = step.values[(y as usize) * 3 + ch];
                    let moved = (f64::from(p[ch]) + sign * share * delta * reach)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    if moved != p[ch] {
                        largest = largest.max(moved.abs_diff(p[ch]));
                        p[ch] = moved;
                    }
                }
            }
        }
    }
    let before = canvas.to_rgb8();
    for (a, b) in before.pixels().zip(rgb.pixels()) {
        if a != b {
            changed += 1;
        }
    }
    let out = match canvas {
        DynamicImage::ImageRgba8(src) => {
            let mut out = src.clone();
            for (x, y, p) in out.enumerate_pixels_mut() {
                let m = rgb.get_pixel(x, y);
                p.0[..3].copy_from_slice(&m.0);
            }
            DynamicImage::ImageRgba8(out)
        }
        DynamicImage::ImageLuma8(_) => {
            DynamicImage::ImageLuma8(DynamicImage::ImageRgb8(rgb).to_luma8())
        }
        _ => DynamicImage::ImageRgb8(rgb),
    };
    (out, changed, largest)
}

/// Keeps `RgbImage` named for the tests' convenience.
pub type Canvas = RgbImage;
