//! `shrink`: the core eroded by a disc (SPEC.md "Operations"); a refinement, so the blend is
//! cleared, the profile and the prescriptions removed, and protect preserved. A core eroded
//! to nothing is not applicable: nothing is published.

use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::op::{
    Context, Cost, CostTier, Envelope, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};

use crate::primitives::erode;
use crate::refine::{count, replaced_core};

/// The op.
pub struct Shrink;

const NAME: &str = "shrink";

impl Op for Shrink {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "Shrinks the core by a disc.",
            "On a mask whose core reaches past what should be repainted: a loose segmentation, a box drawn generously.",
            "To keep pixels for good (use subtract, which protects them) or to soften an edge (use feather).",
            "The same mask with its core eroded by a disc of radius px, cleared under protect; outside the image counts as unset, so a core touching the border shrinks from the border too; not applicable when nothing of the core survives. The blend is cleared and the profile and prescriptions removed: feather it afterwards.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask whose core to shrink.".to_string(),
                many: false,
            }],
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask with the shrunk core.".to_string(),
                many: false,
            }],
            vec![
                Param::required(
                    "px",
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
        let px = crate::float(params, "px").ok_or_else(|| Error::op(NAME, "parameter px"))?;
        let core = erode(&mask.core, px);
        if count(&core) == 0 {
            return Ok(Output {
                envelope: Envelope::not_applicable(format!(
                    "shrinking by {px} px leaves no core ({} core pixels before)",
                    count(&mask.core)
                )),
                outputs: Default::default(),
                frames: Vec::new(),
            });
        }
        Ok(
            replaced_core(mask, core, "shrink.mask", format!("core shrunk by {px} px"))?
                .with_measurement("px", px, "px", Some(&mask.record.frame)),
        )
    }
}
