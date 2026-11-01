//! Mask bundles (task 10.5, ADR 0022): a Mask as one file that leaves a workspace and enters
//! another with the same identity.
//!
//! A bundle is an uncompressed ustar archive. Its members, in this order: `bundle.json` (the
//! [`MaskBundleManifest`]: the mask's identity, its frame chain from the root raw frame to the mask's
//! frame, every other member's path, size and SHA-256, and the image it was made on when known),
//! then `mask/mask.toml` and each file it names, sorted by name. Every header is fixed (a regular file, mode 0644, uid and gid 0,
//! mtime 0, no user or group name) and the archive ends with its two zero blocks, so the same
//! mask gives the same bytes and the bundle's SHA-256 names it.
//!
//! [`read`] trusts nothing (the manifest's `image` is advice it cannot vouch for and that gates
//! nothing): it is bounded ([`MAX_BUNDLE_BYTES`], [`MAX_MEMBERS`],
//! [`MAX_MANIFEST_BYTES`], [`MAX_NAME_BYTES`]), checks the manifest against the members both
//! ways, the archive against the canonical bytes rebuilt from them, every frame of the chain
//! against what its fields and parent make, and the mask's invariants and identity. Any fault
//! is an error naming it; nothing here writes anywhere.
//!
//! [`write`] holds itself to the same [`Limits`]: a mask whose bundle import would refuse (a
//! large mask with detailed planes can pass 256 MiB) is refused at export, naming the size and
//! the limit, before a byte is written.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::artifact::ArtifactType;
use crate::error::{Error, Result};
use crate::frame::{Frame, FrameKind};
use crate::mask::Mask;

/// The largest bundle file read, in bytes; a larger file is refused before it is read.
pub const MAX_BUNDLE_BYTES: u64 = 256 << 20;
/// The most members a bundle may hold, the manifest included.
pub const MAX_MEMBERS: usize = 16;
/// The largest `bundle.json`, in bytes.
pub const MAX_MANIFEST_BYTES: u64 = 1 << 20;
/// The longest member file name under `mask/`, in bytes.
pub const MAX_NAME_BYTES: usize = 64;
/// The longest frame chain.
pub const MAX_FRAMES: usize = 64;
/// The bounds a bundle is read within. [`write`] refuses a bundle outside them, so an export
/// never makes a file import refuses; [`write_within`] takes others, for tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The largest bundle, in bytes.
    pub bundle_bytes: u64,
    /// The most members, the manifest included.
    pub members: usize,
    /// The largest `bundle.json`, in bytes.
    pub manifest_bytes: u64,
    /// The longest member file name under `mask/`, in bytes.
    pub name_bytes: usize,
    /// The longest frame chain.
    pub frames: usize,
}

impl Limits {
    /// The limits [`read`] enforces.
    pub const READ: Limits = Limits {
        bundle_bytes: MAX_BUNDLE_BYTES,
        members: MAX_MEMBERS,
        manifest_bytes: MAX_MANIFEST_BYTES,
        name_bytes: MAX_NAME_BYTES,
        frames: MAX_FRAMES,
    };
}

/// The manifest's member name.
pub const MANIFEST: &str = "bundle.json";
/// The directory the mask's files sit under.
pub const MASK_DIR: &str = "mask/";
/// The bundle version this build writes and reads.
pub const VERSION: u32 = 1;

/// `bundle.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskBundleManifest {
    /// The bundle format's version.
    pub mask_bundle_version: u32,
    /// What the bundle carries: `Mask`.
    pub artifact_type: ArtifactType,
    /// The mask's identity.
    pub identity: String,
    /// The frame chain, the root raw frame first and the mask's frame last.
    pub frames: Vec<Frame>,
    /// Every member but this manifest, in archive order.
    pub members: Vec<MaskBundleMember>,
    /// The image the mask was made on, when the exporting workspace could tell. Images are not
    /// bundled; the target imports it by its own path to apply the mask, and is told this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<MaskBundleImage>,
}

/// The image a bundled mask was made on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskBundleImage {
    /// SHA-256 of the image file, lowercase hex: the `file_sha256` `rascal workspace import`
    /// prints (ADR 0020).
    pub file_sha256: String,
    /// The image's identity.
    pub identity: String,
}

