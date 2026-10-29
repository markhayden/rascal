//! The commit protocol (SPEC.md "Workspace"): staged outputs and an operation record become a
//! new registry generation in one sequence whose sync order makes everything `CURRENT` names
//! durable before `CURRENT` is.
//!
//! 1. sync every staged file and every directory of the staged trees;
//! 2. take the writer lock;
//! 3. read `CURRENT` and the generation, allocate ids and the sequence number, write the
//!    record with its sequence into staging and sync it;
//! 4. rename artifacts into `artifacts/` and `images/` and the record into `ops/` (an
//!    existing identical identity is left in place); sync those directories;
//! 5. write `registry/<n+1>.json` with the new entries, sync it, sync `registry/`;
//! 6. write the pointer to a temporary file, sync it, rename it over `CURRENT`, sync the
//!    workspace directory;
//! 7. release the lock; the caller appends to the journal.
//!
//! Visibility is the rename; durability is acknowledged only after the final sync, and the op
//! reports `committed` only then. A crash before the pointer rename publishes nothing: the
//! files under `artifacts/`, `images/` and `ops/` are unreachable orphans that `repair`
//! collects.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use crate::artifact::ArtifactType;
use crate::error::{Error, Result};
use crate::frame::Frame;
use crate::location::{Location, LocationEntry};
use crate::workspace::Workspace;
use crate::workspace::record::OperationRecord;
use crate::workspace::registry::{ArtifactEntry, Generation};
use crate::workspace::sync::{Fs, sync_tree};

/// One output waiting to be published.
#[derive(Debug, Clone)]
pub struct Pending {
    /// The output port it came from.
    pub port: String,
    /// The id prefix (`img`, `mask`, `band`, `series`, `loc`).
    pub prefix: &'static str,
    pub artifact_type: ArtifactType,
    pub identity: String,
    pub frame: String,
    /// The staged file or directory and its destination relative to the root; `None` for a
    /// location, which is a registry entry.
    pub staged: Option<(PathBuf, String)>,
    /// A sidecar that moves with the file (an image's record).
    pub sidecar: Option<(PathBuf, String)>,
    /// The location, for a location.
    pub location: Option<Location>,
}

/// The outcome of a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    /// The new generation number.
    pub generation: u64,
    /// The sequence number the record committed at.
    pub seq: u64,
}

/// Runs the commit sequence and returns the commit and the local ids by output port, or
/// `None` when cancellation was requested before the writer lock: nothing is published.
#[allow(clippy::too_many_arguments)]
pub fn commit(
    ws: &Workspace,
    fs: &dyn Fs,
    staging_dir: &Path,
    pending: &[Pending],
    frames: &[Frame],
    record: &mut OperationRecord,
    cancel: Option<&crate::workspace::Cancel>,
) -> Result<Option<(Committed, BTreeMap<String, String>)>> {
    // 1. Everything staged is durable before anything moves.
    sync_tree(fs, staging_dir)?;

    // Cancellation is honoured here and nowhere later: once the lock is held the commit runs
    // to completion whatever the flag says.
    if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::SeqCst)) {
        return Ok(None);
    }

    // 2. The writer lock, held only across the commit.
    let lock_path = ws.root.join("lock");
    let lock = File::create(&lock_path).map_err(|e| Error::io(&lock_path, e))?;
    lock.lock().map_err(|e| Error::io(&lock_path, e))?;
    let result = commit_locked(ws, fs, staging_dir, pending, frames, record);
    let _ = lock.unlock();
    let committed = result?;

    // 7. The staging directory has nothing left of value.
    let _ = fs.remove_dir_all(staging_dir);
    Ok(Some(committed))
}

