//! `tighten`: a rough stroke refined to the defect's edges (SPEC.md "Operations", the
//! `tighten` algorithm with `tighten-v1`). The path is resampled; at each sample the luminance
//! is read along the perpendicular over `[−r, r]`, its signed derivative taken, and the best
//! pair of opposite-signed edges inside the stroke chosen; samples without one stay at the
//! full extent; runs of tightened extents are smoothed and a crossing reverts. The core is the
//! tube between consecutive samples' extents with round joins and caps, so an entirely
//! untightened stroke is exactly the mask `rasterize` draws for it.
//!
//! The decisions the plan left to this task (G14): ties after midpoint and width break toward
//! the lower sample index; the direction at an end is one-sided and the path is never extended
//! beyond its ends; a zero central difference takes the previous sample's direction, the first
//! such sample the next nonzero one, and a path with no nonzero direction is not applicable.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use image::{GrayImage, Luma};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::location::Location;
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};
use rascal_core::recipe::mask_artifact;
use rascal_core::series::Series;

use crate::primitives::{luminance, smooth_rows};

/// The op.
pub struct Tighten;

const NAME: &str = "tighten";

/// `tighten-v1`.
pub const TIGHTEN_V1: &str = include_str!("../constants/tighten-v1.toml");

fn set() -> &'static toml::Table {
    static PARSED: OnceLock<toml::Table> = OnceLock::new();
    PARSED.get_or_init(|| {
        TIGHTEN_V1
            .parse::<toml::Table>()
            .unwrap_or_else(|e| panic!("constants tighten-v1: {e}"))
    })
}

/// The tunables a step may override, with `tighten-v1`'s values as the defaults.
const TUNABLES: [(&str, &str); 5] = [
    (
        "min_contrast",
        "The least |g| (luminance levels per pixel) an edge must have to count.",
    ),
    (
        "min_defect_px",
        "The narrowest defect, in pixels between its two edges.",
    ),
    (
        "max_defect_factor",
        "The widest defect, as a multiple of the stroke's radius (2 means the whole stroke).",
    ),
    (
        "path_sigma",
        "The sigma, in samples, of the smoothing of the tightened extents along the path.",
    ),
    (
        "sample_spacing_px",
        "The spacing of the samples along the path, in pixels.",
    ),
];

