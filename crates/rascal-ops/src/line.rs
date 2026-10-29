//! `line`: a line location in the image's frame, as a whole column (`column`, a fraction of
//! the width) or a path, with its full width in pixels. The seam recipe uses the column form
//! at the centre of a cut canvas; the path form is for any straight-ish feature.

use std::collections::BTreeMap;

use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::location::Location;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};

/// The op.
pub struct Line;

const NAME: &str = "line";

impl Op for Line {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "A line location: a whole column or a path, with its full width in pixels.",
            "When the region to mask follows a line: the wrap edge at the centre of a cut canvas (column 0.5), a seam, a crease, a scratch that fan will later widen row by row.",
            "When the mark is a rough hand-drawn stroke to be tightened to the defect (use stroke), or an area (use rect or a polygon).",
            "A line location in the image's frame. Give exactly one of column and path. rasterize makes it a band width_px wide; fan reads the column form.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.01,
            },
            vec![Port {
                name: "image".to_string(),
                artifact_type: ArtifactType::Image,
                required: true,
                purpose: "The image whose frame the line is in.".to_string(),
                many: false,
            }],
            vec![Port {
                name: "location".to_string(),
                artifact_type: ArtifactType::Location,
                required: true,
                purpose: "The line location.".to_string(),
                many: false,
            }],
            vec![
                Param::absent(
                    "column",
                    "The column as a fraction of the width, for a whole-column line.",
                    ParamKind::Number,
                )
                .bounded(0.0, 1.0),
                Param::absent(
                    "path",
                    "Two or more [x, y] points as fractions of the frame, for a path.",
                    ParamKind::PointList,
                ),
                Param::required(
                    "width_px",
                    "The line's full width in pixels of its frame.",
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
        let width_px = crate::float(params, "width_px").unwrap_or(0.0);
        let column = crate::float(params, "column");
        let path = if params.contains_key("path") {
            Some(crate::points(params, "path", NAME)?)
        } else {
            None
        };
        let location = Location::Line {
            frame: frame.id.clone(),
            column,
            path,
            width_px,
            group: None,
        };
        if let Err(e) = location.validate(frame) {
            return Ok(Output::invalid_args(e.to_string()));
        }
        let identity = location.identity(&frame.identity);
        let summary = match &location {
            Location::Line {
                column: Some(c), ..
            } => format!("line at column {c} ({width_px} px wide) in {}", frame.id),
            Location::Line { path: Some(p), .. } => format!(
                "line along {} points ({width_px} px wide) in {}",
                p.len(),
                frame.id
            ),
            _ => unreachable!("validated"),
        };
        let mut outputs = BTreeMap::new();
        outputs.insert(
            "location".to_string(),
            Artifact::Location { location, identity },
        );
        Ok(Output::ok(summary, outputs))
    }
}
