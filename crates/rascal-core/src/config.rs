//! `rascal.toml`: the configuration a pipeline gives rascal: the general sections, the
//! `[fillers.*]` entries (SPEC.md "Fillers") and the `[packs.*]` entries (SPEC.md "The MCP
//! server and client").

use std::collections::BTreeMap;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::recipe::Override;

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
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
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
    /// The tool packs by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub packs: BTreeMap<String, PackConfig>,
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
        for (name, pack) in &config.packs {
            pack.check(name)
                .map_err(|e| Error::usage(format!("{origin}: {e}")))?;
        }
        Ok(config)
    }

    /// Every environment variable a filler names as `key_env`: the names no pack may see.
    pub fn key_envs(&self) -> Vec<String> {
        self.fillers
            .values()
            .filter_map(|f| f.key_env.clone())
            .collect()
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    /// Recipe overrides `heal` applies when this filler paints, `"<step>.<param>" = value` or
    /// `"<step>.constants" = "<set>"` (the matrix's form): the band a filler can carry is a
    /// property of the filler (`docs/results/seam-02.md`). `rascal run` and the lab ignore
    /// them: a run without a filler has none, and a lab arm says everything it runs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, Override>,
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
        if let Some(command) = &self.command {
            for placeholder in ["{image}", "{out}"] {
                if !command.iter().any(|a| a.contains(placeholder)) {
                    return Err(fail(format!("command names no {placeholder}")));
                }
            }
            let wants_mask = command.iter().any(|a| a.contains("{mask}"));
            if wants_mask && !self.mask_input() && !self.send_mask {
                return Err(fail(
                    "command names {mask} but the profile sends none; set send_mask = true".into(),
                ));
            }
        }
        if self.send_mask && self.mask_input() {
            return Err(fail(
                "send_mask applies to an image-input profile; a mask-input profile sends the mask already".into(),
            ));
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

/// How a pack takes the images a call references: by path on the shared filesystem or as
/// base64 PNG bytes inside `_harness`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Accepts {
    #[default]
    Path,
    Base64,
}

/// One tool pack under `[packs.<name>]`: the command that serves MCP over stdio, the
/// environment it may see beyond the baseline, how it takes images, whether it may run calls at
/// once, a revision string for the lab, and two timeouts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackConfig {
    /// The command and its arguments; no placeholders.
    pub command: Vec<String>,
    /// Environment variable names passed through in addition to the baseline (`PATH`, `HOME`,
    /// `LANG`, `LC_ALL`, `TMPDIR`). A name any filler uses as `key_env`, or one ending in
    /// `_KEY`, `_TOKEN` or `_SECRET`, is removed even when declared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    /// `path` (default) or `base64`.
    #[serde(default)]
    pub accepts: Accepts,
    /// Whether calls may run at once; effective only when the tool's manifest agrees.
    #[serde(default)]
    pub concurrent: bool,
    /// Free text the lab hashes into a cell's identity; required for lab use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Seconds for the process to answer `initialize` and list its tools.
    #[serde(default = "default_startup_timeout")]
    pub startup_timeout_s: u64,
    /// Seconds before a call is abandoned as an operational error and the pack shut down.
    #[serde(default = "default_timeout")]
    pub call_timeout_s: u64,
}

fn default_startup_timeout() -> u64 {
    60
}

impl PackConfig {
    /// The entry's shape rules; `name` is for the message.
    pub fn check(&self, name: &str) -> Result<()> {
        let fail = |m: String| Error::usage(format!("packs.{name}: {m}"));
        if self.command.is_empty() || self.command[0].trim().is_empty() {
            return Err(fail("command is empty".into()));
        }
        if self.startup_timeout_s == 0 || self.call_timeout_s == 0 {
            return Err(fail(
                "startup_timeout_s and call_timeout_s must be at least 1".into(),
            ));
        }
        for var in &self.env {
            if var.is_empty()
                || var.contains('=')
                || !var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(fail(format!(
                    "env names a variable {var:?} that is not a plain name"
                )));
            }
        }
        Ok(())
    }
}
