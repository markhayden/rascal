//! Invocation bundles (CONTRACT 4.9; PLAN.md 10.13): a call, or a chain of calls, as one ustar
//! file a host builds from contract pieces, so it runs again with the original workspace gone.
//! Its first member is `invocation.json` (`urn:table-manners:core:invocation-bundle`); every
//! caller image is under `inputs/<file_sha256>.<ext>` and every tool export under
//! `artifacts/sha256-<hex>.<ext>`. No tool builds one; rascal reads one to replay it
//! (`rascal call --bundle`).
//!
//! [`read`] trusts nothing and checks the whole file before anything acts on it, within the
//! bounds of 10.5's reader: at most [`MAX_BUNDLE_BYTES`] and [`MAX_MEMBERS`] members, a manifest
//! of at most [`MAX_MANIFEST_BYTES`], plain member names of the two forms, regular files only,
//! each size checked against what remains. The manifest must validate against the contract's
//! schema; every member it names must be present and no other; each `artifacts/` member's bytes
//! must have its `sha256`, and each `inputs/` member's its image's `file_sha256` (which its name
//! repeats). Any fault is an error naming it; nothing here writes anywhere.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use serde::Deserialize;

use crate::error::{Error, Result};

/// The largest bundle read, in bytes, as for a mask bundle; a larger file is refused unread.
pub const MAX_BUNDLE_BYTES: u64 = 256 << 20;
/// The most members a bundle may hold, the manifest included.
pub const MAX_MEMBERS: usize = 256;
/// The largest `invocation.json`, in bytes: it holds every step's result.
pub const MAX_MANIFEST_BYTES: u64 = 8 << 20;
/// The longest member file name after its directory, in bytes.
pub const MAX_NAME_BYTES: usize = 96;
/// The manifest's member name.
pub const MANIFEST: &str = "invocation.json";
/// The directory caller images sit under.
pub const INPUTS: &str = "inputs/";
/// The directory tool exports sit under.
pub const ARTIFACTS: &str = "artifacts/";
/// The manifest's schema.
pub const SCHEMA: &str = "urn:table-manners:core:invocation-bundle";

/// A bundle, checked whole: its steps, its members by name and the file's digest.
#[derive(Debug, Clone)]
pub struct InvocationBundle {
    pub steps: Vec<Step>,
    pub members: BTreeMap<String, Vec<u8>>,
    /// `sha256:` over the file's bytes.
    pub sha256: String,
}

/// One step of `invocation.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    pub tool: Tool,
    pub invocation: serde_json::Value,
    pub result: serde_json::Value,
    pub inputs: Vec<Input>,
    pub artifacts: Vec<ArtifactMember>,
    pub reference_only: Vec<ReferenceOnly>,
    pub unportable: Vec<Unportable>,
}

/// The tool a step ran in.
#[derive(Debug, Clone, Deserialize)]
pub struct Tool {
    pub name: String,
    pub version: String,
}

/// An image a step took and its member.
#[derive(Debug, Clone, Deserialize)]
pub struct Input {
    pub slot: Option<String>,
    pub port: Option<String>,
    pub image: serde_json::Value,
    pub member: String,
}

impl Input {
    /// The slot or port it filled.
    pub fn name(&self) -> &str {
        self.slot
            .as_deref()
            .or(self.port.as_deref())
            .unwrap_or_default()
    }
}

/// A native input with its export file.
#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactMember {
    pub port: String,
    pub artifact: serde_json::Value,
    pub member: String,
    pub sha256: String,
}

/// An input left out on purpose.
#[derive(Debug, Clone, Deserialize)]
pub struct ReferenceOnly {
    pub slot: Option<String>,
    pub port: Option<String>,
    pub why: String,
}

/// A native input that could not travel.
#[derive(Debug, Clone, Deserialize)]
pub struct Unportable {
    pub port: String,
    pub artifact: serde_json::Value,
    pub reason: String,
}

#[derive(Deserialize)]
struct Manifest {
    steps: Vec<Step>,
}

fn refused(what: impl std::fmt::Display) -> Error {
    Error::usage(format!(
        "invocation bundle refused: {what}; nothing was imported and nothing ran"
    ))
}

/// Reads a bundle file within [`MAX_BUNDLE_BYTES`]: a directory or a larger file is refused
/// before any byte is read.
pub fn read_file(path: &Path) -> Result<Vec<u8>> {
    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    if meta.is_dir() {
        return Err(refused("it is a directory"));
    }
    if meta.len() > MAX_BUNDLE_BYTES {
        return Err(refused(format!(
            "the file is {} bytes, over the limit of {MAX_BUNDLE_BYTES}",
            meta.len()
        )));
    }
    let file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let mut bytes = Vec::new();
    file.take(MAX_BUNDLE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::io(path, e))?;
    if bytes.len() as u64 > MAX_BUNDLE_BYTES {
        return Err(refused(format!(
            "the file grew past the limit of {MAX_BUNDLE_BYTES} bytes while it was read"
        )));
    }
    Ok(bytes)
}

