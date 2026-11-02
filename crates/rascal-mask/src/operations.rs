//! An operation call is a run (CONTRACT 4.7; PLAN.md 10.6): every built-in op is served as an
//! MCP tool whose input is its `params_schema` plus one property per in-port, and a call runs
//! through the machinery the recipe actions use (the claim, the permit, cancellation, a run
//! directory under the runs root with `run.json` written atomically and last, progress with the
//! run's reference), against the session's store.
//!
//! - **Ports.** An in-port takes an artifact reference (3.4), a non-empty array of them for a
//!   `many` port; an `image` port also takes an image in (10.1, by path under the path rule or
//!   inline, with an optional `expect`), and a `location` port a location (9), contract or
//!   rascal's own, validated against the call's image and registered in the store as `workspace
//!   import-location` does. An object with an `identity` is a reference; anything else on a port
//!   that takes no value is invalid params.
//! - **Binding.** Before any run, every reference is bound in the store (10.3's `bind`: type,
//!   version, resolution) and the frames of the bound inputs are checked with the op's
//!   [`Op::check_frames`](rascal_core::op::Op::check_frames) (`wrong_frame`). A call that does
//!   not bind is the `binding` refusal naming every problem, in port order and, within a `many`
//!   port, in the order given; it allocates nothing and releases the call's `client_run_id`.
//! - **The run.** Images and locations given by value are read and registered once the run
//!   exists (a path the rule refuses, or an image off its `expect`, is `invalid`/`input`); the
//!   frame check then runs again over every input. The op runs through `Workspace::invoke_with`
//!   on the store, step id the run's id, so its outputs commit there (one operation record) and
//!   resolve by identity at once. Upstream steps never rerun. The run directory holds the run's
//!   journal line, its images out and the previews, never a workspace.
//! - **The result.** `ok` and `measured_nothing` are `completed`; `not_applicable` is
//!   `completed` with the reason in `notes` and nothing out; `invalid_args` is
//!   `invalid`/`usage`; a failure is `error` with its code. `artifacts.in` holds each input bound
//!   or registered, `artifacts.out` each output (a `many` port once per item, in order), each
//!   with its reference and, for a type that declares one, a preview (`crate::preview`) in the
//!   run directory, also listed in `images.out`. Images taken appear in `images.in` (slot and
//!   role the port's name) and made in `images.out` (role the port's name); locations taken in
//!   `locations.used` and made in `locations.found`. The op's envelope is the `body`.
//! - **The invocation** (4.8): `arguments` are the parameters as the op ran them (the constant
//!   set's values and the manifest's defaults applied, `constants` named), each artifact as the
//!   store's reference, each image as `{"expect": <reference>}` and each location as given;
//!   `effective` names the configuration and, in `x-rascal`, the constant set; no requirement.
//! - **The estimate** (4.5; PLAN.md 10.9): the same ports and binding, the same parameters,
//!   allowed at 0 and ready; its binding digests each reference by identity and each image by
//!   file digest, and a call naming it is checked against the call's binding.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::contract::table_manners::images::{
    ImageOut, ImageReference, ImageTaken, Images, Location as TmLocation, Locations, Locator,
};
use rascal_core::contract::table_manners::operations::{
    ArtifactEntry, ArtifactRef, Artifacts, Binding, BindingProblem,
};
use rascal_core::contract::table_manners::{Effective, Invocation, Refusal, RefusalReason, Status};
use rascal_core::contract::{self, LocationMeta};
use rascal_core::frame::{Frame, FrameTable};
use rascal_core::journal::{Event, Journal, JournalLine};
use rascal_core::op::{Framed, Manifest, Op, Param, ParamKind, Params, Port};
use rascal_core::workspace::{Cancel, Resolved};
use rascal_mask::filler::Spent;
use rascal_mcp::server::{ActionResult, CallError, CallMeta, Progress};
use serde_json::{Map, Value, json};

use crate::actions::{Association, ContractActions, image_in_schema};
use crate::estimate::{self, DELIVERY};
use crate::preview;
use crate::run_json::{self, Pending, Produced};

/// Under the testkit, how long an operation's run waits before its op runs, in milliseconds
/// (`RASCAL_TESTKIT_OPERATION_DELAY_MS`): a slow operation, so a test can cancel one while it
/// is queued or running. Checked for cancellation every few milliseconds.
fn testkit_delay() -> Option<std::time::Duration> {
    #[cfg(feature = "testkit")]
    if let Some(ms) = std::env::var("RASCAL_TESTKIT_OPERATION_DELAY_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        return Some(std::time::Duration::from_millis(ms));
    }
    None
}

/// One in-port item as the call gave it.
#[derive(Debug, Clone)]
enum Given {
    /// An artifact reference (3.4).
    Reference(ArtifactRef),
    /// An image in (10.1): `path` or `inline`, with an optional `expect`.
    Image(Value),
    /// A location (9), contract or rascal's own.
    Location(Value),
}

/// One in-port item: its port, its position in a `many` port, the type the port takes and
/// what was given.
#[derive(Debug, Clone)]
struct Item {
    port: String,
    index: Option<usize>,
    artifact_type: ArtifactType,
    given: Given,
}

impl Item {
    /// The input's name for the op: the port, or `<port>_<n>` for an item of a `many` port.
    fn input_name(&self) -> String {
        match self.index {
            Some(n) => format!("{}_{n}", self.port),
            None => self.port.clone(),
        }
    }

    fn label(&self) -> String {
        match self.index {
            Some(n) => format!("{}[{n}]", self.port),
            None => self.port.clone(),
        }
    }
}

