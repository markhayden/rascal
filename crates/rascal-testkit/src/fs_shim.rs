//! A recording filesystem for the commit protocol's tests. It performs every operation on the
//! real filesystem, records the sequence of writes, syncs and renames, and can crash at a
//! chosen point: from then on every call fails, and what was written or renamed but never
//! synced is dropped, as a power loss would drop it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rascal_core::error::{Error, Result};
use rascal_core::workspace::sync::{Fs, RealFs};

/// One recorded filesystem event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Write(PathBuf),
    Append(PathBuf),
    SyncFile(PathBuf),
    SyncDir(PathBuf),
    Rename(PathBuf, PathBuf),
    RemoveFile(PathBuf),
    RemoveDirAll(PathBuf),
}

impl Event {
    /// A short name for assertions on order.
    pub fn kind(&self) -> &'static str {
        match self {
            Event::Write(_) => "write",
            Event::Append(_) => "append",
            Event::SyncFile(_) => "sync_file",
            Event::SyncDir(_) => "sync_dir",
            Event::Rename(_, _) => "rename",
            Event::RemoveFile(_) => "remove_file",
            Event::RemoveDirAll(_) => "remove_dir_all",
        }
    }

    /// The path the event is about (the destination, for a rename).
    pub fn path(&self) -> &Path {
        match self {
            Event::Write(p)
            | Event::Append(p)
            | Event::SyncFile(p)
            | Event::SyncDir(p)
            | Event::RemoveFile(p)
            | Event::RemoveDirAll(p) => p,
            Event::Rename(_, to) => to,
        }
    }
}

/// Where to crash: before the event with this index (0-based) in the recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrashAt(pub usize);

#[derive(Default)]
struct State {
    events: Vec<Event>,
    /// Files written or appended since their last sync.
    dirty_files: BTreeSet<PathBuf>,
    /// Files appended since their last sync, with their length before the first unsynced
    /// append: a crash truncates to it rather than removing the file.
    dirty_appends: BTreeMap<PathBuf, u64>,
    /// Renames whose containing directory has not been synced since: `(from, to)`.
    dirty_renames: Vec<(PathBuf, PathBuf, Option<Vec<u8>>)>,
    crash_at: Option<usize>,
    crashed: bool,
}

/// The recording filesystem.
pub struct RecordingFs {
    real: RealFs,
    state: Mutex<State>,
}

impl Default for RecordingFs {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordingFs {
    /// A recorder that never crashes.
    pub fn new() -> Self {
        RecordingFs {
            real: RealFs,
            state: Mutex::new(State::default()),
        }
    }

    /// A recorder that crashes before event number `n`.
    pub fn crashing_at(n: usize) -> Self {
        let fs = RecordingFs::new();
        fs.state.lock().unwrap_or_else(|e| e.into_inner()).crash_at = Some(n);
        fs
    }

    /// The events so far.
    pub fn events(&self) -> Vec<Event> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .events
            .clone()
    }

    /// True once the crash point was reached.
    pub fn crashed(&self) -> bool {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).crashed
    }

    /// Clears the recording (not the crash point).
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.events.clear();
        state.dirty_files.clear();
        state.dirty_appends.clear();
        state.dirty_renames.clear();
    }

    /// Records an event, or crashes: on the crash every unsynced write is removed and every
    /// unsynced rename undone, then this and every later call fails.
    fn record(&self, event: Event) -> Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.crashed {
            return Err(Error::usage("filesystem crashed"));
        }
        if state.crash_at == Some(state.events.len()) {
            state.crashed = true;
            let appends = std::mem::take(&mut state.dirty_appends);
            for (path, len) in &appends {
                if let Ok(file) = std::fs::OpenOptions::new().write(true).open(path) {
                    let _ = file.set_len(*len);
                }
            }
            let dirty: Vec<PathBuf> = state
                .dirty_files
                .iter()
                .filter(|p| !appends.contains_key(*p))
                .cloned()
                .collect();
            for path in dirty {
                let _ = std::fs::remove_file(&path);
            }
            let renames = std::mem::take(&mut state.dirty_renames);
            for (from, to, replaced) in renames.into_iter().rev() {
                let _ = std::fs::rename(&to, &from);
                if let Some(bytes) = replaced {
                    let _ = std::fs::write(&to, bytes);
                }
            }
            return Err(Error::usage(format!(
                "filesystem crashed before event {}: {event:?}",
                state.events.len()
            )));
        }
        match &event {
            Event::Write(p) => {
                state.dirty_files.insert(p.clone());
            }
            Event::Append(p) => {
                if !state.dirty_files.contains(p) {
                    let len = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                    state.dirty_appends.insert(p.clone(), len);
                }
                state.dirty_files.insert(p.clone());
            }
            Event::SyncFile(p) => {
                state.dirty_files.remove(p);
                state.dirty_appends.remove(p);
            }
            Event::SyncDir(dir) => {
                let dir = dir.clone();
                state
                    .dirty_renames
                    .retain(|(_, to, _)| to.parent() != Some(dir.as_path()));
            }
            Event::Rename(from, to) => {
                // A renamed file keeps its content's sync state under its new name.
                if state.dirty_files.remove(from) {
                    state.dirty_files.insert(to.clone());
                }
                let replaced = std::fs::read(to).ok();
                state
                    .dirty_renames
                    .push((from.clone(), to.clone(), replaced));
            }
            Event::RemoveFile(p) => {
                state.dirty_files.remove(p);
            }
            Event::RemoveDirAll(_) => {}
        }
        state.events.push(event);
        Ok(())
    }
}

impl Fs for RecordingFs {
    fn create_dir_all(&self, path: &Path) -> Result<()> {
        self.real.create_dir_all(path)
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        self.record(Event::Write(path.to_path_buf()))?;
        self.real.write(path, bytes)
    }

    fn append(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        self.record(Event::Append(path.to_path_buf()))?;
        self.real.append(path, bytes)
    }

    fn sync_file(&self, path: &Path) -> Result<()> {
        self.record(Event::SyncFile(path.to_path_buf()))?;
        self.real.sync_file(path)
    }

    fn sync_dir(&self, path: &Path) -> Result<()> {
        self.record(Event::SyncDir(path.to_path_buf()))?;
        self.real.sync_dir(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        self.record(Event::Rename(from.to_path_buf(), to.to_path_buf()))?;
        self.real.rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> Result<()> {
        self.record(Event::RemoveFile(path.to_path_buf()))?;
        self.real.remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> Result<()> {
        self.record(Event::RemoveDirAll(path.to_path_buf()))?;
        self.real.remove_dir_all(path)
    }
}