/// Reads and checks a bundle's bytes whole.
pub fn read(bytes: &[u8]) -> Result<InvocationBundle> {
    if bytes.len() as u64 > MAX_BUNDLE_BYTES {
        return Err(refused(format!(
            "{} bytes, over the limit of {MAX_BUNDLE_BYTES}",
            bytes.len()
        )));
    }
    if bytes.len() < 512 || bytes.get(257..262) != Some(b"ustar".as_slice()) {
        return Err(refused("it is not a ustar archive"));
    }
    let (manifest_raw, members) = entries(bytes)?;
    let value: serde_json::Value =
        serde_json::from_slice(&manifest_raw).map_err(|e| refused(format!("{MANIFEST}: {e}")))?;
    let problems = crate::contract::schema_errors(SCHEMA, &value).map_err(refused)?;
    if !problems.is_empty() {
        return Err(refused(format!(
            "{MANIFEST} is not a valid invocation bundle manifest: {}",
            problems.join("; ")
        )));
    }
    let manifest: Manifest =
        serde_json::from_value(value).map_err(|e| refused(format!("{MANIFEST}: {e}")))?;
    let mut named: BTreeSet<&str> = BTreeSet::new();
    for (n, step) in manifest.steps.iter().enumerate() {
        let at = n + 1;
        for input in &step.inputs {
            let bytes = members
                .get(&input.member)
                .ok_or_else(|| refused(format!("step {at}: member {} is missing", input.member)))?;
            let file = input.image["file_sha256"].as_str().unwrap_or_default();
            let digest = crate::contract::file_sha256(bytes);
            if digest != file {
                return Err(refused(format!(
                    "step {at}: member {} has sha256 {digest}, but its image's file_sha256 is {file}",
                    input.member
                )));
            }
            if !input.member[INPUTS.len()..].starts_with(&format!("{file}.")) {
                return Err(refused(format!(
                    "step {at}: member {} is not named by its image's file_sha256",
                    input.member
                )));
            }
            named.insert(&input.member);
        }
        for artifact in &step.artifacts {
            let bytes = members.get(&artifact.member).ok_or_else(|| {
                refused(format!("step {at}: member {} is missing", artifact.member))
            })?;
            let digest = crate::contract::file_sha256(bytes);
            if digest != artifact.sha256 {
                return Err(refused(format!(
                    "step {at}: member {} has sha256 {digest}, but the manifest says {}",
                    artifact.member, artifact.sha256
                )));
            }
            named.insert(&artifact.member);
        }
    }
    if let Some(extra) = members.keys().find(|m| !named.contains(m.as_str())) {
        return Err(refused(format!(
            "member {extra} is not one {MANIFEST} names"
        )));
    }
    Ok(InvocationBundle {
        steps: manifest.steps,
        members,
        sha256: crate::bundle::sha256(bytes),
    })
}

/// A bundle's members other than the manifest, by name.
type Members = BTreeMap<String, Vec<u8>>;

/// The archive's members, bounded: the manifest's bytes and every other member by name.
fn entries(bytes: &[u8]) -> Result<(Vec<u8>, Members)> {
    let mut archive = tar::Archive::new(bytes);
    let iter = archive
        .entries()
        .map_err(|e| refused(format!("not a readable tar archive: {e}")))?;
    let mut manifest: Option<Vec<u8>> = None;
    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (index, entry) in iter.enumerate() {
        let mut entry =
            entry.map_err(|e| refused(format!("the archive is damaged at member {index}: {e}")))?;
        if index >= MAX_MEMBERS {
            return Err(refused(format!("more than {MAX_MEMBERS} members")));
        }
        let path = String::from_utf8(entry.path_bytes().into_owned())
            .map_err(|_| refused(format!("member {index}'s name is not UTF-8")))?;
        let kind = entry.header().entry_type();
        if kind != tar::EntryType::Regular {
            return Err(refused(format!(
                "member {path} is a {kind:?}, not a regular file"
            )));
        }
        let size = entry
            .header()
            .size()
            .map_err(|e| refused(format!("member {path}: {e}")))?;
        let remaining = (bytes.len() as u64).saturating_sub(entry.raw_file_position());
        if size > remaining {
            return Err(refused(format!(
                "member {path} claims {size} bytes but {remaining} remain (a truncated archive)"
            )));
        }
        if index == 0 {
            if path != MANIFEST {
                return Err(refused(format!(
                    "its first member is {path}, not {MANIFEST}"
                )));
            }
            if size > MAX_MANIFEST_BYTES {
                return Err(refused(format!(
                    "{MANIFEST} is {size} bytes, over the limit of {MAX_MANIFEST_BYTES}"
                )));
            }
        } else if !plain(&path) {
            return Err(refused(format!(
                "member {path} is not {INPUTS}<name> or {ARTIFACTS}<name> (a plain name of at most {MAX_NAME_BYTES} bytes)"
            )));
        }
        let mut data = Vec::new();
        (&mut entry)
            .take(size)
            .read_to_end(&mut data)
            .map_err(|e| refused(format!("member {path}: {e}")))?;
        if data.len() as u64 != size {
            return Err(refused(format!(
                "member {path} holds {} of its {size} bytes (a truncated archive)",
                data.len()
            )));
        }
        if index == 0 {
            manifest = Some(data);
        } else if members.insert(path.clone(), data).is_some() {
            return Err(refused(format!("member {path} appears twice")));
        }
    }
    let manifest = manifest.ok_or_else(|| refused("the archive is empty"))?;
    Ok((manifest, members))
}

/// `inputs/<name>` or `artifacts/<name>`: the name non-empty, at most [`MAX_NAME_BYTES`], not
/// hidden, without a separator, of lowercase letters, digits, `-` and `.`.
fn plain(path: &str) -> bool {
    let name = path
        .strip_prefix(INPUTS)
        .or_else(|| path.strip_prefix(ARTIFACTS));
    name.is_some_and(|name| {
        !name.is_empty()
            && name.len() <= MAX_NAME_BYTES
            && !name.starts_with('.')
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
    })
}

#[cfg(test)]
mod tests {
    use super::plain;

    #[test]
    fn member_names_are_plain() {
        assert!(plain("inputs/abc.png"));
        assert!(plain("artifacts/sha256-ab.mask.tar"));
        assert!(!plain("inputs/../x"));
        assert!(!plain("inputs/.hidden"));
        assert!(!plain("other/x"));
        assert!(!plain("inputs/"));
        assert!(!plain("inputs/A.png"));
    }
}
