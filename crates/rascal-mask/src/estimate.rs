//! The estimate (table-manners 4.5; PLAN.md 12.5): what one exact call would reserve, bound to
//! its arguments, the configuration, the tool version and the session. The `estimate` tool and
//! `rascal estimate --json` answer with it, and the limits check prices a run with the same
//! function ([`price`]), so the two cannot disagree. An estimate reads the call's images to hash
//! them and nothing else: it never allocates, imports, writes a run, touches the workspace,
//! starts a pack or calls a filler.
//!
//! A server records the bindings it issues ([`Issued`]); a call naming one is checked at the
//! limits check, after its arguments and before the cap (PLAN.md 12.6), and fails
//! `stale_estimate` when this session never issued it, when it differs, or when it expired.
//!
//! The binding is rascal's recipe: the record identity (ADR 0016, kind `estimate`) of
//! `{action, arguments, tool_version, config_identity, session_id}`, where `arguments` leaves
//! out the delivery choices (`return_images`, `image_blocks`), each image slot's value is
//! `{"file_sha256": …}` (a path read, inline bytes decoded; an `expect` verified as the call
//! would, then dropped), each stored-mask input is `{"mask": <its identity>}`, and every other
//! argument enters as sent.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rascal_core::Error;
use rascal_core::config::{Config, ConfigState};
use rascal_core::contract::table_manners::images::ImageReference;
use rascal_core::contract::table_manners::{
    Effective, Estimate, EstimateRefusal, Readiness, ReadinessState, RequirementKind,
    Session as TmSession, Tool as TmTool,
};
use rascal_core::error::Result;
use rascal_core::paths::{PathRule, Want};
use rascal_core::recipe::InputType;
use serde_json::{Map, Value, json};

use crate::run_json::{self, usd};
use rascal_mask::describe::INLINE_BYTES;
use rascal_mask::readiness::{self, SEGMENTER_ACTION, Setup, Use};
use rascal_mask::recipes::load_recipe;

/// How long rascal honours a binding: one hour. Its prices are configuration, fixed for the
/// session; the hour bounds what a host may hold (the contract allows up to a day).
pub const TTL_MS: u128 = 60 * 60 * 1000;

/// The arguments that only choose how results are delivered: left out of the binding (4.5).
pub const DELIVERY: [&str; 2] = ["return_images", "image_blocks"];

/// The tool version a binding is made under.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How long a binding is honoured: [`TTL_MS`], which a testkit build may shorten through
/// `RASCAL_TESTKIT_ESTIMATE_TTL_MS`.
pub fn ttl_ms() -> u128 {
    #[cfg(feature = "testkit")]
    if let Some(ms) = std::env::var("RASCAL_TESTKIT_ESTIMATE_TTL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        return ms;
    }
    TTL_MS
}

/// The estimate request's shape, `{action, arguments}` (the tool's input schema): `None` when
/// the request does not have it, which is plain invalid params, not an estimate's refusal.
pub fn request(value: &Map<String, Value>) -> Option<(&str, &Map<String, Value>)> {
    if value.keys().any(|k| k != "action" && k != "arguments") {
        return None;
    }
    Some((
        value.get("action")?.as_str()?,
        value.get("arguments")?.as_object()?,
    ))
}

/// What one action takes, by the recipe's inputs.
pub struct Slots {
    pub images: Vec<String>,
    pub locations: Vec<String>,
    pub texts: Vec<String>,
    pub masks: Vec<String>,
    pub required: Vec<String>,
    pub steps: Vec<String>,
}

/// The slots of a recipe (a shipped one by name, or a recipe file).
pub fn slots(recipe: &str) -> Result<Slots> {
    let recipe = load_recipe(recipe)?;
    let mut slots = Slots {
        images: Vec::new(),
        locations: Vec::new(),
        texts: Vec::new(),
        masks: Vec::new(),
        required: Vec::new(),
        steps: recipe.step.iter().map(|s| s.id.clone()).collect(),
    };
    for (name, decl) in &recipe.inputs {
        let list = match decl.kind {
            InputType::Image => &mut slots.images,
            InputType::Location => &mut slots.locations,
            InputType::Text => &mut slots.texts,
            InputType::Mask => &mut slots.masks,
            _ => continue,
        };
        list.push(name.clone());
        if !decl.optional {
            slots.required.push(name.clone());
        }
    }
    Ok(slots)
}

