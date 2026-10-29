//! `measure`: the five measures of a seam or a composite (SPEC.md "Measures"), each a
//! versioned definition in `measures-v1`. Per-row results are `Series` with row indices and
//! units; the summaries (mean, median, p95, max, rows skipped) go into the envelope. A measure
//! whose inputs are absent is not applicable for itself and the rest proceed; one whose every
//! row is skipped measured nothing. None of them is a verdict.

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{GenericImageView, RgbImage};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::frame::Frame;
use rascal_core::location::Location;
use rascal_core::mask::Mask;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port, Status,
};
use rascal_core::series::Series;

use crate::fan::windows;
use crate::primitives::{luminance, sobel_magnitude, srgb_to_lab};

/// The op.
pub struct Measure;

const NAME: &str = "measure";

/// `measures-v1`.
pub const MEASURES_V1: &str = include_str!("../constants/measures-v1.toml");

/// The measures, in the order they run and are listed.
pub const MEASURES: [&str; 5] = [
    "continuity",
    "tone_step",
    "transition",
    "texture_match",
    "drift",
];

/// The series each measure writes, by output port: name and unit.
pub const SERIES: [(&str, &str, &str); 6] = [
    ("continuity", "continuity", "ratio"),
    ("tone_step_dl", "tone_step.dL", "L"),
    ("tone_step_dchroma", "tone_step.dchroma", "chroma"),
    ("tone_step_dhue", "tone_step.dhue", "degrees"),
    ("transition", "transition", "ratio"),
    ("texture_match", "texture_match", "ratio"),
];

fn constants() -> toml::Table {
    MEASURES_V1
        .parse::<toml::Table>()
        .expect("measures-v1 is embedded TOML")
}

