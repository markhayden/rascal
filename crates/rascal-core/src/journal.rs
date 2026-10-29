//! The journal: one JSON line per invocation, publishing or not, successful or not. The
//! diagnostics source; never the only copy of anything, since `workspace repair` rebuilds it
//! as a projection of the committed operation records.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::op::Envelope;

/// What a line records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    /// An op invocation.
    Op,
    /// Something assumed or decided outside an op, such as a projection assumed from the
    /// image's aspect.
    Note,
}

/// One journal line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JournalLine {
    /// The line's version.
    pub journal_version: u32,
    /// The line's position in this journal, from 1.
    pub seq: u64,
    pub event: Event,
    /// The step id, for an op.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    /// The op name and version, for an op.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The resolved parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    /// Input identities by port.
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
    /// Output identities by port.
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<Envelope>,
    /// Wall time of the invocation; an observation, never part of an identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// The text of a note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// An append-only `journal.jsonl`. Several processes may append to one journal (the spec's
/// two-shells scenario), so every append takes the file's own lock, assigns the sequence
/// number from what is in the file, and writes the line in one call.
pub struct Journal {
    path: std::path::PathBuf,
    /// How much of the file has been scanned for line counting.
    scanned_bytes: u64,
    /// The sequence number of the next line, as of `scanned_bytes`.
    next_seq: u64,
}

impl Journal {
    /// Opens (creating) the journal at `path`, continuing its sequence.
    pub fn open(path: &Path) -> Result<Journal> {
        let mut journal = Journal {
            path: path.to_path_buf(),
            scanned_bytes: 0,
            next_seq: 1,
        };
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)
            .map_err(|e| Error::io(path, e))?;
        journal.catch_up(&file)?;
        Ok(journal)
    }

    /// Counts the lines appended since the last scan, by this or any other process.
    fn catch_up(&mut self, mut file: &std::fs::File) -> Result<()> {
        use std::io::{Read, Seek, SeekFrom};
        file.seek(SeekFrom::Start(self.scanned_bytes))
            .map_err(|e| Error::io(&self.path, e))?;
        let mut tail = Vec::new();
        file.read_to_end(&mut tail)
            .map_err(|e| Error::io(&self.path, e))?;
        self.next_seq += tail.iter().filter(|b| **b == b'\n').count() as u64;
        self.scanned_bytes += tail.len() as u64;
        Ok(())
    }

    /// Appends a line, assigning its sequence number, and returns it.
    pub fn append(&mut self, mut line: JournalLine) -> Result<JournalLine> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&self.path)
            .map_err(|e| Error::io(&self.path, e))?;
        file.lock().map_err(|e| Error::io(&self.path, e))?;
        let result = self.append_locked(&file, &mut line);
        let _ = file.unlock();
        result?;
        Ok(line)
    }

    fn append_locked(&mut self, mut file: &std::fs::File, line: &mut JournalLine) -> Result<()> {
        self.catch_up(file)?;
        line.seq = self.next_seq;
        line.journal_version = 1;
        let mut text = serde_json::to_string(line).map_err(|source| Error::Json {
            path: self.path.clone(),
            source,
        })?;
        text.push('\n');
        file.write_all(text.as_bytes())
            .map_err(|e| Error::io(&self.path, e))?;
        self.next_seq += 1;
        self.scanned_bytes += text.len() as u64;
        Ok(())
    }

    /// A note line.
    pub fn note(&mut self, text: impl Into<String>) -> Result<JournalLine> {
        self.append(JournalLine {
            journal_version: 1,
            seq: 0,
            event: Event::Note,
            step: None,
            op: None,
            version: None,
            params: None,
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            envelope: None,
            duration_ms: None,
            note: Some(text.into()),
        })
    }

    /// Reads every line of a journal. A complete line always ends with a newline, so an
    /// unterminated tail is the torn last line a crash mid-append leaves: it is skipped, and
    /// `repair` rebuilds the journal from the records.
    pub fn read(path: &Path) -> Result<Vec<JournalLine>> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        let complete = match text.rfind('\n') {
            Some(at) => &text[..=at],
            None => "",
        };
        complete
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                serde_json::from_str(l).map_err(|source| Error::Json {
                    path: path.to_path_buf(),
                    source,
                })
            })
            .collect()
    }
}