/// An image argument as the call gives it: a file by absolute path, or inline bytes.
pub enum ImageIn {
    Path(PathBuf),
    Inline { bytes: Vec<u8>, ext: &'static str },
}

impl ImageIn {
    /// The image's bytes: the file read, or the inline bytes.
    pub fn bytes(&self) -> Result<Vec<u8>> {
        match self {
            ImageIn::Path(path) => std::fs::read(path).map_err(|e| Error::io(path, e)),
            ImageIn::Inline { bytes, .. } => Ok(bytes.clone()),
        }
    }

    fn label(&self) -> String {
        match self {
            ImageIn::Path(path) => path.display().to_string(),
            ImageIn::Inline { .. } => "the inline image".to_string(),
        }
    }
}

/// Reads an image slot's value as the call would (table-manners 10.1): a path the rule admits
/// (CONTRACT 5.2; under `serve`, its canonical form inside an allowed directory), or standard
/// base64 within the inline limit. Its `expect` is checked by [`verify_expect`].
pub fn image_in(slot: &str, value: &Value, paths: &PathRule) -> Result<ImageIn> {
    if let Some(path) = value.get("path").and_then(|p| p.as_str()) {
        return Ok(ImageIn::Path(paths.admit(
            slot,
            Path::new(path),
            Want::File,
        )?));
    }
    if let Some(inline) = value.get("inline") {
        let data = inline["data"].as_str().unwrap_or_default();
        let bytes = rascal_mcp::jsonrpc::base64_decode(data)
            .ok_or_else(|| Error::usage(format!("{slot}: inline data is not standard base64")))?;
        if bytes.len() as u64 > INLINE_BYTES {
            return Err(Error::usage(format!(
                "{slot}: inline image of {} bytes is over the limit of {INLINE_BYTES}",
                bytes.len()
            )));
        }
        let ext = match inline["media_type"].as_str() {
            Some("image/jpeg") => "jpg",
            _ => "png",
        };
        return Ok(ImageIn::Inline { bytes, ext });
    }
    Err(Error::usage(format!("{slot}: give path or inline")))
}

/// Checks the slot's `expect` reference, when it has one, against the image's bytes: its file
/// digest and dimensions. A mismatch is an input the call cannot take (`input`).
pub fn verify_expect(slot: &str, value: &Value, image: &ImageIn, bytes: &[u8]) -> Result<()> {
    let Some(expect) = value.get("expect") else {
        return Ok(());
    };
    let expect: ImageReference = serde_json::from_value(expect.clone())
        .map_err(|e| Error::usage(format!("{slot}: expect is not an image reference: {e}")))?;
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| Error::usage(format!("{slot}: {}: {e}", image.label())))?
        .into_dimensions()
        .map_err(|e| Error::usage(format!("{slot}: {}: {e}", image.label())))?;
    let digest = rascal_core::contract::file_sha256(bytes);
    if digest != expect.file_sha256 || width != expect.width || height != expect.height {
        return Err(Error::usage(format!(
            "{slot}: the image is on another image than expected: file_sha256 {digest} ({width}x{height}), expected {} ({}x{})",
            expect.file_sha256, expect.width, expect.height
        )));
    }
    Ok(())
}

/// The call's effective per-call cap (table-manners 4.5): the configured `call_cap_usd`
/// lowered by `max_spend_usd`, the caller's value when no cap is configured, `None` when
/// neither is. A `max_spend_usd` that is not a number above zero (a cap of zero is not a
/// cap), or that is above the configured cap, is a bad argument (`usage`) naming the amounts,
/// never silently lowered.
pub fn effective_cap(config: Option<&Config>, args: &Map<String, Value>) -> Result<Option<f64>> {
    let configured = config
        .and_then(|c| c.limits.as_ref())
        .and_then(|l| l.call_cap_usd);
    let Some(value) = args.get("max_spend_usd") else {
        return Ok(configured);
    };
    let asked = value.as_f64().filter(|a| *a > 0.0).ok_or_else(|| {
        Error::usage(format!(
            "max_spend_usd: {value} is not a number above zero; a call that must not spend names no filler"
        ))
    })?;
    if let Some(cap) = configured
        && asked > cap
    {
        return Err(Error::usage(format!(
            "max_spend_usd: {asked} is above the configured per-call cap of {cap}; a call may lower the cap, never raise it"
        )));
    }
    Ok(Some(asked))
}

