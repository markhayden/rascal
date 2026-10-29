//! `profile`: the band mask from a `BandProfile` (SPEC.md "Operations"): the core is the band
//! at each row's half-width, the blend the feather with a sigma per row, the profile and its
//! `sigma_px` series stored in the mask for `ramp` and the measures.

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{GrayImage, Luma};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};
use rascal_core::series::Series;

use crate::feather::{feather_blend, support};

/// The op.
pub struct Profile;

const NAME: &str = "profile";

/// `profile-v1`: `feather_ratio`, the sigma as a share of the row's half-width.
pub const PROFILE_V1: &str = include_str!("../constants/profile-v1.toml");

fn feather_ratio_default() -> f64 {
    PROFILE_V1
        .parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("feather_ratio").and_then(|v| v.as_float()))
        .unwrap_or(0.55)
}

impl Op for Profile {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "The band mask of a band profile: the core at each row's half-width about the line, the blend a feather whose sigma follows the row.",
            "After fan, to turn its profile into the seam mask that ramp, the filler and paste use.",
            "On a mask that is not a band along a line (use feather).",
            "A seam Mask in the profile's frame with the profile and its sigma_px series stored; core where |x + 0.5 − c| ≤ half[y]; sigma[y] = feather_ratio · half[y].",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![Port {
                name: "band".to_string(),
                artifact_type: ArtifactType::BandProfile,
                required: true,
                purpose: "The band profile fan produced.".to_string(),
                many: false,
            }],
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The band mask.".to_string(),
                many: false,
            }],
            vec![
                Param::optional(
                    "feather_ratio",
                    "The feather sigma of a row as a share of its half-width.",
                    ParamKind::Number,
                    serde_json::json!(feather_ratio_default()),
                )
                .bounded(0.05, 2.0),
            ],
        )
        .with_constants(&["profile-v1"])
    }

    fn constant_set(&self, name: &str) -> Params {
        if name != "profile-v1" {
            return Params::new();
        }
        [(
            "feather_ratio".to_string(),
            toml::Value::Float(feather_ratio_default()),
        )]
        .into_iter()
        .collect()
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let Some(Artifact::BandProfile(band)) = inputs.get("band") else {
            return Err(Error::op(NAME, "input band is required"));
        };
        let ratio = crate::float(params, "feather_ratio").unwrap_or_else(feather_ratio_default);
        let frame = context.frames.require(&band.frame)?;
        if band.half.len() != frame.height as usize {
            return Ok(Output::invalid_args(format!(
                "the profile has {} rows but frame {} is {} high",
                band.half.len(),
                frame.id,
                frame.height
            )));
        }
        let core = band_core(frame.width, band.column_px, &band.half);
        let sigmas: Vec<f64> = band.half.iter().map(|h| ratio * h).collect();
        let blend = feather_blend(&core, |y| sigmas[y as usize], support());
        let sigma_series = Series::new(
            "profile.sigma",
            "sigma_px",
            "px",
            &frame.id,
            &frame.identity,
            1,
            sigmas.clone(),
        )?;
        let mut mask = Mask::from_core(
            "profile.mask",
            frame,
            UseCase::Seam,
            core,
            Source {
                locations: vec![band.line.clone()],
                instances: Vec::new(),
            },
        )?;
        mask.set_profile(&band.line, (**band).clone(), sigma_series)?;
        mask.set_blend(blend)?;
        let stats = mask.statistics();
        let sigma_max = sigmas.iter().cloned().fold(0.0, f64::max);
        let mut outputs = BTreeMap::new();
        outputs.insert("mask".to_string(), Artifact::Mask(Arc::new(mask)));
        Ok(Output::ok(
            format!(
                "band mask about column {} of {}: {} core and {} blend pixels, sigma up to {sigma_max:.1} px",
                band.column_px, frame.id, stats.core, stats.blend
            ),
            outputs,
        )
        .with_measurement("core_px", stats.core as f64, "px", Some(&frame.id))
        .with_measurement("blend_px", stats.blend as f64, "px", Some(&frame.id))
        .with_measurement("sigma_max_px", sigma_max, "px", Some(&frame.id)))
    }
}

/// The band's core: pixel `x` on row `y` is set when `|x + 0.5 − c| ≤ half[y]`.
pub fn band_core(width: u32, column_px: u32, half: &[f64]) -> GrayImage {
    let c = f64::from(column_px);
    GrayImage::from_fn(width, half.len() as u32, |x, y| {
        let d = (f64::from(x) + 0.5 - c).abs();
        Luma([if d <= half[y as usize] { 255 } else { 0 }])
    })
}
