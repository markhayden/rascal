//! `rascal.toml`: the configuration a pipeline gives rascal: the general sections, the
//! `[fillers.*]` entries (SPEC.md "Fillers"); `[packs]` arrives with task 3.3.

use std::collections::BTreeMap;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Where runs keep their workspace when `--workspace` is not given.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    /// A directory; relative to the configuration file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
}

/// The private corpus (M6).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorpusConfig {
    /// The corpus manifest; relative to the configuration file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<String>,
}

/// The configuration file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The file's version.
    pub config_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corpus: Option<CorpusConfig>,
    /// The fillers by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fillers: BTreeMap<String, FillerConfig>,
}

impl Config {
    /// Parses configuration text.
    pub fn parse(text: &str, origin: &str) -> Result<Config> {
        let config: Config = toml::from_str(text).map_err(|source| Error::Toml {
            path: origin.into(),
            source: Box::new(source),
        })?;
        if config.config_version != 1 {
            return Err(Error::usage(format!(
                "{origin}: config_version {} is not supported (expected 1)",
                config.config_version
            )));
        }
        for (name, filler) in &config.fillers {
            filler
                .check(name)
                .map_err(|e| Error::usage(format!("{origin}: {e}")))?;
        }
        Ok(config)
    }

    /// Reads a configuration file.
    pub fn read(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        Config::parse(&text, &path.display().to_string())
    }
}

/// One filler entry under `[fillers.<name>]`: a command with placeholders or an HTTP endpoint,
/// the rendering profile it takes, the paste mode its result is composited with, a timeout,
/// and for the lab a revision string. `key_env` names the environment variable holding a
/// bearer key, which is read at call time and never stored, logged or journaled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FillerConfig {
    /// The command and its arguments; `{image}`, `{mask}`, `{out}` and `{prompt}` are
    /// substituted. Exactly one of `command` and `url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    /// The endpoint of a multipart POST (`image`, `mask` when the profile has one, `prompt`
    /// when given) that answers with a PNG.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The rendering profile the filler takes: `grey` or `binary` (mask input), `alpha` or
    /// `tinted` (image input).
    pub profile: String,
    /// The paste mode for its result: `feather`, `hard` or `poisson`.
    #[serde(default = "default_paste")]
    pub paste: String,
    /// Seconds before the call is abandoned as an operational error.
    #[serde(default = "default_timeout")]
    pub timeout_s: u64,
    /// For an image-input profile: also send the `grey` rendering as `mask`.
    #[serde(default)]
    pub send_mask: bool,
    /// The environment variable holding the bearer key for `url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_env: Option<String>,
    /// The filler's largest side; the heal graph windows to it. Unset: no window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_side: Option<u32>,
    /// Free text the lab hashes into a cell's identity; required for lab use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

fn default_paste() -> String {
    "feather".to_string()
}

fn default_timeout() -> u64 {
    120
}

/// The paste modes a filler entry may name.
pub const PASTE_MODES: [&str; 3] = ["feather", "hard", "poisson"];

/// The rendering profiles a filler entry may name (the inspection profile is for the eye).
pub const FILLER_PROFILES: [&str; 4] = ["grey", "binary", "alpha", "tinted"];

impl FillerConfig {
    /// The entry's shape rules; `name` is for the message.
    pub fn check(&self, name: &str) -> Result<()> {
        let fail = |m: String| Error::usage(format!("fillers.{name}: {m}"));
        match (&self.command, &self.url) {
            (Some(_), Some(_)) => return Err(fail("give command or url, not both".into())),
            (None, None) => return Err(fail("give command or url".into())),
            (Some(c), None) if c.is_empty() => return Err(fail("command is empty".into())),
            _ => {}
        }
        if !FILLER_PROFILES.contains(&self.profile.as_str()) {
            return Err(fail(format!(
                "profile {} is not one of {}",
                self.profile,
                FILLER_PROFILES.join(", ")
            )));
        }
        if !PASTE_MODES.contains(&self.paste.as_str()) {
            return Err(fail(format!(
                "paste {} is not one of {}",
                self.paste,
                PASTE_MODES.join(", ")
            )));
        }
        if self.timeout_s == 0 {
            return Err(fail("timeout_s must be at least 1".into()));
        }
        if self.key_env.is_some() && self.url.is_none() {
            return Err(fail("key_env goes with url".into()));
        }
        if let Some(m) = self.max_side
            && m == 0
        {
            return Err(fail("max_side must be at least 1".into()));
        }
        Ok(())
    }

    /// True for a profile whose rendering is the filler's `mask` field.
    pub fn mask_input(&self) -> bool {
        matches!(self.profile.as_str(), "grey" | "binary")
    }
}
