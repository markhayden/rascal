//! The three kinds of `--out` (SPEC.md "Commands"): a run directory must be new or empty; a
//! file destination is written over, as `cp` would; a lab directory is resumable. One helper
//! so every command agrees. The lab kind arrives with M6.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// What a command's `--out` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutKind {
    /// A directory that must be new or empty (`run`, `heal`, `inspect --out`, `dash --out <dir>`).
    RunDir,
    /// A file that is overwritten (`render`, `put`, `paste`, `export`, `dash --out <file>`).
    File,
}

/// Prepares `path` for the kind: creates a run directory (refusing a non-empty one with a
/// usage error) or the parent of a file destination. Returns the path.
pub fn prepare(path: &Path, kind: OutKind) -> Result<PathBuf> {
    match kind {
        OutKind::RunDir => {
            if path.exists() {
                if !path.is_dir() {
                    return Err(Error::usage(format!(
                        "--out {}: exists and is not a directory",
                        path.display()
                    )));
                }
                let mut entries = std::fs::read_dir(path).map_err(|e| Error::io(path, e))?;
                if entries.next().is_some() {
                    return Err(Error::usage(format!(
                        "--out {}: a run directory must be new or empty",
                        path.display()
                    )));
                }
            } else {
                std::fs::create_dir_all(path).map_err(|e| Error::io(path, e))?;
            }
        }
        OutKind::File => {
            if path.is_dir() {
                return Err(Error::usage(format!(
                    "--out {}: is a directory, expected a file",
                    path.display()
                )));
            }
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
        }
    }
    Ok(path.to_path_buf())
}
