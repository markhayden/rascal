//! A `Series`: a per-row measurement with row indices, units and the frame it was measured in.
//! Written as `<identity>.series.json`. Used for the feather's per-row sigma, the tone ramp's
//! per-row step, and every per-row measure.

use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::error::{Error, Result};
use crate::identity::Identity;

/// A per-row series. `values` holds `rows.len() × channels` finite numbers, row-major.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Series {
    /// The record's version.
    pub series_version: u32,
    /// The local id.
    pub id: String,
    /// What was measured, such as `sigma_px` or `tone_ramp.step`.
    pub name: String,
    /// The unit: `px`, `levels`, `ratio`, `degrees`.
    pub unit: String,
    /// The frame id the rows belong to.
    pub frame: String,
    /// The frame's identity.
    pub frame_identity: String,
    /// Values per row: 1, or 3 for a colour quantity.
    pub channels: u32,
    /// The row index of each entry, ascending.
    pub rows: Vec<u32>,
    /// `rows.len() × channels` values, row-major.
    pub values: Vec<f64>,
    /// `sha256:` over the name, unit, frame identity, channels, rows and values.
    pub identity: String,
}

impl Series {
    /// A series over consecutive rows `0..values.len() / channels`.
    pub fn new(
        id: &str,
        name: &str,
        unit: &str,
        frame_id: &str,
        frame_identity: &str,
        channels: u32,
        values: Vec<f64>,
    ) -> Result<Series> {
        let rows: Vec<u32> = (0..(values.len() as u32) / channels.max(1)).collect();
        Series::with_rows(
            id,
            name,
            unit,
            frame_id,
            frame_identity,
            channels,
            rows,
            values,
        )
    }

    /// A series over the given rows.
    #[allow(clippy::too_many_arguments)]
    pub fn with_rows(
        id: &str,
        name: &str,
        unit: &str,
        frame_id: &str,
        frame_identity: &str,
        channels: u32,
        rows: Vec<u32>,
        values: Vec<f64>,
    ) -> Result<Series> {
        let mut series = Series {
            series_version: 1,
            id: id.to_string(),
            name: name.to_string(),
            unit: unit.to_string(),
            frame: frame_id.to_string(),
            frame_identity: frame_identity.to_string(),
            channels,
            rows,
            values,
            identity: String::new(),
        };
        series.check()?;
        series.identity = series.compute_identity().as_str().to_string();
        Ok(series)
    }

    /// The identity over the content, references by identity.
    pub fn compute_identity(&self) -> Identity {
        canonical::identity_of(
            "series",
            &serde_json::json!({
                "name": self.name, "unit": self.unit, "frame": self.frame_identity,
                "channels": self.channels, "rows": self.rows, "values": self.values,
            }),
        )
    }

    /// The identity as a typed value.
    pub fn identity(&self) -> Identity {
        Identity::parse_or_hash(&self.identity)
    }

    /// The invariants: a positive channel count, matching lengths, ascending rows, finite values.
    pub fn check(&self) -> Result<()> {
        let fail = |m: String| Error::op("series", format!("{}: {m}", self.id));
        if self.channels == 0 {
            return Err(fail("channels must be at least 1".into()));
        }
        if self.values.len() != self.rows.len() * self.channels as usize {
            return Err(fail(format!(
                "{} values for {} rows × {} channels",
                self.values.len(),
                self.rows.len(),
                self.channels
            )));
        }
        if self.rows.windows(2).any(|w| w[0] >= w[1]) {
            return Err(fail("rows must be ascending".into()));
        }
        if let Some(v) = self.values.iter().find(|v| !v.is_finite()) {
            return Err(fail(format!("value {v} is not finite")));
        }
        Ok(())
    }

    /// The values of one row, by row index.
    pub fn row(&self, row: u32) -> Option<&[f64]> {
        let index = self.rows.binary_search(&row).ok()?;
        let c = self.channels as usize;
        self.values.get(index * c..(index + 1) * c)
    }

    /// Writes the record.
    pub fn write(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self).map_err(|source| Error::Json {
            path: path.to_path_buf(),
            source,
        })?;
        std::fs::write(path, text + "\n").map_err(|e| Error::io(path, e))
    }

    /// Reads and checks a record, including its identity.
    pub fn read(path: &Path) -> Result<Series> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        let series: Series = serde_json::from_str(&text).map_err(|source| Error::Json {
            path: path.to_path_buf(),
            source,
        })?;
        series.check()?;
        if series.compute_identity().as_str() != series.identity {
            return Err(Error::op(
                "series",
                format!(
                    "{}: the stored identity does not match the content",
                    path.display()
                ),
            ));
        }
        Ok(series)
    }
}
