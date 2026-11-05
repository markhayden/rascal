//! Whether a call's result will be checked (SPEC.md "Self-correcting remove", part 3; ADR
//! 0023). One function settles it, before readiness, the estimate and the run, so all three
//! agree: a check runs only for `remove_prompt` with a configured `[checker]`, a filler, its
//! key present and `check` not turned off. Every other case says why it is off, so the run can
//! say "Not checked: …" and the invocation can record it.

use rascal_core::config::Config;
use serde_json::{Map, Value};

use crate::readiness::SEGMENTER_ACTION;

/// The most tries a checked call makes, the first included.
pub const MAX_TRIES: u32 = 3;

/// The argument that turns the check off for one call.
pub const ARGUMENT: &str = "check";

/// Whether the call's result will be checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// It will: up to [`MAX_TRIES`] tries, each filled and checked.
    On,
    /// It will not, and why.
    Off(Off),
}

/// Why a call is not checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Off {
    /// Not `remove_prompt`: only a recipe with a prompt has words to check against.
    NotChecked,
    /// No filler: there is no result to check, only a mask.
    NoFiller,
    /// No `[checker]` in the configuration.
    NoChecker,
    /// The call turned it off (`"check": false`, `--no-check`).
    TurnedOff,
    /// The checker's key is not in the environment (its variable's name).
    KeyMissing(String),
}

impl Check {
    /// The check's record in the invocation's `effective` (`x-rascal.check`): `on`, or `off`
    /// with the reason; none for an action that is never checked or when no checker is
    /// configured, so those effectives are unchanged.
    pub fn record(&self) -> Option<Value> {
        match self {
            Check::On => Some(serde_json::json!({ "on": true })),
            Check::Off(Off::NotChecked | Off::NoChecker) => None,
            Check::Off(off) => Some(serde_json::json!({ "on": false, "reason": off.word() })),
        }
    }

    /// Whether the check runs.
    pub fn on(&self) -> bool {
        matches!(self, Check::On)
    }
}

impl Off {
    /// The reason as a word for the record.
    pub fn word(&self) -> &'static str {
        match self {
            Off::NotChecked => "not_checked",
            Off::NoFiller => "no_filler",
            Off::NoChecker => "no_checker",
            Off::TurnedOff => "turned_off",
            Off::KeyMissing(_) => "key_missing",
        }
    }

    /// The sentence a result's summary says when a configured checker did not run; none when no
    /// checker was configured or the action is never checked (nothing to explain).
    pub fn sentence(&self) -> Option<String> {
        match self {
            Off::NotChecked | Off::NoChecker | Off::NoFiller => None,
            Off::TurnedOff => Some("Not checked: the check was turned off for this call.".into()),
            Off::KeyMissing(var) => Some(format!("Not checked: the checker needs {var}.")),
        }
    }
}

/// Whether a call of `action` with `args` will be checked under `config`. The key's presence is
/// read from the environment by name, never its value.
pub fn resolve(config: Option<&Config>, action: &str, args: &Map<String, Value>) -> Check {
    resolve_with(config, action, args, |var| std::env::var_os(var).is_some())
}

/// [`resolve`] with the environment given, for tests.
pub fn resolve_with(
    config: Option<&Config>,
    action: &str,
    args: &Map<String, Value>,
    has_var: impl Fn(&str) -> bool,
) -> Check {
    if action != SEGMENTER_ACTION {
        return Check::Off(Off::NotChecked);
    }
    let Some(checker) = config.and_then(|c| c.checker.as_ref()) else {
        return Check::Off(Off::NoChecker);
    };
    if !args.get("filler").is_some_and(Value::is_string) {
        return Check::Off(Off::NoFiller);
    }
    if args.get(ARGUMENT) == Some(&Value::Bool(false)) {
        return Check::Off(Off::TurnedOff);
    }
    if let Some(var) = &checker.key_env
        && !has_var(var)
    {
        return Check::Off(Off::KeyMissing(var.clone()));
    }
    Check::On
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CONFIG: &str = r#"
config_version = 1
[fillers.bria]
command = ["fill", "{image}", "{mask}", "{out}"]
profile = "binary"
[checker]
command = ["/opt/squint", "call", "source_fidelity"]
arguments = { source = { path = "{source}" }, candidate = { path = "{candidate}" } }
key_env = "CHECKER_KEY"
spend_usd_per_call = 0.01
"#;

    fn args(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn each_of_the_five_conditions_turns_the_check_off_on_its_own() {
        let config = Config::parse(CONFIG, "rascal.toml").unwrap_or_else(|e| panic!("{e}"));
        let bare =
            Config::parse("config_version = 1", "rascal.toml").unwrap_or_else(|e| panic!("{e}"));
        let with_filler = args(json!({ "prompt": "collar", "filler": "bria" }));
        let key = |_: &str| true;
        let no_key = |_: &str| false;
        assert_eq!(
            resolve_with(Some(&config), "remove_prompt", &with_filler, key),
            Check::On
        );
        assert_eq!(
            resolve_with(Some(&config), "remove", &with_filler, key),
            Check::Off(Off::NotChecked)
        );
        assert_eq!(
            resolve_with(
                Some(&config),
                "remove_prompt",
                &args(json!({ "prompt": "c" })),
                key
            ),
            Check::Off(Off::NoFiller)
        );
        assert_eq!(
            resolve_with(Some(&bare), "remove_prompt", &with_filler, key),
            Check::Off(Off::NoChecker)
        );
        assert_eq!(
            resolve_with(None, "remove_prompt", &with_filler, key),
            Check::Off(Off::NoChecker)
        );
        let off = args(json!({ "prompt": "c", "filler": "bria", "check": false }));
        assert_eq!(
            resolve_with(Some(&config), "remove_prompt", &off, key),
            Check::Off(Off::TurnedOff)
        );
        assert_eq!(
            resolve_with(Some(&config), "remove_prompt", &with_filler, no_key),
            Check::Off(Off::KeyMissing("CHECKER_KEY".into()))
        );
        let on = args(json!({ "prompt": "c", "filler": "bria", "check": true }));
        assert_eq!(
            resolve_with(Some(&config), "remove_prompt", &on, key),
            Check::On
        );
    }

    #[test]
    fn the_record_and_the_sentence_say_why() {
        assert_eq!(Check::On.record(), Some(json!({ "on": true })));
        assert_eq!(Check::Off(Off::NotChecked).record(), None);
        assert_eq!(Check::Off(Off::NoChecker).record(), None);
        assert_eq!(
            Check::Off(Off::KeyMissing("K".into())).record(),
            Some(json!({ "on": false, "reason": "key_missing" }))
        );
        assert_eq!(
            Off::KeyMissing("ANTHROPIC_API_KEY".into())
                .sentence()
                .as_deref(),
            Some("Not checked: the checker needs ANTHROPIC_API_KEY.")
        );
        assert_eq!(Off::NoChecker.sentence(), None);
    }
}
