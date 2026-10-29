//! The protocol: the groups (corpus ids), the two arms compared, the seed, `min_decided` and
//! the rules, committed before `lab run` is invoked; the run records its digest, and a change
//! after a run is a new lab name.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::error::Result;

/// `protocol.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Protocol {
    pub version: u32,
    /// The lab's name; a protocol change after a run is a new name.
    pub name: String,
    /// The seed `lab pairs` draws the order and the sides from; the matrix repeats it.
    pub seed: u64,
    /// The two arms compared, by matrix arm name: `[a, b]`; wins are a's.
    pub arms: [String; 2],
    /// Fewer decided pairs than this in a group is `inconclusive`.
    #[serde(default = "default_min_decided")]
    pub min_decided: u32,
    #[serde(default)]
    pub rules: Rules,
    #[serde(default)]
    pub group: Vec<Group>,
}

/// A group of corpus images judged together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub name: String,
    /// Corpus image ids.
    pub images: Vec<String>,
}

/// The outcome rules, as fractions of the decided pairs (`[numerator, denominator]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    /// `preferred` when wins over decided pairs is at least this.
    pub preferred: [u32; 2],
    /// `not_worse` when losses over decided pairs is below this.
    pub not_worse: [u32; 2],
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            preferred: [2, 3],
            not_worse: [1, 3],
        }
    }
}

fn default_min_decided() -> u32 {
    10
}

impl Protocol {
    /// Parses protocol text: version 1, a name, two distinct arms, `min_decided ≥ 1`, sane
    /// fractions, non-empty groups with unique names and an image in one group at most.
    pub fn parse(text: &str, origin: &str) -> Result<Protocol> {
        let protocol: Protocol = toml::from_str(text).map_err(|source| Error::Toml {
            path: PathBuf::from(origin),
            source: Box::new(source),
        })?;
        let usage = |message: String| Error::usage(format!("{origin}: {message}"));
        if protocol.version != 1 {
            return Err(usage(format!("version {} is not 1", protocol.version)));
        }
        if protocol.name.is_empty() {
            return Err(usage("name is empty".into()));
        }
        if protocol.arms[0] == protocol.arms[1] || protocol.arms.iter().any(String::is_empty) {
            return Err(usage(format!(
                "arms must be two distinct arm names, not {:?}",
                protocol.arms
            )));
        }
        if protocol.min_decided == 0 {
            return Err(usage("min_decided must be at least 1".into()));
        }
        for (name, [n, d]) in [
            ("preferred", protocol.rules.preferred),
            ("not_worse", protocol.rules.not_worse),
        ] {
            if d == 0 || n > d {
                return Err(usage(format!(
                    "rules.{name} = [{n}, {d}] is not a fraction between 0 and 1"
                )));
            }
        }
        if protocol.group.is_empty() {
            return Err(usage("at least one group is required".into()));
        }
        let mut names = BTreeSet::new();
        let mut images = BTreeSet::new();
        for group in &protocol.group {
            if group.name.is_empty() || !names.insert(group.name.as_str()) {
                return Err(usage(format!(
                    "group name {:?} is empty or repeated",
                    group.name
                )));
            }
            if group.images.is_empty() {
                return Err(usage(format!("group {} has no images", group.name)));
            }
            for id in &group.images {
                if !images.insert(id.as_str()) {
                    return Err(usage(format!(
                        "image {id} is in more than one group (group {})",
                        group.name
                    )));
                }
            }
        }
        Ok(protocol)
    }

    /// Reads a protocol file and returns it with the digest of its bytes, which a run records.
    pub fn read(path: &Path) -> Result<(Protocol, String)> {
        let bytes = std::fs::read(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let text = String::from_utf8(bytes.clone())
            .map_err(|e| Error::usage(format!("{}: not UTF-8 ({e})", path.display())))?;
        let protocol = Protocol::parse(&text, &path.display().to_string())?;
        Ok((protocol, super::bytes_digest(&bytes)))
    }

    /// Every image id the protocol's groups name.
    pub fn image_ids(&self) -> impl Iterator<Item = &str> {
        self.group
            .iter()
            .flat_map(|g| g.images.iter().map(String::as_str))
    }
}
