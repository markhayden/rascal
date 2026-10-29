//! The lab's commands (SPEC.md "The lab"): `rascal corpus verify`, and the matrix resolved
//! against its protocol, corpus and recipes for `lab run` (M6). The types and their
//! validation are `rascal_core::lab`; `layout` is the lab directory `lab run` writes; `pairs`
//! is the blind pairs page and the decode of its picks.

pub mod layout;
pub mod pairs;

use std::path::{Path, PathBuf};

use rascal_core::Error;
use rascal_core::error::Result;
use rascal_core::lab::corpus::{Corpus, Verification};
use rascal_core::lab::matrix::{Matrix, Validated};
use rascal_core::lab::protocol::Protocol;
use rascal_core::recipe::Recipe;
use rascal_core::registry::OpRegistry;

use crate::recipes::load_recipe;

/// `rascal corpus verify <manifest> [--root <dir>]`: every image's file under the root
/// against its digest, one line per image; any image missing or changed is an operational
/// error after the whole list is printed.
pub fn corpus_verify(manifest: &Path, root: Option<&Path>) -> Result<()> {
    let corpus = Corpus::read(manifest)?;
    let root = Corpus::root(root);
    let mut failed = 0usize;
    for (image, found) in corpus.verify(&root)? {
        match found {
            Verification::Ok => println!("ok       {} ({}, {})", image.id, image.file, image.group),
            Verification::Missing => {
                failed += 1;
                println!(
                    "missing  {} ({})",
                    image.id,
                    root.join(&image.file).display()
                );
            }
            Verification::Mismatch { actual } => {
                failed += 1;
                println!(
                    "changed  {} ({}: {actual}, manifest {})",
                    image.id,
                    root.join(&image.file).display(),
                    image.sha256
                );
            }
        }
    }
    println!(
        "corpus: {} of {} images verified under {}",
        corpus.image.len() - failed,
        corpus.image.len(),
        root.display()
    );
    if failed > 0 {
        return Err(Error::op(
            "corpus",
            format!(
                "{failed} image{} missing or changed",
                if failed == 1 { "" } else { "s" }
            ),
        ));
    }
    Ok(())
}

/// A matrix file with its protocol and corpus read from beside it and validated against the
/// registry; `finders` names the registry's pack tools. Recipes resolve as shipped names or
/// files relative to the matrix.
pub fn resolve_matrix(path: &Path, registry: &OpRegistry, finders: &[&str]) -> Result<Validated> {
    let matrix = Matrix::read(path)?;
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let (protocol, digest) = Protocol::read(&dir.join(&matrix.protocol))?;
    let corpus = Corpus::read(&dir.join(&matrix.corpus))?;
    let load = |name: &str| -> Result<Recipe> {
        let relative: PathBuf = dir.join(name);
        if relative.is_file() {
            Recipe::read(&relative)
        } else {
            load_recipe(name)
        }
    };
    matrix.validate(&protocol, &digest, &corpus, registry, finders, &load)
}