impl Op for Tighten {
    fn manifest(&self) -> Manifest {
        let number = |key: &str| {
            set()
                .get(key)
                .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null)
        };
        let param = |key: &str, purpose: &str| match key {
            "min_contrast" => {
                Param::optional(key, purpose, ParamKind::Number, number(key)).bounded(0.0, 255.0)
            }
            "min_defect_px" => {
                Param::optional(key, purpose, ParamKind::Number, number(key)).bounded(0.0, 1e4)
            }
            "max_defect_factor" => {
                Param::optional(key, purpose, ParamKind::Number, number(key)).bounded(0.01, 10.0)
            }
            "path_sigma" => {
                Param::optional(key, purpose, ParamKind::Number, number(key)).bounded(0.0, 1e3)
            }
            _ => Param::optional(key, purpose, ParamKind::Number, number(key)).bounded(0.25, 1e3),
        };
        Manifest::new(
            NAME,
            "1.0.0",
            "Tightens a stroke to the defect's edges: at each sample the two strongest opposite-signed luminance edges inside the stroke bound the core.",
            "On a scratch, a tear or a thin defect a person marked roughly with a brush.",
            "On a stain or an area (rasterize a box or a polygon), or where the defect has no edges (the stroke stays as drawn).",
            "A Mask whose core follows the found edges, with round joins and caps, and a Series `tightened` with one row per sample: tightened (0 or 1), and the extents a and b in pixels from the path. A sample with no admissible edge pair keeps the stroke's full radius; an entirely untightened stroke equals rasterize's mask.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![
                Port {
                    name: "stroke".to_string(),
                    artifact_type: ArtifactType::Location,
                    required: true,
                    purpose: "The stroke location, in the image's frame.".to_string(),
                    many: false,
                },
                Port {
                    name: "image".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The image the edges are read from.".to_string(),
                    many: false,
                },
            ],
            vec![
                Port {
                    name: "mask".to_string(),
                    artifact_type: ArtifactType::Mask,
                    required: true,
                    purpose: "The tightened stroke as a mask.".to_string(),
                    many: false,
                },
                Port {
                    name: "tightened".to_string(),
                    artifact_type: ArtifactType::Series,
                    required: true,
                    purpose: "Per sample: tightened (0/1), a, b.".to_string(),
                    many: false,
                },
            ],
            TUNABLES.iter().map(|(k, p)| param(k, p)).collect(),
        )
        .with_constants(&["tighten-v1"])
    }

    fn constant_set(&self, name: &str) -> Params {
        if name != "tighten-v1" {
            return Params::new();
        }
        set()
            .iter()
            .filter(|(k, _)| TUNABLES.iter().any(|(t, _)| t == k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let (Some(Artifact::Location { location, identity }), Some(Artifact::Image(image))) =
            (inputs.get("stroke"), inputs.get("image"))
        else {
            return Err(Error::op(NAME, "inputs stroke and image are required"));
        };
        let Location::Stroke {
            path, radius_px, ..
        } = location
        else {
            return Ok(Output::not_applicable(format!(
                "tighten takes a stroke location, not a {}",
                location.kind()
            )));
        };
        if location.frame() != image.record.frame {
            return Err(Error::op(
                NAME,
                format!(
                    "the stroke is in frame {} but the image is in {}",
                    location.frame(),
                    image.record.frame
                ),
            ));
        }
        let frame = context.frames.require(&image.record.frame)?;
        let number = |key: &str| {
            crate::float(params, key).ok_or_else(|| Error::op(NAME, format!("parameter {key}")))
        };
        let settings = Settings {
            radius: *radius_px,
            min_contrast: number("min_contrast")?,
            min_defect: number("min_defect_px")?,
            max_defect: number("max_defect_factor")? * radius_px,
            path_sigma: number("path_sigma")?,
            spacing: number("sample_spacing_px")?,
        };
        let (width, height) = (frame.width, frame.height);
        let pixels: Vec<[f64; 2]> = path
            .iter()
            .map(|p| [p[0] * f64::from(width), p[1] * f64::from(height)])
            .collect();
        let samples = resample(&pixels, settings.spacing);
        let Some(directions) = directions(&samples) else {
            return Ok(Output::not_applicable(
                "the path has no nonzero direction: every sample sits on the same point",
            ));
        };
        let rgb = image.pixels.to_rgb8();
        let lum = luminance(&rgb);
        let read = |x: f64, y: f64| bilinear(&lum, width, height, x, y);
        let mut extents: Vec<Extent> = samples
            .iter()
            .zip(&directions)
            .map(|(p, d)| {
                let n = [-d[1], d[0]];
                let profile = perpendicular_profile(&read, *p, n, settings.radius);
                let g = derivative(&profile.values);
                match best_pair(&g, &profile.offsets, &settings) {
                    Some((a, b)) => Extent {
                        a: profile.offsets[a],
                        b: profile.offsets[b],
                        tightened: true,
                    },
                    None => Extent::full(settings.radius),
                }
            })
            .collect();
        smooth_extents(&mut extents, settings.path_sigma, settings.radius);
        let core = stroke_core(&samples, &directions, &extents, width, height);
        let painted = core.pixels().filter(|p| p[0] != 0).count();
        if painted == 0 {
            return Ok(Output::not_applicable(
                "the stroke covers no pixel of the frame",
            ));
        }
        let tightened = extents.iter().filter(|e| e.tightened).count();
        let mut values = Vec::with_capacity(extents.len() * 3);
        for e in &extents {
            values.push(if e.tightened { 1.0 } else { 0.0 });
            values.push(e.a);
            values.push(e.b);
        }
        let series = Series::with_rows(
            "tightened",
            "tightened",
            "px",
            &frame.id,
            &frame.identity,
            3,
            (0..extents.len() as u32).collect(),
            values,
        )?;
        let mask = Mask::from_core(
            "mask",
            frame,
            UseCase::Repair,
            core,
            Source {
                locations: vec![identity.as_str().to_string()],
                instances: Vec::new(),
            },
        )?;
        let mut outputs = BTreeMap::new();
        outputs.insert("mask".to_string(), mask_artifact(mask));
        outputs.insert("tightened".to_string(), Artifact::Series(Arc::new(series)));
        let n = extents.len();
        Ok(Output::ok(
            format!(
                "tightened {tightened} of {n} samples; core of {painted} pixels (radius {} px)",
                settings.radius
            ),
            outputs,
        )
        .with_measurement("samples", n as f64, "count", Some(&frame.id))
        .with_measurement(
            "tightened_samples",
            tightened as f64,
            "count",
            Some(&frame.id),
        )
        .with_measurement(
            "tightened_fraction",
            if n == 0 {
                0.0
            } else {
                tightened as f64 / n as f64
            },
            "ratio",
            Some(&frame.id),
        )
        .with_measurement("radius_px", settings.radius, "px", Some(&frame.id)))
    }
}

/// The resolved numbers one call runs with.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub radius: f64,
    pub min_contrast: f64,
    pub min_defect: f64,
    pub max_defect: f64,
    pub path_sigma: f64,
    pub spacing: f64,
}

