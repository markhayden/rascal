//! `subtract`: protect grows by a second mask's support or a location, dilated (SPEC.md
//! "Operations"); core and blend are cleared under the new protect; the profile and the
//! prescriptions stay, since `apply` honours protect.

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{GrayImage, Luma};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::frame::Frame;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};

use crate::primitives::dilate;
use crate::rasterize::location_core;
use crate::refine::count;

/// The op.
pub struct Subtract;

const NAME: &str = "subtract";

impl Op for Subtract {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "Protects pixels: the second mask's support or a location, grown by a margin, joins protect; core and blend are cleared under it.",
            "To keep something the mask would otherwise repaint: a person in front of the object, a logo, a previous edit.",
            "To make the core smaller for its own sake (use shrink): protected pixels are kept for good, by every later step.",
            "The same mask with protect enlarged by the second input dilated by grow_px, and core and blend cleared there. The profile and the prescriptions are kept: apply honours the new protect. At most one of subtract and location; with neither the mask passes through unchanged.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![
                Port {
                    name: "mask".to_string(),
                    artifact_type: ArtifactType::Mask,
                    required: true,
                    purpose: "The mask to protect pixels in.".to_string(),
                    many: false,
                },
                Port {
                    name: "subtract".to_string(),
                    artifact_type: ArtifactType::Mask,
                    required: false,
                    purpose:
                        "A mask whose support (core or blend) becomes protect; in the mask's frame."
                            .to_string(),
                    many: false,
                },
                Port {
                    name: "location".to_string(),
                    artifact_type: ArtifactType::Location,
                    required: false,
                    purpose: "A location whose interior becomes protect; in the mask's frame."
                        .to_string(),
                    many: false,
                },
            ],
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask with the larger protect.".to_string(),
                many: false,
            }],
            vec![
                Param::optional(
                    "grow_px",
                    "How far the protected shape is dilated, in pixels of the mask's frame.",
                    ParamKind::Number,
                    serde_json::json!(0.0),
                )
                .bounded(0.0, 1e4),
            ],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let Some(Artifact::Mask(mask)) = inputs.get("mask") else {
            return Err(Error::op(NAME, "input mask is required"));
        };
        let grow_px = crate::float(params, "grow_px").unwrap_or(0.0);
        let (width, height) = mask.dimensions();
        let shape: GrayImage = match (inputs.get("subtract"), inputs.get("location")) {
            (Some(Artifact::Mask(other)), None) => {
                if other.record.frame != mask.record.frame {
                    return Ok(Output::invalid_args(format!(
                        "subtract is in frame {}, the mask in {}",
                        other.record.frame, mask.record.frame
                    )));
                }
                GrayImage::from_fn(width, height, |x, y| {
                    let on =
                        other.core.get_pixel(x, y)[0] != 0 || other.blend.get_pixel(x, y)[0] != 0;
                    Luma([if on { 255 } else { 0 }])
                })
            }
            (None, Some(Artifact::Location { location, .. })) => {
                if location.frame() != mask.record.frame {
                    return Ok(Output::invalid_args(format!(
                        "location is in frame {}, the mask in {}",
                        location.frame(),
                        mask.record.frame
                    )));
                }
                let frame: &Frame = context.frames.require(&mask.record.frame)?;
                match location_core(location, frame) {
                    Ok(Some(core)) => core,
                    Ok(None) => {
                        return Ok(Output::not_applicable(format!(
                            "a {} location has no interior to protect",
                            location.kind()
                        )));
                    }
                    Err(reason) => return Ok(Output::invalid_args(reason)),
                }
            }
            (Some(_), Some(_)) => {
                return Ok(Output::invalid_args("give subtract or location, not both"));
            }
            _ => {
                // Nothing to protect (an optional input the caller left out): the mask passes
                // through under the step's name, so the recipe reads the same either way.
                let mut out = (**mask).clone();
                out.record.id = "subtract.mask".to_string();
                let frame = mask.record.frame.clone();
                let protected = count(&out.protect);
                let mut outputs = BTreeMap::new();
                outputs.insert("mask".to_string(), Artifact::Mask(Arc::new(out)));
                return Ok(Output::ok(
                    "nothing to protect: no second input; the mask passes through",
                    outputs,
                )
                .with_measurement("grow_px", grow_px, "px", Some(&frame))
                .with_measurement("protect_px_before", protected as f64, "px", Some(&frame))
                .with_measurement(
                    "protect_px_after",
                    protected as f64,
                    "px",
                    Some(&frame),
                ));
            }
        };
        let grown = if grow_px > 0.0 {
            dilate(&shape, grow_px)
        } else {
            shape
        };
        let before = count(&mask.protect);
        let mut out = (**mask).clone();
        out.record.id = "subtract.mask".to_string();
        out.protect_union(&grown)?;
        let after = count(&out.protect);
        let frame = mask.record.frame.clone();
        let mut outputs = BTreeMap::new();
        outputs.insert("mask".to_string(), Artifact::Mask(Arc::new(out)));
        Ok(Output::ok(
            format!(
                "protected {} more pixels (grown by {grow_px} px)",
                after.saturating_sub(before)
            ),
            outputs,
        )
        .with_measurement("grow_px", grow_px, "px", Some(&frame))
        .with_measurement("protect_px_before", before as f64, "px", Some(&frame))
        .with_measurement("protect_px_after", after as f64, "px", Some(&frame)))
    }
}
