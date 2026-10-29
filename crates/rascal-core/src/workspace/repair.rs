//! `rascal workspace repair`: what a crash or a kill can leave behind, put right. Abandoned
//! staging directories are removed; files under `images/`, `artifacts/` and `ops/` that the
//! current generation does not name are orphans and are removed; an entry whose payload is
//! missing on disk is dropped from a new, repaired generation (its record stays); and the
//! journal is rebuilt as a projection of the committed records, in commit order, with a first
//! line saying it was rebuilt.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::journal::{Event, Journal, JournalLine};
use crate::workspace::sync::Fs;
use crate::workspace::{Workspace, staging};

/// What repair did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub abandoned_staging_removed: Vec<String>,
    pub orphans_removed: Vec<String>,
    /// Entries dropped because their payload was missing: `id (identity)`.
    pub missing_dropped: Vec<String>,
    /// The generation written when entries were dropped.
    pub repaired_generation: Option<u64>,
    /// Lines in the rebuilt journal.
    pub journal_lines: usize,
}

impl Workspace {
    /// Repairs the workspace at `root`. Opens it without the missing-payload refusal.
    pub fn repair(root: &Path) -> Result<Report> {
        let ws = Workspace::open_unchecked(root)?;
        ws.repair_in_place()
    }

    /// Repairs this workspace.
    pub fn repair_in_place(&self) -> Result<Report> {
        let mut report = Report::default();

        // Abandoned staging.
        let staging_root = self.root.join("staging");
        if let Ok(entries) = std::fs::read_dir(&staging_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && staging::is_abandoned(&path) {
                    std::fs::remove_dir_all(&path).map_err(|e| Error::io(&path, e))?;
                    report.abandoned_staging_removed.push(name_of(&path));
                }
            }
        }

        // The registry against the disk: drop what is missing, in a new generation.
        let mut generation = self.snapshot()?;
        let missing: Vec<usize> = generation
            .artifacts
            .iter()
            .enumerate()
            .filter(|(_, a)| !self.root.join(&a.path).exists())
            .map(|(i, _)| i)
            .collect();
        if !missing.is_empty() {
            for i in missing.iter().rev() {
                let entry = generation.artifacts.remove(*i);
                report
                    .missing_dropped
                    .push(format!("{} ({})", entry.id, entry.identity));
            }
            generation.n += 1;
            let path = self
                .root
                .join("registry")
                .join(format!("{}.json", generation.n));
            let fs = self.fs();
            fs.write(
                &path,
                serde_json::to_string_pretty(&generation)
                    .map_err(|source| Error::Json {
                        path: path.clone(),
                        source,
                    })?
                    .as_bytes(),
            )?;
            fs.sync_file(&path)?;
            fs.sync_dir(&self.root.join("registry"))?;
            let tmp = self.root.join("CURRENT.tmp");
            fs.write(&tmp, format!("{}\n", generation.n).as_bytes())?;
            fs.sync_file(&tmp)?;
            fs.rename(&tmp, &self.root.join("CURRENT"))?;
            fs.sync_dir(&self.root)?;
            report.repaired_generation = Some(generation.n);
        }

        // Orphans: anything under images/, artifacts/ and ops/ the generation does not name.
        let mut named: BTreeSet<PathBuf> = BTreeSet::new();
        for a in &generation.artifacts {
            named.insert(self.root.join(&a.path));
            named.insert(PathBuf::from(format!(
                "{}.json",
                self.root.join(&a.path).display()
            )));
        }
        for identity in &generation.records {
            let hex = identity.strip_prefix("sha256:").unwrap_or(identity);
            named.insert(self.root.join("ops").join(format!("{hex}.json")));
        }
        for dir in ["images", "artifacts", "ops"] {
            let dir = self.root.join(dir);
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !named.contains(&path) {
                        if path.is_dir() {
                            std::fs::remove_dir_all(&path).map_err(|e| Error::io(&path, e))?;
                        } else {
                            std::fs::remove_file(&path).map_err(|e| Error::io(&path, e))?;
                        }
                        report.orphans_removed.push(
                            path.strip_prefix(&self.root)
                                .unwrap_or(&path)
                                .display()
                                .to_string(),
                        );
                    }
                }
            }
        }

        // The journal, rebuilt from the records.
        let records = self.records()?;
        let journal_path = self.journal_path();
        let rebuilt = journal_path.with_extension("jsonl.rebuilding");
        let _ = std::fs::remove_file(&rebuilt);
        let mut journal = Journal::open(&rebuilt)?;
        journal.note(format!(
            "journal rebuilt by repair from {} committed records at generation {}; only committed invocations are here",
            records.len(),
            generation.n
        ))?;
        for record in &records {
            journal.append(JournalLine {
                journal_version: 1,
                seq: 0,
                event: Event::Op,
                step: Some(record.step.clone()),
                op: Some(record.op.clone()),
                version: Some(record.version.clone()),
                params: Some(record.params.clone()),
                inputs: record.inputs.clone(),
                outputs: record
                    .envelope
                    .artifacts
                    .iter()
                    .map(|(k, a)| (k.clone(), a.identity.clone()))
                    .collect(),
                envelope: Some(record.envelope.clone()),
                duration_ms: Some(record.duration_ms),
                note: None,
            })?;
        }
        let fs = self.fs();
        fs.sync_file(&rebuilt)?;
        fs.rename(&rebuilt, &journal_path)?;
        fs.sync_dir(&self.root)?;
        report.journal_lines = records.len() + 1;
        Ok(report)
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The filesystem a workspace talks to, for repair's own writes.
impl Workspace {
    pub(crate) fn fs(&self) -> std::sync::Arc<dyn Fs> {
        self.fs_handle()
    }
}