impl Op for Measure {
    fn manifest(&self) -> Manifest {
        let outputs = SERIES
            .iter()
            .map(|(port, name, unit)| Port {
                name: port.to_string(),
                artifact_type: ArtifactType::Series,
                required: false,
                purpose: format!("The per-row `{name}` series ({unit}); absent when the measure was not in the set, not applicable or measured nothing."),
                many: false,
            })
            .collect();
        Manifest::new(
            NAME,
            "1.0.0",
            "The seam and composite measures: continuity, tone step, transition, texture match and drift, per row and summarised.",
            "After a heal or a paste, on the before and after images in the mask's frame, to describe what changed at the line and what did not; none of them is a verdict.",
            "On images of different frames or sizes (window or put first).",
            "A Series per measure with row indices and units, and in the envelope each measure's mean, median, p95, max and rows_skipped; drift's per-zone channel means. A measure whose inputs are absent is not applicable for itself; one whose every row is skipped measured nothing and writes no series.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.2,
            },
            vec![
                Port {
                    name: "after".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The image to measure (the composite).".to_string(),
                    many: false,
                },
                Port {
                    name: "before".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: false,
                    purpose: "The image before (for drift).".to_string(),
                    many: false,
                },
                Port {
                    name: "mask".to_string(),
                    artifact_type: ArtifactType::Mask,
                    required: true,
                    purpose: "The mask, with its profile for the band measures.".to_string(),
                    many: false,
                },
                Port {
                    name: "line".to_string(),
                    artifact_type: ArtifactType::Location,
                    required: false,
                    purpose: "The line (column form) for continuity; the profile's column when absent.".to_string(),
                    many: false,
                },
            ],
            outputs,
            vec![Param::optional(
                "set",
                "The measures to run, comma-separated: continuity, tone_step, transition, texture_match, drift.",
                ParamKind::String,
                serde_json::json!(MEASURES.join(",")),
            )],
        )
        .with_constants(&["measures-v1"])
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let (Some(Artifact::Image(after)), Some(Artifact::Mask(mask))) =
            (inputs.get("after"), inputs.get("mask"))
        else {
            return Err(Error::op(NAME, "inputs after and mask are required"));
        };
        let before = match inputs.get("before") {
            Some(Artifact::Image(image)) => Some(image.clone()),
            _ => None,
        };
        let line = match inputs.get("line") {
            Some(Artifact::Location { location, .. }) => Some(location.clone()),
            _ => None,
        };
        let frame = context.frames.require(&mask.record.frame)?;
        let set_text = params.get("set").and_then(|v| v.as_str()).unwrap_or("");
        let mut set = Vec::new();
        for name in set_text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if !MEASURES.contains(&name) {
                return Ok(Output::invalid_args(format!(
                    "unknown measure {name}; the measures are {}",
                    MEASURES.join(", ")
                )));
            }
            if !set.contains(&name) {
                set.push(name);
            }
        }
        if set.is_empty() {
            return Ok(Output::invalid_args("set names no measure"));
        }
        if after.pixels.dimensions() != mask.dimensions()
            || before
                .as_ref()
                .is_some_and(|b| b.pixels.dimensions() != mask.dimensions())
        {
            return Ok(Output::invalid_args(
                "before, after and the mask must be one size (they are bound to one frame)",
            ));
        }
        let table = constants();
        let chroma_floor = table
            .get("chroma_floor")
            .and_then(|v| v.as_float())
            .unwrap_or(1.0);
        let percentile = table
            .get("percentile")
            .and_then(|v| v.as_integer())
            .unwrap_or(95) as f64;
        let after_rgb = after.pixels.to_rgb8();
        let column = line_column(line.as_ref(), mask, frame.width);
        let mut report = Report::default();
        let mut outputs = BTreeMap::new();
        let mut measurements: Vec<(String, f64, String)> = Vec::new();
        let mut statuses = Vec::new();
        let mut sobel_cache: Option<Vec<f32>> = None;
        let emit = |report: &mut Report,
                    outputs: &mut BTreeMap<String, Artifact>,
                    measurements: &mut Vec<(String, f64, String)>,
                    statuses: &mut Vec<String>,
                    measure: &str,
                    series: Vec<(usize, PerRow)>| {
            let mut any = false;
            for (idx, per_row) in series {
                let (port, name, unit) = SERIES[idx];
                measurements.push((
                    format!("{name}.rows_skipped"),
                    per_row.skipped as f64,
                    "rows".to_string(),
                ));
                if per_row.values.is_empty() {
                    continue;
                }
                any = true;
                let summary = summarise(&per_row.values, percentile);
                for (k, v) in [
                    ("mean", summary.mean),
                    ("median", summary.median),
                    ("p95", summary.p95),
                    ("max", summary.max),
                ] {
                    measurements.push((format!("{name}.{k}"), v, unit.to_string()));
                }
                match Series::with_rows(
                    &format!("measure.{port}"),
                    name,
                    unit,
                    &frame.id,
                    &frame.identity,
                    1,
                    per_row.rows.clone(),
                    per_row.values.clone(),
                ) {
                    Ok(s) => {
                        outputs.insert(port.to_string(), Artifact::Series(Arc::new(s)));
                    }
                    Err(e) => report.errors.push(e.to_string()),
                }
            }
            statuses.push(format!(
                "{measure}: {}",
                if any { "ok" } else { "measured_nothing" }
            ));
            if any {
                report.measured += 1;
            } else {
                report.nothing += 1;
            }
        };
        for measure in &set {
            match *measure {
                "continuity" => match column {
                    Some(c) => {
                        let rows = continuity(&after_rgb, c);
                        emit(
                            &mut report,
                            &mut outputs,
                            &mut measurements,
                            &mut statuses,
                            measure,
                            vec![(0, rows)],
                        );
                    }
                    None => {
                        statuses.push("continuity: not_applicable (no line and no profile)".into());
                        report.not_applicable += 1;
                    }
                },
                "tone_step" | "transition" | "texture_match" => match &mask.profile {
                    Some(profile) => {
                        let series = match *measure {
                            "tone_step" => {
                                let [dl, dc, dh] =
                                    tone_step(&after_rgb, &profile.band, chroma_floor);
                                vec![(1, dl), (2, dc), (3, dh)]
                            }
                            "transition" => vec![(
                                4,
                                transition(
                                    &after_rgb,
                                    &profile.band,
                                    sobel_plane(&after_rgb, &mut sobel_cache),
                                ),
                            )],
                            _ => vec![(
                                5,
                                texture_match(
                                    &after_rgb,
                                    mask,
                                    &profile.band,
                                    sobel_plane(&after_rgb, &mut sobel_cache),
                                ),
                            )],
                        };
                        emit(
                            &mut report,
                            &mut outputs,
                            &mut measurements,
                            &mut statuses,
                            measure,
                            series,
                        );
                    }
                    None => {
                        statuses.push(format!(
                            "{measure}: not_applicable (the mask has no profile)"
                        ));
                        report.not_applicable += 1;
                    }
                },
                "drift" => match &before {
                    Some(before) => {
                        let d = drift(&before.pixels.to_rgb8(), &after_rgb, mask);
                        for (zone, values) in
                            [("core", d.core), ("blend", d.blend), ("outside", d.outside)]
                        {
                            for (ch, v) in ["r", "g", "b"].iter().zip(values) {
                                measurements.push((
                                    format!("drift.{zone}.{ch}"),
                                    v,
                                    "levels".into(),
                                ));
                            }
                        }
                        statuses.push("drift: ok".into());
                        report.measured += 1;
                    }
                    None => {
                        statuses.push("drift: not_applicable (no before image)".into());
                        report.not_applicable += 1;
                    }
                },
                _ => unreachable!("validated"),
            }
        }
        if let Some(e) = report.errors.first() {
            return Err(Error::op(NAME, e.clone()));
        }
        let summary = format!("measures in {}: {}", frame.id, statuses.join("; "));
        let mut output = if report.measured > 0 {
            Output::ok(summary, outputs)
        } else if report.nothing > 0 {
            Output {
                envelope: rascal_core::op::Envelope::measured_nothing(summary),
                outputs: BTreeMap::new(),
                frames: Vec::new(),
            }
        } else {
            Output::not_applicable(summary)
        };
        if output.envelope.status != Status::Ok {
            // Nothing is published for a non-ok status; the counts still describe the attempt.
            output.outputs.clear();
        }
        for (name, value, unit) in measurements {
            output = output.with_measurement(&name, value, &unit, Some(&frame.id));
        }
        Ok(output)
    }
}

