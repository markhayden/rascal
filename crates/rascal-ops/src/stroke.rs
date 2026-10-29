//! `stroke`: a rough mark along a path with a radius in pixels, in the image's frame. The
//! repair recipe tightens it to the defect's edges; rasterize draws it as drawn.

use std::collections::BTreeMap;

use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::location::Location;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};

/// The op.
pub struct Stroke;

const NAME: &str = "stroke";

impl Op for Stroke {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "A stroke location: a rough mark along a path with a radius in pixels.",
            "When a person or an evaluator marks a scratch, a tear or a thin defect with a brush: the path and radius are the brush.",
            "When the region is a box or a shape (use rect or a polygon), or a line with a known width (use line).",
            "A stroke location in the image's frame. rasterize draws the path dilated by the radius; tighten refines it to the defect's edges.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.01,
            },
            vec![Port {
                name: "image".to_string(),
                artifact_type: ArtifactType::Image,
                required: true,
                purpose: "The image whose frame the stroke is in.".to_string(),
            }],
            vec![Port {
                name: "location".to_string(),
                artifact_type: ArtifactType::Location,
                required: true,
                purpose: "The stroke location.".to_string(),
            }],
            vec![
                Param::required(
                    "path",
                    "Two or more [x, y] points as fractions of the frame.",
                    ParamKind::PointList,
                ),
                Param::required(
                    "radius_px",
                    "The stroke's radius in pixels of its frame.",
                    ParamKind::Number,
                )
                .bounded(1e-9, 1e9),
            ],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let Some(Artifact::Image(image)) = inputs.get("image") else {
            return Err(Error::op(NAME, "input image is required"));
        };
        let frame = context.frames.require(&image.record.frame)?;
        let path = crate::points(params, "path", NAME)?;
        let radius_px = crate::float(params, "radius_px").unwrap_or(0.0);
        let location = Location::Stroke {
            frame: frame.id.clone(),
            path,
            radius_px,
            group: None,
        };
        if let Err(e) = location.validate(frame) {
            return Ok(Output::invalid_args(e.to_string()));
        }
        let identity = location.identity(&frame.identity);
        let n = location.points().map(|p| p.len()).unwrap_or(0);
        let mut outputs = BTreeMap::new();
        outputs.insert(
            "location".to_string(),
            Artifact::Location { location, identity },
        );
        Ok(Output::ok(
            format!(
                "stroke along {n} points (radius {radius_px} px) in {}",
                frame.id
            ),
            outputs,
        ))
    }
}