/// An input the run holds: its item, its local id in the store, the artifact, and for an image
/// its reference (8).
struct Held {
    item: Item,
    id: String,
    artifact: Artifact,
    image: Option<ImageReference>,
}

/// The schema of an in-port's value (4.7): a reference, a non-empty array of them for a `many`
/// port, and for an `image` port also an image in, for a `location` port also a location.
fn port_schema(port: &Port) -> Value {
    let contract_type = port.artifact_type.contract_type();
    let mut reference = rascal_mask::describe::mask_reference_schema();
    reference["description"] = json!(format!(
        "A {contract_type} artifact reference (CONTRACT 3.4); it resolves by identity in this server's workspace (rascal serve --workspace)."
    ));
    let one = match port.artifact_type {
        ArtifactType::Image => json!({ "anyOf": [
            reference,
            image_in_schema("An image by path or inline (CONTRACT 10.1), with an optional expect; its projection is expect's, else an exactly 2:1 image is equirectangular."),
        ] }),
        ArtifactType::Location => json!({ "anyOf": [
            reference,
            { "type": "object", "description": "A location (CONTRACT 9) on the call's image, or rascal's own location in its frame." },
        ] }),
        _ => reference,
    };
    let mut schema = if port.many {
        json!({ "type": "array", "minItems": 1, "items": one })
    } else {
        one
    };
    schema["description"] = json!(format!(
        "{} ({contract_type}{})",
        port.purpose,
        if port.many {
            ", one or more, in order"
        } else {
            ""
        }
    ));
    schema
}

/// A JSON argument as the TOML value a parameter takes.
fn json_param(param: &Param, value: &Value) -> rascal_core::error::Result<toml::Value> {
    let bad = |what: &str| Error::usage(format!("{}: {value} is not {what}", param.name));
    let numbers = |v: &Value, what: &str| -> rascal_core::error::Result<toml::Value> {
        Ok(toml::Value::Array(
            v.as_array()
                .ok_or_else(|| bad(what))?
                .iter()
                .map(|n| n.as_f64().map(toml::Value::Float).ok_or_else(|| bad(what)))
                .collect::<rascal_core::error::Result<_>>()?,
        ))
    };
    Ok(match param.kind {
        ParamKind::Number => toml::Value::Float(value.as_f64().ok_or_else(|| bad("a number"))?),
        ParamKind::Integer => {
            toml::Value::Integer(value.as_i64().ok_or_else(|| bad("an integer"))?)
        }
        ParamKind::Boolean => {
            toml::Value::Boolean(value.as_bool().ok_or_else(|| bad("true or false"))?)
        }
        ParamKind::String | ParamKind::Choice => {
            toml::Value::String(value.as_str().ok_or_else(|| bad("a string"))?.to_string())
        }
        ParamKind::NumberArray => numbers(value, "an array of numbers")?,
        ParamKind::IntegerList => toml::Value::Array(
            value
                .as_array()
                .ok_or_else(|| bad("a list of integers"))?
                .iter()
                .map(|v| {
                    v.as_i64()
                        .map(toml::Value::Integer)
                        .ok_or_else(|| bad("a list of integers"))
                })
                .collect::<rascal_core::error::Result<_>>()?,
        ),
        ParamKind::BoxList => toml::Value::Array(
            value
                .as_array()
                .ok_or_else(|| bad("a list of [x0, y0, x1, y1] boxes"))?
                .iter()
                .map(|b| match b.as_array() {
                    Some(items) if items.len() == 4 => {
                        numbers(b, "a list of [x0, y0, x1, y1] boxes")
                    }
                    _ => Err(bad("a list of [x0, y0, x1, y1] boxes")),
                })
                .collect::<rascal_core::error::Result<_>>()?,
        ),
        ParamKind::PointList => toml::Value::Array(
            value
                .as_array()
                .ok_or_else(|| bad("a list of [x, y] points"))?
                .iter()
                .map(|p| match p.as_array() {
                    Some(pair) if pair.len() == 2 => numbers(p, "a list of [x, y] points"),
                    _ => Err(bad("a list of [x, y] points")),
                })
                .collect::<rascal_core::error::Result<_>>()?,
        ),
    })
}

/// The `binding` refusal naming every problem (4.7), as a JSON-RPC invalid-params error.
pub fn binding_refusal(bindings: Vec<Binding>) -> CallError {
    let text: Vec<String> = bindings
        .iter()
        .map(|b| {
            let port = match (&b.port, b.index) {
                (Some(port), Some(n)) => format!("{port}[{n}]"),
                (Some(port), None) => port.clone(),
                (None, _) => "artifact".to_string(),
            };
            format!(
                "{port}: {} ({})",
                serde_json::to_value(b.problem)
                    .ok()
                    .and_then(|p| p.as_str().map(str::to_string))
                    .unwrap_or_default(),
                b.got
            )
        })
        .collect();
    let refusal = Refusal {
        reason: RefusalReason::Binding,
        run: None,
        requirements: None,
        error: None,
        bindings: Some(bindings),
    };
    CallError::refused(
        format!("the references do not bind: {}", text.join("; ")),
        serde_json::to_value(&refusal).unwrap_or(Value::Null),
    )
}

/// What a run took and made so far, for its `run.json` whatever its outcome.
#[derive(Default)]
struct Account {
    images_in: Vec<ImageTaken>,
    images_out: Vec<ImageOut>,
    artifacts_in: Vec<ArtifactEntry>,
    artifacts_out: Vec<ArtifactEntry>,
    used: Vec<TmLocation>,
    found: Vec<TmLocation>,
    notes: Vec<String>,
}

