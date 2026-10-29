//! `select`: a mask from a finder's instance planes (SPEC.md "Operations"): the kept
//! instances' union as the core, minus any exclude boxes. The planes are a `many` input port,
//! `plane_0`, `plane_1`, …, bound in a recipe as `plane = "<step>.plane"`.

use std::collections::BTreeMap;

use image::{GrayImage, Luma};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::image::{Image, Role};
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};
use rascal_core::recipe::mask_artifact;

use crate::rasterize::bbox_pixels;
use crate::refine::count;

/// The op.
pub struct Select;

const NAME: &str = "select";

/// The many input port's name.
const PLANE: &str = "plane";

impl Op for Select {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "A mask from a finder's instance planes: the kept instances' union as the core, minus exclude boxes.",
            "After segment: to turn its planes into the mask to remove, keeping some instances and dropping others.",
            "On a single shape (rasterize a location) or to trim a mask (use shrink or subtract).",
            "A new mask in the planes' frame whose core is the union of the kept planes' set pixels, cleared inside every exclude box; blend and protect empty: feather it afterwards. keep names the instances to keep (all when empty), drop the ones to leave out; not both.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![Port {
                name: PLANE.to_string(),
                artifact_type: ArtifactType::Image,
                required: true,
                purpose: "The instance planes (plane role), in one frame: plane_0, plane_1, …"
                    .to_string(),
                many: true,
            }],
            vec![Port {
                name: "mask".to_string(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The selection as a mask.".to_string(),
                many: false,
            }],
            vec![
                Param::optional(
                    "keep",
                    "The instance indices to keep; empty keeps every instance not dropped.",
                    ParamKind::IntegerList,
                    serde_json::json!([]),
                ),
                Param::optional(
                    "drop",
                    "The instance indices to leave out.",
                    ParamKind::IntegerList,
                    serde_json::json!([]),
                ),
                Param::optional(
                    "exclude",
                    "Boxes, as fractions of the frame, cleared from the core.",
                    ParamKind::BoxList,
                    serde_json::json!([]),
                ),
                Param::optional(
                    "use_case",
                    "The use case recorded on the mask.",
                    ParamKind::Choice,
                    serde_json::json!("remove"),
                )
                .choices(&["seam", "remove", "repair", "protect", "custom"]),
            ],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let manifest = self.manifest();
        let port = manifest
            .inputs
            .iter()
            .find(|p| p.name == PLANE)
            .ok_or_else(|| Error::op(NAME, "the manifest declares the plane port"))?;
        let mut planes: Vec<(usize, &Image)> = inputs
            .iter()
            .filter_map(|(name, artifact)| match (port.index_of(name), artifact) {
                (Some(i), Artifact::Image(image)) => Some((i, image.as_ref())),
                _ => None,
            })
            .collect();
        planes.sort_by_key(|(i, _)| *i);
        if planes.is_empty() {
            return Err(Error::usage_for(NAME, "input plane needs at least plane_0"));
        }
        let frame_id = planes[0].1.record.frame.clone();
        for (i, plane) in &planes {
            if plane.record.role != Role::Plane {
                return Ok(Output::invalid_args(format!(
                    "plane_{i} has the {:?} role; select takes instance planes",
                    plane.record.role
                )));
            }
            if plane.record.frame != frame_id {
                return Ok(Output::invalid_args(format!(
                    "plane_{i} is in frame {}, plane_0 in {frame_id}",
                    plane.record.frame
                )));
            }
        }
        let keep = crate::integers(params, "keep");
        let drop = crate::integers(params, "drop");
        if !keep.is_empty() && !drop.is_empty() {
            return Ok(Output::invalid_args("give keep or drop, not both"));
        }
        let known: Vec<usize> = planes.iter().map(|(i, _)| *i).collect();
        for index in keep.iter().chain(&drop) {
            if !known.contains(&(*index as usize)) {
                return Ok(Output::invalid_args(format!(
                    "no instance {index}; the planes are {known:?}"
                )));
            }
        }
        let kept: Vec<&(usize, &Image)> = planes
            .iter()
            .filter(|(i, _)| {
                if keep.is_empty() {
                    !drop.contains(&(*i as i64))
                } else {
                    keep.contains(&(*i as i64))
                }
            })
            .collect();
        let frame = context.frames.require(&frame_id)?;
        let (width, height) = (frame.width, frame.height);
        let mut core = GrayImage::new(width, height);
        for (_, plane) in &kept {
            let grey = plane.pixels.to_luma8();
            if grey.dimensions() != (width, height) {
                return Ok(Output::invalid_args(format!(
                    "a plane is {}x{}, the frame {width}x{height}",
                    grey.width(),
                    grey.height()
                )));
            }
            for (x, y, p) in grey.enumerate_pixels() {
                if p[0] != 0 {
                    core.put_pixel(x, y, Luma([255]));
                }
            }
        }
        let exclude = crate::boxes(params, "exclude");
        for b in &exclude {
            if let Some((x0, y0, x1, y1)) = bbox_pixels(*b, width, height) {
                for y in y0..y1 {
                    for x in x0..x1 {
                        core.put_pixel(x, y, Luma([0]));
                    }
                }
            }
        }
        let painted = count(&core);
        if painted == 0 {
            return Ok(Output::not_applicable(format!(
                "no pixel is left after keeping {} of {} instances and excluding {} boxes",
                kept.len(),
                planes.len(),
                exclude.len()
            )));
        }
        let use_case = params
            .get("use_case")
            .and_then(|v| v.as_str())
            .and_then(|t| serde_json::from_value(serde_json::Value::String(t.to_string())).ok())
            .unwrap_or(UseCase::Remove);
        let source = Source {
            locations: Vec::new(),
            instances: kept
                .iter()
                .map(|(_, p)| p.record.identity.clone())
                .collect(),
        };
        let mask = Mask::from_core("select.mask", frame, use_case, core, source)?;
        let mut outputs = BTreeMap::new();
        outputs.insert("mask".to_string(), mask_artifact(mask));
        Ok(Output::ok(
            format!(
                "core of {painted} pixels from {} of {} instances",
                kept.len(),
                planes.len()
            ),
            outputs,
        )
        .with_measurement("instances", planes.len() as f64, "count", Some(&frame_id))
        .with_measurement("kept", kept.len() as f64, "count", Some(&frame_id))
        .with_measurement("core_px", painted as f64, "px", Some(&frame_id)))
    }
}
