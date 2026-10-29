//! Mapping locations between frames (SPEC.md "Locations", "Mapping"). Every location maps
//! toward its root raw frame through the frame chain. The result is `parts` (one location per
//! contiguous region in the target: a geometry that crosses a discontinuity, the centre of a
//! `cut:` frame, is split), `partial` (geometry partly outside the target was clipped) and the
//! source location's id. Geometry wholly outside the target is an error. Only unclipped
//! mappings round-trip exactly; a clipped mapping keeps the original on the source side.
//!
//! The mappings are affine per region and involve no resampling: pixel widths and radii are
//! unchanged, since every v1 derived frame has its parent's pixel scale.

use crate::error::{Error, Result};
use crate::frame::{Frame, FrameKind, FrameTable, Topology};
use crate::location::Location;

/// The result of mapping one location into another frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Mapping {
    pub parts: Vec<Location>,
    pub partial: bool,
    pub source: String,
}

/// An affine map `x' = ax·x + bx`, `y' = ay·y + by` on normalised coordinates, valid on a
/// source rectangle `[lo, hi] × [ylo, yhi]`, used for one region of one frame step.
#[derive(Debug, Clone, Copy)]
struct Region {
    lo: f64,
    hi: f64,
    ylo: f64,
    yhi: f64,
    ax: f64,
    bx: f64,
    ay: f64,
    by: f64,
}

impl Region {
    fn point(&self, p: [f64; 2]) -> [f64; 2] {
        [self.ax * p[0] + self.bx, self.ay * p[1] + self.by]
    }
    /// The inverse map, valid on this region's image.
    fn inverse(&self) -> Region {
        let (lo, hi) = order(self.ax * self.lo + self.bx, self.ax * self.hi + self.bx);
        let (ylo, yhi) = order(self.ay * self.ylo + self.by, self.ay * self.yhi + self.by);
        Region {
            lo,
            hi,
            ylo,
            yhi,
            ax: 1.0 / self.ax,
            bx: -self.bx / self.ax,
            ay: 1.0 / self.ay,
            by: -self.by / self.ay,
        }
    }
    fn contains(&self, p: [f64; 2]) -> bool {
        p[0] >= self.lo - 1e-12
            && p[0] <= self.hi + 1e-12
            && p[1] >= self.ylo - 1e-12
            && p[1] <= self.yhi + 1e-12
    }
}

/// The regions of a frame step: from `frame` into its parent.
fn regions_to_parent(frame: &Frame, parent: &Frame) -> Result<Vec<Region>> {
    match (frame.kind, &frame.bbox, &frame.plan) {
        (FrameKind::Raw, _, _) => Err(Error::usage(format!(
            "frame {} is a root and has no parent",
            frame.id
        ))),
        (FrameKind::Crop, Some(bbox), _) => {
            let [l, t, _, _] = *bbox;
            let (wc, hc) = (f64::from(frame.width), f64::from(frame.height));
            let (wp, hp) = (f64::from(parent.width), f64::from(parent.height));
            Ok(vec![Region {
                lo: 0.0,
                hi: 1.0,
                ylo: 0.0,
                yhi: 1.0,
                ax: wc / wp,
                bx: f64::from(l) / wp,
                ay: hc / hp,
                by: f64::from(t) / hp,
            }])
        }
        (FrameKind::Cut, _, Some(plan)) => {
            let w = f64::from(plan.width);
            let s = f64::from(plan.strip);
            // Canvas x in [0, 0.5] is the parent's rightmost s columns; [0.5, 1] the leftmost.
            Ok(vec![
                Region {
                    lo: 0.0,
                    hi: 0.5,
                    ylo: 0.0,
                    yhi: 1.0,
                    ax: 2.0 * s / w,
                    bx: (w - s) / w,
                    ay: 1.0,
                    by: 0.0,
                },
                Region {
                    lo: 0.5,
                    hi: 1.0,
                    ylo: 0.0,
                    yhi: 1.0,
                    ax: 2.0 * s / w,
                    bx: -s / w,
                    ay: 1.0,
                    by: 0.0,
                },
            ])
        }
        _ => Err(Error::usage(format!(
            "frame {}: fields do not match its kind",
            frame.id
        ))),
    }
}

/// Maps a location one step, into the frame's parent. A geometry that spans two regions is
/// split; one that reaches outside every region is clipped (`partial`); one wholly outside
/// is an error.
pub fn to_parent(
    location: &Location,
    frame: &Frame,
    parent: &Frame,
    source: &str,
) -> Result<Mapping> {
    let regions = regions_to_parent(frame, parent)?;
    apply(
        location,
        &regions,
        &parent.id,
        source,
        parent.topology == Topology::Wrap,
    )
}

/// Maps a location one step down, from the parent into the frame. The inverse of
/// `to_parent`: parent geometry outside the frame's regions is clipped.
pub fn from_parent(
    location: &Location,
    frame: &Frame,
    parent: &Frame,
    source: &str,
) -> Result<Mapping> {
    let regions: Vec<Region> = regions_to_parent(frame, parent)?
        .iter()
        .map(Region::inverse)
        .collect();
    apply(
        location,
        &regions,
        &frame.id,
        source,
        frame.topology == Topology::Wrap,
    )
}