/// How an operation's run ended.
struct Outcome {
    status: Status,
    error: Option<rascal_core::contract::table_manners::ErrorInfo>,
    body: Map<String, Value>,
}

impl Outcome {
    fn failed(error: &Error, envelope: Option<Value>) -> Outcome {
        let (status, info) = run_json::error_of(error);
        let mut body = Map::new();
        if let Some(envelope) = envelope {
            body.insert("envelope".to_string(), envelope);
        }
        if let Error::Cancelled { step } = error {
            body.insert("stopped_before".to_string(), json!(step));
            return Outcome {
                status: Status::Cancelled,
                error: None,
                body,
            };
        }
        Outcome {
            status,
            error: Some(info),
            body,
        }
    }
}

impl ContractActions {
    /// Whether `name` is one of the description's actions: a shipped recipe's task or an
    /// operation (never a reserved tool).
    pub(crate) fn is_action(&self, name: &str) -> bool {
        rascal_mask::describe::recipe_of(name).is_some() || self.is_operation(name)
    }

    /// Whether `name` is an operation: a built-in op.
    pub(crate) fn is_operation(&self, name: &str) -> bool {
        self.ops.get(name).is_some()
    }

    /// An operation's MCP input schema (4.7, 10.2): its `params_schema`, one property per
    /// in-port ([`port_schema`]), required when the port is, and the delivery choices.
    pub(crate) fn operation_schema(&self, action: &Value) -> Map<String, Value> {
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
        if let Some(op) = action["name"].as_str().and_then(|n| self.ops.get(n)) {
            for port in &op.manifest().inputs {
                properties.insert(port.name.clone(), port_schema(port));
                if port.required {
                    required.push(Value::String(port.name.clone()));
                }
            }
        }
        properties.insert(
            "return_images".into(),
            json!({ "type": "string", "enum": ["path", "inline", "resource"], "description": "How returned images (the previews among them) are located; path by default." }),
        );
        properties.insert(
            "image_blocks".into(),
            json!({ "type": "boolean", "description": "Also send each returned image as an MCP image block." }),
        );
        schema.insert("properties".into(), Value::Object(properties));
        schema.insert("required".into(), Value::Array(required));
        schema
    }

    /// Calls an operation: its ports read and bound before any run, then the run.
    pub(crate) fn call_operation(
        &self,
        name: &str,
        args: &Map<String, Value>,
        meta: &CallMeta,
        progress: &(dyn Fn(Progress) + Send + Sync),
        cancel: Arc<Cancel>,
    ) -> Result<ActionResult, CallError> {
        let op = self
            .ops
            .get(name)
            .ok_or_else(|| CallError::from(format!("no tool {name}")))?;
        let manifest = op.manifest();
        let items = items(&manifest, args)?;
        let bound = self.bind_ports(op, &items)?;
        self.run_operation(op, &manifest, args, items, bound, meta, progress, cancel)
            .map_err(CallError::from)
    }

    /// The estimate of an operation call (4.5; PLAN.md 10.9): its ports read and bound as the
    /// call binds them (one function, so a call that does not bind gets the same `binding`
    /// refusal), its parameters resolved as the run resolves them, then a document allowed at a
    /// worst case of 0 under the configured cap, ready, naming no model and, in `x-rascal`, the
    /// constant set it would use. Its binding ([`ContractActions::operation_bound`]) is recorded,
    /// so a call naming it is checked as a task's. Nothing is read into the store.
    pub(crate) fn estimate_operation(
        &self,
        name: &str,
        args: &Map<String, Value>,
    ) -> Result<Value, CallError> {
        let wrong = |e: Error| crate::actions::invalid_arguments(name, &e);
        let op = self
            .ops
            .get(name)
            .ok_or_else(|| wrong(Error::usage(format!("action: no action {name}"))))?;
        let manifest = op.manifest();
        let items = items(&manifest, args).map_err(|e| wrong(Error::usage(e.message)))?;
        self.bind_ports(op, &items)?;
        let (_, resolution) = resolve(op, &manifest, args).map_err(wrong)?;
        let bound = self
            .operation_bound(&manifest, args, &items)
            .map_err(wrong)?;
        let cap_usd = estimate::effective_cap(self.config.as_ref(), args).map_err(wrong)?;
        let config_identity = self.config_state.identity();
        let ledger = self.ledger();
        let expires_ms = run_json::now_ms() + estimate::ttl_ms();
        self.issued
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record(
                &bound.binding,
                estimate::IssuedEstimate {
                    action: name.to_string(),
                    expires_ms,
                    parts: bound.parts.clone(),
                },
            );
        let price = estimate::Price {
            readiness: rascal_core::contract::table_manners::Readiness {
                state: rascal_core::contract::table_manners::ReadinessState::Ready,
                requirements: vec![],
            },
            model: None,
            config_identity: config_identity.clone(),
            filler: Value::Null,
            constants: BTreeMap::new(),
            worst_case_usd: Some(0.0),
            cap_usd,
        };
        let mut document = estimate::document(
            name,
            &bound,
            price,
            Some(self.held(&ledger)),
            self.session_view_of(&ledger),
            expires_ms,
        );
        let mut extensions = BTreeMap::new();
        extensions.insert(
            "x-rascal".to_string(),
            json!({ "constants": resolution.params.get("constants").and_then(|c| c.as_str()) }),
        );
        document.effective = Effective {
            model: None,
            config_identity,
            extensions,
        };
        serde_json::to_value(&document).map_err(|e| wrong(Error::usage(format!("estimate: {e}"))))
    }

