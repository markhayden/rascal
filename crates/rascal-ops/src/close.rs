//! `close`: the core closed by a disc, dilated then eroded (SPEC.md "Operations"), filling
//! gaps and notches narrower than the disc; a refinement, so the blend is cleared, the profile
//! and the prescriptions removed, and protect preserved.

use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};

use crate::primitives::{dilate, erode_within};
use crate::refine::replaced_core;

/// The op.
pub struct Close;

const NAME: &str = "close";

impl Op for Close {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "Closes the core: dilation then erosion by one disc, so gaps and notches narrower than the disc fill in.",
            "On a blob with holes or ragged bites (a segmenter's instance through a chair back, a stroke that skipped).",
            "To make the core larger overall (use grow) or smaller (use shrink).",
            "The same mask with its core closed by a disc of radius radius_px and cleared under protect; the outer extent is unchanged except where a notch filled, at the image border too. The blend is cleared and the profile and prescriptions removed: feather it afterwards.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask whose core to close.".to_string(),
                many: false,
            }],
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask with the closed core.".to_string(),
                many: false,
            }],
            vec![
                Param::required(
                    "radius_px",
                    "The disc's radius in pixels of the mask's frame.",
                    ParamKind::Number,
                )
                .bounded(0.0, 1e4),
            ],
        )
    }

    fn run(&self, _: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let Some(Artifact::Mask(mask)) = inputs.get("mask") else {
            return Err(Error::op(NAME, "input mask is required"));
        };
        let radius = crate::float(params, "radius_px")
            .ok_or_else(|| Error::op(NAME, "parameter radius_px"))?;
        // Outside the image counts as set for the erosion, so a shape against the border keeps
        // its edge: closing changes nothing but gaps and notches.
        let core = erode_within(&dilate(&mask.core, radius), radius);
        Ok(replaced_core(
            mask,
            core,
            "close.mask",
            format!("core closed by {radius} px"),
        )?
        .with_measurement("radius_px", radius, "px", Some(&mask.record.frame)))
    }
}
