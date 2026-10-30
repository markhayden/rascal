//! `fan`: the band profile along a column line, the seam's per-row half-width (SPEC.md
//! "Operations", the `fan` algorithm). Two rulers per row, the tone step across the line and
//! the texture beside it, decide how far the band may open: wide where the surface is smooth
//! and the two sides differ, `thin` where it is detailed or the sides already agree. The
//! window geometry it resolves is recorded in the profile so `ramp` and the measures reuse
//! exactly the same columns.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::frame::Topology;
use rascal_core::location::Location;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};
use rascal_core::profile::{BandProfile, Windows};

use crate::primitives::{luminance, smooth_rows, sobel_magnitude};

/// The op.
pub struct Fan;

const NAME: &str = "fan";

/// The embedded constant sets, by name; the first is the default.
const SETS: [(&str, &str); 4] = [
    ("fan-v1", include_str!("../constants/fan-v1.toml")),
    ("fan-thin-v1", include_str!("../constants/fan-thin-v1.toml")),
    ("fan-v2", include_str!("../constants/fan-v2.toml")),
    ("fan-v3", include_str!("../constants/fan-v3.toml")),
];

/// A parsed constant set.
fn set(name: &str) -> Option<&'static toml::Table> {
    static PARSED: OnceLock<BTreeMap<&'static str, toml::Table>> = OnceLock::new();
    PARSED
        .get_or_init(|| {
            SETS.iter()
                .map(|(name, text)| {
                    (
                        *name,
                        text.parse::<toml::Table>()
                            .unwrap_or_else(|e| panic!("constants {name}: {e}")),
                    )
                })
                .collect()
        })
        .get(name)
}

/// The names a step may select.
pub fn set_names() -> Vec<&'static str> {
    SETS.iter().map(|(n, _)| *n).collect()
}

/// `object_straddle`: `"thin"` (the row stays at the minimum width) or `"whole"` (the row
/// is left to the rulers, so the object is removed whole).
fn straddle_whole(table: &toml::Table) -> Result<bool> {
    match table.get("object_straddle").and_then(|v| v.as_str()) {
        Some("thin") => Ok(false),
        Some("whole") => Ok(true),
        other => Err(Error::op(
            NAME,
            format!("constant object_straddle must be \"thin\" or \"whole\", got {other:?}"),
        )),
    }
}

/// The vertical run of each strong step: `up[y][x]` counts the chain of strong steps ending
/// at row `y` (each within `wander` columns of the previous row's), `down` the chain starting
/// there; the run is `up + down − 1` (at least 1 on a strong step, 0 elsewhere).
fn vertical_runs(strong: &[bool], ws: usize, hs: usize, wander: u32) -> Vec<u32> {
    let wander = wander as usize;
    let chain = |rows: &mut dyn Iterator<Item = usize>| -> Vec<u32> {
        let mut out = vec![0u32; ws * hs];
        let mut prev: Option<usize> = None;
        for y in rows {
            for x in 0..ws {
                if !strong[y * ws + x] {
                    continue;
                }
                let best = prev.map_or(0, |p| {
                    let lo = x.saturating_sub(wander);
                    let hi = (x + wander).min(ws - 1);
                    (lo..=hi).map(|xx| out[p * ws + xx]).max().unwrap_or(0)
                });
                out[y * ws + x] = best + 1;
            }
            prev = Some(y);
        }
        out
    };
    let up = chain(&mut (0..hs));
    let down = chain(&mut (0..hs).rev());
    up.iter()
        .zip(&down)
        .map(|(u, d)| if *u == 0 { 0 } else { u + d - 1 })
        .collect()
}

/// A whole number from a set.
fn whole_number(table: &toml::Table, key: &str) -> Result<u32> {
    match table.get(key) {
        Some(toml::Value::Integer(i)) if *i >= 0 => u32::try_from(*i)
            .map_err(|_| Error::op(NAME, format!("constant {key} is out of range: {i}"))),
        other => Err(Error::op(
            NAME,
            format!("constant {key} must be a whole number, got {other:?}"),
        )),
    }
}