#[derive(Default)]
struct Report {
    measured: usize,
    nothing: usize,
    not_applicable: usize,
    errors: Vec<String>,
}

/// The line's column in pixels: the line location's column form, else the profile's.
fn line_column(line: Option<&Location>, mask: &Mask, width: u32) -> Option<u32> {
    match line {
        Some(Location::Line {
            column: Some(c), ..
        }) => Some(((c * f64::from(width)).round() as u32).min(width.saturating_sub(1))),
        _ => mask.profile.as_ref().map(|p| p.band.column_px),
    }
}

/// A per-row result: the rows kept with their values, and how many were skipped.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PerRow {
    pub rows: Vec<u32>,
    pub values: Vec<f64>,
    pub skipped: u32,
}

impl PerRow {
    fn push(&mut self, row: u32, value: Option<f64>) {
        match value {
            Some(v) if v.is_finite() => {
                self.rows.push(row);
                self.values.push(v);
            }
            _ => self.skipped += 1,
        }
    }
}

/// mean, median, p95 (nearest rank) and max of a non-empty slice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    pub mean: f64,
    pub median: f64,
    pub p95: f64,
    pub max: f64,
}

pub fn summarise(values: &[f64], percentile: f64) -> Summary {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let rank = |p: f64| sorted[(((p / 100.0) * n as f64).ceil() as usize).clamp(1, n) - 1];
    Summary {
        mean: sorted.iter().sum::<f64>() / n as f64,
        median: if n % 2 == 1 {
            sorted[n / 2]
        } else {
            (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
        },
        p95: rank(percentile),
        max: sorted[n - 1],
    }
}

/// Continuity: per row, the mean over channels of the absolute difference between the two
/// columns adjacent to the line (`c − 1` and `c`), over the row's mean absolute difference
/// between neighbouring columns; a zero denominator skips the row.
pub fn continuity(after: &RgbImage, column_px: u32) -> PerRow {
    let (w, h) = after.dimensions();
    let mut out = PerRow::default();
    if column_px == 0 || column_px >= w || w < 2 {
        out.skipped = h;
        return out;
    }
    let diff = |y: u32, x: u32| -> f64 {
        let a = after.get_pixel(x, y);
        let b = after.get_pixel(x + 1, y);
        (0..3)
            .map(|ch| f64::from(a[ch].abs_diff(b[ch])))
            .sum::<f64>()
            / 3.0
    };
    for y in 0..h {
        let at_line = diff(y, column_px - 1);
        let mean = (0..w - 1).map(|x| diff(y, x)).sum::<f64>() / f64::from(w - 1);
        out.push(y, (mean > 0.0).then(|| at_line / mean));
    }
    out
}

/// The signed circular difference `b − a` in degrees, in `(−180, 180]`.
pub fn hue_difference(a: f64, b: f64) -> f64 {
    let d = (b - a).rem_euclid(360.0);
    if d > 180.0 { d - 360.0 } else { d }
}

/// Tone step: per row, the mean colour of the profile's step windows on each side in CIE Lab,
/// as ΔL, Δchroma and Δhue (right minus left); the hue is omitted where either side's chroma
/// is below `chroma_floor`.
pub fn tone_step(
    after: &RgbImage,
    band: &rascal_core::profile::BandProfile,
    chroma_floor: f64,
) -> [PerRow; 3] {
    let (w, h) = after.dimensions();
    let [(_, l0, l1), (_, r0, r1), _, _] = windows(
        band.column_px,
        band.thin_px,
        band.windows.gap_px,
        band.windows.step_cols,
        band.windows.texture_cols,
        w,
    );
    let mut out: [PerRow; 3] = Default::default();
    for y in 0..h {
        let lab_mean = |lo: i64, hi: i64| -> Option<[f64; 3]> {
            if hi <= lo {
                return None;
            }
            let mut sum = [0.0; 3];
            for x in lo..hi {
                let p = after.get_pixel(x as u32, y);
                let mean_rgb = [p[0], p[1], p[2]];
                let lab = srgb_to_lab(mean_rgb);
                for k in 0..3 {
                    sum[k] += lab[k];
                }
            }
            let n = (hi - lo) as f64;
            Some([sum[0] / n, sum[1] / n, sum[2] / n])
        };
        match (lab_mean(l0, l1), lab_mean(r0, r1)) {
            (Some(l), Some(r)) => {
                let chroma = |lab: [f64; 3]| (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
                let hue = |lab: [f64; 3]| lab[2].atan2(lab[1]).to_degrees();
                out[0].push(y, Some(r[0] - l[0]));
                out[1].push(y, Some(chroma(r) - chroma(l)));
                let hue_ok = chroma(l) >= chroma_floor && chroma(r) >= chroma_floor;
                out[2].push(y, hue_ok.then(|| hue_difference(hue(l), hue(r))));
            }
            _ => {
                for s in &mut out {
                    s.push(y, None);
                }
            }
        }
    }
    out
}

/// The Sobel magnitude of L, computed once per op and shared by the measures that need it.
fn sobel_plane<'a>(after: &RgbImage, cache: &'a mut Option<Vec<f32>>) -> &'a [f32] {
    let (w, h) = after.dimensions();
    cache.get_or_insert_with(|| sobel_magnitude(&luminance(after), w, h))
}