/// One sample's extent across the path, in pixels along the perpendicular.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent {
    pub a: f64,
    pub b: f64,
    pub tightened: bool,
}

impl Extent {
    /// The stroke's full radius, untightened.
    pub fn full(radius: f64) -> Extent {
        Extent {
            a: -radius,
            b: radius,
            tightened: false,
        }
    }
}

/// The path resampled at `spacing` pixels by linear interpolation: the first point, every
/// multiple of the spacing along the cumulative length, and the last point.
pub fn resample(path: &[[f64; 2]], spacing: f64) -> Vec<[f64; 2]> {
    let spacing = spacing.max(1e-6);
    let Some(first) = path.first() else {
        return Vec::new();
    };
    let mut out = vec![*first];
    let mut carried = 0.0; // distance travelled since the last emitted sample
    for seg in path.windows(2) {
        let (p, q) = (seg[0], seg[1]);
        let len = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt();
        if len == 0.0 {
            continue;
        }
        let mut along = spacing - carried;
        while along <= len + 1e-9 {
            let t = along / len;
            out.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
            along += spacing;
        }
        carried = len - (along - spacing);
    }
    if let Some(last) = path.last()
        && out
            .last()
            .is_some_and(|o| ((o[0] - last[0]).powi(2) + (o[1] - last[1]).powi(2)).sqrt() > 1e-6)
    {
        out.push(*last);
    }
    out
}

/// The unit direction at every sample: the central difference of its neighbours, one-sided at
/// the ends; a zero difference takes the previous sample's direction, the first such the next
/// nonzero one. `None` when no sample has a nonzero direction.
pub fn directions(samples: &[[f64; 2]]) -> Option<Vec<[f64; 2]>> {
    let n = samples.len();
    if n < 2 {
        return None;
    }
    let raw: Vec<Option<[f64; 2]>> = (0..n)
        .map(|i| {
            let (p, q) = if i == 0 {
                (samples[0], samples[1])
            } else if i == n - 1 {
                (samples[n - 2], samples[n - 1])
            } else {
                (samples[i - 1], samples[i + 1])
            };
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            let len = (dx * dx + dy * dy).sqrt();
            (len > 1e-9).then(|| [dx / len, dy / len])
        })
        .collect();
    let first_nonzero = raw.iter().find_map(|d| *d)?;
    let mut out = Vec::with_capacity(n);
    let mut previous = first_nonzero;
    for d in raw {
        let d = d.unwrap_or(previous);
        out.push(d);
        previous = d;
    }
    Some(out)
}

