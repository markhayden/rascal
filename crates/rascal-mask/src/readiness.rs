//! Readiness (table-manners 3.1; PLAN.md 12.2): what stands between an action and a run, by
//! local inspection only. It reads the configuration state, the environment's variable names,
//! the file system and what this session's providers already answered; it never starts a pack,
//! sends a request, reads a credential's value or writes anything.
//!
//! One function, [`for_call`], answers for a call (its one filler, or none) and for the
//! description, whose readiness is an action's default use: the mask alone when no filler is
//! configured, healing with any configured filler otherwise.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rascal_core::config::{Config, ConfigState, FillerConfig, PackConfig};
use rascal_core::contract::table_manners::{
    Readiness, ReadinessState, Requirement, RequirementKind, RequirementState,
};
use rascal_mcp::provision::{PACK_NAME, install_dir_of};

/// The configuration requirement's name.
pub const CONFIG_NAME: &str = "rascal.toml";

/// The action that needs the segmenter pack.
pub const SEGMENTER_ACTION: &str = "remove_prompt";

/// What an optional checker requirement is for (table-manners 3.1's `for`).
pub const CHECK_FOR: &str = "checking the result";

/// The variable the segmenter pack reads its model directory from.
pub const MODELS_ENV: &str = "RASCAL_TOOLS_MODELS";

/// What a provider said about a credential in this session (table-manners 3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// A provider accepted it.
    Valid,
    /// A provider answered 401 or 403 to it.
    Invalid,
}

/// The session's answers by credential variable name.
pub type Answers = BTreeMap<String, Answer>;

/// Which fillers a use reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Use<'a> {
    /// The call names no filler: the mask alone.
    MaskOnly,
    /// The call names this filler.
    Filler(&'a str),
    /// The action's default use, for the description: healing with any configured filler (the
    /// union of their requirements), or the mask alone when none is configured.
    Configured,
}

/// What readiness inspects beyond the environment and the file system.
#[derive(Debug, Clone, Copy)]
pub struct Setup<'a> {
    /// The configuration, as read once.
    pub config: &'a ConfigState,
    /// What providers said about credentials in this session (empty outside `serve`).
    pub answers: &'a Answers,
    /// The packs `serve` failed to start, with the reason (empty outside `serve`).
    pub failed_packs: &'a BTreeMap<String, String>,
}

