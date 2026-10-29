//! `rascal export --compat <journal.jsonl> --step <id> --out <file>` and
//! `rascal export --compat --location <l.json> --out <file>`: the projection onto the copied
//! schemas, validated before it is written.

use std::path::{Path, PathBuf};

use rascal_core::Error;
use rascal_core::error::Result;
use rascal_core::export;
use rascal_core::frame::FrameTable;
use rascal_core::journal::Journal;
use rascal_core::location::Location;
use rascal_core::outdir::{self, OutKind};
use rascal_core::schemas::SchemaDir;
use rascal_core::workspace::Workspace;

/// The schema directory: beside the binary's source in a checkout, or `RASCAL_SCHEMAS`.
fn schemas() -> Result<SchemaDir> {
    if let Some(dir) = std::env::var_os("RASCAL_SCHEMAS") {
        return Ok(SchemaDir::new(PathBuf::from(dir)));
    }
    let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    if here.is_dir() {
        return Ok(SchemaDir::new(here));
    }
    Err(Error::usage(
        "the schema directory was not found; set RASCAL_SCHEMAS to a checkout's schemas/ directory",
    ))
}

/// Runs the command and returns the written path.
pub fn run(
    journal: &Option<PathBuf>,
    step: Option<&str>,
    location: &Option<PathBuf>,
    compat: bool,
    out: &Path,
) -> Result<PathBuf> {
    if !compat {
        return Err(Error::usage("export: the only form in v1 is --compat"));
    }
    let schemas = schemas()?;
    let out = outdir::prepare(out, OutKind::File)?;
    let value = match (journal, location) {
        (Some(journal), None) => {
            let step =
                step.ok_or_else(|| Error::usage("export: give --step <id> with a journal"))?;
            let lines = Journal::read(journal)?;
            let line = lines
                .iter()
                .rev()
                .find(|l| l.step.as_deref() == Some(step))
                .ok_or_else(|| {
                    Error::usage(format!("export: no step {step} in {}", journal.display()))
                })?;
            let envelope = line
                .envelope
                .clone()
                .ok_or_else(|| Error::usage(format!("export: step {step} has no envelope")))?;
            // The journal's workspace, for the frames an image mapping needs.
            let frames = match journal.parent().and_then(|p| Workspace::open(p).ok()) {
                Some(ws) => ws.frames()?,
                None => FrameTable::default(),
            };
            let projected = export::project_envelope(&envelope, &frames)?;
            export::validate_envelope(&schemas, &projected)?;
            projected
        }
        (None, Some(path)) => {
            let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
            let location: Location = serde_json::from_str(&text)
                .map_err(|e| Error::usage(format!("{}: not a location: {e}", path.display())))?;
            let projected = export::project_location(&location)?;
            export::validate_location(&schemas, &projected)?;
            projected
        }
        _ => {
            return Err(Error::usage(
                "export: give a journal with --step, or --location",
            ));
        }
    };
    std::fs::write(
        &out,
        serde_json::to_string_pretty(&value).unwrap_or_default() + "\n",
    )
    .map_err(|e| Error::io(&out, e))?;
    Ok(out)
}