/// Columns `[lo, hi)` clipped to the frame whose centres `x + 0.5` lie in `[c + from, c + to)`:
/// the same pixel-centre rule the band's core uses, so "inside the band" here is the core's
/// own columns.
fn span(c: f64, from: f64, to: f64, width: u32) -> (i64, i64) {
    let lo = (c + from - 0.5).ceil().max(0.0) as i64;
    let hi = ((c + to - 0.5).ceil().max(0.0) as i64).min(i64::from(width));
    (lo, hi.max(lo))
}

/// Transition: per row, the mean Sobel magnitude of L over `[c − half, c + half]` divided by
/// the row's mean outside `[c − 2 · half, c + 2 · half]`.
pub fn transition(
    after: &RgbImage,
    band: &rascal_core::profile::BandProfile,
    sobel: &[f32],
) -> PerRow {
    let (w, h) = after.dimensions();
    let c = f64::from(band.column_px);
    let mut out = PerRow::default();
    for y in 0..h {
        let half = band.half[y as usize];
        let at = |x: i64| f64::from(sobel[(y * w) as usize + x as usize]);
        let (i0, i1) = span(c, -half, half, w);
        let (o0, o1) = span(c, -2.0 * half, 2.0 * half, w);
        let inside: Vec<f64> = (i0..i1).map(at).collect();
        let outside: Vec<f64> = (0..o0).chain(o1..i64::from(w)).map(at).collect();
        let value = if inside.is_empty() || outside.is_empty() {
            None
        } else {
            let num = inside.iter().sum::<f64>() / inside.len() as f64;
            let den = outside.iter().sum::<f64>() / outside.len() as f64;
            (den > 0.0).then(|| num / den)
        };
        out.push(y, value);
    }
    out
}

