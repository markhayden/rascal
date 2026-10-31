//! table-manners, the shared contract of the waffle tools (ADR 0020): rascal's side of it. The
//! contract's types come from the `table-manners` crate, pinned exactly; this module builds them
//! from rascal's records and reads them back. Section numbers are CONTRACT.md's.

use sha2::{Digest, Sha256};
pub use table_manners;
use table_manners::images::{ImageReference, Projection as TmProjection, Topology as TmTopology};

use crate::frame::{Frame, Projection, Topology};
use crate::image::Image;
use crate::workspace::record::OperationRecord;

/// The contract version rascal conforms to.
pub const CONTRACT_VERSION: &str = table_manners::CONTRACT_VERSION;

/// The parameter an `import` record carries: the SHA-256 of the caller's file, lowercase hex.
pub const FILE_SHA256: &str = "file_sha256";

/// SHA-256 of bytes as lowercase hex, the contract's form for `file_sha256` (8.1).
pub fn file_sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// A frame's projection in the contract's words.
pub fn projection(frame: &Frame) -> TmProjection {
    match frame.projection {
        Projection::Flat => TmProjection::Flat,
        Projection::Equirectangular => TmProjection::Equirectangular,
    }
}

/// A frame's topology in the contract's words.
pub fn topology(frame: &Frame) -> TmTopology {
    match frame.topology {
        Topology::Wrap => TmTopology::Wrap,
        Topology::Bounded => TmTopology::Bounded,
    }
}

/// The reference for an image in its frame, given the digest of the file it came from or was
/// written to (8.1). The pixel digest is rascal's own recipe, which is the contract's (8.2).
pub fn reference(image: &Image, frame: &Frame, file_sha256: String) -> ImageReference {
    let (width, height) = image.dimensions();
    ImageReference {
        file_sha256,
        width,
        height,
        projection: projection(frame),
        topology: topology(frame),
        pixel_sha256: Some(bare(&image.record.pixel_digest)),
    }
}

/// `sha256:<hex>` to `<hex>`.
pub fn bare(identity: &str) -> String {
    identity
        .strip_prefix("sha256:")
        .unwrap_or(identity)
        .to_string()
}

/// The digest of the file an imported image came from: the `import` record that produced the
/// image (an external image's identity is in its envelope's artifact references). `None` for an image rascal made itself, or imported before rascal recorded it.
pub fn import_file_sha256(records: &[OperationRecord], image_identity: &str) -> Option<String> {
    records
        .iter()
        .rev()
        .filter(|r| r.op == "import")
        .find(|r| {
            r.outputs.values().any(|o| o == image_identity)
                || r.envelope
                    .artifacts
                    .values()
                    .any(|a| a.identity == image_identity)
        })
        .and_then(|r| r.params.get(FILE_SHA256))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// The reference for an image in a workspace (8.1): an imported image names the caller's file,
/// so a reference handed in by a caller matches; an image rascal made names the PNG it wrote.
pub fn image_reference(
    workspace: &crate::workspace::Workspace,
    id_or_identity: &str,
) -> crate::error::Result<ImageReference> {
    use crate::artifact::Artifact;
    use crate::error::Error;
    let generation = workspace.snapshot()?;
    let entry = generation
        .artifact(id_or_identity)
        .ok_or_else(|| Error::usage(format!("no artifact {id_or_identity} in the workspace")))?
        .clone();
    let Artifact::Image(image) = workspace.load(&entry.id)? else {
        return Err(Error::usage(format!("{} is not an image", entry.id)));
    };
    let frame = generation
        .frames
        .iter()
        .find(|f| f.id == entry.frame)
        .cloned()
        .ok_or_else(|| Error::usage(format!("frame {} is not in the workspace", entry.frame)))?;
    let file = match import_file_sha256(&workspace.records()?, &entry.identity) {
        Some(digest) => digest,
        None => {
            let path = workspace.root.join(&entry.path);
            file_sha256(&std::fs::read(&path).map_err(|e| Error::io(&path, e))?)
        }
    };
    Ok(reference(&image, &frame, file))
}
