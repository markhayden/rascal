//! `rascal.toml`: the configuration a pipeline gives rascal. Task 1.6 holds the general
//! sections; `[fillers]` arrives with task 2.7 and `[packs]` with 3.3.

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
        Ok(config)
    }

    /// Reads a configuration file.
    pub fn read(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        Config::parse(&text, &path.display().to_string())
    }
}