/// Luminance by bilinear interpolation at a point in pixel coordinates (pixel `i` is centred
/// at `i + 0.5`), clamped to the image.
pub fn bilinear(plane: &[f32], width: u32, height: u32, x: f64, y: f64) -> f64 {
    let (w, h) = (width as i64, height as i64);
    let fx = (x - 0.5).clamp(0.0, (w - 1) as f64);
    let fy = (y - 0.5).clamp(0.0, (h - 1) as f64);
    let (x0, y0) = (fx.floor() as i64, fy.floor() as i64);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
    let at = |xx: i64, yy: i64| f64::from(plane[(yy * w + xx) as usize]);
    let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
    let bottom = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
    top * (1.0 - ty) + bottom * ty
}

/// A sample's luminance along its perpendicular.
pub struct Profile {
    /// The offsets from the path, `−r … r` at 1 px steps (the last one may be `r` itself).
    pub offsets: Vec<f64>,
    pub values: Vec<f64>,
}

/// Reads `L` along the perpendicular `n` through `p` over `[−r, r]` at 1 px steps.
pub fn perpendicular_profile(
    read: &impl Fn(f64, f64) -> f64,
    p: [f64; 2],
    n: [f64; 2],
    radius: f64,
) -> Profile {
    let mut offsets = Vec::new();
    let mut t = -radius;
    while t <= radius + 1e-9 {
        offsets.push(t);
        t += 1.0;
    }
    if offsets.last().is_some_and(|last| radius - last > 1e-9) {
        offsets.push(radius);
    }
    let values = offsets
        .iter()
        .map(|t| read(p[0] + t * n[0], p[1] + t * n[1]))
        .collect();
    Profile { offsets, values }
}

/// The signed first derivative by central differences, one-sided at the ends.
pub fn derivative(values: &[f64]) -> Vec<f64> {
    let n = values.len();
    (0..n)
        .map(|i| {
            if n < 2 {
                0.0
            } else if i == 0 {
                values[1] - values[0]
            } else if i == n - 1 {
                values[n - 1] - values[n - 2]
            } else {
                (values[i + 1] - values[i - 1]) / 2.0
            }
        })
        .collect()
}

/// The best boundary pair `(a, b)`, `a < b`, of opposite-signed derivatives both at least
/// `min_contrast` in size, `min_defect ≤ offset[b] − offset[a] ≤ max_defect`: the highest
/// `|g(a)| + |g(b)|`; ties break toward the midpoint nearest the path, then the narrower pair,
/// then the lower first index.
pub fn best_pair(g: &[f64], offsets: &[f64], settings: &Settings) -> Option<(usize, usize)> {
    let mut best: Option<((usize, usize), f64, f64, f64)> = None;
    for a in 0..g.len() {
        if g[a].abs() < settings.min_contrast {
            continue;
        }
        for b in a + 1..g.len() {
            if g[b].abs() < settings.min_contrast || g[a].signum() == g[b].signum() {
                continue;
            }
            let width = offsets[b] - offsets[a];
            if width < settings.min_defect - 1e-9 || width > settings.max_defect + 1e-9 {
                continue;
            }
            let score = g[a].abs() + g[b].abs();
            let midpoint = ((offsets[a] + offsets[b]) / 2.0).abs();
            let better = match &best {
                None => true,
                Some((_, s, m, w)) => {
                    score > *s + 1e-9
                        || ((score - s).abs() <= 1e-9
                            && (midpoint < *m - 1e-9
                                || ((midpoint - m).abs() <= 1e-9 && width < *w - 1e-9)))
                }
            };
            if better {
                best = Some(((a, b), score, midpoint, width));
            }
        }
    }
    best.map(|(pair, _, _, _)| pair)
}