/// A number from a set.
fn constant(table: &toml::Table, key: &str) -> Result<f64> {
    table
        .get(key)
        .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        .ok_or_else(|| Error::op(NAME, format!("constant {key} is missing or not a number")))
}

/// The tunables a step may override, with the default set's values as the manifest's defaults.
const TUNABLES: [(&str, &str); 10] = [
    (
        "object_edge",
        "The object cap: a horizontal luminance step (levels per pixel) at and above which a column inside the opening counts as a solid object's edge, beyond the seam's own gap; the row stops short of it, and stays thin when edges stand on both sides. 0 turns the cap off.",
    ),
    (
        "object_margin_px",
        "How far short of an object's edge the band stops, in pixels at ref_width.",
    ),
    (
        "tex_low",
        "Texture (contrast, percent per pixel) at and below which the surface counts as fully smooth.",
    ),
    (
        "tex_high",
        "Texture at and above which the band stays at its minimum width.",
    ),
    (
        "wide_max",
        "The widest half-width, as a fraction of the frame width; 0 fixes every row at thin.",
    ),
    (
        "gate",
        "Whether the tone step across the line gates the opening (a row whose sides already agree stays thin).",
    ),
    (
        "thin_px",
        "The minimum half-width in pixels at ref_width, scaled by the frame width.",
    ),
    (
        "ref_width",
        "The frame width the pixel constants were fitted at.",
    ),
    (
        "row_sigma",
        "Row smoothing of the two rulers, as a fraction of the height.",
    ),
    (
        "width_sigma",
        "Row smoothing of the half-width, as a fraction of the height.",
    ),
];

