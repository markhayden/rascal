//! `invert`: a mask for the second pass (SPEC.md "Operations", "Recipes" `protect`): the old
//! mask's support and protect, mapped through the frame chain into the target frame, become
//! the new protect; every other pixel of the target is the new core. A `cut:` mask lands on
//! the panorama's two edge spans, a `crop:` mask on its bbox; pixels the old frame never
//! covered count as untouched and are editable.

use std::collections::BTreeMap;

use image::{GrayImage, Luma};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::frame::{Frame, FrameKind};
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};
use rascal_core::recipe::mask_artifact;

use crate::refine::count;

/// The op.
pub struct Invert;

const NAME: &str = "invert";

impl Op for Invert {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "The mask for a second pass: everything the first mask reached becomes protect, everything else the core, in the target frame.",
            "After a heal, to edit the rest of the image while keeping the first edit: heal seam then heal protect.",
            "To shrink or grow a mask (use shrink, grow) or to protect one more thing in the same pass (use subtract).",
            "A new mask in the target frame (`to`: a frame id, or `raw` for the mask's root, the default): protect = the old support (core or blend) and the old protect mapped through the frame chain; core = every other pixel; blend empty. A cut: mask lands on the two edge spans, a crop: mask on its bbox.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The first pass's mask.".to_string(),
                many: false,
            }],
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The second pass's mask, in the target frame.".to_string(),
                many: false,
            }],
            vec![Param::optional(
                "to",
                "The target frame: a frame id on the mask's chain, or raw for its root.",
                ParamKind::String,
                serde_json::json!("raw"),
            )],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let Some(Artifact::Mask(mask)) = inputs.get("mask") else {
            return Err(Error::op(NAME, "input mask is required"));
        };
        let to = params
            .get("to")
            .and_then(|v| v.as_str())
            .unwrap_or("raw")
            .to_string();
        let chain: Vec<&Frame> = context.frames.chain(&mask.record.frame)?;
        let target_index = if to == "raw" {
            chain.len() - 1
        } else {
            match chain.iter().position(|f| f.id == to) {
                Some(i) => i,
                None => {
                    return Ok(Output::invalid_args(format!(
                        "frame {to} is not on the chain of {} ({})",
                        mask.record.frame,
                        chain
                            .iter()
                            .map(|f| f.id.as_str())
                            .collect::<Vec<_>>()
                            .join(" → ")
                    )));
                }
            }
        };
        let (width, height) = mask.dimensions();
        let support = GrayImage::from_fn(width, height, |x, y| {
            let on = mask.core.get_pixel(x, y)[0] != 0 || mask.blend.get_pixel(x, y)[0] != 0;
            Luma([if on { 255 } else { 0 }])
        });
        let mut reached = support;
        let mut protect = mask.protect.clone();
        for step in chain[..=target_index].windows(2) {
            reached = map_plane_to_parent(&reached, step[0], step[1])?;
            protect = map_plane_to_parent(&protect, step[0], step[1])?;
        }
        let target = chain[target_index];
        let new_protect = GrayImage::from_fn(target.width, target.height, |x, y| {
            let on = reached.get_pixel(x, y)[0] != 0 || protect.get_pixel(x, y)[0] != 0;
            Luma([if on { 255 } else { 0 }])
        });
        let core = GrayImage::from_fn(target.width, target.height, |x, y| {
            Luma([if new_protect.get_pixel(x, y)[0] != 0 {
                0
            } else {
                255
            }])
        });
        let editable = count(&core);
        if editable == 0 {
            return Ok(Output::not_applicable(
                "the first mask reached every pixel of the target frame; nothing is left to edit",
            ));
        }
        let mut out = Mask::from_core(
            "invert.mask",
            target,
            UseCase::Protect,
            core,
            Source {
                locations: mask.record.source.locations.clone(),
                instances: mask.record.source.instances.clone(),
            },
        )?;
        out.protect_union(&new_protect)?;
        let protected = count(&out.protect);
        let mut outputs = BTreeMap::new();
        outputs.insert("mask".to_string(), mask_artifact(out));
        Ok(Output::ok(
            format!(
                "inverted into {}: {editable} editable pixels, {protected} protected",
                target.id
            ),
            outputs,
        )
        .with_measurement("core_px", editable as f64, "px", Some(&target.id))
        .with_measurement("protect_px", protected as f64, "px", Some(&target.id)))
    }
}

/// A binary plane of `frame` placed into its parent: a crop's bbox offset, a cut canvas's two
/// halves onto the parent's edge strips. Parent pixels the frame does not cover are unset.
pub fn map_plane_to_parent(plane: &GrayImage, frame: &Frame, parent: &Frame) -> Result<GrayImage> {
    let mut out = GrayImage::new(parent.width, parent.height);
    match (frame.kind, &frame.bbox, &frame.plan) {
        (FrameKind::Crop, Some(bbox), _) => {
            let [l, t, _, _] = *bbox;
            for (x, y, p) in plane.enumerate_pixels() {
                if p[0] != 0 && l + x < parent.width && t + y < parent.height {
                    out.put_pixel(l + x, t + y, Luma([255]));
                }
            }
        }
        (FrameKind::Cut, _, Some(plan)) => {
            let s = plan.strip;
            for (x, y, p) in plane.enumerate_pixels() {
                if p[0] == 0 || y >= parent.height {
                    continue;
                }
                // The canvas's left half came from the parent's rightmost s columns, its right
                // half from the leftmost s.
                let px = if x < s { plan.width - s + x } else { x - s };
                if px < parent.width {
                    out.put_pixel(px, y, Luma([255]));
                }
            }
        }
        _ => {
            return Err(Error::op(
                NAME,
                format!("frame {} cannot be mapped into {}", frame.id, parent.id),
            ));
        }
    }
    Ok(out)
}
