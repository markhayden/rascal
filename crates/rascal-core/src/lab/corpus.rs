//! The corpus manifest: images by id, SHA-256, group and licence note, with the files
//! themselves outside the repository under `RASCAL_CORPUS` (default `~/rascal/corpus`). No
//! real photograph is ever committed; the manifest is.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::error::Result;

/// `corpus.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    pub version: u32,
    #[serde(default)]
    pub image: Vec<CorpusImage>,
}

/// One image of the corpus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorpusImage {
    pub id: String,
    /// The file, relative to the corpus root; a plain relative path.
    pub file: String,
    /// `sha256:<hex>` of the file's bytes.
    pub sha256: String,
    /// The group the protocol judges it under (`sky`, `structured`).
    pub group: String,
    /// Where the image came from and under what terms it may be used.
    pub licence: String,
}

/// What verifying one image found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verification {
    Ok,
    Missing,
    Mismatch { actual: String },
}

impl Corpus {
    /// Parses manifest text: version 1, unique ids, plain relative files, well-formed digests.
    pub fn parse(text: &str, origin: &str) -> Result<Corpus> {
        let corpus: Corpus = toml::from_str(text).map_err(|source| Error::Toml {
            path: PathBuf::from(origin),
            source: Box::new(source),
        })?;
        if corpus.version != 1 {
            return Err(Error::usage(format!(
                "{origin}: version {} is not 1",
                corpus.version
            )));
        }
        let mut ids = BTreeSet::new();
        for image in &corpus.image {
            if image.id.is_empty() || !ids.insert(image.id.as_str()) {
                return Err(Error::usage(format!(
                    "{origin}: image id {:?} is empty or repeated",
                    image.id
                )));
            }
            if !is_plain_relative(&image.file) {
                return Err(Error::usage(format!(
                    "{origin}: image {}: file {:?} is not a plain relative path",
                    image.id, image.file
                )));
            }
            if !is_digest(&image.sha256) {
                return Err(Error::usage(format!(
                    "{origin}: image {}: sha256 {:?} is not sha256:<64 hex digits>",
                    image.id, image.sha256
                )));
            }
            if image.group.is_empty() {
                return Err(Error::usage(format!(
                    "{origin}: image {}: group is empty",
                    image.id
                )));
            }
        }
        Ok(corpus)
    }

    /// Reads a manifest file.
    pub fn read(path: &Path) -> Result<Corpus> {
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Corpus::parse(&text, &path.display().to_string())
    }

    /// The corpus root: the given directory, else `RASCAL_CORPUS`, else `~/rascal/corpus`.
    pub fn root(given: Option<&Path>) -> PathBuf {
        if let Some(dir) = given {
            return dir.to_path_buf();
        }
        if let Some(dir) = std::env::var_os("RASCAL_CORPUS") {
            return PathBuf::from(dir);
        }
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("rascal")
            .join("corpus")
    }

    /// The image with this id.
    pub fn image(&self, id: &str) -> Option<&CorpusImage> {
        self.image.iter().find(|i| i.id == id)
    }

    /// Every image's file under `root`, checked against its digest, in manifest order.
    pub fn verify(&self, root: &Path) -> Result<Vec<(&CorpusImage, Verification)>> {
        let mut out = Vec::with_capacity(self.image.len());
        for image in &self.image {
            let path = root.join(&image.file);
            let found = if !path.is_file() {
                Verification::Missing
            } else {
                let actual = super::file_digest(&path)?;
                if actual == image.sha256 {
                    Verification::Ok
                } else {
                    Verification::Mismatch { actual }
                }
            };
            out.push((image, found));
        }
        Ok(out)
    }
}

fn is_plain_relative(rel: &str) -> bool {
    !rel.is_empty()
        && Path::new(rel)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

fn is_digest(text: &str) -> bool {
    text.strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}