/// The readiness of `action` for the use `filler`: every requirement that use reads, and
/// `needs_setup` when one is `missing` or `invalid` (rascal never reports `unavailable`).
pub fn for_call(setup: &Setup<'_>, action: &str, filler: Use<'_>) -> Readiness {
    let mut requirements: Vec<Requirement> = Vec::new();
    let segmenter = action == SEGMENTER_ACTION;
    let reads_config = match filler {
        Use::Filler(_) => true,
        Use::MaskOnly => segmenter,
        Use::Configured => segmenter || setup.config.path().is_some(),
    };
    if reads_config {
        requirements.push(config_requirement(setup.config, segmenter));
    }
    if let Some(config) = setup.config.config() {
        match filler {
            Use::MaskOnly => {}
            Use::Filler(name) => match config.fillers.get(name) {
                Some(entry) => filler_requirements(setup, name, entry, &mut requirements),
                None => requirements.push(Requirement {
                    kind: RequirementKind::Model,
                    name: format!("fillers.{name}"),
                    state: RequirementState::Missing,
                    setup: Some(format!(
                        "Add `[fillers.{name}]` to {CONFIG_NAME}, or name a configured filler."
                    )),
                    optional: false,
                    needed_for: None,
                }),
            },
            Use::Configured => {
                for (name, entry) in &config.fillers {
                    filler_requirements(setup, name, entry, &mut requirements);
                }
            }
        }
    }
    if segmenter {
        segmenter_requirements(setup, &mut requirements);
        let fills = match filler {
            Use::MaskOnly => false,
            Use::Filler(_) => true,
            Use::Configured => setup.config.config().is_some_and(|c| !c.fillers.is_empty()),
        };
        if fills {
            checker_requirements(setup, &mut requirements);
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    requirements.retain(|r| seen.insert((format!("{:?}", r.kind), r.name.clone())));
    Readiness {
        state: if requirements.iter().any(unmet) {
            ReadinessState::NeedsSetup
        } else {
            ReadinessState::Ready
        },
        requirements,
    }
}

/// Whether a requirement stands in the way: `missing` or `invalid`, and not optional (an
/// optional one limits a part of the action, never the action: table-manners 3.1).
pub fn unmet(requirement: &Requirement) -> bool {
    !requirement.optional
        && matches!(
            requirement.state,
            RequirementState::Missing | RequirementState::Invalid
        )
}

/// What the checker needs (ADR 0023), each optional and for checking the result: its program
/// and its key. Missing either, the remove still runs and fills, unchecked, and says why.
fn checker_requirements(setup: &Setup<'_>, out: &mut Vec<Requirement>) {
    let Some(checker) = setup.config.config().and_then(|c| c.checker.as_ref()) else {
        return;
    };
    let optional = |mut r: Requirement| {
        r.optional = true;
        r.needed_for = Some(CHECK_FOR.to_string());
        r
    };
    if let Some(program) = checker.command.first() {
        let found = executable(program);
        out.push(optional(Requirement {
            kind: RequirementKind::Dependency,
            name: "checker".to_string(),
            state: if found {
                RequirementState::Present
            } else {
                RequirementState::Missing
            },
            setup: (!found).then(|| {
                format!("Install `{program}`, or point `checker.command` at a program that exists.")
            }),
            optional: false,
            needed_for: None,
        }));
    }
    if let Some(var) = &checker.key_env {
        out.push(optional(credential(setup.answers, var)));
    }
}

fn config_requirement(state: &ConfigState, segmenter: bool) -> Requirement {
    let (state, setup) = match state {
        ConfigState::Loaded { .. } => (RequirementState::Present, None),
        ConfigState::Invalid { reason, .. } => (
            RequirementState::Invalid,
            Some(format!(
                "{reason}. Fix it, or start rascal with `--config <path>` naming another file."
            )),
        ),
        ConfigState::Missing { path: Some(path) } => (
            RequirementState::Missing,
            Some(format!(
                "{} does not exist. Start rascal with `--config <path>` naming a configuration file.",
                path.display()
            )),
        ),
        ConfigState::Missing { path: None } => (
            RequirementState::Missing,
            Some(if segmenter {
                "Start rascal with `--config <path>` naming the segmenter pack.".to_string()
            } else {
                "Start rascal with `--config <path>` naming the filler.".to_string()
            }),
        ),
    };
    Requirement {
        kind: RequirementKind::Config,
        name: CONFIG_NAME.to_string(),
        state,
        setup,
        optional: false,
        needed_for: None,
    }
}

fn filler_requirements(
    setup: &Setup<'_>,
    name: &str,
    entry: &FillerConfig,
    out: &mut Vec<Requirement>,
) {
    out.push(Requirement {
        kind: RequirementKind::Model,
        name: format!("fillers.{name}"),
        state: RequirementState::Present,
        setup: None,
        optional: false,
        needed_for: None,
    });
    if let Some(var) = &entry.key_env {
        out.push(credential(setup.answers, var));
    }
    if let Some(program) = entry.command.as_ref().and_then(|c| c.first()) {
        let found = executable(program);
        out.push(Requirement {
            kind: RequirementKind::Dependency,
            name: program.clone(),
            state: if found {
                RequirementState::Present
            } else {
                RequirementState::Missing
            },
            setup: (!found).then(|| {
                format!("Install `{program}`, or point `fillers.{name}.command` at a program that exists.")
            }),
            optional: false,
            needed_for: None,
        });
    }
}

/// A credential by its variable's name: `missing` when the variable does not exist, else what
/// a provider said in this session, else `present`. Its value is never read.
fn credential(answers: &Answers, var: &str) -> Requirement {
    let state = if std::env::var_os(var).is_none() {
        RequirementState::Missing
    } else {
        match answers.get(var) {
            Some(Answer::Valid) => RequirementState::Valid,
            Some(Answer::Invalid) => RequirementState::Invalid,
            None => RequirementState::Present,
        }
    };
    let setup = match state {
        RequirementState::Invalid => format!(
            "A provider rejected `{var}` in this session. Set a working key in the environment and restart rascal."
        ),
        _ => format!("Set `{var}` in the environment rascal is started with."),
    };
    Requirement {
        kind: RequirementKind::Credential,
        name: var.to_string(),
        state,
        setup: Some(setup),
        optional: false,
        needed_for: None,
    }
}

/// Whether `program` would start: a path (with a `/`) to an executable file, or a name found
/// as one on `PATH`.
fn executable(program: &str) -> bool {
    if program.contains('/') {
        return is_executable(Path::new(program));
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| is_executable(&dir.join(program))))
        .unwrap_or(false)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// The segmenter pack: the entry whose command is `<dir>/.venv/bin/rascal-tools` (the rule
/// `rascal tools setup` uses), the non-secret variables it passes, and its model files.
fn segmenter_requirements(setup: &Setup<'_>, out: &mut Vec<Requirement>) {
    let entry = setup.config.config().and_then(segmenter_entry);
    let Some((name, entry, dir)) = entry else {
        out.push(Requirement {
            kind: RequirementKind::Dependency,
            name: format!("packs.{PACK_NAME}"),
            state: RequirementState::Missing,
            setup: Some("Run `rascal tools setup --bundle <bundle> --config <path>`.".to_string()),
            optional: false,
            needed_for: None,
        });
        return;
    };
    let launcher = Path::new(&entry.command[0]);
    if !launcher.is_file() {
        out.push(Requirement {
            kind: RequirementKind::Dependency,
            name: format!("packs.{name}"),
            state: RequirementState::Missing,
            setup: Some(format!(
                "{} does not exist. Run `rascal tools setup --bundle <bundle> --config <path>`.",
                launcher.display()
            )),
            optional: false,
            needed_for: None,
        });
        return;
    }
    if let Some(reason) = setup.failed_packs.get(name) {
        out.push(Requirement {
            kind: RequirementKind::Dependency,
            name: format!("packs.{name}"),
            state: RequirementState::Invalid,
            setup: Some(format!(
                "`rascal serve` could not start it: {reason}. Fix the install and restart rascal."
            )),
            optional: false,
            needed_for: None,
        });
        return;
    }
    out.push(Requirement {
        kind: RequirementKind::Dependency,
        name: format!("packs.{name}"),
        state: RequirementState::Present,
        setup: None,
        optional: false,
        needed_for: None,
    });
    let passes_models = entry.env.iter().any(|v| v == MODELS_ENV);
    let models_env = passes_models
        .then(|| std::env::var_os(MODELS_ENV))
        .flatten();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let models = models_dir(&dir, models_env.as_deref(), home.as_deref());
    let files = model_files(&dir, &models);
    let all_present = files.iter().all(|f| !unmet(f));
    let key_envs = setup
        .config
        .config()
        .map(Config::key_envs)
        .unwrap_or_default();
    for var in &entry.env {
        if key_envs.contains(var) || secret_shaped(var) {
            continue;
        }
        if std::env::var_os(var).is_some() {
            out.push(Requirement {
                kind: RequirementKind::Env,
                name: var.clone(),
                state: RequirementState::Present,
                setup: None,
                optional: false,
                needed_for: None,
            });
        } else if var == MODELS_ENV && !all_present {
            out.push(Requirement {
                kind: RequirementKind::Env,
                name: var.clone(),
                state: RequirementState::Missing,
                setup: Some(format!(
                    "Set `{MODELS_ENV}` to the directory holding the model files, or put them in {}.",
                    models.display()
                )),
                optional: false,
                needed_for: None,
            });
        }
    }
    out.extend(files);
}

/// The first entry (by name) that is a `rascal tools setup` install, with its directory.
fn segmenter_entry(config: &Config) -> Option<(&String, &PackConfig, PathBuf)> {
    config.packs.iter().find_map(|(name, entry)| {
        install_dir_of(&entry.command, PACK_NAME).map(|d| (name, entry, d))
    })
}

/// A name the pack's environment never receives (`rascal_mcp`'s allowlist removes it).
fn secret_shaped(var: &str) -> bool {
    ["_KEY", "_TOKEN", "_SECRET"]
        .iter()
        .any(|suffix| var.ends_with(suffix))
}

/// Where the segmenter pack reads its model files, exactly as it looks
/// (`rascal_tools.models.cache.models_dir`): `RASCAL_TOOLS_MODELS` when the pack receives it
/// non-empty, else `models/` in the install directory when it exists, else the pack's default
/// cache, `~/.cache/rascal-tools`.
pub fn models_dir(
    install: &Path,
    models_env: Option<&std::ffi::OsStr>,
    home: Option<&Path>,
) -> PathBuf {
    if let Some(dir) = models_env.filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    let beside = install.join("models");
    if beside.is_dir() {
        return beside;
    }
    home.map(Path::to_path_buf)
        .unwrap_or_default()
        .join(".cache/rascal-tools")
}

/// One `file` requirement per model artifact the install's `artifacts.toml` names: at the
/// install's root (where a bundle puts it) or in the package source (a checkout's
/// `tools/python`). A file is `present` when it is there and not empty; its digest is not
/// hashed here (the pack verifies it before use), so it is never `valid`.
fn model_files(install: &Path, models: &Path) -> Vec<Requirement> {
    let manifest = [
        install.join("artifacts.toml"),
        install.join("src/rascal_tools/artifacts.toml"),
    ]
    .into_iter()
    .find(|p| p.is_file());
    let names = manifest
        .as_deref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| toml::from_str::<ArtifactsToml>(&text).ok())
        .map(|a| a.artifact.into_iter().map(|a| a.name).collect::<Vec<_>>());
    let Some(names) = names else {
        return vec![Requirement {
            kind: RequirementKind::File,
            name: "artifacts.toml".to_string(),
            state: RequirementState::Missing,
            setup: Some(format!(
                "{} holds no readable artifacts.toml. Run `rascal tools setup --bundle <bundle> --config <path>` again.",
                install.display()
            )),
            optional: false,
            needed_for: None,
        }];
    };
    names
        .into_iter()
        .map(|name| {
            let there = std::fs::metadata(models.join(&name)).is_ok_and(|m| m.is_file() && m.len() > 0);
            Requirement {
                kind: RequirementKind::File,
                state: if there {
                    RequirementState::Present
                } else {
                    RequirementState::Missing
                },
                setup: (!there).then(|| {
                    format!(
                        "Not in {}. Run `rascal-tools fetch`, or `rascal tools setup` with a bundle that holds the models, or set `{MODELS_ENV}`.",
                        models.display()
                    )
                }),
                name,
                optional: false,
                needed_for: None,
            }
        })
        .collect()
}

#[derive(serde::Deserialize)]
struct ArtifactsToml {
    #[serde(default)]
    artifact: Vec<ArtifactEntry>,
}

#[derive(serde::Deserialize)]
struct ArtifactEntry {
    name: String,
}

/// The requirements as one line for people: `config rascal.toml missing; credential KEY
/// invalid`. Names only, never a value.
pub fn summary(requirements: &[Requirement]) -> String {
    requirements
        .iter()
        .map(|r| format!("{} {} {}", word(&r.kind), r.name, word(&r.state)))
        .collect::<Vec<_>>()
        .join("; ")
}

/// A contract enum's word (`credential`, `missing`).
fn word<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}
