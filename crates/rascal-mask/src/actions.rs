//! The contract's action tools and run resources for `rascal serve` (table-manners 4, 5 and
//! 10; ADR 0020): one tool per shipped recipe plus `describe`, each a `run` (or a `heal` when a
//! filler is named) in its own run directory under the runs root, its images in by path or
//! inline, its result the run's `run.json`; and the runs as resources.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rascal_core::Error;
use rascal_core::config::Config;
use rascal_core::contract::table_manners::images::{ImageReference, Locator};
use rascal_core::contract::table_manners::{
    RunState, RunSummary, RunsList, Session as TmSession, Tool as TmTool,
};
use rascal_core::workspace::Cancel;
use rascal_mcp::server::{
    ActionResult, Actions, McpResource as Resource, McpResourceTemplate as ResourceTemplate,
    McpTool as Tool, Progress, ResourceBody,
};
use serde_json::{Map, Value, json};

use crate::run_json::{self, Ended, usd};
use crate::{heal, run};
use rascal_mask::describe::{self, INLINE_BYTES, recipe_of};
use rascal_mask::filler::Spent;

/// The contract's actions over the shipped recipes, with runs under `runs`.
pub struct ContractActions {
    pub config_path: Option<PathBuf>,
    pub config: Option<Config>,
    pub runs: PathBuf,
    /// What this server process has spent and holds for calls under way (table-manners 4.5).
    pub ledger: std::sync::Mutex<Ledger>,
    /// The serving session the ledger belongs to (table-manners 4.6).
    pub session: Session,
}

/// A serving session: one process's ledger, named by a random UUID (v4) when `serve` starts.
/// rascal reads its configuration once, so a session lasts the process.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub started_ms: u128,
}

impl Session {
    pub fn start() -> Session {
        Session {
            id: uuid::Uuid::new_v4().to_string(),
            started_ms: run_json::now_ms(),
        }
    }
}

/// The session's spend: a call reserves its worst case when the limits pass, in the same step
/// under the ledger's lock, and its end settles the reservation into what the run spent.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Ledger {
    /// What finished calls spent, unresolved charges included.
    pub spent_usd: f64,
    /// The part of `spent_usd` no answer settled (task 9.1).
    pub unresolved_usd: f64,
    /// The worst cases held for calls under way.
    pub reserved_usd: f64,
}

/// A call's hold on the session budget, settled when the run ends. Dropped unsettled (the run
/// panicked), the whole reservation is counted as unresolved spend, never released.
struct Reservation<'a> {
    ledger: &'a std::sync::Mutex<Ledger>,
    usd: f64,
    settled: bool,
}

impl Reservation<'_> {
    /// Releases the hold and adds what the run spent; `None` (no `run.json` was written)
    /// counts the hold as unresolved.
    fn settle(mut self, spent: Option<Spent>) {
        let spent = spent.unwrap_or(Spent {
            usd: self.usd,
            unresolved_usd: self.usd,
        });
        self.apply(spent);
    }

    fn apply(&mut self, spent: Spent) {
        let mut ledger = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
        ledger.reserved_usd = (ledger.reserved_usd - self.usd).max(0.0);
        ledger.spent_usd += spent.usd;
        ledger.unresolved_usd += spent.unresolved_usd;
        self.settled = true;
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if !self.settled {
            let usd = self.usd;
            self.apply(Spent {
                usd,
                unresolved_usd: usd,
            });
        }
    }
}

/// The default runs root: `$XDG_DATA_HOME/rascal/runs`, else `~/.local/share/rascal/runs`.
pub fn default_runs_root() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rascal/runs")
}

/// How many runs a page of `rascal://runs` holds (table-manners 4.3).
const RUNS_PAGE: usize = 50;

/// A run's start in milliseconds, from its id (`run-<ms>-<pid>`).
fn started_ms(id: &str) -> u128 {
    id.split('-')
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(0)
}

/// A run id as rascal makes them, and nothing else (a resource URI names one).
fn valid_id(id: &str) -> bool {
    let mut parts = id.splitn(3, '-');
    parts.next() == Some("run")
        && parts
            .next()
            .is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        && parts
            .next()
            .is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

fn image_in_schema(description: &str) -> Value {
    json!({
        "description": description,
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "An absolute path (local transports only)." },
            "inline": {
                "type": "object",
                "properties": {
                    "media_type": { "type": "string" },
                    "data": { "type": "string", "description": "Standard base64." }
                },
                "required": ["media_type", "data"]
            },
            "expect": { "type": "object", "description": "A table-manners image reference the image must match (file_sha256, width, height)." }
        }
    })
}