/// One member of a bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskBundleMember {
    /// The path inside the archive: `mask/<file>`.
    pub path: String,
    /// The member's size in bytes.
    pub size: u64,
    /// `sha256:` over the member's bytes.
    pub sha256: String,
}

/// Member bytes by path.
type Members = BTreeMap<String, Vec<u8>>;

/// A bundle that passed every check.
#[derive(Debug, Clone)]
pub struct Bundle {
    pub manifest: MaskBundleManifest,
    /// The mask, its invariants and identity checked.
    pub mask: Mask,
    /// `sha256:` over the bundle's bytes.
    pub sha256: String,
}

/// `sha256:<hex>` over bytes.
pub fn sha256(bytes: &[u8]) -> String {
    format!(
        "sha256:{}",
        crate::canonical::to_hex(&Sha256::digest(bytes))
    )
}

/// The bundle of a mask whose frame chain, root first, is `frames`, and which was made on
/// `image` when that is known, within [`Limits::READ`].
pub fn write(mask: &Mask, frames: &[Frame], image: Option<&MaskBundleImage>) -> Result<Vec<u8>> {
    write_within(mask, frames, image, &Limits::READ)
}

/// The bundle of a mask whose frame chain, root first, is `frames`, refused (a usage error
/// naming the size and the limit) when it would fall outside `limits`.
pub fn write_within(
    mask: &Mask,
    frames: &[Frame],
    image: Option<&MaskBundleImage>,
    limits: &Limits,
) -> Result<Vec<u8>> {
    if frames.len() > limits.frames {
        return Err(not_written(format!(
            "the frame chain has {} frames, over the limit of {}",
            frames.len(),
            limits.frames
        )));
    }
    verify_chain(frames)?;
    check_leaf(frames, mask)?;
    let members: Vec<(String, Vec<u8>)> = mask
        .files()?
        .into_iter()
        .map(|(name, bytes)| (format!("{MASK_DIR}{name}"), bytes))
        .collect();
    let manifest = MaskBundleManifest {
        mask_bundle_version: VERSION,
        artifact_type: ArtifactType::Mask,
        identity: mask.record.identity.clone(),
        frames: frames.to_vec(),
        members: members
            .iter()
            .map(|(path, bytes)| MaskBundleMember {
                path: path.clone(),
                size: bytes.len() as u64,
                sha256: sha256(bytes),
            })
            .collect(),
        image: image.cloned(),
    };
    if let Some(image) = &manifest.image {
        check_image(image)?;
    }
    if members.len() + 1 > limits.members {
        return Err(not_written(format!(
            "it would hold {} members, over the limit of {}",
            members.len() + 1,
            limits.members
        )));
    }
    for (path, _) in &members {
        let name = path.strip_prefix(MASK_DIR).unwrap_or(path);
        if name.len() > limits.name_bytes {
            return Err(not_written(format!(
                "member {path}'s name is {} bytes, over the limit of {}",
                name.len(),
                limits.name_bytes
            )));
        }
        if !plain(path, limits.name_bytes) {
            return Err(not_written(format!(
                "member {path} is not {MASK_DIR}<a plain name>"
            )));
        }
    }
    let manifest = manifest_bytes(&manifest)?;
    if manifest.len() as u64 > limits.manifest_bytes {
        return Err(not_written(format!(
            "{MANIFEST} would be {} bytes, over the limit of {} bytes",
            manifest.len(),
            limits.manifest_bytes
        )));
    }
    // Known before a byte is packed: a 512-byte header per member, its bytes padded to 512,
    // and the two zero blocks that end the archive.
    let padded = |n: usize| 512 + (n as u64).div_ceil(512) * 512;
    let size =
        padded(manifest.len()) + members.iter().map(|(_, b)| padded(b.len())).sum::<u64>() + 1024;
    if size > limits.bundle_bytes {
        return Err(not_written(format!(
            "the bundle would be {size} bytes ({}), over the limit of {} bytes ({}) that import reads",
            mib(size),
            limits.bundle_bytes,
            mib(limits.bundle_bytes)
        )));
    }
    let bytes = pack(&manifest, &members)?;
    if bytes.len() as u64 != size {
        return Err(Error::usage(format!(
            "bundle: packed {} bytes, expected {size}; nothing was written",
            bytes.len()
        )));
    }
    Ok(bytes)
}

