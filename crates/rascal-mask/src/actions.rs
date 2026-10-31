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
use rascal_core::contract::table_manners::{RunState, RunSummary, RunsList, Tool as TmTool};
use rascal_core::workspace::Cancel;
use rascal_mcp::server::{
    ActionResult, Actions, McpResource as Resource, McpResourceTemplate as ResourceTemplate,
    McpTool as Tool, ResourceBody,
};
use serde_json::{Map, Value, json};

use crate::run_json::{self, Ended};
use crate::{heal, run};
use rascal_mask::describe::{self, INLINE_BYTES, recipe_of};

/// The contract's actions over the shipped recipes, with runs under `runs`.
pub struct ContractActions {
    pub config_path: Option<PathBuf>,
    pub config: Option<Config>,
    pub runs: PathBuf,
}

/// The default runs root: `$XDG_DATA_HOME/rascal/runs`, else `~/.local/share/rascal/runs`.
pub fn default_runs_root() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rascal/runs")
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

    /// Runs one action to its `run.json`.
    fn run_action(
        &self,
        recipe: &str,
        args: &Map<String, Value>,
        progress: &(dyn Fn(String) + Send + Sync),
        cancel: Arc<Cancel>,
    ) -> std::result::Result<ActionResult, String> {
        std::fs::create_dir_all(&self.runs).map_err(|e| e.to_string())?;
        let started_ms = run_json::now_ms();
        let id = run_json::new_id(started_ms);
        let out = self.runs.join(&id);
        let inputs = self.runs.join(format!("{id}.in"));
        let filler = args
            .get("filler")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let _ = std::fs::create_dir_all(&inputs);
        let _ = std::fs::write(inputs.join("action"), describe::action_name(recipe));
        progress(format!("run {id} started (rascal://run/{id}/run.json)"));
        let prepared = self.run_args(recipe, args, &out, &inputs, cancel);
        let (pending, ended) = match prepared {
            Err(error) => {
                std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
                let pending = run_json::Pending {
                    id: id.clone(),
                    action: describe::action_name(recipe),
                    dir: out.clone(),
                    workspace: out.clone(),
                    fresh: true,
                    started_ms,
                    input_identity: format!(
                        "sha256:{}",
                        run_json::input_identity(&Value::Object(args.clone()))
                    ),
                    images_in: Vec::new(),
                    locations_used: Vec::new(),
                };
                (pending, run_json::ended_with(recipe, &error))
            }
            Ok(run_args) => {
                let mut pending = run_json::pending_for(&run_args, filler.as_deref());
                pending.id = id.clone();
                pending.started_ms = started_ms;
                let done = AtomicBool::new(false);
                let journal = out.join("journal.jsonl");
                let ended = std::thread::scope(|scope| {
                    scope.spawn(|| watch_journal(&journal, &done, progress));
                    let ended = self.execute(recipe, run_args, filler.as_deref());
                    done.store(true, Ordering::SeqCst);
                    ended
                });
                (pending, ended)
            }
        };
        let cancelled = ended.status == rascal_core::contract::table_manners::Status::Cancelled;
        run_json::conclude(&pending, ended);
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

    fn runs_list(&self) -> RunsList {
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
            let ms: u128 = name
                .split('-')
                .nth(1)
                .and_then(|p| p.parse().ok())
                .unwrap_or(0);
            let done: Option<Value> = std::fs::read_to_string(entry.path().join(run_json::FILE))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok());
            let action = done
                .as_ref()
                .and_then(|d| d["action"].as_str().map(str::to_string))
                .or_else(|| {
                    std::fs::read_to_string(self.runs.join(format!("{name}.in")).join("action"))
                        .ok()
                })
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
                    action,
                    status,
                    started: run_json::rfc3339(ms),
                    resource: format!("rascal://run/{name}/run.json"),
                },
            ));
        }
        runs.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.id.cmp(&a.1.id)));
        RunsList {
            contract: rascal_core::contract::CONTRACT_VERSION.to_string(),
            tool: TmTool {
                name: "rascal".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            runs: runs.into_iter().map(|(_, r)| r).collect(),
        }
    }
}

/// Sends a progress line for every op the run journals, until `done`.
fn watch_journal(journal: &Path, done: &AtomicBool, progress: &(dyn Fn(String) + Send + Sync)) {
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
                    progress(format!("step {step} ({op}): {status}"));
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
        name == "describe" || recipe_of(name).is_some()
    }

    fn call(
        &self,
        name: &str,
        args: &Map<String, Value>,
        progress: &(dyn Fn(String) + Send + Sync),
        cancel: Arc<Cancel>,
    ) -> std::result::Result<ActionResult, String> {
        if name == "describe" {
            return Ok(ActionResult {
                structured: self.description().map_err(|e| e.to_string())?,
                is_error: false,
                images: Vec::new(),
            });
        }
        let recipe = recipe_of(name).ok_or_else(|| format!("no action {name}"))?;
        self.run_action(recipe, args, progress, cancel)
    }

    fn resources(&self) -> Vec<Resource> {
        let mut runs = Resource::new("rascal://runs", "runs");
        runs.description = Some("Recent runs, newest first (table-manners 4.3).".into());
        runs.mime_type = Some("application/json".into());
        vec![runs]
    }

    fn templates(&self) -> Vec<ResourceTemplate> {
        [
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
        if uri == "rascal://runs" {
            return Some(ResourceBody::Text {
                mime: "application/json".into(),
                text: serde_json::to_string_pretty(&self.runs_list()).ok()?,
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