impl ContractActions {
    fn description(&self) -> rascal_core::error::Result<Value> {
        let d = describe::description(self.config.as_ref())?;
        serde_json::to_value(&d).map_err(|e| Error::usage(format!("description: {e}")))
    }

    fn input_schema(action: &Value) -> Map<String, Value> {
        let mut schema = action["params_schema"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let mut properties = schema
            .get("properties")
            .and_then(|p| p.as_object())
            .cloned()
            .unwrap_or_default();
        let mut required: Vec<Value> = schema
            .get("required")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        for slot in action["images"].as_array().into_iter().flatten() {
            let name = slot["slot"].as_str().unwrap_or("image").to_string();
            properties.insert(
                name.clone(),
                image_in_schema("The image, by path or inline."),
            );
            if slot["required"] == true {
                required.push(Value::String(name));
            }
        }
        for slot in action["locations"].as_array().into_iter().flatten() {
            let name = slot["slot"].as_str().unwrap_or("location").to_string();
            properties.insert(
                name.clone(),
                json!({ "type": "object", "description": format!("A table-manners location on the image, of kind {}.", slot["kinds"]) }),
            );
            if slot["required"] == true {
                required.push(Value::String(name));
            }
        }
        properties.insert(
            "return_images".into(),
            json!({ "type": "string", "enum": ["path", "inline", "resource"], "description": "How returned images are located; path by default." }),
        );
        properties.insert(
            "image_blocks".into(),
            json!({ "type": "boolean", "description": "Also send each returned image as an MCP image block." }),
        );
        properties.insert(
            "max_spend_usd".into(),
            json!({ "type": "number", "minimum": 0, "description": "Lower the per-call spend cap for this call." }),
        );
        schema.insert("properties".into(), Value::Object(properties));
        schema.insert("required".into(), Value::Array(required));
        schema
    }

    /// The image slot as a file the run can take: a path checked, or inline bytes written into
    /// the run's input directory; the `expect` reference verified.
    fn image_file(&self, value: &Value, inputs: &Path) -> rascal_core::error::Result<PathBuf> {
        let path = if let Some(path) = value.get("path").and_then(|p| p.as_str()) {
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err(Error::usage(format!(
                    "image: {} is not an absolute path (CONTRACT.md 10.1)",
                    path.display()
                )));
            }
            if !path.is_file() {
                return Err(Error::usage(format!(
                    "image: {} is not a file",
                    path.display()
                )));
            }
            path
        } else if let Some(inline) = value.get("inline") {
            let data = inline["data"].as_str().unwrap_or_default();
            let bytes = rascal_mcp::jsonrpc::base64_decode(data)
                .ok_or_else(|| Error::usage("image: inline data is not standard base64"))?;
            if bytes.len() as u64 > INLINE_BYTES {
                return Err(Error::usage(format!(
                    "image: inline image of {} bytes is over the limit of {INLINE_BYTES}",
                    bytes.len()
                )));
            }
            let ext = match inline["media_type"].as_str() {
                Some("image/jpeg") => "jpg",
                _ => "png",
            };
            std::fs::create_dir_all(inputs).map_err(|e| Error::io(inputs, e))?;
            let path = inputs.join(format!("image.{ext}"));
            std::fs::write(&path, &bytes).map_err(|e| Error::io(&path, e))?;
            path
        } else {
            return Err(Error::usage("image: give path or inline"));
        };
        if let Some(expect) = value.get("expect") {
            let expect: ImageReference = serde_json::from_value(expect.clone()).map_err(|e| {
                Error::usage(format!("image: expect is not an image reference: {e}"))
            })?;
            let bytes = std::fs::read(&path).map_err(|e| Error::io(&path, e))?;
            let (width, height) = image::image_dimensions(&path)
                .map_err(|e| Error::usage(format!("image: {}: {e}", path.display())))?;
            let digest = rascal_core::contract::file_sha256(&bytes);
            if digest != expect.file_sha256 || width != expect.width || height != expect.height {
                return Err(Error::usage(format!(
                    "image: the image is on another image than expected: file_sha256 {digest} ({width}x{height}), expected {} ({}x{})",
                    expect.file_sha256, expect.width, expect.height
                )));
            }
        }
        Ok(path)
    }

