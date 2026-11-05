//! The contract's tools and run resources for `rascal serve` (table-manners 4, 5 and 10; ADR
//! 0020): one tool per action of the description plus the reserved `describe`, `estimate` (the
//! estimate of one exact call, `crate::estimate`) and the controls `export` and `import`
//! (`crate::transfer`, 5.1), and nothing else. A task (a shipped
//! recipe) runs as a `run` (or a `heal` when a filler is named) in its own run directory under
//! the runs root, its images in by path or inline; an operation (a built-in op,
//! `crate::operations`) runs on the session's store. Either's result is the run's `run.json`;
//! the runs and the store's artifacts (`crate::artifact_resource`) are resources.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rascal_core::Error;
use std::collections::BTreeMap;

use rascal_core::config::{Config, ConfigState};
use rascal_core::contract::table_manners::images::Locator;
use rascal_core::contract::table_manners::{
    ErrorCode, Readiness, Refusal, RefusalReason, Requirement, RunState, RunSummary, RunsList,
    Session as TmSession, Tool as TmTool,
};
use rascal_core::workspace::{Cancel, Workspace};
use rascal_mcp::server::{
    ActionResult, Actions, CallError, CallMeta, McpResource as Resource,
    McpResourceTemplate as ResourceTemplate, McpTool as Tool, Progress, ResourceBody,
};
use serde_json::{Map, Value, json};

use crate::run_json::{self, Ended, usd};
use crate::{artifact_resource, estimate, heal, invocation, run};
use rascal_mask::describe::{self, recipe_of};
use rascal_mask::filler::{CALL_PREFIX, FillerCall, Spent};
use rascal_mask::readiness::{self, Answer, Answers, Setup, Use};

/// The contract's actions over the shipped recipes, with runs under `runs`.
pub struct ContractActions {
    /// The configuration file, when it loaded (runs read it again by path).
    pub config_path: Option<PathBuf>,
    /// The loaded configuration, `config_state`'s when the file loaded.
    pub config: Option<Config>,
    /// The configuration as read once at start, in whatever state (table-manners 3.1, 4.6).
    pub config_state: ConfigState,
    /// The packs that did not start, with the reason: their requirement reads `invalid`.
    pub failed_packs: BTreeMap<String, String>,
    /// What providers said about each credential in this session, by variable name: a fresh
    /// session has none, so a key reads `present` until a real call settles it (3.1).
    pub answers: std::sync::Mutex<Answers>,
    pub runs: PathBuf,
    /// What this server process has spent and holds for calls under way (table-manners 4.5).
    pub ledger: std::sync::Mutex<Ledger>,
    /// The serving session the ledger belongs to (table-manners 4.6); none for `rascal call`,
    /// whose runs have no session (`session_id` null) and no budget (the cap still applies).
    pub session: Option<Session>,
    /// The estimates this session issued, for the calls that name them (table-manners 4.5).
    pub issued: std::sync::Mutex<estimate::Issued>,
    /// The directories callers' paths may name, fixed for the session (CONTRACT 5.2).
    pub paths: rascal_core::paths::PathRule,
    /// The session's store (CONTRACT 3.4, 4.7): `serve`'s workspace, where an artifact
    /// reference resolves by identity (PLAN.md 10.3) and an operation's outputs commit.
    pub store: Arc<Workspace>,
    /// The built-in ops, each served as an operation (PLAN.md 10.6).
    pub ops: rascal_core::registry::OpRegistry,
}

/// The Masks a call bound by reference, by argument, as bundles for its run.
type Bound = BTreeMap<String, run::MaskIn>;

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
    /// Whether the result is checked, and the tries the hold covers (SPEC.md "Self-correcting
    /// remove": spend, reserved up front).
    check: rascal_mask::checker::Check,
    tries: u32,
}

impl Reservation<'_> {
    /// Releases the hold and adds what the run spent; `None` (no `run.json` was written)
    /// counts the hold as unresolved.
    fn settle(mut self, spent: Option<Spent>) {
        let spent = spent.unwrap_or(Spent {
            usd: self.usd,
            unresolved_usd: self.usd,
            measured: false,
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
                measured: false,
            });
        }
    }
}

