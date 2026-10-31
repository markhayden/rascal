//! `run.json`: the contract's result header and rascal's body, written atomically and last into
//! every run directory (table-manners 2 and 11.1; ADR 0020). `run` and `heal` call [`finish`]
//! with however far they got; a signal calls it through [`Pending`] with status `cancelled`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rascal_core::Error;
use rascal_core::contract::table_manners::images::{
    ImageOut, ImageReference, ImageTaken, Images, Location as TmLocation, Locations, Locator,
    Projection as TmProjection, Source, Topology as TmTopology,
};
use rascal_core::contract::table_manners::{
    Cache, ErrorCode, ErrorInfo, Run, RunResult, Spend, SpendBasis, Status, Tool,
};
use rascal_core::contract::{self, LocationMeta, location_out};
use serde_json::Value;

use rascal_mask::describe::{SEGMENT_SCORE_KIND, action_name};

/// The file's name in a run directory.
pub const FILE: &str = "run.json";

/// A run's id: unique per process and millisecond, readable in a listing.
pub fn new_id(started_ms: u128) -> String {
    format!("run-{started_ms}-{}", std::process::id())
}

/// Milliseconds since the epoch.
pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// RFC 3339 in UTC with milliseconds, from epoch milliseconds.
pub fn rfc3339(ms: u128) -> String {
    let secs = (ms / 1000) as i64;
    let millis = (ms % 1000) as u32;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}.{millis:03}Z")
}

/// What is known about a run before it ends: enough to write `run.json` on a signal.
#[derive(Debug, Clone)]
pub struct Pending {
    pub id: String,
    pub action: String,
    pub dir: PathBuf,
    /// Where the run's workspace is (the run directory unless `--workspace` named another).
    pub workspace: PathBuf,
    /// Whether the run directory is the caller's to write into ([`fresh`]).
    pub fresh: bool,
    pub started_ms: u128,
    pub input_identity: String,
    pub images_in: Vec<ImageTaken>,
    pub locations_used: Vec<TmLocation>,
}

/// The run a signal would cancel, if any.
pub static CURRENT: Mutex<Option<Pending>> = Mutex::new(None);

/// The action a recipe argument names: a shipped recipe's action, else `custom`.
pub fn action_of(recipe: &str) -> String {
    if rascal_mask::recipes::SHIPPED
        .iter()
        .any(|(n, _)| *n == recipe)
    {
        action_name(recipe)
    } else {
        "custom".to_string()
    }
}

/// The input identity (2.3): a digest over what was asked, so equal identities ran the same
/// thing. Its recipe is rascal's: the action, the recipe text, the image's file digest, the
/// location files' bytes, the overrides, the prompt and the filler.
pub fn input_identity(parts: &Value) -> String {
    contract::file_sha256(serde_json::to_string(parts).unwrap_or_default().as_bytes())
}

/// The contract's error for a rascal error (2.2): its status, code and whether to retry.
pub fn error_of(error: &Error) -> (Status, ErrorInfo) {
    let (status, code, retryable) = match error {
        Error::Usage { .. } | Error::InvalidArgs { .. } | Error::Recipe { .. } => {
            let input = matches!(error, Error::Usage { message } if
                message.contains("another image") || message.contains("not a table-manners") ||
                message.contains("rascal holds"));
            (
                Status::Invalid,
                if input {
                    ErrorCode::Input
                } else {
                    ErrorCode::Usage
                },
                false,
            )
        }
        Error::Io { .. } => (Status::Error, ErrorCode::Io, true),
        Error::Budget { .. } => (Status::Error, ErrorCode::Budget, false),
        Error::Image { .. } => (Status::Invalid, ErrorCode::Input, false),
        other => {
            let text = other.to_string();
            if text.contains("timed out") || text.contains("timeout") {
                (Status::Error, ErrorCode::Timeout, true)
            } else if text.contains("pack") || text.contains("filler command") {
                (Status::Error, ErrorCode::ToolProcess, true)
            } else if text.contains("filler") || text.contains("HTTP") || text.contains("http") {
                (Status::Error, ErrorCode::Provider, true)
            } else {
                (Status::Error, ErrorCode::Internal, false)
            }
        }
    };
    (
        status,
        ErrorInfo {
            code,
            retryable,
            reason: error.to_string(),
        },
    )
}

/// Whether a run directory is the caller's to write `run.json` into: it did not exist, or was
/// empty, when the run began (an `--out` that held something else is refused untouched).
pub fn fresh(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => true,
    }
}

