//! `rect`: a bbox location from four fractions, in the image's frame. The find op behind
//! `--bbox`; `rasterize` turns it into a mask.

use std::collections::BTreeMap;

use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::location::Location;
use rascal_core::op::{Context, Inputs, Manifest, Op, Output, Param, Params, Port};

/// The op.
pub struct Rect;

const NAME: &str = "rect";

impl Op for Rect {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: NAME.to_string(),
            version: "1.0.0".to_string(),
            purpose: "A rectangle as a location, from four fractions of the image's frame."
                .to_string(),
            inputs: vec![Port {
                name: "image".to_string(),
                artifact_type: ArtifactType::Image,
                required: true,
            }],
            outputs: vec![Port {
                name: "location".to_string(),
                artifact_type: ArtifactType::Location,
                required: true,
            }],
            params: vec![Param {
                name: "bbox".to_string(),
                purpose: "[x0, y0, x1, y1] as fractions of the frame; x0 < x1 and y0 < y1"
                    .to_string(),
                required: true,
            }],
        }
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let Some(Artifact::Image(image)) = inputs.get("image") else {
            return Err(Error::op(NAME, "input image is required"));
        };
        let bbox = crate::float_array::<4>(params, "bbox", NAME)?;
        let location = Location::bbox(&image.record.frame, bbox)
            .map_err(|e| Error::op(NAME, e.to_string()))?;
        let [x0, y0, x1, y1] = bbox;
        if x1 <= 0.0 || y1 <= 0.0 || x0 >= 1.0 || y0 >= 1.0 {
            return Ok(Output::not_applicable(
                "the box lies wholly outside the frame",
            ));
        }
        let frame = context
            .frames
            .get(&image.record.frame)
            .ok_or_else(|| Error::op(NAME, format!("frame {} is unknown", image.record.frame)))?;
        let identity = location.identity(&frame.identity);
        let mut outputs = BTreeMap::new();
        outputs.insert(
            "location".to_string(),
            Artifact::Location { location, identity },
        );
        Ok(Output::ok(
            format!("bbox {x0},{y0},{x1},{y1} in {}", image.record.frame),
            outputs,
            &BTreeMap::new(),
        ))
    }
}
