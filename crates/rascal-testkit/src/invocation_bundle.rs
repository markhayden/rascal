//! A host-side writer of invocation bundles (CONTRACT 4.9; PLAN.md 10.13), for tests: no tool
//! builds one, so the tests build one as a host would, from contract pieces (each step's
//! `run.json`, the caller's image files, and the files `export` wrote).
//!
//! The archive is byte-stable ustar: `invocation.json` first, then every other member once,
//! sorted by name; every header a regular file, mode 0644, uid and gid 0, mtime 0, no user or
//! group name; the two zero blocks at the end. The same steps give the same bytes. [`pack`]
//! takes any manifest and members, so a test can build a tampered bundle.

use std::collections::BTreeMap;

use serde_json::{Value, json};

/// The manifest's member name.
pub const MANIFEST: &str = "invocation.json";

/// An image a step took: the slot (a task's) or port (an operation's) it filled, its image
/// reference (as the result's `images.in` gives it), the file's bytes and its extension.
pub struct InputFile {
    pub slot: Option<String>,
    pub port: Option<String>,
    pub image: Value,
    pub bytes: Vec<u8>,
    pub ext: String,
}

/// A native input with the file `export` wrote for it: the port, its reference, the bytes and
/// the extension (`mask.tar`, `location.json`).
pub struct ArtifactFile {
    pub port: String,
    pub artifact: Value,
    pub bytes: Vec<u8>,
    pub ext: String,
}

/// One step: the original result and what travels with it.
pub struct Step {
    pub result: Value,
    pub inputs: Vec<InputFile>,
    pub artifacts: Vec<ArtifactFile>,
    pub reference_only: Vec<Value>,
    pub unportable: Vec<Value>,
}

/// SHA-256 of `bytes` as 64 lowercase hex digits.
pub fn sha256_hex(bytes: &[u8]) -> String {
    rascal_core::contract::file_sha256(bytes)
}

/// The manifest and the members of `steps`.
pub fn parts(steps: &[Step]) -> (Value, BTreeMap<String, Vec<u8>>) {
    let mut members = BTreeMap::new();
    let mut entries = Vec::new();
    for step in steps {
        let inputs: Vec<Value> = step
            .inputs
            .iter()
            .map(|input| {
                let member = format!("inputs/{}.{}", sha256_hex(&input.bytes), input.ext);
                members.insert(member.clone(), input.bytes.clone());
                let mut entry = json!({ "image": input.image, "member": member });
                if let Some(slot) = &input.slot {
                    entry["slot"] = json!(slot);
                }
                if let Some(port) = &input.port {
                    entry["port"] = json!(port);
                }
                entry
            })
            .collect();
        let artifacts: Vec<Value> = step
            .artifacts
            .iter()
            .map(|artifact| {
                let identity = artifact.artifact["identity"]
                    .as_str()
                    .unwrap_or_default()
                    .replace(':', "-");
                let member = format!("artifacts/{identity}.{}", artifact.ext);
                members.insert(member.clone(), artifact.bytes.clone());
                json!({
                    "port": artifact.port,
                    "artifact": artifact.artifact,
                    "member": member,
                    "sha256": sha256_hex(&artifact.bytes),
                })
            })
            .collect();
        entries.push(json!({
            "contract": step.result["contract"],
            "tool": step.result["tool"],
            "invocation": step.result["invocation"],
            "result": step.result,
            "inputs": inputs,
            "artifacts": artifacts,
            "reference_only": step.reference_only,
            "unportable": step.unportable,
        }));
    }
    (json!({ "bundle_version": 1, "steps": entries }), members)
}

/// The bundle of `steps`.
pub fn write(steps: &[Step]) -> Vec<u8> {
    let (manifest, members) = parts(steps);
    pack(&manifest_bytes(&manifest), &members)
}

/// The manifest as written: pretty JSON and a newline.
pub fn manifest_bytes(manifest: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(manifest).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

/// The byte-stable archive of a manifest and members, in that order (members sorted).
pub fn pack(manifest: &[u8], members: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let entries = std::iter::once((MANIFEST, manifest))
        .chain(members.iter().map(|(p, b)| (p.as_str(), b.as_slice())));
    for (path, bytes) in entries {
        let mut header = tar::Header::new_ustar();
        header
            .set_path(path)
            .unwrap_or_else(|e| panic!("member {path}: {e}"));
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_cksum();
        builder
            .append(&header, bytes)
            .unwrap_or_else(|e| panic!("member {path}: {e}"));
    }
    builder.into_inner().unwrap_or_default()
}
