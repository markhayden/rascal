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
