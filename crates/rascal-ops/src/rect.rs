//! `rect`: a bbox location from four fractions, in the image's frame. The find op behind
//! `--bbox`; `rasterize` turns it into a mask.

use std::collections::BTreeMap;

use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::location::Location;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};

/// The op.
pub struct Rect;

const NAME: &str = "rect";

impl Op for Rect {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "A rectangle as a location, from four fractions of the image's frame.",
            "When the region to mask is a known box: a caller's coordinates, a detector's bbox, a plain test shape.",
            "When the region has a shape (use a polygon or a stroke) or must be found (use a finder).",
            "A bbox location in the image's frame; `rasterize` makes it a mask. Not applicable when the box lies wholly outside the frame.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.01,
            },
            vec![Port {
                name: "image".to_string(),
                artifact_type: ArtifactType::Image,
                required: true,
                purpose: "The image whose frame the box is in.".to_string(),
            }],
            vec![Port {
                name: "location".to_string(),
                artifact_type: ArtifactType::Location,
                required: true,
                purpose: "The box as a bbox location.".to_string(),
            }],
            vec![
                Param::required(
                    "bbox",
                    "[x0, y0, x1, y1] as fractions of the frame; x0 < x1 and y0 < y1",
                    ParamKind::NumberArray,
                )
                .length(4),
            ],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let Some(Artifact::Image(image)) = inputs.get("image") else {
            return Err(Error::op(NAME, "input image is required"));
        };
        let bbox = crate::float_array::<4>(params, "bbox", NAME)?;
        let frame = context.frames.require(&image.record.frame)?;
        let location = match Location::bbox(&image.record.frame, bbox)
            .and_then(|l| l.validate(frame).map(|()| l))
        {
            Ok(location) => location,
            Err(error) => return Ok(Output::invalid_args(error.to_string())),
        };
        let [x0, y0, x1, y1] = bbox;
        if x1 <= 0.0 || y1 <= 0.0 || x0 >= 1.0 || y0 >= 1.0 {
            return Ok(Output::not_applicable(
                "the box lies wholly outside the frame",
            ));
        }
        let identity = location.identity(&frame.identity);
        let mut outputs = BTreeMap::new();
        outputs.insert(
            "location".to_string(),
            Artifact::Location { location, identity },
        );
        Ok(Output::ok(
            format!("bbox {x0},{y0},{x1},{y1} in {}", image.record.frame),
            outputs,
        ))
    }
}