    /// The binding of an operation call (4.5; 12.5's recipe, extended): a digest over the
    /// action, each argument (delivery left out) with every in-port's reference by its identity
    /// and every image by its file digest (a `many` port's items in order), the version, the
    /// configuration's identity and the session, so an estimate and the call it binds agree.
    fn operation_bound(
        &self,
        manifest: &Manifest,
        args: &Map<String, Value>,
        items: &[Item],
    ) -> rascal_core::error::Result<estimate::Bound> {
        let mut entered_ports: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
        for item in items {
            let label = match item.index {
                Some(n) => format!("{}[{n}]", item.port),
                None => item.port.clone(),
            };
            let entered = match &item.given {
                Given::Reference(reference) => json!({ "artifact": reference.identity }),
                Given::Image(value) => {
                    let image = estimate::image_in(&label, value, &self.paths)?;
                    let bytes = image.bytes()?;
                    estimate::verify_expect(&label, value, &image, &bytes)?;
                    json!({ "file_sha256": rascal_core::contract::file_sha256(&bytes) })
                }
                Given::Location(value) => value.clone(),
            };
            entered_ports.entry(&item.port).or_default().push(entered);
        }
        let mut arguments = Map::new();
        let mut parts = BTreeMap::new();
        for (key, value) in args {
            if DELIVERY.contains(&key.as_str()) {
                continue;
            }
            let entered = match manifest.inputs.iter().find(|p| &p.name == key) {
                Some(port) => {
                    let mut values = entered_ports.remove(key.as_str()).unwrap_or_default();
                    if port.many {
                        Value::Array(values)
                    } else {
                        values.pop().unwrap_or(Value::Null)
                    }
                }
                None => value.clone(),
            };
            parts.insert(
                key.clone(),
                rascal_core::canonical::identity("estimate-argument", &entered)
                    .as_str()
                    .to_string(),
            );
            arguments.insert(key.clone(), entered);
        }
        let payload = json!({
            "action": manifest.name,
            "arguments": arguments,
            "tool_version": estimate::VERSION,
            "config_identity": self.config_state.identity(),
            "session_id": self.session_id(),
        });
        Ok(estimate::Bound {
            binding: rascal_core::canonical::identity("estimate", &payload)
                .as_str()
                .to_string(),
            parts,
        })
    }

