//! The lab's three files (SPEC.md "The lab"): the corpus manifest, the protocol frozen before
//! a run, and the matrix of images and arms. Each is a boundary type with a generated schema
//! under `schemas/rascal/`; `matrix::validate` checks a matrix against its protocol, its
//! corpus and the recipes its arms name, including the dependent-finder rejection. The
//! commands (`rascal corpus verify`, `rascal lab …`) live in the binary crate; `cell` holds
//! what `lab run` writes.

pub mod cell;
pub mod corpus;
pub mod matrix;
pub mod protocol;

use std::path::Path;

use crate::Error;
use crate::error::Result;

/// The `sha256:<hex>` of a file's bytes: how a protocol and a corpus image are identified.
pub fn file_digest(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(bytes_digest(&bytes))
}

/// The `sha256:<hex>` of bytes.
pub fn bytes_digest(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!(
        "sha256:{}",
        crate::canonical::to_hex(&sha2::Sha256::digest(bytes))
    )
}