/// Checks a call's arguments as the run reads them, without writing anything: the first step of
/// the limits check, and an estimate's. `max_spend_usd` first, then every argument by name (one
/// the action does not take is refused), then the inputs the recipe requires. A bad argument is
/// `usage` naming it; an image that does not match its `expect`, and a path the rule does not
/// admit, are `input`. What only a run can tell (a `set` parameter an op does not take, a
/// location off its image) the run still checks.
pub fn check_arguments(
    recipe: &str,
    action: &str,
    args: &Map<String, Value>,
    config: Option<&Config>,
    paths: &PathRule,
) -> Result<()> {
    effective_cap(config, args)?;
    let slots = slots(recipe)?;
    let is = |list: &[String], key: &str| list.iter().any(|n| n == key);
    for (key, value) in args {
        match key.as_str() {
            k if is(&slots.images, k) => {
                let image = image_in(k, value, paths)?;
                if value.get("expect").is_some() {
                    verify_expect(k, value, &image, &image.bytes()?)?;
                }
            }
            k if is(&slots.locations, k) => {
                if !value.is_object() {
                    return Err(Error::usage(format!(
                        "{k}: not a table-manners location object"
                    )));
                }
            }
            k if is(&slots.texts, k) => {
                if !value.as_str().is_some_and(|s| !s.is_empty()) {
                    return Err(Error::usage(format!("{k}: not a non-empty string")));
                }
            }
            k if is(&slots.masks, k) => {
                let Some(path) = value.as_str() else {
                    return Err(Error::usage(format!(
                        "{k}: not a stored mask directory's path"
                    )));
                };
                paths.admit(k, Path::new(path), Want::MaskDir)?;
            }
            "filler" => {
                if !value.is_string() {
                    return Err(Error::usage("filler: not a filler's name"));
                }
            }
            "set" => check_set(value, &slots.steps)?,
            "projection" => match value.as_str() {
                Some("flat" | "equirectangular") => {}
                _ => return Err(Error::usage(format!("projection: {value}"))),
            },
            "strict" | "image_blocks" => {
                if !value.is_boolean() {
                    return Err(Error::usage(format!("{key}: not true or false")));
                }
            }
            "return_images" => match value.as_str() {
                Some("path" | "inline" | "resource") => {}
                _ => {
                    return Err(Error::usage(format!(
                        "return_images: {value} is not path, inline or resource"
                    )));
                }
            },
            "max_spend_usd" => {}
            other => {
                return Err(Error::usage(format!(
                    "{other}: not an argument of {action}"
                )));
            }
        }
    }
    if let Some(missing) = slots.required.iter().find(|r| !args.contains_key(*r)) {
        return Err(Error::usage(format!("{missing}: required")));
    }
    Ok(())
}

/// `set`: an object of `<step>.<param>` to a string, number or boolean, the step one of the
/// recipe's.
fn check_set(value: &Value, steps: &[String]) -> Result<()> {
    let set = value
        .as_object()
        .ok_or_else(|| Error::usage("set: not an object of <step>.<param> to a value"))?;
    for (key, value) in set {
        let Some((step, param)) = key.split_once('.') else {
            return Err(Error::usage(format!("set: {key} is not <step>.<param>")));
        };
        if param.is_empty() || !steps.iter().any(|s| s == step) {
            return Err(Error::usage(format!(
                "set: {key}: the recipe has no step {step}"
            )));
        }
        if !(value.is_string() || value.is_number() || value.is_boolean()) {
            return Err(Error::usage(format!(
                "set: {key}: not a string, number or boolean"
            )));
        }
    }
    Ok(())
}

/// What the limits check makes of a call (table-manners 4.5), its arguments already checked:
/// the call's own readiness, what it would use, its worst case and its effective cap.
#[derive(Debug, Clone)]
pub struct Price {
    pub readiness: Readiness,
    /// `fillers.<name>` for the call's filler when it is configured.
    pub model: Option<String>,
    /// The configuration in force for this call; none only when it needs one that is missing or
    /// invalid.
    pub config_identity: Option<String>,
    /// `x-rascal`: the filler's name, kind, revision, paste and scaled side, or null.
    pub filler: Value,
    /// `x-rascal`: the constant set in force at each recipe step whose op has sets.
    pub constants: BTreeMap<String, String>,
    /// What the limits check reserves; none when the call cannot be priced.
    pub worst_case_usd: Option<f64>,
    pub cap_usd: Option<f64>,
}