    fn run_args(
        &self,
        recipe: &str,
        args: &Map<String, Value>,
        out: &Path,
        inputs: &Path,
        cancel: Arc<Cancel>,
    ) -> rascal_core::error::Result<run::RunArgs> {
        let image = self.image_file(
            args.get("image")
                .ok_or_else(|| Error::usage("image: required"))?,
            inputs,
        )?;
        let mut location = None;
        let mut stroke = None;
        for (slot, target) in [("location", &mut location), ("stroke", &mut stroke)] {
            if let Some(value) = args.get(slot) {
                std::fs::create_dir_all(inputs).map_err(|e| Error::io(inputs, e))?;
                let path = inputs.join(format!("{slot}.json"));
                std::fs::write(&path, value.to_string()).map_err(|e| Error::io(&path, e))?;
                *target = Some(path);
            }
        }
        let text = |k: &str| args.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let mut sets = Vec::new();
        if let Some(set) = args.get("set").and_then(|s| s.as_object()) {
            for (key, value) in set {
                let rendered = match value {
                    Value::String(s) => toml::Value::String(s.clone()).to_string(),
                    other => other.to_string(),
                };
                sets.push(format!("{key}={rendered}"));
            }
        }
        let projection = match text("projection").as_deref() {
            Some("flat") => Some(rascal_core::frame::Projection::Flat),
            Some("equirectangular") => Some(rascal_core::frame::Projection::Equirectangular),
            Some(other) => return Err(Error::usage(format!("projection: {other}"))),
            None => None,
        };
        Ok(run::RunArgs {
            recipe: recipe.to_string(),
            image,
            out: out.to_path_buf(),
            bbox: None,
            location,
            stroke,
            protect: text("protect").map(PathBuf::from),
            mask: text("mask").map(PathBuf::from),
            prompt: text("prompt"),
            config: self.config_path.clone(),
            workspace: None,
            projection,
            strict: args
                .get("strict")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            replay: None,
            overrides: Default::default(),
            overrides_from: None,
            sets,
            cancel: Some(cancel),
        })
    }

    /// What a call to the filler costs at worst, and the limits that apply: the per-call cap
    /// (the configuration's, lowered by `max_spend_usd`) and the session budget.
    fn limits(&self, filler: Option<&str>, args: &Map<String, Value>, ledger: &Ledger) -> Limits {
        let price = filler
            .and_then(|f| self.config.as_ref()?.fillers.get(f)?.spend_usd_per_call)
            .unwrap_or(0.0);
        let configured = self
            .config
            .as_ref()
            .and_then(|c| c.limits.clone())
            .unwrap_or_default();
        let asked = args.get("max_spend_usd").and_then(|v| v.as_f64());
        let cap = match (configured.call_cap_usd, asked) {
            (Some(c), Some(a)) => Some(c.min(a)),
            (c, a) => c.or(a),
        };
        Limits {
            worst_case_usd: price,
            call_cap_usd: cap,
            session_budget_usd: configured.session_budget_usd,
            session_held_usd: ledger.spent_usd + ledger.reserved_usd,
        }
    }

    /// The ledger as it stands.
    fn ledger(&self) -> Ledger {
        *self.ledger.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Checks the limits and reserves the call's worst case in one step under the ledger's
    /// lock, so two calls never both pass against the same spend.
    fn reserve(
        &self,
        filler: Option<&str>,
        args: &Map<String, Value>,
    ) -> rascal_core::error::Result<Reservation<'_>> {
        let mut ledger = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
        let limits = self.limits(filler, args, &ledger);
        limits.check()?;
        ledger.reserved_usd += limits.worst_case_usd;
        Ok(Reservation {
            ledger: &self.ledger,
            usd: limits.worst_case_usd,
            settled: false,
        })
    }