/// Registers a run a signal may cancel, and returns it.
pub fn begin(pending: Pending) -> Pending {
    if let Ok(mut current) = CURRENT.lock() {
        *current = Some(pending.clone());
    }
    pending
}

/// How a run ended, as far as the command knows.
pub struct Ended {
    pub status: Status,
    pub error: Option<ErrorInfo>,
    /// The renders by profile (`grey` is the mask's).
    pub renders: Vec<(String, PathBuf)>,
    /// The healed image, for `heal`.
    pub result: Option<PathBuf>,
    pub body: serde_json::Map<String, Value>,
    /// What the run spent, by the filler's configured price.
    pub spend_usd: f64,
    /// The fill cache: `Some(true)` a hit, `Some(false)` a miss, `None` not consulted.
    pub cache: Option<bool>,
}

/// An output file's reference: its bytes' digest and its header's dimensions; the input's
/// projection and topology when it is the input's size, else a flat bounded picture.
fn output_reference(path: &Path, input: Option<&ImageReference>) -> Option<ImageReference> {
    let bytes = std::fs::read(path).ok()?;
    let (width, height) = image::image_dimensions(path).ok()?;
    let (projection, topology) = match input {
        Some(i) if i.width == width && i.height == height => (i.projection, i.topology),
        _ => (TmProjection::Flat, TmTopology::Bounded),
    };
    Some(ImageReference {
        file_sha256: contract::file_sha256(&bytes),
        width,
        height,
        projection,
        topology,
        pixel_sha256: None,
    })
}