/// Prices a call of `action` (running `recipe`) whose arguments are checked: the one function
/// the estimate, the limits check and the invocation share. A call needing a configuration that
/// is missing or invalid, naming a filler that is not configured, or `remove_prompt` without its
/// pack cannot be priced (`worst_case_usd` none), and is never ready.
pub fn price(
    setup: &Setup<'_>,
    recipe: &str,
    action: &str,
    args: &Map<String, Value>,
) -> Result<Price> {
    let config = setup.config.config();
    let filler = args.get("filler").and_then(|v| v.as_str());
    let readiness = readiness::for_call(setup, action, filler.map_or(Use::MaskOnly, Use::Filler));
    let unmet = |kind: RequirementKind, prefix: &str| {
        readiness
            .requirements
            .iter()
            .any(|r| r.kind == kind && r.name.starts_with(prefix) && readiness::unmet(r))
    };
    let config_unmet = unmet(RequirementKind::Config, "");
    let pack_unmet = action == SEGMENTER_ACTION && unmet(RequirementKind::Dependency, "packs.");
    let entry = filler.and_then(|name| Some((name, config?.fillers.get(name)?)));
    let config_identity = match setup.config {
        ConfigState::Loaded { identity, .. } => Some(identity.clone()),
        _ if config_unmet => None,
        // A call that reads no configuration runs on the built-in defaults.
        _ => Some(Config::builtin().identity()),
    };
    let priced = !config_unmet && !pack_unmet && (filler.is_none() || entry.is_some());
    let overrides = entry.map(|(_, e)| e.overrides.clone()).unwrap_or_default();
    Ok(Price {
        constants: constants(recipe, &overrides, args.get("set"))?,
        model: entry.map(|(name, _)| format!("fillers.{name}")),
        config_identity,
        filler: entry.map_or(Value::Null, |(name, e)| {
            json!({
                "name": name,
                "kind": if e.url.is_some() { "http" } else { "command" },
                "revision": e.revision,
                "paste": e.paste,
                "fill_max_side": e.fill_max_side,
            })
        }),
        worst_case_usd: priced
            .then(|| entry.and_then(|(_, e)| e.spend_usd_per_call).unwrap_or(0.0)),
        cap_usd: effective_cap(config, args)?,
        readiness,
    })
}

/// The constant set in force at each step of `recipe` whose op has sets (the shipped ops'
/// manifests): the recipe's own, then a filler's `"<step>.constants"` override (`heal` applies
/// them when that filler paints), then the call's `set`, the last word, as the run applies them;
/// a step that names none runs with its op's first set. A pack's ops have no sets here.
pub fn constants(
    recipe: &str,
    overrides: &BTreeMap<String, rascal_core::recipe::Override>,
    set: Option<&Value>,
) -> Result<BTreeMap<String, String>> {
    let recipe = load_recipe(recipe)?;
    let registry = rascal_ops::registry()?;
    let mut chosen: BTreeMap<String, String> = BTreeMap::new();
    for (key, value) in overrides {
        if let (Some(step), rascal_core::recipe::Override::String(name)) =
            (key.strip_suffix(".constants"), value)
        {
            chosen.insert(step.to_string(), name.clone());
        }
    }
    for (key, value) in set.and_then(|s| s.as_object()).into_iter().flatten() {
        if let (Some(step), Some(name)) = (key.strip_suffix(".constants"), value.as_str()) {
            chosen.insert(step.to_string(), name.to_string());
        }
    }
    let mut out = BTreeMap::new();
    for step in &recipe.step {
        let Some(manifest) = registry.get(&step.op).map(|op| op.manifest()) else {
            continue;
        };
        let Some(first) = manifest.constants.first() else {
            continue;
        };
        let name = chosen
            .get(&step.id)
            .or(step.constants.as_ref())
            .unwrap_or(first);
        out.insert(step.id.clone(), name.clone());
    }
    Ok(out)
}