/// Texture match: per row, the mean Sobel magnitude of L inside the core divided by that over
/// `[half, 2 · half]` beyond the core on each side.
pub fn texture_match(
    after: &RgbImage,
    mask: &Mask,
    band: &rascal_core::profile::BandProfile,
    sobel: &[f32],
) -> PerRow {
    let (w, h) = after.dimensions();
    let c = f64::from(band.column_px);
    let mut out = PerRow::default();
    for y in 0..h {
        let half = band.half[y as usize];
        let at = |x: i64| f64::from(sobel[(y * w) as usize + x as usize]);
        let core: Vec<f64> = (0..w)
            .filter(|x| mask.core.get_pixel(*x, y)[0] != 0)
            .map(|x| at(i64::from(x)))
            .collect();
        let (l0, l1) = span(c, -2.0 * half, -half, w);
        let (r0, r1) = span(c, half, 2.0 * half, w);
        let beyond: Vec<f64> = (l0..l1).chain(r0..r1).map(at).collect();
        let value = if core.is_empty() || beyond.is_empty() {
            None
        } else {
            let num = core.iter().sum::<f64>() / core.len() as f64;
            let den = beyond.iter().sum::<f64>() / beyond.len() as f64;
            (den > 0.0).then(|| num / den)
        };
        out.push(y, value);
    }
    out
}

/// Drift: the mean absolute 8-bit difference per channel inside the core, inside the blend
/// (nonzero blend outside the core) and where `w = 0`; the last must be zero after a correct
/// paste. A zone with no pixel reports zero.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drift {
    pub core: [f64; 3],
    pub blend: [f64; 3],
    pub outside: [f64; 3],
}

pub fn drift(before: &RgbImage, after: &RgbImage, mask: &Mask) -> Drift {
    let (w, h) = mask.dimensions();
    let mut sums = [[0.0f64; 3]; 3];
    let mut counts = [0u64; 3];
    for y in 0..h {
        for x in 0..w {
            let zone = if mask.weight(x, y) <= 0.0 {
                2
            } else if mask.core.get_pixel(x, y)[0] != 0 {
                0
            } else {
                1
            };
            let a = before.get_pixel(x, y);
            let b = after.get_pixel(x, y);
            for ch in 0..3 {
                sums[zone][ch] += f64::from(a[ch].abs_diff(b[ch]));
            }
            counts[zone] += 1;
        }
    }
    let mean = |zone: usize| -> [f64; 3] {
        let n = counts[zone].max(1) as f64;
        [sums[zone][0] / n, sums[zone][1] / n, sums[zone][2] / n]
    };
    Drift {
        core: mean(0),
        blend: mean(1),
        outside: mean(2),
    }
}

/// The frame helper the CLI uses: a raw frame of the images' size.
pub fn frame_for(width: u32, height: u32) -> Frame {
    Frame::raw(width, height, rascal_core::frame::Projection::Flat)
}
