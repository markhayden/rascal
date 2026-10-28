//! The `schemas/` directory: the copied schemas with their recorded provenance, rascal's own
//! schemas, and validators over any of them.
//!
//! Inside a schema, another schema file is referenced as `urn:rascal:schemas:<dir>:<name>`,
//! which names `schemas/<dir>/<name>.json`. The validator resolves those references from the
//! directory and nowhere else: no file path, no network.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// The copied tool-manifest schema, unchanged from its origin.
pub const COPIED_MANIFEST: &str = "copied/manifest-v1.json";
/// The copied result-envelope schema, unchanged from its origin; its `Location` definition is
/// what rascal's location schema references for the five copied kinds.
pub const COPIED_ENVELOPE: &str = "copied/envelope-v1.json";
/// rascal's location schema: the copied kinds plus `line` and `stroke`.
pub const LOCATION: &str = "rascal/location-v1.json";

const URN_PREFIX: &str = "urn:rascal:schemas:";

/// A `schemas/` directory.
#[derive(Debug, Clone)]
pub struct SchemaDir {
    root: PathBuf,
}

impl SchemaDir {
    /// The directory at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The directory's path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The path of a schema given relative to the directory, such as `rascal/location-v1.json`.
    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// The parsed schema at `relative`.
    pub fn read(&self, relative: &str) -> Result<serde_json::Value> {
        read_json(&self.path(relative))
    }

    /// A validator for the schema at `relative`, resolving `urn:rascal:schemas:` references
    /// from this directory.
    pub fn validator(&self, relative: &str) -> Result<jsonschema::Validator> {
        let path = self.path(relative);
        let schema = read_json(&path)?;
        self.validator_for(&schema)
            .map_err(|message| Error::Schema { path, message })
    }

    /// A validator for an in-memory schema that may reference this directory's files.
    pub fn validator_for(
        &self,
        schema: &serde_json::Value,
    ) -> std::result::Result<jsonschema::Validator, String> {
        jsonschema::options()
            .with_retriever(Retriever {
                root: self.root.clone(),
            })
            .build(schema)
            .map_err(|error| error.to_string())
    }

    /// The provenance records in `SOURCES.toml`.
    pub fn sources(&self) -> Result<Sources> {
        let path = self.root.join("SOURCES.toml");
        let text = std::fs::read_to_string(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| Error::Toml {
            path,
            source: Box::new(source),
        })
    }

    /// Checks every copied file against the sha256 recorded in `SOURCES.toml`.
    pub fn verify_copied(&self) -> Result<()> {
        for file in self.sources()?.file {
            let path = self.root.join(&file.path);
            let bytes = std::fs::read(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            let actual = hex(&Sha256::digest(&bytes));
            if actual != file.sha256 {
                return Err(Error::CopiedSchemaChanged {
                    path,
                    expected: file.sha256,
                    actual,
                });
            }
        }
        Ok(())
    }
}

/// `schemas/SOURCES.toml`: where each copied file came from.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    pub file: Vec<SourceFile>,
}

/// One copied file's provenance.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFile {
    /// Relative to the schema directory, such as `copied/envelope-v1.json`.
    pub path: String,
    /// Where the file was copied from.
    pub origin: String,
    /// The origin's commit.
    pub commit: String,
    /// The origin's version at that commit.
    pub version: String,
    /// The origin's licence.
    pub licence: String,
    /// The file's sha256, lower-case hex.
    pub sha256: String,
}

/// Resolves `urn:rascal:schemas:<dir>:<name>` to `<root>/<dir>/<name>.json`.
struct Retriever {
    root: PathBuf,
}

impl jsonschema::Retrieve for Retriever {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> std::result::Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let text = uri.as_str();
        let unknown = || Error::UnknownSchemaUri {
            uri: text.to_string(),
        };
        let rest = text.strip_prefix(URN_PREFIX).ok_or_else(unknown)?;
        let (dir, name) = rest.split_once(':').ok_or_else(unknown)?;
        if dir.is_empty()
            || name.is_empty()
            || dir.contains(['/', '.'])
            || name.contains(['/', '.'])
        {
            return Err(Box::new(unknown()));
        }
        Ok(read_json(
            &self.root.join(dir).join(format!("{name}.json")),
        )?)
    }
}

fn read_json(path: &Path) -> Result<serde_json::Value> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| Error::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
