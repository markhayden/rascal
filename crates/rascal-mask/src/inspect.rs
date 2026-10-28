//! `rascal inspect <mask> --json`: the record, the zone statistics and the identity, as JSON on
//! stdout. The overlay JPEG, the per-row chart and `--replay` arrive with task 1.9.

use std::path::Path;

use rascal_core::Error;
use rascal_core::error::Result;
use rascal_core::mask::Mask;

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

/// `inspect` without `--json` is the overlay and the chart, which task 1.9 brings.
pub fn inspect_pages(_mask_dir: &Path) -> Result<()> {
    Err(Error::usage(
        "inspect without --json (the overlay and the chart) arrives in task 1.9; give --json",
    ))
}