impl Op for Fan {
    fn manifest(&self) -> Manifest {
        let defaults = set("fan-v1").expect("the default set is embedded");
        let number = |key: &str| -> serde_json::Value {
            match defaults.get(key) {
                Some(toml::Value::Float(f)) => serde_json::json!(f),
                Some(toml::Value::Integer(i)) => serde_json::json!(i),
                Some(toml::Value::Boolean(b)) => serde_json::json!(b),
                _ => serde_json::Value::Null,
            }
        };
        let param = |key: &str, purpose: &str| -> Param {
            match key {
                "gate" => Param::optional(key, purpose, ParamKind::Boolean, number(key)),
                "ref_width" => {
                    Param::optional(key, purpose, ParamKind::Integer, number(key)).bounded(1.0, 1e6)
                }
                "wide_max" => {
                    Param::optional(key, purpose, ParamKind::Number, number(key)).bounded(0.0, 0.5)
                }
                "object_edge" => Param::optional(key, purpose, ParamKind::Number, number(key))
                    .bounded(0.0, 255.0),
                "object_margin_px" => {
                    Param::optional(key, purpose, ParamKind::Number, number(key)).bounded(0.0, 1e4)
                }
                "thin_px" => {
                    Param::optional(key, purpose, ParamKind::Number, number(key)).bounded(0.5, 1e4)
                }
                "row_sigma" | "width_sigma" => {
                    Param::optional(key, purpose, ParamKind::Number, number(key)).bounded(0.0, 1.0)
                }
                _ => Param::optional(key, purpose, ParamKind::Number, number(key))
                    .bounded(0.0, 100.0),
            }
        };
        Manifest::new(
            NAME,
            "1.2.1",
            "The band profile along a column line: a half-width per row, wide where the surface beside the line is smooth and the two sides differ in tone, thin where it is detailed, and stopping short of a solid object the opening would cross (the object cap, fan-v2; fan-v3 removes a straddling object whole and tapers the cap).",
            "On a cut canvas whose centre column is the wrap edge of a panorama, or any image with a vertical line to heal: the step before profile and ramp.",
            "When the line is a path or the region is an area (use rasterize); on an uncut equirectangular image (cut it first).",
            "A BandProfile: half[y], the centre column, thin and the resolved windows. Not applicable when a sampling window clips to fewer than two columns (the line is too near an edge for the frame width).",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.05,
            },
            vec![
                Port {
                    name: "image".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The image, in a bounded frame.".to_string(),
                    many: false,
                },
                Port {
                    name: "line".to_string(),
                    artifact_type: ArtifactType::Location,
                    required: true,
                    purpose: "The line location in column form, in the image's frame.".to_string(),
                    many: false,
                },
            ],
            vec![Port {
                name: "band".to_string(),
                artifact_type: ArtifactType::BandProfile,
                required: true,
                purpose: "The band profile.".to_string(),
                many: false,
            }],
            TUNABLES.iter().map(|(k, p)| param(k, p)).collect(),
        )
        .with_constants(&set_names())
    }

    fn constant_set(&self, name: &str) -> Params {
        set(name)
            .map(|table| {
                table
                    .iter()
                    .filter(|(k, _)| TUNABLES.iter().any(|(t, _)| t == k))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let (Some(Artifact::Image(image)), Some(Artifact::Location { location, identity })) =
            (inputs.get("image"), inputs.get("line"))
        else {
            return Err(Error::op(NAME, "inputs image and line are required"));
        };
        let frame = context.frames.require(&image.record.frame)?;
        if frame.topology != Topology::Bounded {
            return Ok(Output::invalid_args(format!(
                "frame {} wraps; fan needs a bounded frame (cut the panorama first)",
                frame.id
            )));
        }
        let Location::Line {
            column: Some(column),
            ..
        } = location
        else {
            return Ok(Output::invalid_args(
                "fan reads a line in column form; a path is for rasterize or tighten",
            ));
        };
        let constants_name = params
            .get("constants")
            .and_then(|c| c.as_str())
            .unwrap_or("fan-v1")
            .to_string();
        let table = set(&constants_name)
            .ok_or_else(|| Error::op(NAME, format!("no constant set {constants_name}")))?;
        let number = |key: &str| -> Result<f64> {
            match crate::float(params, key) {
                Some(v) => Ok(v),
                None => constant(table, key),
            }
        };
        let gate = params
            .get("gate")
            .and_then(|v| v.as_bool())
            .or_else(|| table.get("gate").and_then(|v| v.as_bool()))
            .unwrap_or(true);
        let settings = Settings {
            ref_width: number("ref_width")?,
            thin_px: number("thin_px")?,
            gap_px: constant(table, "gap_px")?,
            step_cols: constant(table, "step_cols")?,
            texture_cols: constant(table, "texture_cols")?,
            tex_low: number("tex_low")?,
            tex_high: number("tex_high")?,
            lum_floor: constant(table, "lum_floor")?,
            step_min: constant(table, "step_min")?,
            step_ref: constant(table, "step_ref")?,
            wide_max: number("wide_max")?,
            row_sigma: number("row_sigma")?,
            width_sigma: number("width_sigma")?,
            gate,
            object_edge: number("object_edge")?,
            object_ratio: constant(table, "object_ratio")?,
            object_margin_px: number("object_margin_px")?,
            object_sigma: constant(table, "object_sigma")?,
            object_taper: constant(table, "object_taper")?,
            object_run: constant(table, "object_run")?,
            object_wander_px: whole_number(table, "object_wander_px")?,
            object_straddle_whole: straddle_whole(table)?,
        };
        if settings.tex_high <= settings.tex_low {
            return Ok(Output::invalid_args(format!(
                "tex_high ({}) must exceed tex_low ({})",
                settings.tex_high, settings.tex_low
            )));
        }
        let rgb = image.pixels.to_rgb8();
        let profile = match fan_profile(&rgb, *column, &settings) {
            Ok(p) => p,
            Err(Clipped(window, columns)) => {
                return Ok(Output::not_applicable(format!(
                    "the {window} window clips to {columns} column(s) at column {column} of a {} px wide frame; fan needs at least 2",
                    frame.width
                )));
            }
        };
        let rows_widened = profile
            .half
            .iter()
            .filter(|h| **h > profile.thin + 0.5)
            .count();
        let half_max = profile.half.iter().cloned().fold(0.0, f64::max);
        let rows_capped = profile.cap.iter().filter(|c| c.is_finite()).count();
        let band = BandProfile::new(
            "fan.band",
            &frame.id,
            &frame.identity,
            identity.as_str(),
            profile.column_px,
            profile.thin,
            profile.half.clone(),
            profile.windows.clone(),
            &constants_name,
        )?;
        let mut outputs = BTreeMap::new();
        outputs.insert("band".to_string(), Artifact::BandProfile(Arc::new(band)));
        Ok(Output::ok(
            format!(
                "band at column {} of {}: thin {:.1} px, widest half {:.1} px, {rows_widened} of {} rows widened",
                profile.column_px, frame.id, profile.thin, half_max, frame.height
            ),
            outputs,
        )
        .with_measurement("thin_px", profile.thin, "px", Some(&frame.id))
        .with_measurement("half_max_px", half_max, "px", Some(&frame.id))
        .with_measurement("rows_widened", rows_widened as f64, "rows", Some(&frame.id))
        .with_measurement("rows_capped", rows_capped as f64, "rows", Some(&frame.id)))
    }
}

/// The resolved constants and tunables, in the units of the spec.
#[derive(Debug, Clone)]
pub struct Settings {
    pub ref_width: f64,
    pub thin_px: f64,
    pub gap_px: f64,
    pub step_cols: f64,
    pub texture_cols: f64,
    pub tex_low: f64,
    pub tex_high: f64,
    pub lum_floor: f64,
    pub step_min: f64,
    pub step_ref: f64,
    pub wide_max: f64,
    pub row_sigma: f64,
    pub width_sigma: f64,
    pub gate: bool,
    /// The object cap (0 is off): the horizontal luminance step that marks an object's edge.
    pub object_edge: f64,
    /// An edge must also exceed this multiple of the row's median gradient.
    pub object_ratio: f64,
    /// How far short of the edge the band stops, in pixels at `ref_width`.
    pub object_margin_px: f64,
    /// The cap's reach along the rows, as a fraction of the height (a running minimum).
    pub object_sigma: f64,
    /// The taper beyond the cap's reach, as a fraction of the height: the band eases from
    /// the cap back to its width over about three times this; 0 steps.
    pub object_taper: f64,
    /// The vertical run an edge needs to count, as a fraction of the height: a strong step
    /// chained through at least this many rows is an object's edge, a shorter one is
    /// texture (clouds, foliage). 0 takes every strong step.
    pub object_run: f64,
    /// How many columns the chain may wander per row.
    pub object_wander_px: u32,
    /// Whether an object the line runs through is left to the rulers (removed whole) rather
    /// than capped at `thin`.
    pub object_straddle_whole: bool,
}

impl Settings {
    /// The default set's settings, for tests and the lab.
    pub fn fan_v1() -> Settings {
        Settings::embedded("fan-v1")
    }

    /// `fan-thin-v1`: the fixed-width arm, `wide_max 0.0`.
    pub fn fan_thin_v1() -> Settings {
        Settings::embedded("fan-thin-v1")
    }

    pub fn fan_v2() -> Settings {
        Settings::embedded("fan-v2")
    }

    pub fn fan_v3() -> Settings {
        Settings::embedded("fan-v3")
    }

    fn embedded(name: &str) -> Settings {
        let table = set(name).expect("embedded");
        let c = |k: &str| constant(table, k).expect("the embedded set is complete");
        Settings {
            ref_width: c("ref_width"),
            thin_px: c("thin_px"),
            gap_px: c("gap_px"),
            step_cols: c("step_cols"),
            texture_cols: c("texture_cols"),
            tex_low: c("tex_low"),
            tex_high: c("tex_high"),
            lum_floor: c("lum_floor"),
            step_min: c("step_min"),
            step_ref: c("step_ref"),
            wide_max: c("wide_max"),
            row_sigma: c("row_sigma"),
            width_sigma: c("width_sigma"),
            gate: true,
            object_edge: c("object_edge"),
            object_ratio: c("object_ratio"),
            object_margin_px: c("object_margin_px"),
            object_sigma: c("object_sigma"),
            object_taper: c("object_taper"),
            object_run: c("object_run"),
            object_wander_px: whole_number(table, "object_wander_px")
                .expect("the embedded set is complete"),
            object_straddle_whole: straddle_whole(table).expect("the embedded set is complete"),
        }
    }
}

/// The profile `fan` computed, before it becomes an artifact.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub column_px: u32,
    pub thin: f64,
    pub half: Vec<f64>,
    /// `raw_half` before the width smoothing and clamp, for the tests of the gate.
    pub raw_half: Vec<f64>,
    /// The object cap per row: the half-width an object inside the opening limited the row
    /// to, or infinity where no object stood in the way (every row when the cap is off).
    pub cap: Vec<f64>,
    pub step: Vec<f64>,
    pub texture: Vec<f64>,
    pub windows: Windows,
}