    /// Runs one action to its `run.json`.
    fn run_action(
        &self,
        recipe: &str,
        args: &Map<String, Value>,
        client_run_id: Option<&str>,
        progress: &(dyn Fn(Progress) + Send + Sync),
        cancel: Arc<Cancel>,
    ) -> std::result::Result<ActionResult, String> {
        // A cancel that lands before allocation leaves no run (table-manners 4.2).
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".to_string());
        }
        std::fs::create_dir_all(&self.runs).map_err(|e| e.to_string())?;
        let started_ms = run_json::allocate_ms();
        let id = run_json::new_id(started_ms);
        let out = self.runs.join(&id);
        let inputs = self.runs.join(format!("{id}.in"));
        let filler = args
            .get("filler")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let _ = std::fs::create_dir_all(&inputs);
        // The run's directory and its association exist from allocation, before the first
        // progress line and any paid work, so the runs list shows it `running` with its ids from
        // that moment (tasks 9.4 and 11.3).
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        let association = Association {
            action: describe::action_name(recipe),
            session_id: Some(self.session.id.clone()),
            client_run_id: client_run_id.map(str::to_string),
        };
        let _ = std::fs::write(
            inputs.join(Association::FILE),
            serde_json::to_string(&association).unwrap_or_default(),
        );
        progress(Progress::Run(json!({
            "id": id,
            "resource": format!("rascal://run/{id}/run.json"),
            "session_id": association.session_id,
            "client_run_id": association.client_run_id,
        })));
        progress(Progress::Line("run started".to_string()));
        let prepared = self
            .run_args(recipe, args, &out, &inputs, cancel)
            .and_then(|run_args| {
                self.reserve(filler.as_deref(), args)
                    .map(|reservation| (run_args, reservation))
            });
        let (pending, ended, reservation) = match prepared {
            Err(error) => {
                let pending = run_json::Pending {
                    id: id.clone(),
                    action: describe::action_name(recipe),
                    dir: out.clone(),
                    workspace: out.clone(),
                    fresh: true,
                    started_ms,
                    session_id: Some(self.session.id.clone()),
                    client_run_id: client_run_id.map(str::to_string),
                    input_identity: format!(
                        "sha256:{}",
                        run_json::input_identity(&Value::Object(args.clone()))
                    ),
                    images_in: Vec::new(),
                    locations_used: Vec::new(),
                };
                (pending, run_json::ended_with(recipe, &error), None)
            }
            Ok((run_args, reservation)) => {
                let mut pending = run_json::pending_for(&run_args, filler.as_deref());
                pending.id = id.clone();
                pending.started_ms = started_ms;
                pending.session_id = Some(self.session.id.clone());
                pending.client_run_id = client_run_id.map(str::to_string);
                let done = AtomicBool::new(false);
                let journal = out.join("journal.jsonl");
                let ended = std::thread::scope(|scope| {
                    scope.spawn(|| watch_journal(&journal, &done, progress));
                    let ended = self.execute(recipe, run_args, filler.as_deref());
                    done.store(true, Ordering::SeqCst);
                    ended
                });
                (pending, ended, Some(reservation))
            }
        };
        let cancelled = ended.status == rascal_core::contract::table_manners::Status::Cancelled;
        let spent = run_json::conclude(&pending, ended);
        if let Some(reservation) = reservation {
            reservation.settle(spent);
        }
        if cancelled {
            return Err("cancelled".to_string());
        }
        self.result_of(&id, args)
    }

    /// Runs the recipe, through `heal` when a filler is named.
    fn execute(&self, recipe: &str, run_args: run::RunArgs, filler: Option<&str>) -> Ended {
        match filler {
            Some(filler) => match heal::heal(&heal::HealArgs {
                run: run_args,
                filler: filler.to_string(),
                no_dash: true,
                replay: None,
                cache: None,
            }) {
                Ok(report) => run_json::ended_ok(
                    recipe,
                    report.status,
                    report.stopped_at.as_deref(),
                    report.reason.as_deref(),
                    None,
                    Some(filler),
                    report.renders,
                    report.result,
                    report.cache,
                ),
                Err(error) => run_json::ended_with(recipe, &error),
            },
            None => match run::run(&run_args) {
                Ok(report) => run_json::ended_ok(
                    recipe,
                    report.status,
                    report.stopped_at.as_deref(),
                    report.reason.as_deref(),
                    report.mask_dir.as_deref(),
                    None,
                    report.renders,
                    None,
                    None,
                ),
                Err(error) => run_json::ended_with(recipe, &error),
            },
        }
    }

    /// The run's `run.json` as the call's result, its images located as the caller asked.
    fn result_of(
        &self,
        id: &str,
        args: &Map<String, Value>,
    ) -> std::result::Result<ActionResult, String> {
        let path = self.runs.join(id).join(run_json::FILE);
        let mut structured: Value = serde_json::from_str(
            &std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?,
        )
        .map_err(|e| e.to_string())?;
        let how = args
            .get("return_images")
            .and_then(|v| v.as_str())
            .unwrap_or("path");
        let mut blocks = Vec::new();
        if let Some(outs) = structured
            .pointer_mut("/images/out")
            .and_then(|o| o.as_array_mut())
        {
            for out in outs {
                let Some(file) = out.get("path").and_then(|p| p.as_str()).map(PathBuf::from) else {
                    continue;
                };
                let role = out["role"].as_str().unwrap_or("image").to_string();
                let bytes = std::fs::read(&file).ok();
                if args.get("image_blocks").and_then(|v| v.as_bool()) == Some(true)
                    && let Some(bytes) = &bytes
                {
                    blocks.push(bytes.clone());
                }
                if let Some(object) = out.as_object_mut() {
                    match how {
                        "inline" => {
                            object.remove("path");
                            object.insert(
                                "inline".into(),
                                json!({ "media_type": "image/png", "data": rascal_core::panel::base64(bytes.as_deref().unwrap_or_default()) }),
                            );
                        }
                        "resource" => {
                            object.remove("path");
                            object.insert(
                                "resource".into(),
                                Value::String(format!("rascal://run/{id}/images/{role}")),
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
        let is_error = matches!(structured["status"].as_str(), Some("error" | "invalid"));
        Ok(ActionResult {
            structured,
            is_error,
            images: blocks,
        })
    }

    /// The `rascal://session` observation: one copy of the ledger, taken under its lock.
    fn session_view(&self) -> TmSession {
        let ledger = self.ledger();
        let budget = self
            .config
            .as_ref()
            .and_then(|c| c.limits.as_ref())
            .and_then(|l| l.session_budget_usd);
        TmSession {
            contract: rascal_core::contract::CONTRACT_VERSION.to_string(),
            tool: TmTool {
                name: "rascal".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            id: self.session.id.clone(),
            started: run_json::rfc3339(self.session.started_ms),
            budget_usd: budget.map(usd),
            spent_usd: usd(ledger.spent_usd),
            unresolved_usd: usd(ledger.unresolved_usd),
            reserved_usd: usd(ledger.reserved_usd),
            remaining_usd: budget
                .map(|b| usd((b - ledger.spent_usd - ledger.reserved_usd).max(0.0))),
        }
    }

    /// One page of the runs (table-manners 4.3): newest first by (started, id), `RUNS_PAGE` a
    /// page, starting after `cursor`. The cursor is the last entry's run id, which carries its
    /// own position (`run-<ms>-<pid>`); clients treat it as opaque. A cursor that is not a run id
    /// gives no page.
    fn runs_list(&self, cursor: Option<&str>) -> Option<RunsList> {
        let after = match cursor {
            None => None,
            Some(id) if valid_id(id) => Some((started_ms(id), id.to_string())),
            Some(_) => return None,
        };
        let mut runs: Vec<(u128, RunSummary)> = Vec::new();
        for entry in std::fs::read_dir(&self.runs)
            .into_iter()
            .flatten()
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if !valid_id(&name) {
                continue;
            }
            let ms = started_ms(&name);
            let done: Option<Value> = std::fs::read_to_string(entry.path().join(run_json::FILE))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok());
            let association: Option<Association> = std::fs::read_to_string(
                self.runs.join(format!("{name}.in")).join(Association::FILE),
            )
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok());
            let action = association
                .as_ref()
                .map(|a| a.action.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let status = match done.as_ref().and_then(|d| d["status"].as_str()) {
                Some("completed") => RunState::Completed,
                Some("cancelled") => RunState::Cancelled,
                Some("error") => RunState::Error,
                Some("invalid") => RunState::Invalid,
                _ => RunState::Running,
            };
            runs.push((
                ms,
                RunSummary {
                    id: name.clone(),
                    resource: format!("rascal://run/{name}/run.json"),
                    session_id: association.as_ref().and_then(|a| a.session_id.clone()),
                    client_run_id: association.and_then(|a| a.client_run_id),
                    action,
                    status,
                    started: run_json::rfc3339(ms),
                },
            ));
        }
        runs.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.id.cmp(&a.1.id)));
        let mut rest: Vec<RunSummary> = runs
            .into_iter()
            .filter(|(ms, run)| {
                after
                    .as_ref()
                    .is_none_or(|(at, id)| (*ms, &run.id) < (*at, id))
            })
            .map(|(_, run)| run)
            .collect();
        let more = rest.len() > RUNS_PAGE;
        rest.truncate(RUNS_PAGE);
        let next_cursor = more.then(|| rest.last().map(|r| r.id.clone())).flatten();
        Some(RunsList {
            contract: rascal_core::contract::CONTRACT_VERSION.to_string(),
            tool: TmTool {
                name: "rascal".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            runs: rest,
            next_cursor,
        })
    }
}

/// What a run was asked as, written into its inputs directory when it is allocated, before any
/// paid work (table-manners 4.3): the runs list reads it for a run still going.
#[derive(serde::Serialize, serde::Deserialize)]
struct Association {
    action: String,
    session_id: Option<String>,
    client_run_id: Option<String>,
}

impl Association {
    const FILE: &str = "association.json";
}

/// Sends a progress line for every op the run journals, until `done`.
fn watch_journal(journal: &Path, done: &AtomicBool, progress: &(dyn Fn(Progress) + Send + Sync)) {
    let mut seen = 0usize;
    loop {
        let finished = done.load(Ordering::SeqCst);
        if let Ok(text) = std::fs::read_to_string(journal) {
            for line in text.lines().skip(seen) {
                seen += 1;
                let Ok(value) = serde_json::from_str::<Value>(line) else {
                    continue;
                };
                if let (Some(step), Some(op)) = (value["step"].as_str(), value["op"].as_str()) {
                    let status = value["envelope"]["status"].as_str().unwrap_or("done");
                    progress(Progress::Line(format!("step {step} ({op}): {status}")));
                }
            }
        }
        if finished {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

impl Actions for ContractActions {
    fn tools(&self) -> Vec<Tool> {
        let mut tools = Vec::new();
        let description = self.description().unwrap_or(Value::Null);
        for action in description["actions"].as_array().into_iter().flatten() {
            let name = action["name"].as_str().unwrap_or_default().to_string();
            let text = format!(
                "{} {} (table-manners action; the result is the run's run.json.)",
                action["title"].as_str().unwrap_or_default(),
                action["purpose"].as_str().unwrap_or_default()
            );
            tools.push(Tool::new(name, text, Arc::new(Self::input_schema(action))));
        }
        // `estimate` is in the description with its own schema.
        if let Some(estimate) = description["actions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| a["name"] == "estimate")
            && let Some(position) = tools.iter().position(|t| t.name == "estimate")
        {
            let schema = estimate["params_schema"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            tools[position] = Tool::new(
                "estimate",
                estimate["purpose"].as_str().unwrap_or_default().to_string(),
                Arc::new(schema),
            );
        }
        let mut empty = Map::new();
        empty.insert("type".into(), Value::String("object".into()));
        tools.push(Tool::new(
            "describe",
            "The table-manners description document: every action, what it takes, returns and may spend.",
            Arc::new(empty),
        ));
        tools
    }

    fn handles(&self, name: &str) -> bool {
        name == "describe" || name == "estimate" || recipe_of(name).is_some()
    }

    fn exclusive(&self, name: &str) -> bool {
        recipe_of(name).is_some()
    }

    fn call(
        &self,
        name: &str,
        args: &Map<String, Value>,
        client_run_id: Option<&str>,
        progress: &(dyn Fn(Progress) + Send + Sync),
        cancel: Arc<Cancel>,
    ) -> std::result::Result<ActionResult, String> {
        if name == "describe" {
            return Ok(ActionResult {
                structured: self.description().map_err(|e| e.to_string())?,
                is_error: false,
                images: Vec::new(),
            });
        }
        if name == "estimate" {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if recipe_of(action).is_none() {
                return Err(format!("estimate: no action {action}"));
            }
            let filler = args.get("filler").and_then(|v| v.as_str());
            let ledger = self.ledger();
            let limits = self.limits(filler, args, &ledger);
            return Ok(ActionResult {
                structured: json!({
                    "contract": rascal_core::contract::CONTRACT_VERSION,
                    "action": action,
                    "worst_case_usd": usd(limits.worst_case_usd),
                    "call_cap_usd": limits.call_cap_usd.map(usd),
                    "allowed": limits.check().is_ok(),
                }),
                is_error: false,
                images: Vec::new(),
            });
        }
        let recipe = recipe_of(name).ok_or_else(|| format!("no action {name}"))?;
        self.run_action(recipe, args, client_run_id, progress, cancel)
    }

    fn resources(&self) -> Vec<Resource> {
        let mut runs = Resource::new("rascal://runs", "runs");
        runs.description = Some(
            "Runs, newest first, the first page; rascal://runs?cursor=<next_cursor> reads the next (table-manners 4.3)."
                .into(),
        );
        runs.mime_type = Some("application/json".into());
        let mut session = Resource::new("rascal://session", "session");
        session.description =
            Some("This server's spend ledger, one atomic observation (table-manners 4.6).".into());
        session.mime_type = Some("application/json".into());
        vec![runs, session]
    }

    fn templates(&self) -> Vec<ResourceTemplate> {
        [
            ("rascal://runs{?cursor}", "runs", "application/json"),
            ("rascal://run/{id}/run.json", "run", "application/json"),
            (
                "rascal://run/{id}/journal",
                "journal",
                "application/x-ndjson",
            ),
            ("rascal://run/{id}/images/{role}", "image", "image/png"),
        ]
        .into_iter()
        .map(|(uri, name, mime)| {
            let mut t = ResourceTemplate::new(uri, name);
            t.mime_type = Some(mime.into());
            t
        })
        .collect()
    }

    fn read(&self, uri: &str) -> Option<ResourceBody> {
        if uri == "rascal://session" {
            return Some(ResourceBody::Text {
                mime: "application/json".into(),
                text: serde_json::to_string_pretty(&self.session_view()).ok()?,
            });
        }
        if uri == "rascal://runs" || uri.starts_with("rascal://runs?") {
            let cursor = match uri.strip_prefix("rascal://runs?") {
                None => None,
                Some(query) => Some(query.strip_prefix("cursor=")?),
            };
            return Some(ResourceBody::Text {
                mime: "application/json".into(),
                text: serde_json::to_string_pretty(&self.runs_list(cursor)?).ok()?,
            });
        }
        let rest = uri.strip_prefix("rascal://run/")?;
        let (id, what) = rest.split_once('/')?;
        if !valid_id(id) {
            return None;
        }
        let dir = self.runs.join(id);
        match what {
            "run.json" => Some(ResourceBody::Text {
                mime: "application/json".into(),
                text: std::fs::read_to_string(dir.join(run_json::FILE)).ok()?,
            }),
            "journal" => Some(ResourceBody::Text {
                mime: "application/x-ndjson".into(),
                text: std::fs::read_to_string(dir.join("journal.jsonl")).ok()?,
            }),
            other => {
                let role = other.strip_prefix("images/")?;
                let result: Value =
                    serde_json::from_str(&std::fs::read_to_string(dir.join(run_json::FILE)).ok()?)
                        .ok()?;
                let out = result["images"]["out"]
                    .as_array()?
                    .iter()
                    .find(|o| o["role"] == role)?;
                let image: rascal_core::contract::table_manners::images::ImageOut =
                    serde_json::from_value(out.clone()).ok()?;
                let Locator::Path(path) = image.locator else {
                    return None;
                };
                Some(ResourceBody::Blob {
                    mime: "image/png".into(),
                    bytes: std::fs::read(path).ok()?,
                })
            }
        }
    }
}

/// The spend a call may make, and the limits it is held to (table-manners 4.5).
struct Limits {
    worst_case_usd: f64,
    call_cap_usd: Option<f64>,
    session_budget_usd: Option<f64>,
    /// What the session has spent plus what calls under way hold.
    session_held_usd: f64,
}

impl Limits {
    /// Refuses a call whose worst case is over the cap or the budget left.
    fn check(&self) -> rascal_core::error::Result<()> {
        if let Some(cap) = self.call_cap_usd
            && self.worst_case_usd > cap
        {
            return Err(Error::Budget {
                reason: format!(
                    "the call may spend {:.4} USD, over the per-call cap of {cap:.4}",
                    self.worst_case_usd
                ),
            });
        }
        if let Some(budget) = self.session_budget_usd
            && self.session_held_usd + self.worst_case_usd > budget
        {
            return Err(Error::Budget {
                reason: format!(
                    "the call may spend {:.4} USD; the session has {:.4} of {budget:.4} left",
                    self.worst_case_usd,
                    (budget - self.session_held_usd).max(0.0)
                ),
            });
        }
        Ok(())
    }
}
