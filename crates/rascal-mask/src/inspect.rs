//! `rascal inspect <mask> [--json] [--out <dir>] [--replay]`: the record, zone statistics and
//! identity as JSON; the overlay JPEG and the per-row chart into a directory; and the mask
//! rebuilt from its record and diffed against the stored planes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::artifact::Artifact;
use rascal_core::error::Result;
use rascal_core::mask::Mask;
use rascal_core::op::Params;
use rascal_core::outdir::{self, OutKind};
use rascal_core::registry::OpRegistry;
use rascal_core::replay;
use rascal_core::workspace::Workspace;

/// What `--json` prints.
pub fn inspect_json(mask_dir: &Path) -> Result<serde_json::Value> {
    let mask = Mask::read(mask_dir)?;
    let statistics = mask.statistics();
    Ok(serde_json::json!({
        "inspect_version": 1,
        "path": mask_dir.display().to_string(),
        "identity": mask.record.identity,
        "record": mask.record,
        "zones": statistics,
        "profile": mask.profile.as_ref().map(|p| serde_json::json!({
            "line": p.line,
            "band": p.band.identity,
            "sigma_px": p.sigma_px.identity,
            "rows": p.band.half.len(),
            "column_px": p.band.column_px,
            "thin_px": p.band.thin_px,
        })),
        "prescriptions": mask.prescriptions.len(),
    }))
}

/// The workspace a stored mask lives in: `<workspace>/artifacts/<id>.mask`.
pub fn workspace_of(mask_dir: &Path) -> Result<Workspace> {
    let root = mask_dir
        .parent()
        .filter(|p| p.file_name().is_some_and(|n| n == "artifacts"))
        .and_then(|p| p.parent())
        .ok_or_else(|| {
            Error::usage(format!(
                "{}: not inside a workspace's artifacts/ directory",
                mask_dir.display()
            ))
        })?;
    Workspace::open(root)
}

/// Writes `overlay.jpg`, `chart.svg` and `inspect.json` into `out` (a run directory: new or
/// empty). The overlay is drawn on the first canonical image in the mask's frame.
pub fn inspect_pages(registry: &OpRegistry, mask_dir: &Path, out: &Path) -> Result<Vec<PathBuf>> {
    let ws = workspace_of(mask_dir)?;
    let mask = Mask::read(mask_dir)?;
    let out = outdir::prepare(out, OutKind::RunDir)?;
    let snapshot = ws.snapshot()?;
    let image_entry = snapshot
        .artifacts
        .iter()
        .find(|a| {
            a.artifact_type == rascal_core::artifact::ArtifactType::Image
                && a.frame == mask.record.frame
        })
        .ok_or_else(|| {
            Error::usage(format!(
                "no image in frame {} to draw the overlay on",
                mask.record.frame
            ))
        })?;
    let mut inputs = BTreeMap::new();
    inputs.insert("mask".to_string(), Artifact::Mask(Arc::new(mask.clone())));
    inputs.insert("image".to_string(), ws.load(&image_entry.id)?);
    let invocation = ws.invoke("inspect", "inspect", registry, &inputs, &Params::new())?;
    let mut written = Vec::new();
    if let Some(Artifact::Image(overlay)) = invocation.output.outputs.get("overlay") {
        let path = out.join("overlay.jpg");
        image::DynamicImage::ImageRgb8(overlay.pixels.to_rgb8())
            .save_with_format(&path, image::ImageFormat::Jpeg)
            .map_err(|source| Error::Image {
                path: path.clone(),
                source,
            })?;
        written.push(path);
    }
    if let Some(Artifact::Series(widths)) = invocation.output.outputs.get("widths") {
        let path = out.join("chart.svg");
        std::fs::write(
            &path,
            rascal_ops::inspect::chart_svg("core width per row", "px", &widths.values),
        )
        .map_err(|e| Error::io(&path, e))?;
        written.push(path);
    }
    let path = out.join("inspect.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&inspect_json(mask_dir)?).unwrap_or_default() + "\n",
    )
    .map_err(|e| Error::io(&path, e))?;
    written.push(path);
    Ok(written)
}

/// Rebuilds the mask from its record and the stored inputs and reports the plane diff.
pub fn inspect_replay(registry: &OpRegistry, mask_dir: &Path) -> Result<replay::ReplayReport> {
    let ws = workspace_of(mask_dir)?;
    let stored = replay::read_untrusted(mask_dir)?;
    replay::replay_mask(&ws, registry, &stored)
}
