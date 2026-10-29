//! The filesystem the workspace talks to, as a trait, so a test can record the order of writes,
//! syncs and renames and drop what was never synced. `RealFs` is the real one.
//!
//! Durability (ADR 0017): a file is durable after `sync_file`, and its name in a directory
//! after `sync_dir` on that directory. On Linux both are `fsync`; on macOS `File::sync_all`
//! issues `F_FULLFSYNC`, which asks the drive to flush its cache too.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// What the commit protocol needs from a filesystem.
pub trait Fs: Send + Sync {
    /// Creates a directory and its parents.
    fn create_dir_all(&self, path: &Path) -> Result<()>;
    /// Writes a whole file, replacing any existing content.
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<()>;
    /// Appends to a file, creating it.
    fn append(&self, path: &Path, bytes: &[u8]) -> Result<()>;
    /// Makes a file's content durable.
    fn sync_file(&self, path: &Path) -> Result<()>;
    /// Makes a directory's entries durable.
    fn sync_dir(&self, path: &Path) -> Result<()>;
    /// Renames; atomic within one filesystem.
    fn rename(&self, from: &Path, to: &Path) -> Result<()>;
    /// Removes a file.
    fn remove_file(&self, path: &Path) -> Result<()>;
    /// Removes a directory tree.
    fn remove_dir_all(&self, path: &Path) -> Result<()>;
}

/// The real filesystem.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFs;

impl Fs for RealFs {
    fn create_dir_all(&self, path: &Path) -> Result<()> {
        std::fs::create_dir_all(path).map_err(|e| Error::io(path, e))
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        std::fs::write(path, bytes).map_err(|e| Error::io(path, e))
    }

    fn append(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| Error::io(path, e))?;
        file.write_all(bytes).map_err(|e| Error::io(path, e))
    }

    fn sync_file(&self, path: &Path) -> Result<()> {
        let file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
        file.sync_all().map_err(|e| Error::io(path, e))
    }

    fn sync_dir(&self, path: &Path) -> Result<()> {
        let dir = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
        dir.sync_all().map_err(|e| Error::io(path, e))
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        std::fs::rename(from, to).map_err(|e| Error::io(from, e))
    }

    fn remove_file(&self, path: &Path) -> Result<()> {
        std::fs::remove_file(path).map_err(|e| Error::io(path, e))
    }

    fn remove_dir_all(&self, path: &Path) -> Result<()> {
        std::fs::remove_dir_all(path).map_err(|e| Error::io(path, e))
    }
}

/// Syncs every file in a tree and every directory of it, deepest first; what a staged artifact
/// (a mask is a directory) needs before it is renamed into place.
pub fn sync_tree(fs: &dyn Fs, root: &Path) -> Result<()> {
    if root.is_file() {
        return fs.sync_file(root);
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| Error::io(&dir, e))? {
            let path = entry.map_err(|e| Error::io(&dir, e))?.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                fs.sync_file(&path)?;
            }
        }
        dirs.push(dir);
    }
    for dir in dirs.iter().rev() {
        fs.sync_dir(dir)?;
    }
    Ok(())
}
