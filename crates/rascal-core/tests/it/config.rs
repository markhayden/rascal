//! Task 12.2 (table-manners 3.1, 4.6): the configuration is read once into a state that never
//! fails: missing (no `--config`, or a path naming no file), invalid (it does not parse or
//! check, with the reason and never the offending text), or loaded with its identity, the
//! SHA-256 of its canonical serialization; with no file, the built-in defaults' identity.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use rascal_core::config::{Config, ConfigState};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-config-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn no_file_is_missing_under_the_built_in_defaults_identity() {
    let state = ConfigState::read(None);
    assert_eq!(state, ConfigState::Missing { path: None });
    assert!(state.config().is_none());
    let identity = state.identity().unwrap();
    assert_eq!(identity, Config::builtin().identity());
    assert!(
        identity.starts_with("sha256:") && identity.len() == 71,
        "{identity}"
    );
}

#[test]
fn a_path_that_names_no_file_is_missing_and_has_no_identity() {
    let dir = scratch("absent");
    let path = dir.join("nowhere.toml");
    let state = ConfigState::read(Some(&path));
    assert_eq!(
        state,
        ConfigState::Missing {
            path: Some(path.clone())
        }
    );
    assert_eq!(state.path(), Some(path.as_path()));
    assert!(state.identity().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_does_not_parse_or_check_is_invalid_without_its_text() {
    let dir = scratch("invalid");
    let path = dir.join("bad.toml");
    std::fs::write(
        &path,
        "config_version = 1\nzq_secret_looking = \"zq-value-zq\n",
    )
    .unwrap();
    let ConfigState::Invalid { reason, .. } = ConfigState::read(Some(&path)) else {
        panic!("not invalid");
    };
    assert!(reason.contains("line 2"), "{reason}");
    assert!(!reason.contains("zq-value-zq"), "{reason}");
    // A file that parses but does not check is invalid too, with the check's reason.
    std::fs::write(&path, "config_version = 2\n").unwrap();
    let state = ConfigState::read(Some(&path));
    let ConfigState::Invalid { reason, .. } = &state else {
        panic!("not invalid");
    };
    assert!(reason.contains("config_version 2"), "{reason}");
    assert!(state.identity().is_none() && state.config().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_loaded_file_is_identified_by_what_it_configures_not_its_layout() {
    let dir = scratch("loaded");
    let a = dir.join("a.toml");
    let b = dir.join("b.toml");
    std::fs::write(
        &a,
        "config_version = 1\n[fillers.f]\ncommand = [\"x\", \"{image}\", \"{out}\"]\nprofile = \"alpha\"\n",
    )
    .unwrap();
    std::fs::write(
        &b,
        "# the same filler, spelled out\nconfig_version = 1\n\n[fillers.f]\nprofile = \"alpha\"\npaste = \"feather\"\ncommand = [\"x\", \"{image}\", \"{out}\"]\n",
    )
    .unwrap();
    let (sa, sb) = (ConfigState::read(Some(&a)), ConfigState::read(Some(&b)));
    assert!(sa.config().is_some());
    assert_eq!(sa.identity(), sb.identity());
    assert_ne!(sa.identity(), Some(Config::builtin().identity()));
    // Another price is another configuration.
    std::fs::write(
        &b,
        "config_version = 1\n[fillers.f]\ncommand = [\"x\", \"{image}\", \"{out}\"]\nprofile = \"alpha\"\nspend_usd_per_call = 0.01\n",
    )
    .unwrap();
    assert_ne!(sa.identity(), ConfigState::read(Some(&b)).identity());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Task 10.1: `[serve] allow_dirs` takes absolute directories; a relative one does not check.
/// Unset, there is no `[serve]` in the identity, so earlier files keep theirs.
#[test]
fn serve_allow_dirs_are_absolute() {
    let config = Config::parse(
        "config_version = 1\n[serve]\nallow_dirs = [\"/srv/in\", \"/srv/more\"]\n",
        "t",
    )
    .unwrap();
    assert_eq!(
        config.serve.unwrap().allow_dirs.unwrap(),
        vec!["/srv/in".to_string(), "/srv/more".to_string()]
    );
    let empty = Config::parse("config_version = 1\n[serve]\nallow_dirs = []\n", "t").unwrap();
    assert_eq!(empty.serve.unwrap().allow_dirs, Some(Vec::new()));
    let error = Config::parse("config_version = 1\n[serve]\nallow_dirs = [\"in\"]\n", "t")
        .unwrap_err()
        .to_string();
    assert_eq!(error, "t: serve.allow_dirs: in is not an absolute path");
    assert_eq!(
        Config::parse("config_version = 1\n", "t")
            .unwrap()
            .identity(),
        Config::builtin().identity()
    );
}

/// Task 13.10: `[checker]` (SPEC.md "Self-correcting remove", part 3): the command, the
/// checker's own arguments with the three placeholders, a key, a price per check, a timeout.
const CHECKER: &str = r#"
config_version = 1
[checker]
command = ["/opt/squint/bin/squint", "call", "source_fidelity"]
arguments = { source = { path = "{source}" }, candidate = { path = "{candidate}" }, requirements = [{ id = "RQ1", kind = "must_not", text = "Any part of {prompt} is visible." }] }
key_env = "ANTHROPIC_API_KEY"
spend_usd_per_call = 0.01
revision = "squint 0.1, claude-haiku-5-5"
"#;

#[test]
fn a_checker_parses_and_its_key_is_kept_from_packs() {
    let config = Config::parse(CHECKER, "rascal.toml").unwrap();
    let checker = config.checker.as_ref().unwrap();
    assert_eq!(checker.timeout_s, 120, "the filler's default timeout");
    assert_eq!(checker.spend_usd_per_call, 0.01);
    assert!(config.key_envs().contains(&"ANTHROPIC_API_KEY".to_string()));
}

#[test]
fn a_checker_entry_that_does_not_check_is_refused_naming_why() {
    for (from, to, why) in [
        (
            r#"command = ["/opt/squint/bin/squint", "call", "source_fidelity"]"#,
            "command = []",
            "command is empty",
        ),
        (
            "spend_usd_per_call = 0.01",
            "spend_usd_per_call = -1.0",
            "spend_usd_per_call",
        ),
        (
            "spend_usd_per_call = 0.01",
            "spend_usd_per_call = 0.01\ntimeout_s = 0",
            "timeout_s",
        ),
        (
            r#"key_env = "ANTHROPIC_API_KEY""#,
            r#"key_env = "A KEY""#,
            "key_env",
        ),
        (
            "{prompt} is visible",
            "{object} is visible",
            "arguments name {object}",
        ),
        (
            r#"candidate = { path = "{candidate}" }, "#,
            "",
            "arguments name no {candidate}",
        ),
        (
            r#"source = { path = "{source}" }, "#,
            "",
            "arguments name no {source}",
        ),
    ] {
        assert!(CHECKER.contains(from), "{from}");
        let text = CHECKER.replace(from, to);
        let error = Config::parse(&text, "rascal.toml").unwrap_err().to_string();
        assert!(
            error.contains("checker") && error.contains(why),
            "{why}: {error}"
        );
    }
    let missing_price = CHECKER.replace("spend_usd_per_call = 0.01\n", "");
    assert!(
        Config::parse(&missing_price, "rascal.toml").is_err(),
        "a price is required"
    );
}