    /// Binds every reference among `items` (10.3) and checks the bound inputs' frames
    /// together (`wrong_frame`); every problem in one `binding` refusal, in item order.
    fn bind_ports(
        &self,
        op: &dyn Op,
        items: &[Item],
    ) -> Result<BTreeMap<usize, Resolved>, CallError> {
        let store_error = |e: Error| CallError::from(format!("the store: {e}"));
        let mut problems: BTreeMap<usize, Binding> = BTreeMap::new();
        let mut bound: BTreeMap<usize, Resolved> = BTreeMap::new();
        for (at, item) in items.iter().enumerate() {
            let Given::Reference(reference) = &item.given else {
                continue;
            };
            let given = rascal_core::binding::Given {
                port: &item.port,
                index: item.index.map(|n| n as u32),
                expected: item.artifact_type,
                reference,
            };
            match rascal_core::binding::bind(&self.store, given).map_err(store_error)? {
                Ok(resolved) => {
                    bound.insert(at, resolved);
                }
                Err(problem) => {
                    problems.insert(at, problem);
                }
            }
        }
        let frames = self.store.frames().map_err(store_error)?;
        let framed: Vec<(usize, Framed<'_>)> = bound
            .iter()
            .filter_map(|(at, resolved)| {
                let item = &items[*at];
                frames.get(&resolved.frame).map(|frame| {
                    (
                        *at,
                        Framed {
                            port: &item.port,
                            index: item.index,
                            artifact_type: item.artifact_type,
                            frame,
                        },
                    )
                })
            })
            .collect();
        let only: Vec<Framed<'_>> = framed.iter().map(|(_, f)| *f).collect();
        for mismatch in op.check_frames(&only) {
            let Some((at, input)) = framed.get(mismatch.at) else {
                continue;
            };
            let expected = rascal_core::binding::expected(input.artifact_type);
            problems.insert(
                *at,
                Binding {
                    port: Some(input.port.to_string()),
                    index: input.index.map(|n| n as u32),
                    problem: BindingProblem::WrongFrame,
                    expected: format!("{expected} in {}", mismatch.wanted),
                    got: format!("{expected} in frame {}", input.frame.identity),
                    remedy: None,
                },
            );
        }
        if problems.is_empty() {
            Ok(bound)
        } else {
            Err(binding_refusal(problems.into_values().collect()))
        }
    }

    /// Runs one operation to its `run.json` (the shape of `run_action`, ContractActions').
    #[allow(clippy::too_many_arguments)]
    fn run_operation(
        &self,
        op: &dyn Op,
        manifest: &Manifest,
        args: &Map<String, Value>,
        items: Vec<Item>,
        bound: BTreeMap<usize, Resolved>,
        meta: &CallMeta,
        progress: &(dyn Fn(Progress) + Send + Sync),
        cancel: Arc<Cancel>,
    ) -> Result<ActionResult, String> {
        // A cancel that lands before allocation leaves no run (4.2).
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".to_string());
        }
        std::fs::create_dir_all(&self.runs).map_err(|e| e.to_string())?;
        let started_ms = run_json::allocate_ms();
        let id = run_json::new_id(started_ms);
        let out = self.runs.join(&id);
        let inputs_dir = self.runs.join(format!("{id}.in"));
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        let _ = std::fs::create_dir_all(&inputs_dir);
        let association = Association {
            action: manifest.name.clone(),
            session_id: self.session_id(),
            client_run_id: meta.client_run_id.clone(),
        };
        let _ = std::fs::write(
            inputs_dir.join(Association::FILE),
            serde_json::to_string(&association).unwrap_or_default(),
        );
        progress(Progress::Run(json!({
            "id": id,
            "resource": format!("rascal://run/{id}/run.json"),
            "session_id": association.session_id,
            "client_run_id": association.client_run_id,
        })));
        progress(Progress::Line("run started".to_string()));
        let mut pending = Pending {
            id: id.clone(),
            action: manifest.name.clone(),
            dir: out.clone(),
            workspace: out.clone(),
            fresh: true,
            started_ms,
            session_id: self.session_id(),
            client_run_id: meta.client_run_id.clone(),
            input_identity: format!(
                "sha256:{}",
                run_json::input_identity(&json!({ "action": manifest.name, "arguments": args }))
            ),
            images_in: Vec::new(),
            locations_used: Vec::new(),
            invocation: None,
            notes: meta.notes.clone(),
        };
        let mut account = Account::default();
        let outcome = self.operate(
            op,
            manifest,
            args,
            items,
            bound,
            meta,
            &cancel,
            &mut pending,
            &mut account,
        );
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err((error, envelope)) => Outcome::failed(&error, envelope),
        };
        let cancelled = outcome.status == Status::Cancelled;
        let line = match outcome.body.get("envelope") {
            Some(envelope) => {
                let status = envelope["status"].as_str().unwrap_or("done");
                format!("step {id} ({}): {status}", manifest.name)
            }
            None => format!("step {id} ({}): {:?}", manifest.name, outcome.status),
        };
        progress(Progress::Line(line));
        let produced = Produced {
            images: Images {
                taken: account.images_in,
                out: account.images_out,
            },
            artifacts: Artifacts {
                taken: account.artifacts_in,
                out: account.artifacts_out,
            },
            locations: Locations {
                used: account.used,
                found: account.found,
            },
            notes: account.notes,
            // An operation calls no provider: what it spent is a known zero.
            spent: Spent {
                usd: 0.0,
                unresolved_usd: 0.0,
                measured: true,
            },
        };
        run_json::finish_with(
            &pending,
            outcome.status,
            outcome.error,
            None,
            outcome.body,
            produced,
        )
        .map_err(|e| format!("run.json: {e}"))?;
        if cancelled {
            return Err("cancelled".to_string());
        }
        self.result_of(&id, args)
    }

    /// The run's work after allocation: the parameters, the inputs given by value, the
    /// invocation, the op, and what it made. An `Err` is how the run failed, with the envelope
    /// the op wrote when there is one.
    #[allow(clippy::too_many_arguments)]
    fn operate(
        &self,
        op: &dyn Op,
        manifest: &Manifest,
        args: &Map<String, Value>,
        items: Vec<Item>,
        bound: BTreeMap<usize, Resolved>,
        meta: &CallMeta,
        cancel: &Cancel,
        pending: &mut Pending,
        account: &mut Account,
    ) -> Result<Outcome, (Error, Option<Value>)> {
        let plain = |e: Error| (e, None);
        if let Some(named) = &meta.estimate {
            // Checked as a task's (4.5, 12.6): the call's binding from the function its
            // estimate used (PLAN.md 10.9).
            let now = self
                .operation_bound(manifest, args, &items)
                .map_err(plain)?;
            self.check_estimate(named, &manifest.name, &now, &image_ports(manifest))
                .map_err(plain)?;
        }
        let (params, resolution) = resolve(op, manifest, args).map_err(plain)?;
        // Until the invocation is known, whatever stops the run is its arguments not resolving:
        // `invalid` (4.8 allows a null invocation only there), never an `error`.
        let resolving = |e: Error| match run_json::error_of(&e).0 {
            Status::Invalid => (e, None),
            _ => (Error::usage(e.to_string()), None),
        };
        let held = self.hold(items, bound, account).map_err(resolving)?;
        let frames = self.store.frames().map_err(resolving)?;
        check_held_frames(op, &held, &frames).map_err(resolving)?;
        let mut inputs = rascal_core::op::Inputs::new();
        let mut identities = BTreeMap::new();
        for h in &held {
            inputs.insert(h.item.input_name(), h.artifact.clone());
            identities.insert(
                h.item.input_name(),
                h.artifact.identity().as_str().to_string(),
            );
        }
        let resolved_json = serde_json::to_value(&resolution.params).unwrap_or(Value::Null);
        pending.input_identity = format!(
            "sha256:{}",
            run_json::input_identity(&json!({
                "action": manifest.name, "version": manifest.version,
                "params": resolved_json, "inputs": identities,
            }))
        );
        pending.invocation = Some(self.invocation(manifest, &resolution.params, &held, account));
        if let Some(delay) = testkit_delay() {
            let until = std::time::Instant::now() + delay;
            while std::time::Instant::now() < until {
                if cancel.load(Ordering::SeqCst) {
                    return Err(plain(Error::Cancelled {
                        step: manifest.name.clone(),
                    }));
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        let step = pending.id.clone();
        let invoked = self.store.invoke_with(
            &step,
            &manifest.name,
            &self.ops,
            &inputs,
            &params,
            Some(cancel),
        );
        let journal = |envelope: Option<&rascal_core::op::Envelope>, note: Option<String>| {
            let line = JournalLine {
                journal_version: 1,
                seq: 0,
                event: if note.is_some() {
                    Event::Note
                } else {
                    Event::Op
                },
                step: Some(step.clone()),
                op: Some(manifest.name.clone()),
                version: Some(manifest.version.clone()),
                params: Some(resolved_json.clone()),
                inputs: identities.clone(),
                outputs: envelope
                    .map(|e| {
                        e.artifacts
                            .iter()
                            .map(|(k, a)| (k.clone(), a.identity.clone()))
                            .collect()
                    })
                    .unwrap_or_default(),
                envelope: envelope.cloned(),
                duration_ms: envelope.map(|e| e.provenance.duration_ms),
                note,
            };
            let _ = Journal::open(&pending.dir.join("journal.jsonl"))
                .and_then(|mut j| j.append(line).map(|_| ()));
        };
        let invocation = match invoked {
            Ok(invocation) => invocation,
            Err(Error::InvalidArgs { op, envelope, .. }) => {
                journal(Some(&envelope), None);
                let value = serde_json::to_value(&*envelope).ok();
                let reason = envelope.reason.clone().unwrap_or_default();
                return Err((Error::usage(format!("{op}: {reason}")), value));
            }
            Err(error) => {
                journal(None, Some(format!("{}: {error}", manifest.name)));
                return Err(plain(error));
            }
        };
        journal(Some(&invocation.output.envelope), None);
        let envelope = serde_json::to_value(&invocation.output.envelope).unwrap_or(Value::Null);
        let mut body = Map::new();
        body.insert("envelope".to_string(), envelope.clone());
        if invocation.committed.is_none() && invocation.cancelled {
            return Err((
                Error::Cancelled {
                    step: manifest.name.clone(),
                },
                Some(envelope),
            ));
        }
        if let Some(reason) = invocation
            .output
            .envelope
            .reason
            .as_ref()
            .filter(|_| invocation.output.envelope.status != rascal_core::op::Status::Ok)
        {
            let status = serde_json::to_value(invocation.output.envelope.status)
                .ok()
                .and_then(|s| s.as_str().map(str::to_string))
                .unwrap_or_default();
            account
                .notes
                .push(format!("{}: {status}: {reason}", manifest.name));
        }
        self.account_outputs(manifest, &invocation.ids, &held, &pending.dir, account)
            .map_err(|e| (e, Some(envelope.clone())))?;
        Ok(Outcome {
            status: Status::Completed,
            error: None,
            body,
        })
    }

    /// Every input as the store holds it: a bound reference loaded, an image given by value
    /// imported, then each location given by value validated against the call's image and
    /// registered. Each enters the account (`artifacts.in`, `images.in`, `locations.used`) as
    /// it is held.
    fn hold(
        &self,
        items: Vec<Item>,
        bound: BTreeMap<usize, Resolved>,
        account: &mut Account,
    ) -> rascal_core::error::Result<Vec<Held>> {
        let mut held: Vec<Option<Held>> = Vec::new();
        let mut locations = Vec::new();
        for (at, item) in items.into_iter().enumerate() {
            match &item.given {
                Given::Reference(_) => {
                    let resolved = bound
                        .get(&at)
                        .ok_or_else(|| Error::op("serve", "a reference was not bound"))?;
                    let artifact = self.store.load(&resolved.id)?;
                    held.push(Some(Held {
                        id: resolved.id.clone(),
                        item,
                        artifact,
                        image: None,
                    }));
                }
                Given::Image(value) => {
                    let label = item.label();
                    let image = estimate::image_in(&label, value, &self.paths)?;
                    let bytes = image.bytes()?;
                    estimate::verify_expect(&label, value, &image, &bytes)?;
                    let projection = value
                        .get("expect")
                        .and_then(|e| serde_json::from_value::<ImageReference>(e.clone()).ok())
                        .map(|e| match e.projection {
                            rascal_core::contract::table_manners::images::Projection::Equirectangular => {
                                rascal_core::frame::Projection::Equirectangular
                            }
                            _ => rascal_core::frame::Projection::Flat,
                        });
                    let (id, _, artifact) = match &image {
                        estimate::ImageIn::Path(path) => self.store.import(path, projection)?,
                        estimate::ImageIn::Inline { .. } => {
                            self.store.import_bytes(&bytes, "inline", projection)?
                        }
                    };
                    held.push(Some(Held {
                        id,
                        item,
                        artifact,
                        image: None,
                    }));
                }
                Given::Location(_) => {
                    held.push(None);
                    locations.push((held.len() - 1, item));
                }
            }
        }
        for (slot, item) in locations {
            let Given::Location(value) = &item.given else {
                continue;
            };
            let (frame, reference) =
                self.anchor(held.iter().flatten())?.ok_or_else(|| {
                    Error::usage(format!(
                        "{}: a location given by value needs the call's image (an image port, or an input made on one image in its raw frame); pass the location's reference instead",
                        item.label()
                    ))
                })?;
            let location = contract::location_from_json(value.clone(), &frame, &reference)
                .map_err(|e| match e {
                    Error::Usage { message } => {
                        Error::usage(format!("{}: {message}", item.label()))
                    }
                    other => other,
                })?;
            let (id, artifact) = self.store.add_location(location)?;
            held[slot] = Some(Held {
                id,
                item,
                artifact,
                image: None,
            });
        }
        let mut held: Vec<Held> = held.into_iter().flatten().collect();
        for h in &mut held {
            if matches!(h.artifact, Artifact::Image(_)) {
                h.image = Some(contract::image_reference(&self.store, &h.id)?);
            }
        }
        for h in &held {
            let reference = contract::artifact_ref(&self.store, &h.id)?;
            account.artifacts_in.push(ArtifactEntry {
                port: h.item.port.clone(),
                artifact: reference,
                preview: None,
            });
            match &h.artifact {
                Artifact::Image(_) => {
                    if let Some(image) = &h.image {
                        account.images_in.push(ImageTaken {
                            slot: h.item.port.clone(),
                            role: h.item.port.clone(),
                            image: image.clone(),
                        });
                    }
                }
                Artifact::Location { .. } => {
                    if let Some(location) = self.contract_location(&h.id, &held)? {
                        account.used.push(location);
                    }
                }
                _ => {}
            }
        }
        Ok(held)
    }

    /// The frame and image reference a location given by value is read against: the call's
    /// first image, else the root image an input in a raw frame was made on.
    fn anchor<'a>(
        &self,
        held: impl Iterator<Item = &'a Held> + Clone,
    ) -> rascal_core::error::Result<Option<(Frame, ImageReference)>> {
        let frames = self.store.frames()?;
        if let Some(h) = held
            .clone()
            .find(|h| matches!(h.artifact, Artifact::Image(_)))
        {
            let frame = frames.require(h.artifact.frame())?.clone();
            let reference = match &h.image {
                Some(reference) => reference.clone(),
                None => contract::image_reference(&self.store, &h.id)?,
            };
            return Ok(Some((frame, reference)));
        }
        for h in held {
            let frame = frames.require(h.artifact.frame())?;
            if !frame.is_raw() {
                continue;
            }
            if let Some(on) = contract::artifact_ref(&self.store, &h.id)?.on {
                return Ok(Some((frame.clone(), on)));
            }
        }
        Ok(None)
    }

    /// A location of the store as a contract location (9) on the image of its frame: one of
    /// the call's images in that frame, else the image the records lead back to. `None` when no
    /// image of its frame is known, or the location cannot leave rascal (`view`).
    fn contract_location(
        &self,
        id: &str,
        held: &[Held],
    ) -> rascal_core::error::Result<Option<TmLocation>> {
        let Artifact::Location { location, identity } = self.store.load(id)? else {
            return Ok(None);
        };
        let image = match held.iter().find(|h| {
            matches!(h.artifact, Artifact::Image(_)) && h.artifact.frame() == location.frame()
        }) {
            Some(h) => Some(h.id.clone()),
            None => preview::frame_image(&self.store, identity.as_str(), location.frame())?,
        };
        let Some(image) = image else {
            return Ok(None);
        };
        let reference = contract::image_reference(&self.store, &image)?;
        let source = rascal_core::contract::table_manners::images::Source {
            tool: "rascal".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            run: None,
            item: None,
        };
        Ok(contract::location_out(
            &[location],
            reference,
            LocationMeta {
                source: Some(source),
                ..LocationMeta::default()
            },
        )
        .ok())
    }

    /// The call as it resolved (4.8).
    fn invocation(
        &self,
        manifest: &Manifest,
        resolved: &Params,
        held: &[Held],
        account: &Account,
    ) -> Invocation {
        let mut arguments: Map<String, Value> = match serde_json::to_value(resolved) {
            Ok(Value::Object(map)) => map,
            _ => Map::new(),
        };
        let mut ports: BTreeMap<String, Vec<(Option<usize>, Value)>> = BTreeMap::new();
        for (h, entry) in held.iter().zip(&account.artifacts_in) {
            let value = match &h.item.given {
                Given::Reference(_) => serde_json::to_value(&entry.artifact).unwrap_or(Value::Null),
                Given::Image(_) => json!({ "expect": h.image }),
                Given::Location(value) => value.clone(),
            };
            ports
                .entry(h.item.port.clone())
                .or_default()
                .push((h.item.index, value));
        }
        for (port, mut values) in ports {
            values.sort_by_key(|(index, _)| *index);
            let many = manifest.inputs.iter().any(|p| p.name == port && p.many);
            let value = if many {
                Value::Array(values.into_iter().map(|(_, v)| v).collect())
            } else {
                values
                    .into_iter()
                    .next()
                    .map(|(_, v)| v)
                    .unwrap_or(Value::Null)
            };
            arguments.insert(port, value);
        }
        let mut extensions = BTreeMap::new();
        extensions.insert(
            "x-rascal".to_string(),
            json!({ "constants": resolved.get("constants").and_then(|c| c.as_str()) }),
        );
        Invocation {
            action: manifest.name.clone(),
            arguments,
            effective: Effective {
                model: None,
                config_identity: self.config_state.identity(),
                extensions,
            },
            requirements: Vec::new(),
        }
    }

    /// Each output the op published, in port order and, for a `many` port, in item order: its
    /// reference, its image file in the run directory for an image, its preview for a type that
    /// declares one, and its contract location for a location.
    fn account_outputs(
        &self,
        manifest: &Manifest,
        ids: &BTreeMap<String, String>,
        held: &[Held],
        dir: &Path,
        account: &mut Account,
    ) -> rascal_core::error::Result<()> {
        for port in &manifest.outputs {
            let mut names: Vec<(usize, &String)> = ids
                .keys()
                .filter_map(|name| {
                    if port.many {
                        port.index_of(name).map(|n| (n, name))
                    } else {
                        (name == &port.name).then_some((0, name))
                    }
                })
                .collect();
            names.sort();
            for (_, name) in names {
                let id = &ids[name];
                let reference = contract::artifact_ref(&self.store, id)?;
                let artifact = self.store.load(id)?;
                if let Artifact::Image(image) = &artifact {
                    let file = dir.join(format!("{name}.png"));
                    let stored = self.store.image_path(image);
                    let bytes = std::fs::read(&stored).map_err(|e| Error::io(&stored, e))?;
                    std::fs::write(&file, &bytes).map_err(|e| Error::io(&file, e))?;
                    account.images_out.push(ImageOut {
                        role: port.name.clone(),
                        image: contract::image_reference(&self.store, id)?,
                        locator: Locator::Path(absolute(&file)),
                    });
                }
                if let Artifact::Location { .. } = &artifact
                    && let Some(location) = self.contract_location(id, held)?
                {
                    account.found.push(location);
                }
                let preview = match preview::draw(&self.store, &self.ops, id)? {
                    Some((bytes, frame)) => {
                        let file = dir.join(format!("preview-{name}.png"));
                        std::fs::write(&file, &bytes).map_err(|e| Error::io(&file, e))?;
                        let out =
                            preview::image_out(&bytes, &frame, Locator::Path(absolute(&file)))?;
                        account.images_out.push(out.clone());
                        Some(out)
                    }
                    None => None,
                };
                account.artifacts_out.push(ArtifactEntry {
                    port: port.name.clone(),
                    artifact: reference,
                    preview,
                });
            }
        }
        Ok(())
    }
}

/// The frame check over every input the run holds, values included: a mismatch is the run's
/// `invalid`/`usage`, since the run exists by now.
fn check_held_frames(
    op: &dyn Op,
    held: &[Held],
    frames: &FrameTable,
) -> rascal_core::error::Result<()> {
    let mut framed = Vec::new();
    for h in held {
        framed.push(Framed {
            port: &h.item.port,
            index: h.item.index,
            artifact_type: h.item.artifact_type,
            frame: frames.require(h.artifact.frame())?,
        });
    }
    if let Some(mismatch) = op.check_frames(&framed).first()
        && let Some(input) = framed.get(mismatch.at)
    {
        return Err(Error::usage(format!(
            "{}: drawn in frame {}, not in {}",
            input.label(),
            input.frame.id,
            mismatch.wanted
        )));
    }
    Ok(())
}

/// A call's in-port items in port order, each `many` port's in the order given. A missing
/// required port, a `many` port that is not a non-empty array, and a value that is neither a
/// reference nor a value the port takes are invalid params: the input schema rejects them.
fn items(manifest: &Manifest, args: &Map<String, Value>) -> Result<Vec<Item>, CallError> {
    let mut items = Vec::new();
    for port in &manifest.inputs {
        let Some(value) = args.get(&port.name) else {
            if port.required {
                return Err(CallError::from(format!("{}: required", port.name)));
            }
            continue;
        };
        if port.many {
            let values = value.as_array().filter(|a| !a.is_empty()).ok_or_else(|| {
                CallError::from(format!(
                    "{}: a non-empty array of artifact references",
                    port.name
                ))
            })?;
            for (n, value) in values.iter().enumerate() {
                items.push(item(port, Some(n), value)?);
            }
        } else {
            items.push(item(port, None, value)?);
        }
    }
    Ok(items)
}

fn item(port: &Port, index: Option<usize>, value: &Value) -> Result<Item, CallError> {
    let label = match index {
        Some(n) => format!("{}[{n}]", port.name),
        None => port.name.clone(),
    };
    let is_reference = value.get("identity").is_some();
    let given = match port.artifact_type {
        ArtifactType::Image if !is_reference && value.is_object() => Given::Image(value.clone()),
        ArtifactType::Location if !is_reference && value.is_object() => {
            Given::Location(value.clone())
        }
        _ => Given::Reference(
            rascal_core::binding::reference_in(&label, value)
                .map_err(|e| CallError::from(e.to_string()))?,
        ),
    };
    Ok(Item {
        port: port.name.clone(),
        index,
        artifact_type: port.artifact_type,
        given,
    })
}

/// An operation's parameters from its arguments as given, and resolved as the run resolves them
/// (the constant set's values and the manifest's defaults applied): the run's and the
/// estimate's one function. A bad one is `usage`.
fn resolve(
    op: &dyn Op,
    manifest: &Manifest,
    args: &Map<String, Value>,
) -> rascal_core::error::Result<(Params, rascal_core::op::Resolution)> {
    let mut params = Params::new();
    for (key, value) in args {
        if manifest.inputs.iter().any(|p| &p.name == key) || DELIVERY.contains(&key.as_str()) {
            continue;
        }
        if key == "constants" {
            let set = value.as_str().ok_or_else(|| {
                Error::usage(format!(
                    "constants: {value} is not the name of a constant set"
                ))
            })?;
            params.insert(key.clone(), toml::Value::String(set.to_string()));
        } else if let Some(param) = manifest.param(key) {
            params.insert(key.clone(), json_param(param, value)?);
        } else {
            return Err(Error::usage(format!(
                "{key}: {} takes no argument {key}",
                manifest.name
            )));
        }
    }
    let resolution = rascal_core::op::resolve_step(op, &params).map_err(|e| {
        Error::usage(match e {
            rascal_core::op::Unresolved::NoSet(m) | rascal_core::op::Unresolved::Invalid(m) => {
                format!("{}: {m}", manifest.name)
            }
        })
    })?;
    Ok((params, resolution))
}

/// The in-ports that take an image, which a stale estimate names when its bytes changed.
fn image_ports(manifest: &Manifest) -> Vec<String> {
    manifest
        .inputs
        .iter()
        .filter(|p| p.artifact_type == ArtifactType::Image)
        .map(|p| p.name.clone())
        .collect()
}

fn absolute(path: &Path) -> String {
    std::path::absolute(path)
        .unwrap_or_else(|_| PathBuf::from(path))
        .display()
        .to_string()
}
