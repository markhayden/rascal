//! A staging directory: one per in-flight op, holding a `lock` the owning process keeps locked
//! for the op's lifetime. On open, a staging directory whose lock can be acquired has no owner
//! and is removed; one that cannot is active and left alone.

use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::error::{Error, Result};

/// Staging directories owned by this process. File locks are per open file description, so a
/// second handle in the same process could take a lock this process already holds; the sweep
/// consults this set first.
static LIVE: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// An in-flight op's private directory.
#[derive(Debug)]
pub struct Staging {
    pub dir: PathBuf,
    /// Held locked for the staging's lifetime.
    _lock: File,
}

impl Staging {
    /// Creates `staging/<token>/` with its lock taken.
    pub fn create(staging_root: &Path, token: &str) -> Result<Staging> {
        let dir = staging_root.join(token);
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        let lock_path = dir.join("lock");
        let lock = File::create(&lock_path).map_err(|e| Error::io(&lock_path, e))?;
        lock.try_lock()
            .map_err(|e| Error::usage(format!("{}: staging token in use: {e}", dir.display())))?;
        LIVE.lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(dir.clone());
        Ok(Staging { dir, _lock: lock })
    }

    /// A path inside the staging directory.
    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Removes the directory; the lock goes with it.
    pub fn discard(self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        LIVE.lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.dir);
    }
}

/// True when the staging directory's lock can be taken, which means no process owns it.
pub fn is_abandoned(dir: &Path) -> bool {
    if LIVE.lock().unwrap_or_else(|e| e.into_inner()).contains(dir) {
        return false;
    }
    if let Ok(meta) = std::fs::metadata(dir)
        && let Ok(modified) = meta.modified()
        && modified
            .elapsed()
            .map(|age| age.as_secs() < 5)
            .unwrap_or(true)
    {
        return false;
    }
    let lock_path = dir.join("lock");
    match File::open(&lock_path) {
        Ok(file) => match file.try_lock() {
            Ok(()) => {
                let _ = file.unlock();
                true
            }
            Err(_) => false,
        },
        // No lock file at all: nothing owns it.
        Err(_) => true,
    }
}

/// A token unlikely to collide: process id, a counter and the clock.
pub fn token() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{}-{}-{:x}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        nanos
    )
}