fn commit_locked(
    ws: &Workspace,
    fs: &dyn Fs,
    staging_dir: &Path,
    pending: &[Pending],
    frames: &[Frame],
    record: &mut OperationRecord,
) -> Result<(Committed, BTreeMap<String, String>)> {
    // 3. The generation under the lock: ids and the sequence number are allocated here, so two
    // concurrent ops cannot take the same ones.
    let current = ws.read_current()?;
    let mut generation = match current {
        Some(n) => {
            let mut g = ws.read_generation(n)?;
            g.n += 1;
            g
        }
        None => Generation::first(),
    };
    let seq = generation.records.len() as u64 + 1;
    record.seq = seq;
    let record_staged = staging_dir.join("record.json");
    fs.write(
        &record_staged,
        serde_json::to_string_pretty(&*record)
            .map_err(|source| Error::Json {
                path: record_staged.clone(),
                source,
            })?
            .as_bytes(),
    )?;
    fs.sync_file(&record_staged)?;

    let mut ids: BTreeMap<String, String> = BTreeMap::new();
    let mut new_entries: Vec<ArtifactEntry> = Vec::new();
    let mut new_locations: Vec<LocationEntry> = Vec::new();
    let mut touched_dirs: Vec<PathBuf> = Vec::new();
    let mut touch = |dir: &Path| {
        if !touched_dirs.iter().any(|d| d == dir) {
            touched_dirs.push(dir.to_path_buf());
        }
    };

    for item in pending {
        if let Some(location) = &item.location {
            let id = match generation.location(&item.identity) {
                Some(existing) => existing.id.clone(),
                None => {
                    let id = generation.allocate(item.prefix);
                    new_locations.push(LocationEntry {
                        id: id.clone(),
                        identity: item.identity.clone(),
                        location: location.clone(),
                    });
                    id
                }
            };
            ids.insert(item.port.clone(), id);
            continue;
        }
        let Some((staged, destination)) = &item.staged else {
            continue;
        };
        // 4. Into place, an identical identity already there left alone.
        let final_path = ws.root.join(destination);
        if final_path.exists() {
            if staged.is_dir() {
                let _ = fs.remove_dir_all(staged);
            } else {
                let _ = fs.remove_file(staged);
            }
        } else {
            fs.rename(staged, &final_path)?;
        }
        if let Some(parent) = final_path.parent() {
            touch(parent);
        }
        if let Some((sidecar, sidecar_destination)) = &item.sidecar {
            let final_sidecar = ws.root.join(sidecar_destination);
            if final_sidecar.exists() {
                let _ = fs.remove_file(sidecar);
            } else {
                fs.rename(sidecar, &final_sidecar)?;
            }
        }
        let id = match generation.artifact(&item.identity) {
            Some(existing) => existing.id.clone(),
            None => {
                let id = generation.allocate(item.prefix);
                new_entries.push(ArtifactEntry {
                    id: id.clone(),
                    artifact_type: item.artifact_type,
                    identity: item.identity.clone(),
                    frame: item.frame.clone(),
                    path: destination.clone(),
                    seq,
                });
                id
            }
        };
        ids.insert(item.port.clone(), id);
    }
    let record_final = ws
        .root
        .join("ops")
        .join(format!("{}.json", record.identity().hex()));
    if record_final.exists() {
        let _ = fs.remove_file(&record_staged);
    } else {
        fs.rename(&record_staged, &record_final)?;
    }
    touch(&ws.root.join("ops"));
    for dir in &touched_dirs {
        fs.sync_dir(dir)?;
    }

    // 5. The new generation.
    for frame in frames {
        if generation.frame(&frame.id).is_none() {
            generation.frames.push(frame.clone());
        }
    }
    generation.locations.extend(new_locations);
    generation.artifacts.extend(new_entries);
    generation.records.push(record.identity.clone());
    let generation_path = ws
        .root
        .join("registry")
        .join(format!("{}.json", generation.n));
    fs.write(
        &generation_path,
        serde_json::to_string_pretty(&generation)
            .map_err(|source| Error::Json {
                path: generation_path.clone(),
                source,
            })?
            .as_bytes(),
    )?;
    fs.sync_file(&generation_path)?;
    fs.sync_dir(&ws.root.join("registry"))?;

    // 6. The pointer.
    let pointer_tmp = ws.root.join("CURRENT.tmp");
    fs.write(&pointer_tmp, format!("{}\n", generation.n).as_bytes())?;
    fs.sync_file(&pointer_tmp)?;
    fs.rename(&pointer_tmp, &ws.root.join("CURRENT"))?;
    fs.sync_dir(&ws.root)?;

    Ok((
        Committed {
            generation: generation.n,
            seq,
        },
        ids,
    ))
}