/// What a priced call uses where its arguments leave the choice to rascal (4.5): the one
/// function behind both the estimate's `effective` and the invocation's (4.8), so the two are
/// equal for the same call. `x-rascal` holds the filler (its name, kind, revision, paste and
/// scaled side) or null, and the constant set in force by step ([`constants`]).
pub fn effective(price: &Price) -> Effective {
    let mut extensions = BTreeMap::new();
    extensions.insert(
        "x-rascal".to_string(),
        json!({ "filler": price.filler, "constants": price.constants }),
    );
    Effective {
        model: price.model.clone(),
        config_identity: price.config_identity.clone(),
        extensions,
    }
}

/// The spend a call may make, and the limits it is held to (table-manners 4.5).
pub struct Limits {
    pub worst_case_usd: f64,
    pub call_cap_usd: Option<f64>,
    pub session_budget_usd: Option<f64>,
    /// What the session has spent plus what calls under way hold.
    pub session_held_usd: f64,
}

impl Limits {
    /// Refuses a call whose worst case is over the effective cap (`cap`), then one the
    /// session's budget left cannot hold (`budget`).
    pub fn check(&self) -> Result<()> {
        if let Some(cap) = self.call_cap_usd
            && self.worst_case_usd > cap
        {
            return Err(Error::Cap {
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

/// The session's budget and what it holds, for the budget check; none on the command line.
#[derive(Debug, Clone, Copy)]
pub struct Held {
    pub budget_usd: Option<f64>,
    pub held_usd: f64,
}

/// Why the limits check would refuse the call now: `readiness` when it is not ready, else
/// `cap`, else `budget` (only with a session), else none.
pub fn refusal(price: &Price, held: Option<Held>) -> Option<EstimateRefusal> {
    if price.readiness.state != ReadinessState::Ready {
        return Some(EstimateRefusal::Readiness);
    }
    let limits = Limits {
        worst_case_usd: price.worst_case_usd.unwrap_or(0.0),
        call_cap_usd: price.cap_usd,
        session_budget_usd: held.and_then(|h| h.budget_usd),
        session_held_usd: held.map_or(0.0, |h| h.held_usd),
    };
    match limits.check() {
        Err(Error::Cap { .. }) => Some(EstimateRefusal::Cap),
        Err(_) => Some(EstimateRefusal::Budget),
        Ok(()) => None,
    }
}

/// A binding and the parts it was made from: each argument's digest by name (an image's by its
/// slot), so a call that no longer matches can be told what changed.
#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    pub binding: String,
    pub parts: BTreeMap<String, String>,
}

/// The binding of a call (module docs): `session_id` is none on the command line, so no server
/// accepts it. Its images and masks are read through `paths`, as the call reads them.
pub fn bind(
    recipe: &str,
    action: &str,
    args: &Map<String, Value>,
    config_identity: Option<&str>,
    session_id: Option<&str>,
    version: &str,
    paths: &PathRule,
) -> Result<Bound> {
    let slots = slots(recipe)?;
    let mut arguments = Map::new();
    let mut parts = BTreeMap::new();
    for (key, value) in args {
        if DELIVERY.contains(&key.as_str()) {
            continue;
        }
        let entered = if slots.images.contains(key) {
            let image = image_in(key, value, paths)?;
            let bytes = image.bytes()?;
            verify_expect(key, value, &image, &bytes)?;
            json!({ "file_sha256": rascal_core::contract::file_sha256(&bytes) })
        } else if slots.masks.contains(key) {
            let path = value.as_str().unwrap_or_default();
            let dir = paths.admit(key, Path::new(path), Want::MaskDir)?;
            let mask = rascal_core::mask::Mask::read(&dir).map_err(|e| {
                Error::usage(format!("{key}: {path} is not a stored rascal mask: {e}"))
            })?;
            json!({ "mask": mask.identity().as_str() })
        } else {
            value.clone()
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
        "action": action,
        "arguments": arguments,
        "tool_version": version,
        "config_identity": config_identity,
        "session_id": session_id,
    });
    Ok(Bound {
        binding: rascal_core::canonical::identity("estimate", &payload)
            .as_str()
            .to_string(),
        parts,
    })
}

/// A binding the server issued (PLAN.md 12.6): when it stops being honoured, and what it was
/// made from, so a call that no longer matches is told what changed.
#[derive(Debug, Clone, PartialEq)]
pub struct IssuedEstimate {
    pub action: String,
    pub expires_ms: u128,
    pub parts: BTreeMap<String, String>,
}

/// The bindings a server issued, newest last, bounded to the newest [`Issued::LIMIT`].
#[derive(Debug, Default)]
pub struct Issued {
    entries: std::collections::VecDeque<(String, IssuedEstimate)>,
}

impl Issued {
    /// How many bindings a server remembers.
    pub const LIMIT: usize = 1024;

    /// Records a binding (again, when it was issued before: the newest expiry stands).
    pub fn record(&mut self, binding: &str, issued: IssuedEstimate) {
        self.entries.retain(|(b, _)| b != binding);
        self.entries.push_back((binding.to_string(), issued));
        while self.entries.len() > Self::LIMIT {
            self.entries.pop_front();
        }
    }

    /// The binding, when this server issued it and still remembers it.
    pub fn get(&self, binding: &str) -> Option<&IssuedEstimate> {
        self.entries
            .iter()
            .find(|(b, _)| b == binding)
            .map(|(_, issued)| issued)
    }
}

/// What differs between the call an estimate was made for and the call as it arrived, for
/// people: each image whose bytes changed and each argument added, removed or changed.
pub fn changed(
    recipe: &str,
    issued: &IssuedEstimate,
    action: &str,
    now: &BTreeMap<String, String>,
) -> String {
    let images = slots(recipe).map(|s| s.images).unwrap_or_default();
    let mut what = Vec::new();
    if issued.action != action {
        what.push(format!("it was made for {}, not {action}", issued.action));
    }
    let names: std::collections::BTreeSet<&String> =
        issued.parts.keys().chain(now.keys()).collect();
    for name in names {
        let image = images.contains(name);
        match (issued.parts.get(name), now.get(name)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(_), Some(_)) if image => what.push(format!("the image `{name}` changed")),
            (Some(_), Some(_)) => what.push(format!("the argument `{name}` changed")),
            (Some(_), None) => what.push(format!("the argument `{name}` was removed")),
            (None, _) => what.push(format!("the argument `{name}` was added")),
        }
    }
    if what.is_empty() {
        "the call is not the one estimated".to_string()
    } else {
        what.join("; ")
    }
}

/// The estimate document (`schemas/core/estimate.json`).
pub fn document(
    action: &str,
    bound: &Bound,
    price: Price,
    held: Option<Held>,
    session: Option<TmSession>,
    expires_ms: u128,
) -> Estimate {
    let refused = refusal(&price, held);
    Estimate {
        contract: rascal_core::contract::CONTRACT_VERSION.to_string(),
        tool: TmTool {
            name: "rascal".to_string(),
            version: VERSION.to_string(),
        },
        action: action.to_string(),
        binding: bound.binding.clone(),
        session_id: session.as_ref().map(|s| s.id.clone()),
        effective: effective(&price),
        worst_case_usd: price.worst_case_usd.map(usd),
        cap_usd: price.cap_usd.map(usd),
        allowed: refused.is_none(),
        refused,
        readiness: price.readiness,
        session,
        expires: run_json::rfc3339(expires_ms),
    }
}

/// An estimate with no session, as `rascal estimate --json` makes it: the cap and readiness
/// apply, the budget never does (there is no ledger), and the paths are the person's own.
pub fn without_session(state: &ConfigState, request: &Value) -> Result<Estimate> {
    let shape = request.as_object().and_then(self::request).ok_or_else(|| {
        Error::usage("the request is not {\"action\": <name>, \"arguments\": {…}}")
    })?;
    let (action, args) = shape;
    let recipe = rascal_mask::describe::recipe_of(action)
        .ok_or_else(|| Error::usage(format!("action: no action {action}")))?;
    let answers = Default::default();
    let failed = Default::default();
    let setup = Setup {
        config: state,
        answers: &answers,
        failed_packs: &failed,
    };
    check_arguments(recipe, action, args, state.config(), &PathRule::Own)?;
    let price = price(&setup, recipe, action, args)?;
    let bound = bind(
        recipe,
        action,
        args,
        price.config_identity.as_deref(),
        None,
        VERSION,
        &PathRule::Own,
    )?;
    Ok(document(
        action,
        &bound,
        price,
        None,
        None,
        run_json::now_ms() + ttl_ms(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pano() -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/images/step-pano.png")
            .display()
            .to_string()
    }

    /// The tool version is part of the binding: one call, one configuration, one session, two
    /// versions, two bindings.
    #[test]
    fn the_binding_changes_with_the_version() {
        let args = json!({ "image": { "path": pano() }, "filler": "fake" });
        let args = args.as_object().unwrap();
        let at = |version: &str| {
            bind(
                "seam",
                "seam",
                args,
                Some("sha256:00"),
                Some("s"),
                version,
                &PathRule::Own,
            )
            .unwrap()
        };
        assert_eq!(at("0.1.1"), at("0.1.1"));
        assert_ne!(at("0.1.1").binding, at("0.1.2").binding);
        assert_eq!(at("0.1.1").parts, at("0.1.2").parts);
    }

    /// Delivery choices are left out; an image enters by its digest.
    #[test]
    fn the_binding_leaves_out_delivery_and_hashes_images() {
        let plain = json!({ "image": { "path": pano() } });
        let delivered = json!({ "image": { "path": pano() }, "return_images": "resource",
                                "image_blocks": true });
        let bind_of = |v: &Value| {
            bind(
                "seam",
                "seam",
                v.as_object().unwrap(),
                None,
                None,
                VERSION,
                &PathRule::Own,
            )
            .unwrap()
        };
        assert_eq!(bind_of(&plain), bind_of(&delivered));
        let bytes = std::fs::read(pano()).unwrap();
        let digest = rascal_core::canonical::identity(
            "estimate-argument",
            &json!({ "file_sha256": rascal_core::contract::file_sha256(&bytes) }),
        );
        assert_eq!(bind_of(&plain).parts["image"], digest.as_str());
    }

    fn issued(parts: &[(&str, &str)]) -> IssuedEstimate {
        IssuedEstimate {
            action: "seam".to_string(),
            expires_ms: 0,
            parts: parts
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    /// The constant sets in force: the recipe's, a filler's override over it, the call's `set`
    /// over both; a step naming none runs its op's first set; a step without sets is absent.
    #[test]
    fn the_constants_in_force_are_the_last_word_by_step() {
        use rascal_core::recipe::Override;
        let seam = constants("seam", &BTreeMap::new(), None).unwrap();
        assert_eq!(
            seam,
            [
                ("fan", "fan-v3"),
                ("band", "profile-v1"),
                ("ramp", "ramp-v1")
            ]
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .into()
        );
        let filler: BTreeMap<String, Override> = [(
            "fan.constants".to_string(),
            Override::String("fan-v1".to_string()),
        )]
        .into();
        assert_eq!(constants("seam", &filler, None).unwrap()["fan"], "fan-v1");
        let set = json!({ "fan.constants": "fan-v2", "fan.min_px": 3 });
        assert_eq!(
            constants("seam", &filler, Some(&set)).unwrap()["fan"],
            "fan-v2"
        );
        // `remove`'s feather names no set: its op's first.
        assert_eq!(
            constants("remove", &BTreeMap::new(), None).unwrap(),
            [("feather".to_string(), "feather-v1".to_string())].into()
        );
    }

    /// The table keeps the newest [`Issued::LIMIT`] bindings.
    #[test]
    fn the_issued_table_keeps_the_newest() {
        let mut table = Issued::default();
        for n in 0..=Issued::LIMIT {
            table.record(&format!("b{n}"), issued(&[]));
        }
        assert!(table.get("b0").is_none());
        assert!(table.get("b1").is_some());
        assert!(table.get(&format!("b{}", Issued::LIMIT)).is_some());
        // Recording one again makes it the newest.
        table.record("b1", issued(&[]));
        table.record("fresh", issued(&[]));
        assert!(table.get("b1").is_some());
        assert!(table.get("b2").is_none());
    }

    /// What changed is named: an image by its slot, any other argument added, removed or changed.
    #[test]
    fn what_changed_is_named() {
        let then = issued(&[("image", "a"), ("filler", "f"), ("strict", "t")]);
        let now: BTreeMap<String, String> = [("image", "b"), ("filler", "g"), ("set", "s")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let text = changed("seam", &then, "seam", &now);
        assert_eq!(
            text,
            "the argument `filler` changed; the image `image` changed; the argument `set` was added; the argument `strict` was removed"
        );
        assert_eq!(
            changed("seam", &then, "remove", &then.parts),
            "it was made for seam, not remove"
        );
    }
}