/// Bytes in MiB, one decimal, for a person.
fn mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / f64::from(1u32 << 20))
}

fn not_written(what: impl std::fmt::Display) -> Error {
    Error::usage(format!(
        "mask bundle not written: {what}; import refuses such a bundle, so nothing was written"
    ))
}

/// The manifest's canonical bytes: pretty JSON and a newline.
fn manifest_bytes(manifest: &MaskBundleManifest) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|e| Error::usage(format!("{MANIFEST}: {e}")))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// The canonical archive of the manifest and the members, in that order.
fn pack(manifest: &[u8], members: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut builder = tar::Builder::new(Vec::new());
    let entries = std::iter::once((MANIFEST, manifest))
        .chain(members.iter().map(|(p, b)| (p.as_str(), b.as_slice())));
    for (path, bytes) in entries {
        let mut header = tar::Header::new_ustar();
        header
            .set_path(path)
            .map_err(|e| Error::usage(format!("bundle member {path}: {e}")))?;
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_cksum();
        builder
            .append(&header, bytes)
            .map_err(|e| Error::usage(format!("bundle member {path}: {e}")))?;
    }
    builder
        .into_inner()
        .map_err(|e| Error::usage(format!("bundle: {e}")))
}

/// Reads a bundle file within [`MAX_BUNDLE_BYTES`]: a directory or a larger file is refused
/// before any byte is read.
pub fn read_file(path: &Path) -> Result<Vec<u8>> {
    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    if meta.is_dir() {
        return Err(not_a_bundle(
            "it is a directory (a mask directory goes in with `rascal workspace export-mask` from its workspace first)",
        ));
    }
    if meta.len() > MAX_BUNDLE_BYTES {
        return Err(refused(format!(
            "the file is {} bytes, over the limit of {MAX_BUNDLE_BYTES}; nothing was read",
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

fn not_a_bundle(what: impl std::fmt::Display) -> Error {
    Error::usage(format!("not a mask bundle: {what}; nothing was imported"))
}

fn refused(what: impl std::fmt::Display) -> Error {
    Error::usage(format!("mask bundle refused: {what}; nothing was imported"))
}

/// What a file that is not a tar archive looks like, for the refusal.
fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.is_empty() {
        return Some("the file is empty");
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(
            "it is a PNG image; an image goes in with `rascal workspace import`, and a rendering of a mask is an Image, never a Mask",
        );
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("it is a JPEG image; an image goes in with `rascal workspace import`");
    }
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return Some("it is gzip-compressed; a mask bundle is an uncompressed tar");
    }
    if bytes.len() < 512 || bytes.get(257..262) != Some(b"ustar".as_slice()) {
        return Some("it is not a tar archive");
    }
    None
}

/// Reads and checks a bundle's bytes.
pub fn read(bytes: &[u8]) -> Result<Bundle> {
    if bytes.len() as u64 > MAX_BUNDLE_BYTES {
        return Err(refused(format!(
            "{} bytes, over the limit of {MAX_BUNDLE_BYTES}",
            bytes.len()
        )));
    }
    if let Some(what) = sniff(bytes) {
        return Err(not_a_bundle(what));
    }
    let (manifest_raw, members) = entries(bytes)?;
    // The versions first, from a loose parse: a newer bundle may carry fields this build does
    // not know, and the person needs to hear "upgrade", not "unknown field".
    let loose: serde_json::Value =
        serde_json::from_slice(&manifest_raw).map_err(|e| refused(format!("{MANIFEST}: {e}")))?;
    if let Some(found) = loose.get("mask_bundle_version").and_then(|v| v.as_u64()) {
        check_version(MANIFEST, "bundle version", found, VERSION)?;
    }
    if let Some(toml_bytes) = members.get(&format!("{MASK_DIR}{}", crate::mask::MASK_TOML))
        && let Ok(text) = std::str::from_utf8(toml_bytes)
        && let Ok(record) = text.parse::<toml::Table>()
        && let Some(found) = record.get("version").and_then(|v| v.as_integer())
    {
        check_version(
            &format!("{MASK_DIR}{}", crate::mask::MASK_TOML),
            "mask record version",
            u64::try_from(found).unwrap_or(0),
            crate::mask::RECORD_VERSION,
        )?;
    }
    let manifest: MaskBundleManifest =
        serde_json::from_slice(&manifest_raw).map_err(|e| refused(format!("{MANIFEST}: {e}")))?;
    if manifest.artifact_type != ArtifactType::Mask {
        return Err(refused(format!(
            "{MANIFEST}: it carries a {}, not a Mask",
            manifest.artifact_type
        )));
    }
    if manifest_bytes(&manifest)? != manifest_raw {
        return Err(refused(format!("{MANIFEST} is not in its canonical form")));
    }
    if let Some(image) = &manifest.image {
        check_image(image).map_err(refused)?;
    }
    check_members(&manifest, &members)?;
    let ordered: Vec<(String, Vec<u8>)> = manifest
        .members
        .iter()
        .filter_map(|m| members.get(&m.path).map(|b| (m.path.clone(), b.clone())))
        .collect();
    let canonical = pack(&manifest_raw, &ordered)?;
    if canonical != bytes {
        let at = canonical
            .iter()
            .zip(bytes)
            .position(|(a, b)| a != b)
            .unwrap_or(canonical.len().min(bytes.len()));
        return Err(refused(format!(
            "the archive differs from its canonical form at byte {at} (a header, padding, or bytes after the end)"
        )));
    }
    verify_chain(&manifest.frames).map_err(refused)?;
    let files: BTreeMap<String, Vec<u8>> = members
        .into_iter()
        .filter_map(|(path, b)| path.strip_prefix(MASK_DIR).map(|n| (n.to_string(), b)))
        .collect();
    let mask = Mask::from_files(&files).map_err(refused)?;
    let named: Vec<String> = mask.file_names();
    for name in files.keys() {
        if !named.iter().any(|n| n == name) {
            return Err(refused(format!(
                "member {MASK_DIR}{name} is not a file mask.toml names"
            )));
        }
    }
    check_leaf(&manifest.frames, &mask).map_err(refused)?;
    if mask.record.identity != manifest.identity {
        return Err(refused(format!(
            "{MANIFEST} names identity {}, the mask's is {}",
            manifest.identity, mask.record.identity
        )));
    }
    Ok(Bundle {
        manifest,
        mask,
        sha256: sha256(bytes),
    })
}

/// The image's digests are what they claim to be: 64 lowercase hex digits, the identity with
/// its `sha256:` prefix. They reach an error message, so nothing else gets through.
fn check_image(image: &MaskBundleImage) -> Result<()> {
    let hex = |s: &str| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    if !hex(&image.file_sha256) {
        return Err(Error::usage(format!(
            "{MANIFEST}: image.file_sha256 is not 64 lowercase hex digits"
        )));
    }
    if !image.identity.strip_prefix("sha256:").is_some_and(hex) {
        return Err(Error::usage(format!(
            "{MANIFEST}: image.identity is not sha256:<64 lowercase hex digits>"
        )));
    }
    Ok(())
}

/// A version read from a bundle against the one this build reads: a newer one says to upgrade,
/// an older one that this build no longer reads it.
fn check_version(member: &str, what: &str, found: u64, reads: u32) -> Result<()> {
    if found == u64::from(reads) {
        Ok(())
    } else if found > u64::from(reads) {
        Err(refused(format!(
            "{member}: made by a newer rascal ({what} {found}); this rascal reads version {reads}: upgrade rascal"
        )))
    } else {
        Err(refused(format!(
            "{member}: made by an older rascal ({what} {found}) that this rascal no longer reads"
        )))
    }
}

/// The archive's members, bounded: the manifest's bytes and every other member by path.
fn entries(bytes: &[u8]) -> Result<(Vec<u8>, Members)> {
    let mut archive = tar::Archive::new(bytes);
    let iter = archive
        .entries()
        .map_err(|e| not_a_bundle(format!("not a readable tar archive: {e}")))?;
    let mut manifest: Option<Vec<u8>> = None;
    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (index, entry) in iter.enumerate() {
        let mut entry = entry.map_err(|e| {
            if index == 0 {
                not_a_bundle(format!("not a readable tar archive: {e}"))
            } else {
                refused(format!("the archive is damaged after member {index}: {e}"))
            }
        })?;
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
                return Err(not_a_bundle(format!(
                    "its first member is {path}, not {MANIFEST}"
                )));
            }
            if size > MAX_MANIFEST_BYTES {
                return Err(refused(format!(
                    "{MANIFEST} is {size} bytes, over the limit of {MAX_MANIFEST_BYTES}"
                )));
            }
        } else {
            check_member_path(&path)?;
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
    let manifest = manifest.ok_or_else(|| not_a_bundle("the archive is empty"))?;
    Ok((manifest, members))
}

/// A member other than the manifest is `mask/<plain name>`.
fn check_member_path(path: &str) -> Result<()> {
    if plain(path, MAX_NAME_BYTES) {
        Ok(())
    } else {
        Err(refused(format!(
            "member {path} is not {MASK_DIR}<a plain name of at most {MAX_NAME_BYTES} bytes>"
        )))
    }
}

/// `mask/<name>`, the name non-empty, at most `name_bytes`, not hidden and without a separator.
fn plain(path: &str, name_bytes: usize) -> bool {
    path.strip_prefix(MASK_DIR).is_some_and(|name| {
        !name.is_empty()
            && name.len() <= name_bytes
            && !name.starts_with('.')
            && !name.contains(['/', '\\'])
    })
}

/// The manifest's members against the archive's, both ways, by size and digest.
fn check_members(manifest: &MaskBundleManifest, members: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    for listed in &manifest.members {
        let Some(bytes) = members.get(&listed.path) else {
            return Err(refused(format!(
                "member {} is in {MANIFEST} but not in the archive",
                listed.path
            )));
        };
        if bytes.len() as u64 != listed.size {
            return Err(refused(format!(
                "member {} is {} bytes, {MANIFEST} says {}",
                listed.path,
                bytes.len(),
                listed.size
            )));
        }
        let digest = sha256(bytes);
        if digest != listed.sha256 {
            return Err(refused(format!(
                "member {}: sha256 is {digest}, {MANIFEST} says {}",
                listed.path, listed.sha256
            )));
        }
    }
    for path in members.keys() {
        if !manifest.members.iter().any(|m| &m.path == path) {
            return Err(refused(format!(
                "member {path} is in the archive but not in {MANIFEST}"
            )));
        }
    }
    Ok(())
}

/// A frame chain, root first: it starts at a raw frame, each frame names the one before it as
/// its parent, and each is what its fields and its parent make.
pub fn verify_chain(frames: &[Frame]) -> Result<()> {
    if frames.is_empty() {
        return Err(Error::usage("the frame chain is empty"));
    }
    if frames.len() > MAX_FRAMES {
        return Err(Error::usage(format!(
            "the frame chain has {} frames, over {MAX_FRAMES}",
            frames.len()
        )));
    }
    if frames[0].kind != FrameKind::Raw {
        return Err(Error::usage(format!(
            "the frame chain starts at {}, not at a raw frame",
            frames[0].id
        )));
    }
    let mut parent: Option<&Frame> = None;
    for frame in frames {
        frame.verify(parent)?;
        parent = Some(frame);
    }
    Ok(())
}

/// The mask is in the chain's last frame.
fn check_leaf(frames: &[Frame], mask: &Mask) -> Result<()> {
    let Some(leaf) = frames.last() else {
        return Err(Error::usage("the frame chain is empty"));
    };
    if leaf.id != mask.record.frame || leaf.identity != mask.record.frame_identity {
        return Err(Error::usage(format!(
            "the mask is in frame {} ({}), but the chain ends at {} ({})",
            mask.record.frame, mask.record.frame_identity, leaf.id, leaf.identity
        )));
    }
    if (leaf.width, leaf.height) != (mask.record.width, mask.record.height) {
        return Err(Error::usage(format!(
            "the mask is {}x{} but its frame {} is {}x{}",
            mask.record.width, mask.record.height, leaf.id, leaf.width, leaf.height
        )));
    }
    Ok(())
}
