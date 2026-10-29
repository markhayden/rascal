//! What every core-replacing refinement shares (SPEC.md "The mask record", refinement
//! postconditions): the new core minus protect, the blend cleared, the profile and the
//! prescriptions removed, protect preserved, and the report of what was cleared.

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{GrayImage, Luma};
use rascal_core::artifact::Artifact;
use rascal_core::error::Result;
use rascal_core::mask::{Mask, Refinement};
use rascal_core::op::Output;

/// `mask` with `core` as its new core (cleared under protect), as the op's output under the
/// output name `mask`, with the refinement report and the core counts as measurements.
pub(crate) fn replaced_core(
    mask: &Mask,
    mut core: GrayImage,
    id: &str,
    summary: String,
) -> Result<Output> {
    for (x, y, p) in mask.protect.enumerate_pixels() {
        if p[0] != 0 {
            core.put_pixel(x, y, Luma([0]));
        }
    }
    let before = count(&mask.core);
    let mut out = mask.clone();
    out.record.id = id.to_string();
    let report: Refinement = out.replace_core(core)?;
    let after = count(&out.core);
    let frame = mask.record.frame.clone();
    let mut outputs = BTreeMap::new();
    outputs.insert("mask".to_string(), Artifact::Mask(Arc::new(out)));
    Ok(Output::ok(summary, outputs)
        .with_refinement(report)
        .with_measurement("core_px_before", before as f64, "px", Some(&frame))
        .with_measurement("core_px_after", after as f64, "px", Some(&frame)))
}

/// Set pixels of a binary plane.
pub(crate) fn count(plane: &GrayImage) -> usize {
    plane.pixels().filter(|p| p[0] != 0).count()
}

/// The union of `plane` with itself shifted down by 1 … `rows` pixels: a shadow under a blob.
pub(crate) fn shifted_down_union(plane: &GrayImage, rows: u32) -> GrayImage {
    let (w, h) = plane.dimensions();
    GrayImage::from_fn(w, h, |x, y| {
        let hit = (0..=rows.min(y)).any(|dy| plane.get_pixel(x, y - dy)[0] != 0);
        Luma([if hit { 255 } else { 0 }])
    })
}
