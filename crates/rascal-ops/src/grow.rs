//! `grow`: the core dilated by a disc, then extended downward for a shadow (SPEC.md
//! "Operations"); a refinement, so the blend is cleared, the profile and the prescriptions
//! removed, and protect preserved and winning.

use image::GrayImage;
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};

use crate::primitives::dilate;
use crate::refine::{replaced_core, shifted_down_union};

/// The op.
pub struct Grow;

const NAME: &str = "grow";

impl Op for Grow {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "Grows the core by a disc, and further downward for a shadow.",
            "On a blob that must cover a little more than it was found to (a segmenter's instance, a rasterized shape), and on an object that casts a shadow below it.",
            "On a band along a line (its width is the fan's), or to soften an edge (use feather).",
            "The same mask with its core dilated by a disc of radius px and, when down_px is set, unioned with itself shifted down by every row up to down_px; cleared under protect. The blend is cleared and the profile and prescriptions removed: feather it afterwards.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask whose core to grow.".to_string(),
                many: false,
            }],
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask with the grown core.".to_string(),
                many: false,
            }],
            vec![
                Param::required(
                    "px",
                    "The disc's radius in pixels of the mask's frame.",
                    ParamKind::Number,
                )
                .bounded(0.0, 1e4),
                Param::optional(
                    "down_px",
                    "How far down, in pixels, the grown core is extended for a shadow.",
                    ParamKind::Number,
                    serde_json::json!(0.0),
                )
                .bounded(0.0, 1e4),
            ],
        )
    }

    fn run(&self, _: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let Some(Artifact::Mask(mask)) = inputs.get("mask") else {
            return Err(Error::op(NAME, "input mask is required"));
        };
        let px = crate::float(params, "px").ok_or_else(|| Error::op(NAME, "parameter px"))?;
        let down = crate::float(params, "down_px").unwrap_or(0.0);
        let core = grow_core(&mask.core, px, down);
        let summary = if down > 0.0 {
            format!("core grown by {px} px and {down} px downward")
        } else {
            format!("core grown by {px} px")
        };
        Ok(replaced_core(mask, core, "grow.mask", summary)?
            .with_measurement("px", px, "px", Some(&mask.record.frame))
            .with_measurement("down_px", down, "px", Some(&mask.record.frame)))
    }
}

/// The grown core: dilated by a disc of radius `px`, then unioned with itself shifted down by
/// 1 … round(`down_px`) rows.
pub fn grow_core(core: &GrayImage, px: f64, down_px: f64) -> GrayImage {
    let dilated = dilate(core, px);
    let rows = down_px.round().max(0.0) as u32;
    if rows == 0 {
        dilated
    } else {
        shifted_down_union(&dilated, rows)
    }
}
