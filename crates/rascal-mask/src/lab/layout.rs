//! The lab directory (SPEC.md "The lab"): `lab.json` at the root, `cells/<identity>/cell.json`,
//! `cells/<identity>/attempts/<n>/` with the run directory and `attempt.json`, `latest` naming
//! the attempt in use, and `ledger.jsonl` with every external call. Resume reuses a cell only
//! when its identity matches and `latest` is complete; `--redo` adds an attempt and never
//! overwrites one.

use std::io::Write;
use std::path::{Path, PathBuf};

use rascal_core::Error;
use rascal_core::error::Result;
use rascal_core::lab::cell::{AttemptRecord, CellRecord, LabRecord, LedgerLine};

/// A lab directory.
#[derive(Debug, Clone)]
pub struct LabDir {
    pub root: PathBuf,
}

/// The attempt in use for a cell, by number, with its record.
#[derive(Debug, Clone)]
pub struct Latest {
    pub attempt: u32,
    pub record: AttemptRecord,
}

fn io(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(|e| Error::op("lab", e.to_string()))?;
    std::fs::write(path, text + "\n").map_err(|e| io(path, e))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let text = std::fs::read_to_string(path).map_err(|e| io(path, e))?;
    serde_json::from_str(&text).map_err(|source| Error::Json {
        path: path.to_path_buf(),
        source,
    })
}

impl LabDir {
    /// Opens the lab at `root` for `record`: a new or empty directory is created and the
    /// record written; a directory holding a `lab.json` is resumed when its name and
    /// protocol digest agree (a protocol change after a run is a new lab name); any other
    /// non-empty directory is refused. Returns the lab and whether it was resumed.
    pub fn open_or_create(root: &Path, record: &LabRecord) -> Result<(LabDir, bool)> {
        let lab = LabDir {
            root: root.to_path_buf(),
        };
        let path = lab.root.join("lab.json");
        if path.is_file() {
            let existing: LabRecord = read_json(&path)?;
            if existing.name != record.name {
                return Err(Error::usage(format!(
                    "{}: holds lab {}, not {}; one lab name, one directory",
                    root.display(),
                    existing.name,
                    record.name
                )));
            }
            if existing.protocol_digest != record.protocol_digest {
                return Err(Error::usage(format!(
                    "{}: the protocol changed since this lab ran ({} then, {} now); a protocol change after a run is a new lab name",
                    root.display(),
                    existing.protocol_digest,
                    record.protocol_digest
                )));
            }
            return Ok((lab, true));
        }
        match std::fs::read_dir(root).map(|mut entries| entries.next().is_some()) {
            Ok(true) => {
                return Err(Error::usage(format!(
                    "{}: exists, is not empty and holds no lab.json; choose a new directory",
                    root.display()
                )));
            }
            Ok(false) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir_all(root).map_err(|e| io(root, e))?;
            }
            Err(e) => return Err(io(root, e)),
        }
        std::fs::create_dir_all(lab.root.join("cells")).map_err(|e| io(&lab.root, e))?;
        write_json(&path, record)?;
        Ok((lab, false))
    }

    /// `lab.json`.
    pub fn record(&self) -> Result<LabRecord> {
        read_json(&self.root.join("lab.json"))
    }

    /// Records the cells of the run that just finished, by `<arm>/<image>`.
    pub fn set_current(&self, current: std::collections::BTreeMap<String, String>) -> Result<()> {
        let mut record = self.record()?;
        record.current = current;
        write_json(&self.root.join("lab.json"), &record)
    }

    /// `cells/<hex>` for an identity (`sha256:<hex>`).
    pub fn cell_dir(&self, identity: &str) -> PathBuf {
        self.root
            .join("cells")
            .join(identity.strip_prefix("sha256:").unwrap_or(identity))
    }

    /// Writes a cell's record, creating its directory; an existing record must be this one.
    pub fn write_cell(&self, cell: &CellRecord) -> Result<PathBuf> {
        let dir = self.cell_dir(&cell.identity);
        std::fs::create_dir_all(dir.join("attempts")).map_err(|e| io(&dir, e))?;
        let path = dir.join("cell.json");
        if path.is_file() {
            let existing: CellRecord = read_json(&path)?;
            if existing.identity != cell.identity {
                return Err(Error::op(
                    "lab",
                    format!("{}: holds another cell's record", path.display()),
                ));
            }
        } else {
            write_json(&path, cell)?;
        }
        Ok(dir)
    }

    /// Every cell record, in directory order.
    pub fn cells(&self) -> Result<Vec<CellRecord>> {
        let cells = self.root.join("cells");
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&cells) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(io(&cells, e)),
        };
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.join("cell.json").is_file())
            .collect();
        dirs.sort();
        for dir in dirs {
            out.push(read_json(&dir.join("cell.json"))?);
        }
        Ok(out)
    }

    /// The attempt in use for a cell, if any.
    pub fn latest(&self, identity: &str) -> Result<Option<Latest>> {
        let dir = self.cell_dir(identity);
        let path = dir.join("latest");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io(&path, e)),
        };
        let attempt: u32 = text
            .trim()
            .parse()
            .map_err(|_| Error::op("lab", format!("{}: not an attempt number", path.display())))?;
        let record = self.attempt(identity, attempt)?;
        Ok(Some(Latest { attempt, record }))
    }

    /// One attempt's record.
    pub fn attempt(&self, identity: &str, attempt: u32) -> Result<AttemptRecord> {
        read_json(&self.attempt_dir(identity, attempt).join("attempt.json"))
    }

    /// `cells/<hex>/attempts/<n>`.
    pub fn attempt_dir(&self, identity: &str, attempt: u32) -> PathBuf {
        self.cell_dir(identity)
            .join("attempts")
            .join(attempt.to_string())
    }

    /// The next attempt number: one above the highest present, never reusing a directory.
    pub fn next_attempt(&self, identity: &str) -> Result<u32> {
        let attempts = self.cell_dir(identity).join("attempts");
        let mut highest = 0u32;
        if let Ok(entries) = std::fs::read_dir(&attempts) {
            for entry in entries.flatten() {
                if let Some(n) = entry
                    .file_name()
                    .to_str()
                    .and_then(|s| s.parse::<u32>().ok())
                {
                    highest = highest.max(n);
                }
            }
        }
        Ok(highest + 1)
    }

    /// Writes an attempt's record and points `latest` at it.
    pub fn finish_attempt(&self, identity: &str, record: &AttemptRecord) -> Result<()> {
        let dir = self.attempt_dir(identity, record.attempt);
        std::fs::create_dir_all(&dir).map_err(|e| io(&dir, e))?;
        write_json(&dir.join("attempt.json"), record)?;
        let latest = self.cell_dir(identity).join("latest");
        std::fs::write(&latest, format!("{}\n", record.attempt)).map_err(|e| io(&latest, e))
    }

    /// Appends one external call to the ledger.
    pub fn ledger_append(&self, line: &LedgerLine) -> Result<()> {
        let path = self.root.join("ledger.jsonl");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| io(&path, e))?;
        let text = serde_json::to_string(line).map_err(|e| Error::op("lab", e.to_string()))?;
        writeln!(file, "{text}").map_err(|e| io(&path, e))
    }

    /// Every ledger line.
    pub fn ledger(&self) -> Result<Vec<LedgerLine>> {
        let path = self.root.join("ledger.jsonl");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io(&path, e)),
        };
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                serde_json::from_str(l).map_err(|source| Error::Json {
                    path: path.clone(),
                    source,
                })
            })
            .collect()
    }
}
