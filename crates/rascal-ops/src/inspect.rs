//! `inspect`: a mask as a person wants to see it: the overlay (the image tinted by `w` with
//! the core's edge in white) and a per-row series of the core's width, the chart's data. With
//! `--json` the binary prints the record and zone statistics; with `--replay` it rebuilds the
//! record and diffs the planes.

use std::collections::BTreeMap;
use std::sync::Arc;

use image::DynamicImage;
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::image::{Image, Role};
use rascal_core::mask::Mask;
use rascal_core::op::{Context, Cost, CostTier, Inputs, Manifest, Op, Output, Params, Port};
use rascal_core::rendering;
use rascal_core::series::Series;

/// The op.
pub struct Inspect;

const NAME: &str = "inspect";

impl Op for Inspect {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "A mask for the eye: the overlay on its image and the per-row width of its core.",
            "When a person wants to see what a mask covers and how its width varies row by row; the run page uses it too.",
            "When a filler needs a rendering (use render), or when an op needs the mask itself.",
            "An inspection Image (the overlay) and a Series of the core's width per row in pixels. Nothing a model reads.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![
                Port {
                    name: "mask".to_string(),
                    artifact_type: ArtifactType::Mask,
                    required: true,
                    purpose: "The mask to inspect.".to_string(),
                    many: false,
                },
                Port {
                    name: "image".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The image to draw the overlay on; in the mask's frame.".to_string(),
                    many: false,
                },
            ],
            vec![
                Port {
                    name: "overlay".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The overlay, an inspection image.".to_string(),
                    many: false,
                },
                Port {
                    name: "widths".to_string(),
                    artifact_type: ArtifactType::Series,
                    required: true,
                    purpose: "The core's width per row, in pixels.".to_string(),
                    many: false,
                },
            ],
            vec![],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, _params: &Params) -> Result<Output> {
        let (Some(Artifact::Mask(mask)), Some(Artifact::Image(image))) =
            (inputs.get("mask"), inputs.get("image"))
        else {
            return Err(Error::op(NAME, "inputs mask and image are required"));
        };
        let frame = context.frames.require(&mask.record.frame)?;
        let overlay = rendering::overlay(mask, &image.pixels, [255, 0, 255])?;
        let overlay = Image::new(
            "inspect.overlay",
            DynamicImage::ImageRgb8(overlay),
            frame,
            Role::Inspection,
            None,
        );
        let widths = core_widths(mask);
        let series = Series::new(
            "inspect.widths",
            "core_width",
            "px",
            &frame.id,
            &frame.identity,
            1,
            widths,
        )?;
        let mut outputs = BTreeMap::new();
        outputs.insert("overlay".to_string(), Artifact::Image(Arc::new(overlay)));
        outputs.insert("widths".to_string(), Artifact::Series(Arc::new(series)));
        let stats = mask.statistics();
        Ok(Output::ok(
            format!(
                "overlay and {} rows of core width in {}",
                frame.height, frame.id
            ),
            outputs,
        )
        .with_measurement("core_px", stats.core as f64, "px", Some(&frame.id))
        .with_measurement("painted_px", stats.painted as f64, "px", Some(&frame.id)))
    }
}

/// The number of core pixels on each row.
pub fn core_widths(mask: &Mask) -> Vec<f64> {
    let (w, h) = mask.dimensions();
    (0..h)
        .map(|y| {
            (0..w)
                .filter(|x| mask.core.get_pixel(*x, y)[0] != 0)
                .count() as f64
        })
        .collect()
}

/// A small SVG line chart of a per-row series: rows down the page, values across.
pub fn chart_svg(title: &str, unit: &str, values: &[f64]) -> String {
    let rows = values.len().max(1);
    let max = values.iter().cloned().fold(0.0_f64, f64::max).max(1.0);
    let (width, height) = (320.0_f64, (rows as f64).clamp(40.0, 1200.0));
    let scale_y = height / rows as f64;
    let points: Vec<String> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            format!(
                "{:.1},{:.1}",
                40.0 + v / max * (width - 50.0),
                (i as f64 + 0.5) * scale_y
            )
        })
        .collect();
    format!(
        concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" role=\"img\" aria-label=\"{title}\">",
            "<title>{title}</title>",
            "<rect width=\"{w}\" height=\"{h}\" fill=\"white\"/>",
            "<line x1=\"40\" y1=\"0\" x2=\"40\" y2=\"{h}\" stroke=\"#888\"/>",
            "<text x=\"42\" y=\"12\" font-size=\"10\" fill=\"#444\">0</text>",
            "<text x=\"{right}\" y=\"12\" font-size=\"10\" fill=\"#444\" text-anchor=\"end\">{max:.0} {unit}</text>",
            "<polyline fill=\"none\" stroke=\"#c0c\" stroke-width=\"1.5\" points=\"{points}\"/>",
            "</svg>"
        ),
        w = width,
        h = height,
        title = rascal_core::panel::escape(title),
        right = width - 4.0,
        max = max,
        unit = rascal_core::panel::escape(unit),
        points = points.join(" ")
    )
}
