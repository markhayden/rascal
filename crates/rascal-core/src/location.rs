//! Locations: tagged records, each in one frame, with normalised coordinates. The five copied
//! kinds share their shape with the copied envelope schema; `line` and `stroke` are rascal's.
//!
//! Task 1.1 validates the `bbox` kind. REPLACED BY 1.2 (every kind's validity rules, the
//! wrap-seam rule, mapping toward the root).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::identity::Identity;

/// A location in one frame. Serialized with `kind` as the tag, matching
/// `schemas/rascal/location-v1.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Location {
    /// `[x0, y0, x1, y1]` as fractions of the frame.
    Bbox {
        frame: String,
        bbox: [f64; 4],
    },
    /// Three to sixty-four vertices as fractions of the frame.
    Polygon {
        frame: String,
        polygon: Vec<[f64; 2]>,
    },
    /// A direction on a wrapped frame, in degrees.
    View {
        frame: String,
        yaw: f64,
        pitch: f64,
        fov: f64,
    },
    WholeImage {
        frame: String,
    },
    /// Something missing from this frame.
    Absent {
        frame: String,
    },
    /// A line the seam band follows: a column or a path, with its full width in pixels.
    Line {
        frame: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        column: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<Vec<[f64; 2]>>,
        width_px: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group: Option<String>,
    },
    /// A rough mark along a path with a radius in pixels.
    Stroke {
        frame: String,
        path: Vec<[f64; 2]>,
        radius_px: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group: Option<String>,
    },
}

impl Location {
    /// The frame id the coordinates are in.
    pub fn frame(&self) -> &str {
        match self {
            Location::Bbox { frame, .. }
            | Location::Polygon { frame, .. }
            | Location::View { frame, .. }
            | Location::WholeImage { frame }
            | Location::Absent { frame }
            | Location::Line { frame, .. }
            | Location::Stroke { frame, .. } => frame,
        }
    }

    /// The kind's name as it appears in `kind`.
    pub fn kind(&self) -> &'static str {
        match self {
            Location::Bbox { .. } => "bbox",
            Location::Polygon { .. } => "polygon",
            Location::View { .. } => "view",
            Location::WholeImage { .. } => "whole_image",
            Location::Absent { .. } => "absent",
            Location::Line { .. } => "line",
            Location::Stroke { .. } => "stroke",
        }
    }

    /// A bbox from the command line's `x0,y0,x1,y1`. Ordering is checked here; a box that
    /// reaches outside the frame is allowed and clipped when rasterized.
    pub fn bbox(frame: &str, bbox: [f64; 4]) -> Result<Location> {
        let [x0, y0, x1, y1] = bbox;
        if !bbox.iter().all(|v| v.is_finite()) {
            return Err(Error::usage("bbox: every value must be a finite number"));
        }
        if x0 >= x1 || y0 >= y1 {
            return Err(Error::usage(format!(
                "bbox: need x0 < x1 and y0 < y1, got {x0},{y0},{x1},{y1}"
            )));
        }
        Ok(Location::Bbox {
            frame: frame.to_string(),
            bbox,
        })
    }

    /// Parses `x0,y0,x1,y1`.
    pub fn parse_bbox(text: &str) -> Result<[f64; 4]> {
        let parts: Vec<f64> = text
            .split(',')
            .map(|p| p.trim().parse::<f64>())
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| {
                Error::usage(format!(
                    "bbox: expected four numbers x0,y0,x1,y1, got '{text}'"
                ))
            })?;
        let [x0, y0, x1, y1] = parts[..] else {
            return Err(Error::usage(format!(
                "bbox: expected four numbers x0,y0,x1,y1, got '{text}'"
            )));
        };
        Ok([x0, y0, x1, y1])
    }

    /// The identity: the kind, the frame's identity and the geometry, with the frame id
    /// replaced by the identity so two workspaces agree.
    pub fn identity(&self, frame_identity: &str) -> Identity {
        let mut value = serde_json::to_value(self).unwrap_or_default();
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "frame".to_string(),
                serde_json::Value::String(frame_identity.to_string()),
            );
        }
        let text = serde_json::to_string(&value).unwrap_or_default();
        Identity::of_parts(&[b"location", text.as_bytes()])
    }
}

/// A location as a run or workspace lists it in `locations.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocationEntry {
    pub id: String,
    pub identity: String,
    pub location: Location,
}