/// Maps a location toward its root raw frame through the chain. A location already in a raw
/// frame maps to itself.
pub fn to_root(location: &Location, frames: &FrameTable, source: &str) -> Result<Mapping> {
    let chain = frames.chain(location.frame())?;
    let mut current = Mapping {
        parts: vec![location.clone()],
        partial: false,
        source: source.to_string(),
    };
    for pair in chain.windows(2) {
        let (frame, parent) = (pair[0], pair[1]);
        let mut next = Mapping {
            parts: Vec::new(),
            partial: current.partial,
            source: source.to_string(),
        };
        for part in &current.parts {
            let mapped = to_parent(part, frame, parent, source)?;
            next.partial |= mapped.partial;
            next.parts.extend(mapped.parts);
        }
        current = next;
    }
    Ok(current)
}

fn apply(
    location: &Location,
    regions: &[Region],
    target: &str,
    source: &str,
    target_wraps: bool,
) -> Result<Mapping> {
    let group_for = |n: usize, existing: &Option<String>| -> Option<String> {
        if n > 1 {
            Some(
                existing
                    .clone()
                    .unwrap_or_else(|| format!("{source}:split")),
            )
        } else {
            existing.clone()
        }
    };
    let mut parts = Vec::new();
    let mut clipped = false;
    match location {
        Location::Absent { .. } => parts.push(Location::Absent {
            frame: target.to_string(),
        }),
        Location::View { .. } => {
            return Err(Error::usage(
                "a view lives only on a raw wrapped frame and does not map",
            ));
        }
        Location::WholeImage { .. } => {
            for region in regions {
                let [x0, y0] = region.point([region.lo, region.ylo]);
                let [x1, y1] = region.point([region.hi, region.yhi]);
                let (x0, x1) = order(x0, x1);
                let (y0, y1) = order(y0, y1);
                parts.push(Location::Bbox {
                    frame: target.to_string(),
                    bbox: [x0, y0, x1, y1],
                });
            }
        }
        Location::Bbox { bbox, .. } => {
            let [x0, y0, x1, y1] = *bbox;
            let mut covered = 0.0;
            let mut y_clipped = false;
            for region in regions {
                let (lo, hi) = (x0.max(region.lo), x1.min(region.hi));
                let (ylo, yhi) = (y0.max(region.ylo), y1.min(region.yhi));
                if hi <= lo || yhi <= ylo {
                    continue;
                }
                covered += hi - lo;
                y_clipped |= ylo > y0 || yhi < y1;
                let [px0, py0] = region.point([lo, ylo]);
                let [px1, py1] = region.point([hi, yhi]);
                let (px0, px1) = order(px0, px1);
                let (py0, py1) = order(py0, py1);
                parts.push(Location::Bbox {
                    frame: target.to_string(),
                    bbox: [px0, py0, px1, py1],
                });
            }
            clipped = covered + 1e-12 < (x1 - x0) || y_clipped;
        }
        Location::Polygon { polygon, .. } => {
            for region in regions {
                let piece = clip_polygon(polygon, region);
                if piece.len() >= 3 {
                    parts.push(Location::Polygon {
                        frame: target.to_string(),
                        polygon: piece.iter().map(|p| region.point(*p)).collect(),
                    });
                }
            }
            clipped = polygon
                .iter()
                .any(|p| !regions.iter().any(|r| r.contains(*p)));
        }
        Location::Line {
            column: Some(c),
            width_px,
            group,
            ..
        } => {
            for region in regions {
                if *c >= region.lo && *c <= region.hi {
                    let [x, _] = region.point([*c, 0.0]);
                    let x = if target_wraps && (x - 1.0).abs() < 1e-12 {
                        0.0
                    } else {
                        x
                    };
                    parts.push(Location::Line {
                        frame: target.to_string(),
                        column: Some(x),
                        path: None,
                        width_px: *width_px,
                        group: group.clone(),
                    });
                    break;
                }
            }
        }
        Location::Line {
            path: Some(path),
            width_px,
            group,
            ..
        } => {
            let runs = split_polyline(path, regions);
            clipped = runs.clipped;
            let n = runs.pieces.len();
            for (region, points) in runs.pieces {
                parts.push(Location::Line {
                    frame: target.to_string(),
                    column: None,
                    path: Some(points.iter().map(|p| region.point(*p)).collect()),
                    width_px: *width_px,
                    group: group_for(n, group),
                });
            }
        }
        Location::Line { .. } => {
            return Err(Error::usage("line: give exactly one of column and path"));
        }
        Location::Stroke {
            path,
            radius_px,
            group,
            ..
        } => {
            let runs = split_polyline(path, regions);
            clipped = runs.clipped;
            let n = runs.pieces.len();
            for (region, points) in runs.pieces {
                parts.push(Location::Stroke {
                    frame: target.to_string(),
                    path: points.iter().map(|p| region.point(*p)).collect(),
                    radius_px: *radius_px,
                    group: group_for(n, group),
                });
            }
        }
    }
    if parts.is_empty() {
        return Err(Error::usage(format!(
            "{} in frame {} lies wholly outside {target}",
            location.kind(),
            location.frame()
        )));
    }
    Ok(Mapping {
        parts,
        partial: clipped,
        source: source.to_string(),
    })
}