/// A sampling window clipped to fewer than two columns: its name and the columns left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clipped(pub &'static str, pub u32);

/// The four sampling windows as column ranges `[lo, hi)`, from the centre column, `thin` and
/// the scaled geometry; `ramp` and the measures call this with the profile's recorded values
/// so they sample exactly these columns.
pub fn windows(
    column_px: u32,
    thin: f64,
    gap: u32,
    step_cols: u32,
    texture_cols: u32,
    width: u32,
) -> [(&'static str, i64, i64); 4] {
    let c = f64::from(column_px);
    // The first column at or past the reference edge on each side.
    let edge_l = (c - thin).ceil() as i64;
    let edge_r = (c + thin).ceil() as i64;
    let (gap, step, tex) = (
        i64::from(gap),
        i64::from(step_cols),
        i64::from(texture_cols),
    );
    let clip = |lo: i64, hi: i64| (lo.max(0), hi.min(i64::from(width)));
    let (a, b) = clip(edge_l - gap - step, edge_l - gap);
    let (c0, d) = clip(edge_r + gap, edge_r + gap + step);
    let (e, f) = clip(edge_l - tex, edge_l);
    let (g, h) = clip(edge_r, edge_r + tex);
    [
        ("left step", a, b),
        ("right step", c0, d),
        ("left texture", e, f),
        ("right texture", g, h),
    ]
}

/// The algorithm on an RGB image, pure.
pub fn fan_profile(
    image: &image::RgbImage,
    column: f64,
    s: &Settings,
) -> std::result::Result<Profile, Clipped> {
    let (w, h) = image.dimensions();
    let scale = f64::from(w) / s.ref_width;
    let scaled = |v: f64| ((v * scale).round() as u32).max(1);
    let column_px = ((column * f64::from(w)).round() as u32).min(w.saturating_sub(1));
    let thin = s.thin_px * scale;
    let (gap, step_cols, texture_cols) = (
        scaled(s.gap_px),
        scaled(s.step_cols),
        scaled(s.texture_cols),
    );
    let wins = windows(column_px, thin, gap, step_cols, texture_cols, w);
    // A window keeps at least two columns after clipping to the frame, or all of its columns
    // when the scaled geometry asked for fewer (a step window is one column on a small
    // canvas); otherwise the line is too near an edge for this width.
    for (i, (name, lo, hi)) in wins.into_iter().enumerate() {
        let asked = if i < 2 { step_cols } else { texture_cols };
        let left = (hi - lo).max(0) as u32;
        if left < asked.min(2) {
            return Err(Clipped(name, left));
        }
    }
    let [(_, sl0, sl1), (_, sr0, sr1), (_, tl0, tl1), (_, tr0, tr1)] = wins;
    let lum = luminance(image);
    let sobel = sobel_magnitude(&lum, w, h);
    let row_mean = |lo: i64, hi: i64, f: &dyn Fn(u32) -> f64| -> f64 {
        let n = (hi - lo) as f64;
        (lo..hi).map(|x| f(x as u32)).sum::<f64>() / n
    };
    let mut step = Vec::with_capacity(h as usize);
    let mut texture = Vec::with_capacity(h as usize);
    for y in 0..h {
        // Step: the mean over channels of |mean(left) − mean(right)| on the step windows.
        let channel = |ch: usize, lo: i64, hi: i64| {
            row_mean(lo, hi, &|x| f64::from(image.get_pixel(x, y)[ch]))
        };
        let st = (0..3)
            .map(|ch| (channel(ch, sl0, sl1) - channel(ch, sr0, sr1)).abs())
            .sum::<f64>()
            / 3.0;
        step.push(st);
        // Texture: the mean Sobel magnitude over both texture windows, over the mean luminance
        // of the same columns floored at lum_floor, in percent.
        let at = |plane: &[f32], x: u32| f64::from(plane[(y * w + x) as usize]);
        let cols = (tl1 - tl0 + tr1 - tr0) as f64;
        let sum_over = |plane: &[f32]| {
            (tl0..tl1)
                .chain(tr0..tr1)
                .map(|x| at(plane, x as u32))
                .sum::<f64>()
                / cols
        };
        let grad = sum_over(&sobel);
        let mean_l = sum_over(&lum).max(s.lum_floor);
        texture.push(grad / mean_l * 100.0);
    }
    let row_sigma = s.row_sigma * f64::from(h);
    let step_s = smooth_rows(&step, row_sigma);
    let texture_s = smooth_rows(&texture, row_sigma);
    let wide = thin.max(s.wide_max * f64::from(w));
    let raw_half: Vec<f64> = step_s
        .iter()
        .zip(&texture_s)
        .map(|(st, tx)| {
            let smooth = ((s.tex_high - tx) / (s.tex_high - s.tex_low)).clamp(0.0, 1.0);
            let open = if s.gate {
                ((st - s.step_min) / (s.step_ref - s.step_min)).clamp(0.0, 1.0)
            } else {
                1.0
            };
            thin + (wide - thin) * smooth * open
        })
        .collect();
    let smoothed: Vec<f64> = smooth_rows(&raw_half, s.width_sigma * f64::from(h))
        .into_iter()
        .map(|v| v.clamp(thin, wide))
        .collect();
    let cap = object_cap(&lum, w, h, column_px, gap, thin, scale, &smoothed, s);
    let half: Vec<f64> = smoothed
        .iter()
        .zip(&cap)
        .map(|(v, c)| v.min(*c).clamp(thin, wide))
        .collect();
    Ok(Profile {
        column_px,
        thin,
        half,
        raw_half,
        cap,
        step: step_s,
        texture: texture_s,
        windows: Windows {
            gap_px: gap,
            step_cols,
            texture_cols,
        },
    })
}

/// The object cap, per row: inside the row's opening `[c − half, c + half]` and beyond the
/// seam's own gap, a column whose horizontal luminance step is at least `object_edge` and
/// `object_ratio` times the row's median step, and whose vertical run of such steps is at
/// least `object_run · h` rows, is a solid object's edge. An edge on one side
/// caps the row at its distance less `object_margin_px`; edges on both sides, or one too
/// close for the band to stop short of it, mean the object is on the line: the row stays at
/// `thin`, or with `object_straddle = "whole"` the row and those within reach of it are
/// left to the rulers so the object is removed whole. The cap reaches `object_sigma · h`
/// rows up and down as a running minimum, so the band clears the object's ends too, and
/// eases back to the band's width over `object_taper · h` beyond (0 steps). Infinity where
/// nothing stands in the way; every row when `object_edge` is 0.
#[allow(clippy::too_many_arguments)]
fn object_cap(
    lum: &[f32],
    w: u32,
    h: u32,
    column_px: u32,
    gap: u32,
    thin: f64,
    scale: f64,
    half: &[f64],
    s: &Settings,
) -> Vec<f64> {
    let mut cap = vec![f64::INFINITY; h as usize];
    if s.object_edge <= 0.0 || w < 3 {
        return cap;
    }
    let margin = s.object_margin_px * scale;
    let exclude = f64::from(gap.max(2));
    let c = f64::from(column_px);
    let (hs, ws) = (h as usize, w as usize - 1);
    // The strong steps: between columns x and x + 1 (at x + 0.5), at least `object_edge`
    // and `object_ratio` times the row's median step.
    let mut strong = vec![false; hs * ws];
    let mut steps: Vec<f64> = Vec::with_capacity(ws);
    for y in 0..hs {
        let row = &lum[y * w as usize..(y + 1) * w as usize];
        steps.clear();
        steps.extend(row.windows(2).map(|p| f64::from((p[1] - p[0]).abs())));
        let mut sorted = steps.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let median = sorted[sorted.len() / 2];
        let threshold = s.object_edge.max(s.object_ratio * median);
        for (x, step) in steps.iter().enumerate() {
            strong[y * ws + x] = *step >= threshold;
        }
    }
    // The vertical run through each strong step: how many consecutive rows above and below
    // hold a strong step within `object_wander_px` columns, chained. A solid object's edge
    // runs for many rows; cloud and foliage texture does not.
    let min_run = (s.object_run * f64::from(h)).round() as u32;
    let runs = vertical_runs(&strong, ws, hs, s.object_wander_px);
    // Per row, the nearest qualifying edge on each side, and whether the object is on the
    // line: edges on both sides, or one so close that the band could not stop short of it.
    let mut on_line = vec![false; hs];
    for y in 0..hs {
        let reach = half[y];
        let lo = ((c - reach).floor().max(0.0)) as usize;
        let hi = ((c + reach).ceil().min(f64::from(w) - 2.0)) as usize;
        let mut left: Option<f64> = None;
        let mut right: Option<f64> = None;
        for x in lo..=hi {
            if !strong[y * ws + x] || runs[y * ws + x] < min_run {
                continue;
            }
            let d = (x as f64 + 0.5) - c;
            if d.abs() <= exclude {
                continue;
            }
            if d < 0.0 {
                left = Some(left.map_or(-d, |v: f64| v.min(-d)));
            } else {
                right = Some(right.map_or(d, |v: f64| v.min(d)));
            }
        }
        let nearest = match (left, right) {
            (Some(l), Some(r)) => Some(l.min(r)),
            (Some(d), None) | (None, Some(d)) => Some(d),
            (None, None) => None,
        };
        on_line[y] = matches!((left, right), (Some(_), Some(_)))
            || nearest.is_some_and(|d| d - margin <= thin);
        cap[y] = match nearest {
            Some(_) if on_line[y] => thin,
            Some(d) => d - margin,
            None => f64::INFINITY,
        };
    }
    // A running minimum over ±reach rows, so the band clears the object's ends.
    let reach = (s.object_sigma * f64::from(h)).round() as usize;
    let n = cap.len();
    let mut spread = cap.clone();
    if reach > 0 {
        for (y, out) in spread.iter_mut().enumerate() {
            let lo = y.saturating_sub(reach);
            let hi = (y + reach).min(n - 1);
            *out = cap[lo..=hi].iter().cloned().fold(f64::INFINITY, f64::min);
        }
    }
    // An object on the line is removed whole (`object_straddle = "whole"`): its rows, and
    // the rows within reach of them where only one of its ends was found, are left to the
    // rulers rather than striped at thin.
    if s.object_straddle_whole {
        for (y, out) in spread.iter_mut().enumerate() {
            let lo = y.saturating_sub(reach);
            let hi = (y + reach).min(n - 1);
            if on_line[lo..=hi].iter().any(|b| *b) {
                *out = f64::INFINITY;
            }
        }
    }
    // Then the taper: the cap, carried three sigmas further out as a running minimum and
    // then smoothed with that sigma against the uncapped width, eases the band in and out
    // over the rows beyond the reach instead of stepping, and is continuous at the cap's
    // boundary (the smoothed step has settled by the time it reaches a capped row). A
    // capped row keeps its cap, so the band never comes back over the object.
    let taper = s.object_taper * f64::from(h);
    if taper <= 0.0 || spread.iter().all(|c| c.is_infinite()) {
        return spread;
    }
    let filled: Vec<f64> = spread
        .iter()
        .zip(half)
        .map(|(c, h)| if c.is_finite() { *c } else { *h })
        .collect();
    let carry = (3.0 * taper).ceil() as usize;
    let carried: Vec<f64> = (0..n)
        .map(|y| {
            let lo = y.saturating_sub(carry);
            let hi = (y + carry).min(n - 1);
            filled[lo..=hi]
                .iter()
                .cloned()
                .fold(f64::INFINITY, f64::min)
        })
        .collect();
    let eased = smooth_rows(&carried, taper);
    spread
        .iter()
        .zip(eased)
        .zip(half)
        .map(|((c, e), h)| {
            if c.is_finite() {
                c.min(e)
            } else if e < *h - 1e-9 {
                e
            } else {
                f64::INFINITY
            }
        })
        .collect()
}