fn absolute(path: &Path) -> String {
    std::path::absolute(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

/// The run's input image, the locations it took in and found, and its notes, from the
/// workspace when the run got as far as making one.
fn from_workspace(
    workspace_dir: &Path,
) -> (
    Option<ImageReference>,
    Vec<TmLocation>,
    Vec<TmLocation>,
    Vec<String>,
) {
    let mut notes = Vec::new();
    if let Ok(lines) = rascal_core::journal::Journal::read(&workspace_dir.join("journal.jsonl")) {
        notes.extend(lines.into_iter().filter_map(|l| l.note));
    }
    let Ok(ws) = rascal_core::workspace::Workspace::open(workspace_dir) else {
        return (None, vec![], vec![], notes);
    };
    let records = ws.records().unwrap_or_default();
    let identity_of = |r: &rascal_core::workspace::record::OperationRecord, port: &str| {
        r.envelope.artifacts.get(port).map(|a| a.identity.clone())
    };
    let input = records
        .iter()
        .find(|r| r.op == "import")
        .and_then(|r| identity_of(r, "image"))
        .and_then(|id| contract::image_reference(&ws, &id).ok());
    let source = || Source {
        tool: "rascal".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        run: None,
        item: None,
    };
    let mut used = Vec::new();
    let mut found = Vec::new();
    if let Some(reference) = &input {
        for r in records.iter().filter(|r| r.op == "locate") {
            let Some(id) = identity_of(r, "location") else {
                continue;
            };
            if let Ok(rascal_core::artifact::Artifact::Location { location, .. }) = ws.load(&id)
                && let Ok(l) = location_out(&[location], reference.clone(), LocationMeta::default())
            {
                used.push(l);
            }
        }
        for r in records.iter().filter(|r| r.op == "segment") {
            for region in &r.envelope.regions {
                let meta = LocationMeta {
                    label: Some(region.label.clone()),
                    score: Some(region.score.clamp(0.0, 1.0)),
                    score_kind: Some(SEGMENT_SCORE_KIND.to_string()),
                    reason: None,
                    source: Some(Source {
                        item: Some(region.id.clone()),
                        ..source()
                    }),
                };
                if let Ok(l) = location_out(
                    std::slice::from_ref(&region.location),
                    reference.clone(),
                    meta,
                ) {
                    found.push(l);
                }
            }
        }
    }
    (input, used, found, notes)
}

/// Writes `run.json` into the run directory, atomically (a temporary name, then a rename).
pub fn finish(
    pending: &Pending,
    workspace_dir: &Path,
    ended: Ended,
) -> rascal_core::error::Result<()> {
    if let Ok(mut current) = CURRENT.lock() {
        *current = None;
    }
    let finished = now_ms();
    let (input, used, found, notes) = from_workspace(workspace_dir);
    let mut taken = pending.images_in.clone();
    if taken.is_empty()
        && let Some(reference) = &input
    {
        taken.push(ImageTaken {
            slot: "image".to_string(),
            role: "source".to_string(),
            image: reference.clone(),
        });
    }
    let mut out = Vec::new();
    for (profile, path) in &ended.renders {
        if profile == "grey"
            && let Some(image) = output_reference(path, input.as_ref())
        {
            out.push(ImageOut {
                role: "mask".to_string(),
                image,
                locator: Locator::Path(absolute(path)),
            });
        }
    }
    if let Some(path) = &ended.result
        && let Some(image) = output_reference(path, input.as_ref())
    {
        out.push(ImageOut {
            role: "result".to_string(),
            image,
            locator: Locator::Path(absolute(path)),
        });
    }
    let result = RunResult {
        contract: contract::CONTRACT_VERSION.to_string(),
        tool: Tool {
            name: "rascal".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        action: pending.action.clone(),
        run: Run {
            id: pending.id.clone(),
            resource: format!("rascal://run/{}/run.json", pending.id),
            dir: Some(absolute(&pending.dir)),
        },
        status: ended.status,
        error: ended.error,
        started: rfc3339(pending.started_ms),
        finished: rfc3339(finished),
        duration_ms: u64::try_from(finished.saturating_sub(pending.started_ms)).unwrap_or(u64::MAX),
        input_identity: pending.input_identity.clone(),
        spend: Spend {
            usd: ended.spend_usd,
            basis: SpendBasis::Estimated,
        },
        cache: match ended.cache {
            Some(true) => Cache::Hit,
            Some(false) => Cache::Miss,
            None => Cache::Off,
        },
        images: Some(Images { taken, out }),
        locations: Some(Locations {
            used: if pending.locations_used.is_empty() {
                used
            } else {
                pending.locations_used.clone()
            },
            found,
        }),
        notes,
        body: ended.body,
    };
    write(&pending.dir, &result)
}

/// `run.json` into `dir`, atomically.
pub fn write(dir: &Path, result: &RunResult) -> rascal_core::error::Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let text =
        serde_json::to_string_pretty(result).map_err(|e| Error::usage(format!("run.json: {e}")))?;
    let tmp = dir.join(".run.json.tmp");
    std::fs::write(&tmp, text + "\n").map_err(|e| Error::io(&tmp, e))?;
    let path = dir.join(FILE);
    std::fs::rename(&tmp, &path).map_err(|e| Error::io(&path, e))
}

/// The pending record for a `run` or `heal` of `args` (with `filler` for heal), before it
/// starts: its id, action, directory, start time and input identity.
pub fn pending_for(args: &crate::run::RunArgs, filler: Option<&str>) -> Pending {
    let started_ms = now_ms();
    let digest_of = |p: &Option<PathBuf>| {
        p.as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .map(|b| contract::file_sha256(&b))
    };
    let recipe_text = rascal_mask::recipes::SHIPPED
        .iter()
        .find(|(n, _)| *n == args.recipe)
        .map(|(_, t)| contract::file_sha256(t.as_bytes()))
        .or_else(|| digest_of(&Some(PathBuf::from(&args.recipe))));
    let parts = serde_json::json!({
        "action": action_of(&args.recipe),
        "recipe": recipe_text,
        "image": digest_of(&Some(args.image.clone())),
        "bbox": args.bbox,
        "location": digest_of(&args.location),
        "stroke": digest_of(&args.stroke),
        "protect": args.protect.as_ref().map(|p| p.display().to_string()),
        "mask": args.mask.as_ref().map(|p| p.display().to_string()),
        "prompt": args.prompt,
        "set": args.sets,
        "projection": args.projection.map(|p| format!("{p:?}")),
        "filler": filler,
    });
    Pending {
        id: new_id(started_ms),
        action: action_of(&args.recipe),
        dir: args.out.clone(),
        workspace: args.workspace.clone().unwrap_or_else(|| args.out.clone()),
        fresh: fresh(&args.out),
        started_ms,
        input_identity: format!("sha256:{}", input_identity(&parts)),
        images_in: Vec::new(),
        locations_used: Vec::new(),
    }
}

/// The body for a run that returned: rascal's outcome by its own words.
pub fn body_of(
    recipe: &str,
    status: rascal_core::op::Status,
    stopped_at: Option<&str>,
    reason: Option<&str>,
    mask_dir: Option<&Path>,
    filler: Option<&str>,
) -> serde_json::Map<String, Value> {
    let mut body = serde_json::Map::new();
    body.insert("recipe".into(), Value::String(recipe.to_string()));
    body.insert(
        "outcome".into(),
        serde_json::to_value(status).unwrap_or(Value::Null),
    );
    if let Some(step) = stopped_at {
        body.insert("stopped_at".into(), Value::String(step.to_string()));
    }
    if let Some(reason) = reason {
        body.insert("reason".into(), Value::String(reason.to_string()));
    }
    if let Some(dir) = mask_dir {
        body.insert("mask_dir".into(), Value::String(absolute(dir)));
    }
    if let Some(filler) = filler {
        body.insert("filler".into(), Value::String(filler.to_string()));
    }
    body
}

/// Writes `run.json` for a command that ended, when the run directory is the caller's and the
/// run got as far as making it. A failure to write is reported on stderr, never as the run's.
pub fn conclude(pending: &Pending, ended: Ended) {
    let workspace_dir = pending.workspace.clone();
    let workspace_dir = workspace_dir.as_path();
    if !pending.fresh || !pending.dir.is_dir() {
        if let Ok(mut current) = CURRENT.lock() {
            *current = None;
        }
        return;
    }
    let mut ended = ended;
    if ended.renders.is_empty() {
        // A run that failed after the mask was rendered keeps it (2.3): an agent can salvage the
        // mask and call another filler.
        if let Ok(entries) = std::fs::read_dir(&pending.dir) {
            let mut greys: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.to_string_lossy().ends_with("-grey.png"))
                .collect();
            greys.sort();
            ended
                .renders
                .extend(greys.into_iter().map(|p| ("grey".to_string(), p)));
        }
    }
    if let Err(error) = finish(pending, workspace_dir, ended) {
        eprintln!("rascal: run.json: {error}");
    }
}

/// How a command that failed ended: the contract's status and error, the body naming the step.
/// A cancellation is not an error: `cancelled`, with the step it stopped before.
pub fn ended_with(recipe: &str, error: &Error) -> Ended {
    if let Error::Cancelled { step } = error {
        let mut body = serde_json::Map::new();
        body.insert("recipe".into(), Value::String(recipe.to_string()));
        body.insert("stopped_before".into(), Value::String(step.clone()));
        return Ended {
            status: Status::Cancelled,
            error: None,
            renders: Vec::new(),
            result: None,
            body,
            spend_usd: 0.0,
            cache: None,
        };
    }
    let (status, info) = error_of(error);
    let mut body = serde_json::Map::new();
    body.insert("recipe".into(), Value::String(recipe.to_string()));
    Ended {
        status,
        error: Some(info),
        renders: Vec::new(),
        result: None,
        body,
        spend_usd: 0.0,
        cache: None,
    }
}

/// Watches for SIGINT and SIGTERM while a `run` or `heal` works (4.2): the run in [`CURRENT`]
/// gets `run.json` with status `cancelled` and what it reached, and the process exits 130 or
/// 143. A commit is staged and renamed, so stopping between any two writes leaves the workspace
/// whole; a filler command still running is left to its own timeout.
pub fn watch_signals() {
    std::thread::spawn(|| {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        let code = runtime.block_on(async {
            use tokio::signal::unix::{SignalKind, signal};
            let (Ok(mut int), Ok(mut term)) = (
                signal(SignalKind::interrupt()),
                signal(SignalKind::terminate()),
            ) else {
                return None;
            };
            tokio::select! {
                _ = int.recv() => Some(130),
                _ = term.recv() => Some(143),
            }
        });
        let Some(code) = code else { return };
        let pending = CURRENT.lock().ok().and_then(|mut c| c.take());
        if let Some(pending) = pending
            && pending.fresh
            && pending.dir.is_dir()
        {
            let mut body = serde_json::Map::new();
            body.insert(
                "signal".into(),
                Value::String(if code == 130 { "SIGINT" } else { "SIGTERM" }.to_string()),
            );
            let _ = finish(
                &pending,
                &pending.workspace.clone(),
                Ended {
                    status: Status::Cancelled,
                    error: None,
                    renders: Vec::new(),
                    result: None,
                    body,
                    spend_usd: 0.0,
                    cache: None,
                },
            );
        }
        eprintln!("rascal: cancelled by signal");
        std::process::exit(code);
    });
}

/// How a command that returned ended: `completed`, with rascal's outcome in the body.
#[allow(clippy::too_many_arguments)]
pub fn ended_ok(
    recipe: &str,
    status: rascal_core::op::Status,
    stopped_at: Option<&str>,
    reason: Option<&str>,
    mask_dir: Option<&Path>,
    filler: Option<&str>,
    renders: Vec<(String, PathBuf)>,
    result: Option<PathBuf>,
    spend_usd: f64,
    cache: Option<bool>,
) -> Ended {
    Ended {
        status: Status::Completed,
        error: None,
        renders,
        result,
        body: body_of(recipe, status, stopped_at, reason, mask_dir, filler),
        spend_usd,
        cache,
    }
}
