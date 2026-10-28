//! A `BandProfile`: the per-row half-width of a band along a line, with the line, the centre
//! column and the window geometry `fan` resolved, so `ramp` and the measures reuse exactly
//! the same columns. Written as `<identity>.profile.json`.

use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::error::{Error, Result};
use crate::identity::Identity;

/// The sampling windows `fan` resolved, in pixels of the frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Windows {
    pub gap_px: u32,
    pub step_cols: u32,
    pub texture_cols: u32,
}

/// The band profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BandProfile {
    /// The record's version.
    pub band_profile_version: u32,
    /// The local id.
    pub id: String,
    /// The frame id.
    pub frame: String,
    /// The frame's identity.
    pub frame_identity: String,
    /// The identity of the `line` location the band follows.
    pub line: String,
    /// The centre column `c`, in pixels.
    pub column_px: u32,
    /// The minimum half-width `thin`, in pixels.
    pub thin_px: f64,
    /// The half-width per row, `height` entries, each at least `thin_px`.
    pub half: Vec<f64>,
    pub windows: Windows,
    /// The constant set that produced it, such as `fan-v1`.
    pub constants: String,
    /// `sha256:` over the frame identity, the line identity, the column, `thin_px`, `half`,
    /// the windows and the constants.
    pub identity: String,
}

impl BandProfile {
    /// A profile; checks the invariants and computes the identity.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: &str,
        frame_id: &str,
        frame_identity: &str,
        line_identity: &str,
        column_px: u32,
        thin_px: f64,
        half: Vec<f64>,
        windows: Windows,
        constants: &str,
    ) -> Result<BandProfile> {
        let mut profile = BandProfile {
            band_profile_version: 1,
            id: id.to_string(),
            frame: frame_id.to_string(),
            frame_identity: frame_identity.to_string(),
            line: line_identity.to_string(),
            column_px,
            thin_px,
            half,
            windows,
            constants: constants.to_string(),
            identity: String::new(),
        };
        profile.check()?;
        profile.identity = profile.compute_identity().as_str().to_string();
        Ok(profile)
    }

    /// The identity over the content, references by identity.
    pub fn compute_identity(&self) -> Identity {
        canonical::identity_of(
            "band_profile",
            &serde_json::json!({
                "frame": self.frame_identity, "line": self.line, "column_px": self.column_px,
                "thin_px": self.thin_px, "half": self.half, "windows": self.windows,
                "constants": self.constants,
            }),
        )
    }

    /// The identity as a typed value.
    pub fn identity(&self) -> Identity {
        Identity::parse_or_hash(&self.identity)
    }

    /// The invariants: a positive `thin_px`, finite half-widths none below `thin_px`, at least
    /// one row.
    pub fn check(&self) -> Result<()> {
        let fail = |m: String| Error::op("band_profile", format!("{}: {m}", self.id));
        if !(self.thin_px.is_finite() && self.thin_px > 0.0) {
            return Err(fail(format!(
                "thin_px must be positive and finite, got {}",
                self.thin_px
            )));
        }
        if self.half.is_empty() {
            return Err(fail("half must have one entry per row".into()));
        }
        if let Some((y, h)) = self
            .half
            .iter()
            .enumerate()
            .find(|(_, h)| !h.is_finite() || **h < self.thin_px - 1e-9)
        {
            return Err(fail(format!(
                "half[{y}] = {h} is below thin_px = {} or not finite",
                self.thin_px
            )));
        }
        Ok(())
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
    pub fn read(path: &Path) -> Result<BandProfile> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        let profile: BandProfile = serde_json::from_str(&text).map_err(|source| Error::Json {
            path: path.to_path_buf(),
            source,
        })?;
        profile.check()?;
        if profile.compute_identity().as_str() != profile.identity {
            return Err(Error::op(
                "band_profile",
                format!(
                    "{}: the stored identity does not match the content",
                    path.display()
                ),
            ));
        }
        Ok(profile)
    }
}