fn order(a: f64, b: f64) -> (f64, f64) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Sutherland–Hodgman against the region's four edges.
fn clip_polygon(points: &[[f64; 2]], region: &Region) -> Vec<[f64; 2]> {
    let planes: [Plane; 4] = [
        Plane {
            inside: |p, v| p[0] >= v,
            intersect: cross_x,
            value: region.lo,
        },
        Plane {
            inside: |p, v| p[0] <= v,
            intersect: cross_x,
            value: region.hi,
        },
        Plane {
            inside: |p, v| p[1] >= v,
            intersect: cross_y,
            value: region.ylo,
        },
        Plane {
            inside: |p, v| p[1] <= v,
            intersect: cross_y,
            value: region.yhi,
        },
    ];
    let mut output: Vec<[f64; 2]> = points.to_vec();
    for Plane {
        inside,
        intersect,
        value,
    } in planes
    {
        let input = std::mem::take(&mut output);
        if input.is_empty() {
            break;
        }
        let mut prev = input[input.len() - 1];
        for &current in &input {
            let (cur_in, prev_in) = (inside(current, value), inside(prev, value));
            if cur_in {
                if !prev_in {
                    output.push(intersect(prev, current, value));
                }
                output.push(current);
            } else if prev_in {
                output.push(intersect(prev, current, value));
            }
            prev = current;
        }
    }
    output.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12);
    if output.len() > 1
        && (output[0][0] - output[output.len() - 1][0]).abs() < 1e-12
        && (output[0][1] - output[output.len() - 1][1]).abs() < 1e-12
    {
        output.pop();
    }
    output
}

/// One clipping half-plane: a test for the inside and the intersection with its edge.
struct Plane {
    inside: fn([f64; 2], f64) -> bool,
    intersect: fn([f64; 2], [f64; 2], f64) -> [f64; 2],
    value: f64,
}

fn cross_x(a: [f64; 2], b: [f64; 2], x: f64) -> [f64; 2] {
    let t = (x - a[0]) / (b[0] - a[0]);
    [x, a[1] + t * (b[1] - a[1])]
}

fn cross_y(a: [f64; 2], b: [f64; 2], y: f64) -> [f64; 2] {
    let t = (y - a[1]) / (b[1] - a[1]);
    [a[0] + t * (b[0] - a[0]), y]
}

struct Runs {
    pieces: Vec<(Region, Vec<[f64; 2]>)>,
    clipped: bool,
}

/// Splits a polyline into runs, one per region crossed, cutting each segment at region
/// boundaries; parts outside every region are dropped and reported as clipping.
fn split_polyline(path: &[[f64; 2]], regions: &[Region]) -> Runs {
    let mut pieces: Vec<(Region, Vec<[f64; 2]>)> = Vec::new();
    let mut clipped = false;
    for region in regions {
        let mut current: Vec<[f64; 2]> = Vec::new();
        for pair in path.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            match clip_segment(a, b, region) {
                None => {
                    if !region.contains(a) || !region.contains(b) {
                        clipped = true;
                    }
                    if current.len() >= 2 {
                        pieces.push((*region, std::mem::take(&mut current)));
                    } else {
                        current.clear();
                    }
                }
                Some((p, q)) => {
                    if p != a || q != b {
                        clipped = true;
                    }
                    if current.is_empty() || current[current.len() - 1] != p {
                        if current.len() >= 2 {
                            pieces.push((*region, std::mem::take(&mut current)));
                        } else {
                            current.clear();
                        }
                        current.push(p);
                    }
                    current.push(q);
                }
            }
        }
        if current.len() >= 2 {
            pieces.push((*region, current));
        }
    }
    // A split exactly at a region boundary is not clipping: both halves were kept.
    if clipped {
        clipped = path.iter().any(|p| !regions.iter().any(|r| r.contains(*p)));
    }
    Runs { pieces, clipped }
}

/// The part of segment `ab` inside the region's x-range and the unit y-range (Liang–Barsky).
fn clip_segment(a: [f64; 2], b: [f64; 2], region: &Region) -> Option<([f64; 2], [f64; 2])> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let mut t0 = 0.0_f64;
    let mut t1 = 1.0_f64;
    for (p, q) in [
        (-dx, a[0] - region.lo),
        (dx, region.hi - a[0]),
        (-dy, a[1]),
        (dy, 1.0 - a[1]),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    if t0 > t1 {
        return None;
    }
    let point = |t: f64| {
        if t == 0.0 {
            a
        } else if t == 1.0 {
            b
        } else {
            [a[0] + t * dx, a[1] + t * dy]
        }
    };
    Some((point(t0), point(t1)))
}