/// The data home: `$XDG_DATA_HOME`, else `~/.local/share`.
pub fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The default runs root: `$XDG_DATA_HOME/rascal/runs`, else `~/.local/share/rascal/runs`.
pub fn default_runs_root() -> PathBuf {
    data_home().join("rascal/runs")
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

pub(crate) fn image_in_schema(description: &str) -> Value {
    json!({
        "description": description,
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "An absolute path inside the directories rascal serve allows (--allow-dir; CONTRACT 5.2)." },
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
        let d = self.with_setup(describe::description)?;
        serde_json::to_value(&d).map_err(|e| Error::usage(format!("description: {e}")))
    }

    /// Runs `f` with what readiness inspects in this session (the answers as they stand).
    fn with_setup<R>(&self, f: impl FnOnce(&Setup<'_>) -> R) -> R {
        let answers = self
            .answers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        f(&Setup {
            config: &self.config_state,
            answers: &answers,
            failed_packs: &self.failed_packs,
        })
    }

    /// Keeps what providers said about credentials in a run that ended (3.1): from its filler
    /// call records, a 2xx makes the filler's key `valid` and a 401 or 403 `invalid`; any other
    /// answer, or a cache hit (no call), leaves it as it was.
    fn learn(&self, result: &Value) {
        let Some(config) = &self.config else {
            return;
        };
        let Some(dir) = result.pointer("/run/dir").and_then(|d| d.as_str()) else {
            return;
        };
        let Ok(lines) = rascal_core::journal::Journal::read(&Path::new(dir).join("journal.jsonl"))
        else {
            return;
        };
        let mut answers = self.answers.lock().unwrap_or_else(|e| e.into_inner());
        for note in lines.iter().filter_map(|l| l.note.as_deref()) {
            let Some(call) = note
                .strip_prefix(CALL_PREFIX)
                .and_then(|json| serde_json::from_str::<FillerCall>(json).ok())
            else {
                continue;
            };
            let Some(var) = config
                .fillers
                .get(&call.filler)
                .and_then(|f| f.key_env.clone())
            else {
                continue;
            };
            match call.status {
                200..=299 => {
                    answers.insert(var, Answer::Valid);
                }
                401 | 403 => {
                    answers.insert(var, Answer::Invalid);
                }
                _ => {}
            }
        }
    }

    /// Refuses a call that needs a requirement that is missing or invalid, before any run
    /// (table-manners 3.1): the call's own readiness, for the filler its arguments name.
    fn ready_for(&self, action: &str, args: &Map<String, Value>) -> Result<(), CallError> {
        let filler = match args.get("filler").and_then(|v| v.as_str()) {
            Some(name) => Use::Filler(name),
            None => Use::MaskOnly,
        };
        let readiness = self.with_setup(|setup| readiness::for_call(setup, action, filler));
        not_ready(action, &readiness).map_or(Ok(()), Err)
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
        schema.insert("properties".into(), Value::Object(properties));
        schema.insert("required".into(), Value::Array(required));
        schema
    }

    /// The image slot as a file the run can take: a path admitted (its canonical form), or
    /// inline bytes written into the run's input directory; the `expect` reference verified
    /// (`estimate::image_in`, as an estimate reads it).
    fn image_file(&self, value: &Value, inputs: &Path) -> rascal_core::error::Result<PathBuf> {
        let image = estimate::image_in("image", value, &self.paths)?;
        if value.get("expect").is_some() {
            estimate::verify_expect("image", value, &image, &image.bytes()?)?;
        }
        match image {
            estimate::ImageIn::Path(path) => Ok(path),
            estimate::ImageIn::Inline { bytes, ext } => {
                std::fs::create_dir_all(inputs).map_err(|e| Error::io(inputs, e))?;
                let path = inputs.join(format!("image.{ext}"));
                std::fs::write(&path, &bytes).map_err(|e| Error::io(&path, e))?;
                Ok(path)
            }
        }
    }

    /// The checked arguments (`estimate::check_arguments`, which `run_action` calls first) as a
    /// run's: the image as a file, the locations written into the inputs directory.
    fn run_args(
        &self,
        recipe: &str,
        args: &Map<String, Value>,
        bound: &Bound,
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
        // The projection is the call's `projection`, else the one the image's `expect` reference
        // states (table-manners 3.4): a caller that describes the image as flat is never second-
        // guessed by the 2:1 rule. The two must agree when both are given.
        let parse = |name: &str, value: &str| match value {
            "flat" => Ok(rascal_core::frame::Projection::Flat),
            "equirectangular" => Ok(rascal_core::frame::Projection::Equirectangular),
            other => Err(Error::usage(format!("{name}: {other}"))),
        };
        let given = text("projection")
            .map(|v| parse("projection", &v))
            .transpose()?;
        let described = args
            .get("image")
            .and_then(|i| i.pointer("/expect/projection"))
            .and_then(|v| v.as_str())
            .map(|v| parse("image.expect.projection", v))
            .transpose()?;
        let projection = match (given, described) {
            (Some(g), Some(d)) if g != d => {
                return Err(Error::usage(format!(
                    "projection: {} contradicts the image's expect.projection {}",
                    g.name(),
                    d.name()
                )));
            }
            (given, described) => given.or(described),
        };
        Ok(run::RunArgs {
            recipe: recipe.to_string(),
            image,
            out: out.to_path_buf(),
            bbox: None,
            location,
            stroke,
            protect: bound.get("protect").cloned(),
            mask: bound.get("mask").cloned(),
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

    /// Binds a call's artifact references before any run (CONTRACT 4.7; PLAN.md 10.3): each
    /// `mask` and `protect` argument must be a `rascal:Mask` reference (anything else, a path
    /// among them, is plain invalid params) that resolves by identity in the store. References
    /// that do not bind are the `binding` refusal naming every one, port being the argument's
    /// name, which allocates nothing and releases the call's `client_run_id`. Bound, each is
    /// replaced in the arguments by the store's own reference (what the invocation records) and
    /// its Mask goes to the run as a bundle.
    fn bind(
        &self,
        recipe: &str,
        args: &Map<String, Value>,
    ) -> Result<(Map<String, Value>, Bound), CallError> {
        let slots = estimate::slots(recipe).map_err(|e| CallError::from(e.to_string()))?;
        let mut references = Vec::new();
        for slot in &slots.masks {
            if let Some(value) = args.get(slot) {
                let reference = rascal_core::binding::reference_in(slot, value)
                    .map_err(|e| CallError::from(e.to_string()))?;
                references.push((slot.clone(), reference));
            }
        }
        let given: Vec<rascal_core::binding::Given<'_>> = references
            .iter()
            .map(|(slot, reference)| rascal_core::binding::Given {
                port: slot,
                index: None,
                expected: rascal_core::artifact::ArtifactType::Mask,
                reference,
            })
            .collect();
        let store_error = |e: Error| CallError::from(format!("the store: {e}"));
        let resolved =
            match rascal_core::binding::bind_all(&self.store, &given).map_err(store_error)? {
                Ok(resolved) => resolved,
                Err(bindings) => return Err(crate::operations::binding_refusal(bindings)),
            };
        let mut args = args.clone();
        let mut bound = Bound::new();
        for ((slot, _), found) in references.iter().zip(resolved) {
            let reference =
                rascal_core::contract::artifact_ref(&self.store, &found.id).map_err(store_error)?;
            args.insert(
                slot.clone(),
                serde_json::to_value(reference).map_err(|e| CallError::from(e.to_string()))?,
            );
            let bundle = self.store.export_mask(&found.id).map_err(store_error)?;
            bound.insert(slot.clone(), run::MaskIn::Bundle(Arc::new(bundle)));
        }
        Ok((args, bound))
    }

    /// The ledger as it stands.
    pub(crate) fn ledger(&self) -> Ledger {
        *self.ledger.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The limits check (table-manners 4.5, docs/contract.md): after allocation and before any
    /// paid step, in a fixed order. The arguments first (`estimate::check_arguments` in
    /// `run_action`, `max_spend_usd` among them: not above zero or above the configured cap is
    /// `invalid`/`usage`), then the
    /// estimate the call names (`stale_estimate`), then the cap (the worst case over the
    /// effective cap is `cap`), then the budget (a session that cannot hold it is `budget`), and
    /// the reservation of the worst case in the same step under the ledger's lock, so two calls
    /// never both pass against the same spend. The call is priced by `estimate::price`, as its
    /// estimate is; a matching estimate is no reservation, so the cap and the budget are
    /// checked as for any call.
    fn reserve(
        &self,
        recipe: &str,
        args: &Map<String, Value>,
        named: Option<&Value>,
    ) -> rascal_core::error::Result<Reservation<'_>> {
        let action = describe::action_name(recipe);
        let price = self.with_setup(|setup| estimate::price(setup, recipe, &action, args))?;
        if let Some(named) = named {
            let now = estimate::bind(
                recipe,
                &action,
                args,
                price.config_identity.as_deref(),
                self.session_id().as_deref(),
                estimate::VERSION,
                &self.paths,
            )?;
            let images = estimate::slots(recipe)?.images;
            self.check_estimate(named, &action, &now, &images)?;
        }
        // The number of tries and their total are chosen and reserved in one step under the
        // ledger's lock (SPEC.md "Self-correcting remove"), so two calls never count the same
        // balance.
        let mut ledger = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
        let held = self.held(&ledger);
        let (tries, worst_case_usd) = price.tries(Some(held)).unwrap_or((1, 0.0));
        let limits = estimate::Limits {
            worst_case_usd,
            call_cap_usd: price.cap_usd,
            session_budget_usd: held.budget_usd,
            session_held_usd: held.held_usd,
        };
        limits.check()?;
        ledger.reserved_usd += limits.worst_case_usd;
        Ok(Reservation {
            ledger: &self.ledger,
            usd: limits.worst_case_usd,
            settled: false,
            check: price.check,
            tries,
        })
    }

    /// Checks the estimate a call names against the call as it arrived (table-manners 4.5): a
    /// value that is not a binding, a binding this session never issued (another session's,
    /// which for rascal is also any other configuration's, or the command line's), one that
    /// differs from the call's recomputed binding (naming what changed) or one past its
    /// `expires` is `stale_estimate`. `now` is the call's binding, made by the function its
    /// estimate used (a task's `estimate::bind`, an operation's `ContractActions::operation_bound`); `images`
    /// name its image slots or ports, so what changed is said.
    pub(crate) fn check_estimate(
        &self,
        named: &Value,
        action: &str,
        now: &estimate::Bound,
        images: &[String],
    ) -> rascal_core::error::Result<()> {
        let stale = |reason: String| Error::StaleEstimate { reason };
        let Some(binding) = named
            .as_str()
            .filter(|b| rascal_core::identity::Identity::parse(b).is_some())
        else {
            return Err(stale(format!(
                "the estimate named, {named}, is not a binding (sha256:<64 hex>)"
            )));
        };
        let issued = self
            .issued
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(binding)
            .cloned();
        let Some(issued) = issued else {
            return Err(stale(format!(
                "this session never issued the estimate {binding} (another session's, another configuration's or the command line's); ask this server for a new estimate"
            )));
        };
        if now.binding != binding {
            return Err(stale(format!(
                "the call differs from its estimate: {}; ask for a new estimate",
                estimate::changed(images, &issued, action, &now.parts)
            )));
        }
        if run_json::now_ms() >= issued.expires_ms {
            return Err(stale(format!(
                "the estimate expired at {}; ask for a new estimate",
                run_json::rfc3339(issued.expires_ms)
            )));
        }
        Ok(())
    }

    /// The session budget and what the ledger holds against it.
    pub(crate) fn held(&self, ledger: &Ledger) -> estimate::Held {
        estimate::Held {
            budget_usd: self.budget_usd(),
            held_usd: ledger.spent_usd + ledger.reserved_usd,
        }
    }

    /// The configured session budget, none when uncapped.
    fn budget_usd(&self) -> Option<f64> {
        self.session.as_ref()?;
        self.config
            .as_ref()
            .and_then(|c| c.limits.as_ref())
            .and_then(|l| l.session_budget_usd)
    }

    /// The estimate of one call (table-manners 4.5): its arguments checked as the run checks
    /// them, priced as the limits check prices it, bound to this session, and the session
    /// observed once for both the document and the budget check. It allocates, writes and
    /// waits for nothing.
    fn estimate(
        &self,
        action: &str,
        args: &Map<String, Value>,
    ) -> rascal_core::error::Result<Value> {
        let recipe =
            recipe_of(action).ok_or_else(|| Error::usage(format!("action: no action {action}")))?;
        estimate::check_arguments(recipe, action, args, self.config.as_ref(), &self.paths)?;
        let price = self.with_setup(|setup| estimate::price(setup, recipe, action, args))?;
        let bound = estimate::bind(
            recipe,
            action,
            args,
            price.config_identity.as_deref(),
            self.session_id().as_deref(),
            estimate::VERSION,
            &self.paths,
        )?;
        let ledger = self.ledger();
        let expires_ms = run_json::now_ms() + estimate::ttl_ms();
        self.issued
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record(
                &bound.binding,
                estimate::IssuedEstimate {
                    action: action.to_string(),
                    expires_ms,
                    parts: bound.parts.clone(),
                },
            );
        let document = estimate::document(
            action,
            &bound,
            price,
            Some(self.held(&ledger)),
            self.session_view_of(&ledger),
            expires_ms,
        );
        serde_json::to_value(&document).map_err(|e| Error::usage(format!("estimate: {e}")))
    }

    /// Runs one action to its `run.json`.
    fn run_action(
        &self,
        recipe: &str,
        args: &Map<String, Value>,
        bound: &Bound,
        meta: &CallMeta,
        progress: &(dyn Fn(Progress) + Send + Sync),
        cancel: Arc<Cancel>,
    ) -> std::result::Result<ActionResult, String> {
        let client_run_id = meta.client_run_id.as_deref();
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
            session_id: self.session_id(),
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
        // The arguments first; once they check the call resolves to its invocation (4.8), which
        // every result after that carries, a refusal by the limits included.
        let action = describe::action_name(recipe);
        let mut invocation = None;
        let prepared =
            estimate::check_arguments(recipe, &action, args, self.config.as_ref(), &self.paths)
                .and_then(|()| {
                    let price =
                        self.with_setup(|setup| estimate::price(setup, recipe, &action, args))?;
                    invocation = Some(invocation::of_call(
                        recipe,
                        &action,
                        args,
                        &self.paths,
                        &price,
                    )?);
                    self.run_args(recipe, args, bound, &out, &inputs, cancel)
                })
                .and_then(|run_args| {
                    self.reserve(recipe, args, meta.estimate.as_ref())
                        .map(|reservation| (run_args, reservation))
                });
        // The image file the run read, which a Mask it makes is published with.
        let mut image: Option<PathBuf> = None;
        let (pending, ended, reservation) = match prepared {
            Err(error) => {
                let pending = run_json::Pending {
                    id: id.clone(),
                    action: describe::action_name(recipe),
                    dir: out.clone(),
                    workspace: out.clone(),
                    fresh: true,
                    started_ms,
                    session_id: self.session_id(),
                    client_run_id: client_run_id.map(str::to_string),
                    input_identity: format!(
                        "sha256:{}",
                        run_json::input_identity(&Value::Object(args.clone()))
                    ),
                    images_in: Vec::new(),
                    locations_used: Vec::new(),
                    invocation,
                    notes: meta.notes.clone(),
                };
                (pending, run_json::ended_with(recipe, &error), None)
            }
            Ok((run_args, reservation)) => {
                image = Some(run_args.image.clone());
                let mut pending =
                    run_json::pending_for(&run_args, filler.as_deref(), reservation.check.record());
                pending.invocation = invocation;
                pending.id = id.clone();
                pending.started_ms = started_ms;
                pending.session_id = self.session_id();
                pending.client_run_id = client_run_id.map(str::to_string);
                pending.notes = meta.notes.clone();
                let done = AtomicBool::new(false);
                let journal = out.join("journal.jsonl");
                let ended = std::thread::scope(|scope| {
                    scope.spawn(|| watch_journal(&journal, &done, progress));
                    let ended = self.execute(
                        recipe,
                        run_args,
                        filler.as_deref(),
                        &reservation.check,
                        reservation.tries,
                    );
                    done.store(true, Ordering::SeqCst);
                    ended
                });
                (pending, ended, Some(reservation))
            }
        };
        let cancelled = ended.status == rascal_core::contract::table_manners::Status::Cancelled;
        let publish = |identity: &str| match &image {
            Some(image) => self.publish(&out, image, identity),
            None => Ok(()),
        };
        let spent = run_json::conclude_publishing(&pending, ended, Some(&publish));
        if let Some(reservation) = reservation {
            reservation.settle(spent);
        }
        if cancelled {
            return Err("cancelled".to_string());
        }
        self.result_of(&id, args)
    }

    /// Publishes the Mask `identity` a task made into the session's store (PLAN.md 10.8), so its
    /// reference binds there: the image file the run read first (by its digest, unless the
    /// store holds it; in the projection of the Mask's root frame, so the frames agree), then
    /// the Mask with its frame chain through 10.5's bundle import (in memory, the same checks,
    /// idempotent by identity: a Mask already there adds no record). The preview the run drew
    /// seeds the artifact resource's cache, so the resource gives the same bytes.
    fn publish(
        &self,
        run_dir: &Path,
        image: &Path,
        identity: &str,
    ) -> rascal_core::error::Result<()> {
        let ws = Workspace::open_read_only(run_dir)?;
        let frame = ws.load(identity)?.frame().to_string();
        let frames = ws.frames()?;
        let chain = frames.chain(&frame)?;
        let root = chain
            .last()
            .ok_or_else(|| Error::usage(format!("frame {frame} has no root")))?;
        let bytes = std::fs::read(image).map_err(|e| Error::io(image, e))?;
        let file = format!("sha256:{}", rascal_core::contract::file_sha256(&bytes));
        if self
            .store
            .resolve(rascal_core::artifact::ArtifactType::Image, &file)?
            .is_none()
        {
            self.store.import(image, Some(root.projection))?;
        }
        let bundle = ws.export_mask(identity)?;
        let source = run_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        self.store
            .import_mask_bytes(&bundle.bytes, identity, &source, Some(&bundle.sha256))?;
        if let Ok(preview) = std::fs::read(run_dir.join(run_json::PREVIEW_FILE)) {
            artifact_resource::seed(identity, &preview)?;
        }
        Ok(())
    }

    /// Runs the recipe, through `heal` when a filler is named, checked as `check` says over at
    /// most `tries` tries.
    fn execute(
        &self,
        recipe: &str,
        run_args: run::RunArgs,
        filler: Option<&str>,
        check: &rascal_mask::checker::Check,
        tries: u32,
    ) -> Ended {
        match filler {
            Some(filler) => match heal::heal(&heal::HealArgs {
                run: run_args,
                filler: filler.to_string(),
                no_dash: true,
                replay: None,
                cache: None,
                check: check.clone(),
                tries,
            }) {
                Ok(report) if report.cancelled.is_some() => run_json::ended_cancelled(
                    recipe,
                    report.cancelled.as_deref().unwrap_or_default(),
                    report.tries,
                ),
                Ok(report) => Ended {
                    verdict: report.verdict,
                    tries: report.tries,
                    ..run_json::ended_ok(
                        recipe,
                        report.status,
                        report.stopped_at.as_deref(),
                        report.reason.as_deref(),
                        None,
                        Some(filler),
                        report.renders,
                        report.result,
                        report.cache,
                    )
                },
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

    /// The run's `run.json` as the call's result, its images (`images.out`, and the previews in
    /// `artifacts`) located as the caller asked; the image blocks, when asked, are those of
    /// `images.out`, the previews among them.
    pub(crate) fn result_of(
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
        let blocks_asked = args.get("image_blocks").and_then(|v| v.as_bool()) == Some(true);
        let mut blocks = Vec::new();
        let locate = |out: &mut Value, blocks: Option<&mut Vec<Vec<u8>>>| {
            let Some(file) = out.get("path").and_then(|p| p.as_str()).map(PathBuf::from) else {
                return;
            };
            let role = out["role"].as_str().unwrap_or("image").to_string();
            let bytes = std::fs::read(&file).ok();
            if let (Some(blocks), Some(bytes)) = (blocks, &bytes) {
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
        };
        if let Some(outs) = structured
            .pointer_mut("/images/out")
            .and_then(|o| o.as_array_mut())
        {
            for out in outs {
                locate(out, blocks_asked.then_some(&mut blocks));
            }
        }
        for list in ["/artifacts/in", "/artifacts/out"] {
            if let Some(entries) = structured.pointer_mut(list).and_then(|o| o.as_array_mut()) {
                for entry in entries {
                    if let Some(preview) = entry.get_mut("preview").filter(|p| p.is_object()) {
                        locate(preview, None);
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
    fn session_view(&self) -> Option<TmSession> {
        self.session_view_of(&self.ledger())
    }

    /// The session's id; none without a session (`rascal call`).
    pub(crate) fn session_id(&self) -> Option<String> {
        self.session.as_ref().map(|s| s.id.clone())
    }

    /// The session observation for one copy of the ledger; none without a session.
    pub(crate) fn session_view_of(&self, ledger: &Ledger) -> Option<TmSession> {
        let session = self.session.as_ref()?;
        let budget = self.budget_usd();
        Some(TmSession {
            contract: rascal_core::contract::CONTRACT_VERSION.to_string(),
            tool: TmTool {
                name: "rascal".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            id: session.id.clone(),
            started: run_json::rfc3339(session.started_ms),
            budget_usd: budget.map(usd),
            spent_usd: usd(ledger.spent_usd),
            unresolved_usd: usd(ledger.unresolved_usd),
            reserved_usd: usd(ledger.reserved_usd),
            remaining_usd: budget
                .map(|b| usd((b - ledger.spent_usd - ledger.reserved_usd).max(0.0))),
            allowed_dirs: self
                .paths
                .dirs()
                .iter()
                .map(|d| d.display().to_string())
                .collect(),
        })
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

/// The `not_ready` refusal for a call whose readiness is not `ready`: a JSON-RPC `-32602` with
/// `{reason: "not_ready", run: null, requirements}` naming exactly the missing or invalid
/// requirements (`refusal.json`). None when the call is ready.
pub fn not_ready(action: &str, readiness: &Readiness) -> Option<CallError> {
    let unmet: Vec<Requirement> = readiness
        .requirements
        .iter()
        .filter(|r| readiness::unmet(r))
        .cloned()
        .collect();
    if unmet.is_empty() {
        return None;
    }
    let refusal = Refusal {
        reason: RefusalReason::NotReady,
        run: None,
        requirements: Some(unmet.clone()),
        error: None,
        bindings: None,
    };
    Some(CallError::refused(
        format!("{action} is not ready: {}", readiness::summary(&unmet)),
        serde_json::to_value(&refusal).unwrap_or(Value::Null),
    ))
}

/// The `invalid_arguments` refusal for an estimate's wrong request (table-manners 4.5): a
/// JSON-RPC `-32602` with `{reason: "invalid_arguments", run: null, error}`, the error's code
/// as the run would have given it (`input` for an image it cannot take, else `usage`), never
/// retryable.
pub fn invalid_arguments(action: &str, error: &Error) -> CallError {
    let (_, mut info) = run_json::error_of(error);
    if info.code != ErrorCode::Usage {
        info.code = ErrorCode::Input;
    }
    info.retryable = false;
    let refusal = Refusal {
        reason: RefusalReason::InvalidArguments,
        run: None,
        requirements: None,
        error: Some(info),
        bindings: None,
    };
    CallError::refused(
        format!("estimate of {action}: {error}"),
        serde_json::to_value(&refusal).unwrap_or(Value::Null),
    )
}

/// What a run was asked as, written into its inputs directory when it is allocated, before any
/// paid work (table-manners 4.3): the runs list reads it for a run still going.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Association {
    pub(crate) action: String,
    pub(crate) session_id: Option<String>,
    pub(crate) client_run_id: Option<String>,
}

impl Association {
    pub(crate) const FILE: &str = "association.json";
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
        // One tool per action, in the description's order (CONTRACT 5): a task's input is its
        // params_schema with its image and location slots, an operation's its params_schema
        // with its in-ports (4.7).
        for action in description["actions"].as_array().into_iter().flatten() {
            let name = action["name"].as_str().unwrap_or_default().to_string();
            let operation = action["kind"] == "operation";
            let text = format!(
                "{} {} (table-manners {}; the result is the run's run.json.)",
                action["title"].as_str().unwrap_or_default(),
                action["purpose"].as_str().unwrap_or_default(),
                if operation { "operation" } else { "action" },
            );
            let schema = if operation {
                self.operation_schema(action)
            } else {
                Self::input_schema(action)
            };
            tools.push(Tool::new(name, text, Arc::new(schema)));
        }
        // `estimate` is reserved (table-manners 3, 4.5, 5): a tool, never an action. `action` is
        // a string, not an enum, so an unknown action reaches rascal's own check and its
        // `invalid_arguments` refusal.
        let estimate = json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "description": "The action the call would run." },
                "arguments": { "type": "object", "description": "Exactly the arguments the call would send, images included." }
            },
            "required": ["action", "arguments"],
            "additionalProperties": false,
        });
        tools.push(Tool::new(
            "estimate",
            "The worst case one exact call would spend, the per-call cap, whether the limits would allow it now and a binding to send back with the call (table-manners estimate); runs nothing.",
            Arc::new(estimate.as_object().cloned().unwrap_or_default()),
        ));
        // `export` and `import` are reserved controls (5.1): tools, never actions.
        tools.extend(crate::transfer::tools());
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
        name == "describe"
            || name == "estimate"
            || crate::transfer::handles(name)
            || recipe_of(name).is_some()
            || self.is_operation(name)
    }

    fn exclusive(&self, name: &str) -> bool {
        recipe_of(name).is_some() || self.is_operation(name)
    }

    fn call(
        &self,
        name: &str,
        args: &Map<String, Value>,
        meta: &CallMeta,
        progress: &(dyn Fn(Progress) + Send + Sync),
        cancel: Arc<Cancel>,
    ) -> std::result::Result<ActionResult, CallError> {
        if name == "describe" {
            return Ok(ActionResult {
                structured: self.description().map_err(|e| e.to_string())?,
                is_error: false,
                images: Vec::new(),
            });
        }
        if crate::transfer::handles(name) {
            return crate::transfer::call(name, args, &self.store, &self.paths);
        }
        if name == "estimate" {
            // What the input schema rejects is plain invalid params; any other wrong request
            // is the `invalid_arguments` refusal (table-manners 4.5).
            let (action, arguments) = estimate::request(args).ok_or_else(|| {
                CallError::from(
                    "estimate: the arguments are {\"action\": <name>, \"arguments\": {…}}",
                )
            })?;
            if self.is_operation(action) {
                return Ok(ActionResult {
                    structured: self.estimate_operation(action, arguments)?,
                    is_error: false,
                    images: Vec::new(),
                });
            }
            return Ok(ActionResult {
                structured: self
                    .estimate(action, arguments)
                    .map_err(|e| invalid_arguments(action, &e))?,
                is_error: false,
                images: Vec::new(),
            });
        }
        if self.is_operation(name) {
            // A built-in op reads no configuration: always ready (its readiness is `ready` with
            // no requirement).
            return self.call_operation(name, args, meta, progress, cancel);
        }
        let recipe = recipe_of(name).ok_or_else(|| format!("no action {name}"))?;
        self.ready_for(name, args)?;
        let (args, bound) = self.bind(recipe, args)?;
        let result = self
            .run_action(recipe, &args, &bound, meta, progress, cancel)
            .map_err(CallError::from)?;
        self.learn(&result.structured);
        Ok(result)
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
            (artifact_resource::TEMPLATE, "artifact", "application/json"),
            (artifact_resource::PREVIEW_TEMPLATE, "preview", "image/png"),
        ]
        .into_iter()
        .map(|(uri, name, mime)| {
            let mut t = ResourceTemplate::new(uri, name);
            t.mime_type = Some(mime.into());
            t
        })
        .collect()
    }

    fn read(&self, uri: &str) -> std::result::Result<Option<ResourceBody>, CallError> {
        if let Some(answer) = artifact_resource::read(&self.store, &self.ops, uri) {
            return answer.map(Some);
        }
        Ok(self.read_run_resource(uri))
    }
}

impl ContractActions {
    /// The runs list, the session and a run's resources, when `uri` names one.
    fn read_run_resource(&self, uri: &str) -> Option<ResourceBody> {
        if uri == "rascal://session" {
            return Some(ResourceBody::Text {
                mime: "application/json".into(),
                text: serde_json::to_string_pretty(&self.session_view()?).ok()?,
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
