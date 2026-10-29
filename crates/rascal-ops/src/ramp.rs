//! `ramp`: the per-row colour step across a band's line, estimated on the step windows the
//! band profile recorded and stored on the mask as a `tone_ramp` prescription for `apply`
//! (SPEC.md "Operations"). Nothing is painted here: a prescription says what a later apply
//! will do, so the mask stays a description.

use std::collections::BTreeMap;
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};
use rascal_core::profile::BandProfile;
use rascal_core::series::Series;

use crate::fan::windows;
use crate::primitives::smooth_rows;

/// The op.
pub struct Ramp;

const NAME: &str = "ramp";

/// `ramp-v1`: `share` of the step each side moves, and `ramp_sigma` of the row smoothing.
pub const RAMP_V1: &str = include_str!("../constants/ramp-v1.toml");

fn ramp_v1() -> toml::Table {
    RAMP_V1
        .parse::<toml::Table>()
        .expect("ramp-v1 is embedded TOML")
}

fn default(key: &str) -> f64 {
    ramp_v1()
        .get(key)
        .and_then(|v| v.as_float())
        .unwrap_or_else(|| panic!("ramp-v1 has {key}"))
}

impl Op for Ramp {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "The per-row colour step across a band's line, as a tone-ramp prescription on the mask.",
            "After profile, on the canvas the band was measured on, so apply or the filler's composite can meet the two sides in colour at the line.",
            "On a mask without a band profile (the step windows come from it), or when the sides already agree.",
            "The same mask with a tone_ramp prescription: Δ[y] per channel (right minus left on the profile's step windows, row-smoothed) and the share. apply performs it; nothing is painted here.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.05,
            },
            vec![
                Port {
                    name: "image".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The canvas the band was measured on.".to_string(),
                    many: false,
                },
                Port {
                    name: "mask".to_string(),
                    artifact_type: ArtifactType::Mask,
                    required: true,
                    purpose: "The band mask, with its profile.".to_string(),
                    many: false,
                },
            ],
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask with the prescription added.".to_string(),
                many: false,
            }],
            vec![
                Param::optional(
                    "share",
                    "The share of the step each side moves toward the other at the line.",
                    ParamKind::Number,
                    serde_json::json!(default("share")),
                )
                .bounded(0.0, 1.0),
                Param::optional(
                    "ramp_sigma",
                    "Row smoothing of the step, as a fraction of the height.",
                    ParamKind::Number,
                    serde_json::json!(default("ramp_sigma")),
                )
                .bounded(0.0, 1.0),
            ],
        )
        .with_constants(&["ramp-v1"])
    }

    fn constant_set(&self, name: &str) -> Params {
        if name != "ramp-v1" {
            return Params::new();
        }
        ramp_v1().into_iter().collect()
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let (Some(Artifact::Image(image)), Some(Artifact::Mask(mask))) =
            (inputs.get("image"), inputs.get("mask"))
        else {
            return Err(Error::op(NAME, "inputs image and mask are required"));
        };
        let share = crate::float(params, "share").unwrap_or_else(|| default("share"));
        let ramp_sigma =
            crate::float(params, "ramp_sigma").unwrap_or_else(|| default("ramp_sigma"));
        let frame = context.frames.require(&mask.record.frame)?;
        let Some(profile) = &mask.profile else {
            return Ok(Output::invalid_args(
                "the mask has no band profile; ramp measures on the profile's step windows (run profile first)",
            ));
        };
        let rgb = image.pixels.to_rgb8();
        let (w, h) = rgb.dimensions();
        let step = tone_step(&rgb, &profile.band, ramp_sigma * f64::from(h));
        let series = Series::new(
            "ramp.step",
            "tone_ramp.step",
            "levels",
            &frame.id,
            &frame.identity,
            3,
            step,
        )?;
        let largest = series
            .values
            .iter()
            .cloned()
            .fold(0.0, |m: f64, v| m.max(v.abs()));
        let mut out = (**mask).clone();
        out.record.id = "ramp.mask".to_string();
        out.push_tone_ramp(series, share)?;
        let mut outputs = BTreeMap::new();
        outputs.insert("mask".to_string(), Artifact::Mask(Arc::new(out)));
        Ok(Output::ok(
            format!(
                "tone ramp prescribed across column {} of {} ({w} px wide): step up to {largest:.1} levels, share {share}",
                profile.band.column_px, frame.id
            ),
            outputs,
        )
        .with_measurement("step_max_levels", largest, "levels", Some(&frame.id))
        .with_measurement("share", share, "ratio", Some(&frame.id)))
    }
}

/// `Δ[y]` per channel: `mean(right) − mean(left)` on the profile's step windows, smoothed
/// along the rows with `sigma_rows`; `3 × height` values, row-major.
pub fn tone_step(image: &image::RgbImage, band: &BandProfile, sigma_rows: f64) -> Vec<f64> {
    let (w, h) = image.dimensions();
    let [(_, l0, l1), (_, r0, r1), _, _] = windows(
        band.column_px,
        band.thin_px,
        band.windows.gap_px,
        band.windows.step_cols,
        band.windows.texture_cols,
        w,
    );
    let mut per_channel: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for y in 0..h {
        for (ch, out) in per_channel.iter_mut().enumerate() {
            let mean = |lo: i64, hi: i64| {
                let n = (hi - lo).max(1) as f64;
                (lo..hi)
                    .map(|x| f64::from(image.get_pixel(x as u32, y)[ch]))
                    .sum::<f64>()
                    / n
            };
            out.push(mean(r0, r1) - mean(l0, l1));
        }
    }
    let smoothed: Vec<Vec<f64>> = per_channel
        .iter()
        .map(|v| smooth_rows(v, sigma_rows))
        .collect();
    (0..h as usize)
        .flat_map(|y| (0..3).map(move |ch| (y, ch)))
        .map(|(y, ch)| smoothed[ch][y])
        .collect()
}
