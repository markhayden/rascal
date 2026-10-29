//! Locations: tagged records, each in one frame, with normalised coordinates. The five copied
//! kinds share their shape with the copied envelope schema; `line` and `stroke` are rascal's.
//! Validity rules are SPEC.md "Locations"; mapping toward the root is in `mapping`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::error::{Error, Result};
use crate::frame::{Frame, Topology};
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
    /// Three to sixty-four vertices as fractions of the frame, a simple polygon.
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

/// The spec's rule for a box or polygon on a wrapped frame.
pub const SEAM_RULE: &str = "the seam rule: on a wrapped frame a location may not straddle x = 0 (the wrap edge); emit two locations, one on each side";

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

    /// The same location in another frame id; used by mapping.
    pub fn with_frame(&self, frame: &str) -> Location {
        let mut out = self.clone();
        match &mut out {
            Location::Bbox { frame: f, .. }
            | Location::Polygon { frame: f, .. }
            | Location::View { frame: f, .. }
            | Location::WholeImage { frame: f }
            | Location::Absent { frame: f }
            | Location::Line { frame: f, .. }
            | Location::Stroke { frame: f, .. } => *f = frame.to_string(),
        }
        out
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

    /// A bbox from four fractions; ordering is checked here, the frame's rules in `validate`.
    pub fn bbox(frame: &str, bbox: [f64; 4]) -> Result<Location> {
        let location = Location::Bbox {
            frame: frame.to_string(),
            bbox,
        };
        location.validate_geometry()?;
        Ok(location)
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

    /// The rules that need no frame: finite numbers, ordering, vertex counts, simplicity,
    /// positive widths and radii, exactly one line form.
    pub fn validate_geometry(&self) -> Result<()> {
        let finite = |values: &[f64], what: &str| -> Result<()> {
            if values.iter().all(|v| v.is_finite()) {
                Ok(())
            } else {
                Err(Error::usage(format!(
                    "{}: {what} must be finite numbers",
                    self.kind()
                )))
            }
        };
        let points_ok = |points: &[[f64; 2]], min: usize, what: &str| -> Result<()> {
            if points.len() < min {
                return Err(Error::usage(format!(
                    "{}: {what} needs at least {min} points, got {}",
                    self.kind(),
                    points.len()
                )));
            }
            finite(&points.iter().flatten().copied().collect::<Vec<_>>(), what)
        };
        match self {
            Location::Bbox { bbox, .. } => {
                finite(bbox, "bbox")?;
                let [x0, y0, x1, y1] = *bbox;
                if x0 >= x1 || y0 >= y1 {
                    return Err(Error::usage(format!(
                        "bbox: need x0 < x1 and y0 < y1, got {x0},{y0},{x1},{y1}"
                    )));
                }
            }
            Location::Polygon { polygon, .. } => {
                points_ok(polygon, 3, "polygon")?;
                if polygon.len() > 64 {
                    return Err(Error::usage(format!(
                        "polygon: at most 64 vertices, got {}",
                        polygon.len()
                    )));
                }
                if !is_simple(polygon) {
                    return Err(Error::usage(
                        "polygon: the edges cross; a polygon must be simple",
                    ));
                }
            }
            Location::View {
                yaw, pitch, fov, ..
            } => {
                finite(&[*yaw, *pitch, *fov], "yaw, pitch and fov")?;
                if !(-180.0..=180.0).contains(yaw)
                    || !(-90.0..=90.0).contains(pitch)
                    || !(10.0..=120.0).contains(fov)
                {
                    return Err(Error::usage(format!(
                        "view: yaw in [-180, 180], pitch in [-90, 90], fov in [10, 120]; got {yaw}, {pitch}, {fov}"
                    )));
                }
            }
            Location::WholeImage { .. } | Location::Absent { .. } => {}
            Location::Line {
                column,
                path,
                width_px,
                ..
            } => {
                finite(&[*width_px], "width_px")?;
                if *width_px <= 0.0 {
                    return Err(Error::usage(format!(
                        "line: width_px must be positive, got {width_px}"
                    )));
                }
                match (column, path) {
                    (Some(c), None) => {
                        finite(&[*c], "column")?;
                        if !(0.0..=1.0).contains(c) {
                            return Err(Error::usage(format!(
                                "line: column must be in [0, 1], got {c}"
                            )));
                        }
                    }
                    (None, Some(path)) => points_ok(path, 2, "path")?,
                    _ => return Err(Error::usage("line: give exactly one of column and path")),
                }
            }
            Location::Stroke {
                path, radius_px, ..
            } => {
                finite(&[*radius_px], "radius_px")?;
                if *radius_px <= 0.0 {
                    return Err(Error::usage(format!(
                        "stroke: radius_px must be positive, got {radius_px}"
                    )));
                }
                points_ok(path, 2, "path")?;
            }
        }
        Ok(())
    }

    /// Every rule: the geometry, the frame match, `view` only on wrapped frames, and the seam
    /// rule for a box or polygon on a wrapped frame.
    pub fn validate(&self, frame: &Frame) -> Result<()> {
        self.validate_geometry()?;
        if self.frame() != frame.id {
            return Err(Error::usage(format!(
                "{}: names frame {} but was checked against {}",
                self.kind(),
                self.frame(),
                frame.id
            )));
        }
        let wraps = frame.topology == Topology::Wrap;
        // A pixel width or radius is bounded by the frame: beyond its diagonal it means
        // nothing, and the ops that walk it would walk for ever.
        let diagonal = (f64::from(frame.width).powi(2) + f64::from(frame.height).powi(2)).sqrt();
        let (kind, px) = match self {
            Location::Line { width_px, .. } => ("line: width_px", Some(*width_px)),
            Location::Stroke { radius_px, .. } => ("stroke: radius_px", Some(*radius_px)),
            _ => ("", None),
        };
        if let Some(px) = px
            && px > diagonal
        {
            return Err(Error::usage(format!(
                "{kind} {px} is larger than the frame's diagonal ({diagonal:.1} px)"
            )));
        }
        match self {
            Location::View { .. } if !wraps => {
                return Err(Error::usage(format!(
                    "view: frame {} does not wrap; a view needs an equirectangular frame",
                    frame.id
                )));
            }
            Location::Bbox {
                bbox: [x0, _, x1, _],
                ..
            } if wraps && (*x0 < 0.0 || *x1 > 1.0) => {
                return Err(Error::usage(format!("bbox {x0}..{x1}: {SEAM_RULE}")));
            }
            Location::Polygon { polygon, .. }
                if wraps && polygon.iter().any(|p| p[0] < 0.0 || p[0] > 1.0) =>
            {
                return Err(Error::usage(format!("polygon: {SEAM_RULE}")));
            }
            _ => {}
        }
        Ok(())
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
        canonical::identity("location", &value)
    }

    /// The points of a polygon, path or stroke; `None` for the other kinds.
    pub fn points(&self) -> Option<&[[f64; 2]]> {
        match self {
            Location::Polygon { polygon, .. } => Some(polygon),
            Location::Line {
                path: Some(path), ..
            } => Some(path),
            Location::Stroke { path, .. } => Some(path),
            _ => None,
        }
    }
}

/// True when no two non-adjacent edges of the closed polygon intersect.
fn is_simple(points: &[[f64; 2]]) -> bool {
    let n = points.len();
    for i in 0..n {
        for j in i + 1..n {
            // Adjacent edges share a vertex; the first and last edges are adjacent too.
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let (a, b) = (points[i], points[(i + 1) % n]);
            let (c, d) = (points[j], points[(j + 1) % n]);
            if segments_cross(a, b, c, d) {
                return false;
            }
        }
    }
    true
}

fn segments_cross(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let orient = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let (o1, o2, o3, o4) = (
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    );
    (o1 * o2 < 0.0) && (o3 * o4 < 0.0)
}

/// A location as the registry lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocationEntry {
    pub id: String,
    pub identity: String,
    pub location: Location,
}
