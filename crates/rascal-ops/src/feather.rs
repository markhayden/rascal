//! `feather`: the blend as the feather of the core's boundary outward (SPEC.md "Operations",
//! "Feather"), one sigma for the whole mask; the existing blend is replaced, protect is
//! preserved and wins. `profile` uses the same feather with a sigma per row.

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{GrayImage, Luma};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};

use crate::primitives::distance_transform;

/// The op.
pub struct Feather;

const NAME: &str = "feather";

/// `feather-v1`: the support in sigmas beyond which the blend is zero.
pub const FEATHER_V1: &str = include_str!("../constants/feather-v1.toml");

/// The support of `feather-v1`, in sigmas.
pub fn support() -> f64 {
    FEATHER_V1
        .parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("support").and_then(|v| v.as_float()))
        .unwrap_or(3.0)
}

impl Op for Feather {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "The blend as a half-Gaussian feather of the core's boundary outward, with one sigma.",
            "On any mask whose edge should fade: a rasterized box, a grown blob, a mask whose core was just replaced by a refinement (the recipe re-feathers afterwards).",
            "On a band along a line, where the sigma follows the row (use profile).",
            "The same mask with its blend replaced by the feather: round(255 · exp(−D² / 2σ²)) for 0 < D ≤ support · σ from the core's edge, zero beyond; zero inside the core and under protect. The profile and the prescriptions are kept.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask whose core to feather.".to_string(),
            }],
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask with the new blend.".to_string(),
            }],
            vec![
                Param::required(
                    "sigma_px",
                    "The feather's sigma in pixels of the mask's frame.",
                    ParamKind::Number,
                )
                .bounded(0.1, 1e4),
            ],
        )
        .with_constants(&["feather-v1"])
    }

    fn run(&self, _: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let Some(Artifact::Mask(mask)) = inputs.get("mask") else {
            return Err(Error::op(NAME, "input mask is required"));
        };
        let sigma = crate::float(params, "sigma_px")
            .ok_or_else(|| Error::op(NAME, "parameter sigma_px is required"))?;
        let support = support();
        let blend = feather_blend(&mask.core, |_| sigma, support);
        let mut out = (**mask).clone();
        out.record.id = "feather.mask".to_string();
        out.set_blend(blend)?;
        let painted = out.blend.pixels().filter(|p| p[0] != 0).count();
        let mut outputs = BTreeMap::new();
        outputs.insert("mask".to_string(), Artifact::Mask(Arc::new(out)));
        Ok(Output::ok(
            format!("feathered the core edge with sigma {sigma} px ({painted} blend pixels)"),
            outputs,
        )
        .with_measurement("sigma_px", sigma, "px", Some(&mask.record.frame))
        .with_measurement("blend_px", painted as f64, "px", Some(&mask.record.frame)))
    }
}

/// The feather of a binary core: for every pixel outside it, `D` is the distance from the
/// pixel's centre to the nearest core pixel's edge (the exact distance transform of the core,
/// less the half pixel to the boundary), and the blend is `round(255 · exp(−D² / 2σ²))` when
/// `0 < D ≤ support · σ`, else 0, with `σ` the pixel's row's. Zero inside the core.
pub fn feather_blend(
    core: &GrayImage,
    sigma_of_row: impl Fn(u32) -> f64,
    support: f64,
) -> GrayImage {
    let (w, h) = core.dimensions();
    let distance = distance_transform(core);
    let mut blend = GrayImage::new(w, h);
    for y in 0..h {
        let sigma = sigma_of_row(y);
        for x in 0..w {
            if core.get_pixel(x, y)[0] != 0 {
                continue;
            }
            let d = f64::from(distance[(y * w + x) as usize]) - 0.5;
            if !(d > 0.0 && d <= support * sigma) || sigma <= 0.0 {
                continue;
            }
            let v = (255.0 * (-d * d / (2.0 * sigma * sigma)).exp()).round();
            blend.put_pixel(x, y, Luma([v.clamp(0.0, 255.0) as u8]));
        }
    }
    blend
}