/// Smooths `a` and `b` separately over every maximal run of tightened samples with
/// `smooth_rows(·, sigma)`; a sample whose smoothed `a ≥ b` reverts to the full extent. (One
/// kernel smooths both, so proper extents never cross by smoothing alone; the reversion is
/// the guard the spec asks for.)
pub fn smooth_extents(extents: &mut [Extent], sigma: f64, radius: f64) {
    let n = extents.len();
    let mut start = 0;
    while start < n {
        if !extents[start].tightened {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < n && extents[end].tightened {
            end += 1;
        }
        let a: Vec<f64> = extents[start..end].iter().map(|e| e.a).collect();
        let b: Vec<f64> = extents[start..end].iter().map(|e| e.b).collect();
        let (sa, sb) = (smooth_rows(&a, sigma), smooth_rows(&b, sigma));
        for (i, e) in extents[start..end].iter_mut().enumerate() {
            if sa[i] >= sb[i] {
                *e = Extent::full(radius);
            } else {
                e.a = sa[i];
                e.b = sb[i];
            }
        }
        start = end;
    }
}

/// The core: for each segment between consecutive samples, the pixels whose foot on the
/// segment lies inside it and whose signed perpendicular offset is within the extents
/// interpolated along the segment, plus a round cap at each sample of its local half-extent
/// centred on its local midpoint (the joins and the two ends). An untightened stroke, every
/// extent `[−r, r]`, is exactly the dilated path `rasterize` draws.
pub fn stroke_core(
    samples: &[[f64; 2]],
    directions: &[[f64; 2]],
    extents: &[Extent],
    width: u32,
    height: u32,
) -> GrayImage {
    let mut core = GrayImage::new(width, height);
    if samples.is_empty() {
        return core;
    }
    let reach = extents
        .iter()
        .map(|e| e.a.abs().max(e.b.abs()))
        .fold(0.0_f64, f64::max);
    let (min_x, max_x, min_y, max_y) = samples.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |acc, p| {
            (
                acc.0.min(p[0]),
                acc.1.max(p[0]),
                acc.2.min(p[1]),
                acc.3.max(p[1]),
            )
        },
    );
    let x_lo = (min_x - reach - 1.0).floor().max(0.0) as u32;
    let x_hi = ((max_x + reach + 1.0).ceil().max(0.0) as u32).min(width);
    let y_lo = (min_y - reach - 1.0).floor().max(0.0) as u32;
    let y_hi = ((max_y + reach + 1.0).ceil().max(0.0) as u32).min(height);
    // Caps: centre and radius per sample, along the sample's own normal.
    let caps: Vec<([f64; 2], f64)> = samples
        .iter()
        .zip(directions)
        .zip(extents)
        .map(|((p, d), e)| {
            let n = [-d[1], d[0]];
            let m = (e.a + e.b) / 2.0;
            ([p[0] + m * n[0], p[1] + m * n[1]], (e.b - e.a) / 2.0)
        })
        .collect();
    for y in y_lo..y_hi {
        let cy = f64::from(y) + 0.5;
        for x in x_lo..x_hi {
            let cx = f64::from(x) + 0.5;
            let in_cap = caps
                .iter()
                .any(|(c, r)| (cx - c[0]).powi(2) + (cy - c[1]).powi(2) <= r * r + 1e-9);
            let in_tube = !in_cap
                && samples.windows(2).enumerate().any(|(i, seg)| {
                    let (p, q) = (seg[0], seg[1]);
                    let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
                    let len2 = dx * dx + dy * dy;
                    if len2 == 0.0 {
                        return false;
                    }
                    let u = ((cx - p[0]) * dx + (cy - p[1]) * dy) / len2;
                    if !(0.0..=1.0).contains(&u) {
                        return false;
                    }
                    let len = len2.sqrt();
                    // The signed offset along the segment's left normal (−dy, dx).
                    let d = (-(dy) * (cx - p[0]) + dx * (cy - p[1])) / len;
                    let (ea, eb) = (&extents[i], &extents[i + 1]);
                    let a = ea.a + u * (eb.a - ea.a);
                    let b = ea.b + u * (eb.b - ea.b);
                    d >= a - 1e-9 && d <= b + 1e-9
                });
            if in_cap || in_tube {
                core.put_pixel(x, y, Luma([255]));
            }
        }
    }
    core
}
