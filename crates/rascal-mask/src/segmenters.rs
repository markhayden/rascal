//! The segmenter catalogue as rascal sees it (SPEC.md "Choosable segmenters"): the entries of
//! the configured segmenter pack's `segmenters.toml` and each model's installation state, read
//! from disk with no network and no pack started (a local inspection, as `describe` must be).
//! The model a call runs is resolved here, once: the call's `model`, else `[segment] model`,
//! else the catalogue's default.

use std::path::{Path, PathBuf};

use rascal_core::Error;
use rascal_core::config::Config;
use rascal_core::error::Result;
use serde::Deserialize;

use crate::readiness::{MODELS_ENV, models_dir};

/// The catalogue file inside a `rascal tools setup` install (or a checkout's `tools/python`).
pub const CATALOGUE: &str = "segmenters.toml";

/// One catalogued model, as `segmenters.toml` lists it.
#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub params: Vec<String>,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub licence: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub licence_files: Vec<String>,
    #[serde(default)]
    pub download_mb: u64,
    #[serde(default)]
    pub installed_mb: u64,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub lab: Option<toml::Table>,
}

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    segmenter: Vec<Entry>,
}

/// The configured segmenter pack's catalogue and where its models live.
#[derive(Debug, Clone)]
pub struct Segmenters {
    /// The pack's install directory.
    pub install: PathBuf,
    /// Where its models are installed (`RASCAL_TOOLS_MODELS`, else the install's `models/`,
    /// else `~/.cache/rascal-tools`), as the pack reads it.
    pub models: PathBuf,
    pub entries: Vec<Entry>,
}

/// An installable's state, read from disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Installed,
    NotInstalled,
    Installing,
    Unavailable,
}

impl Segmenters {
    /// The catalogue of the first configured pack that is a `rascal tools setup` install and
    /// carries one; none when there is no such pack.
    pub fn of(config: &Config) -> Option<Segmenters> {
        let (_, entry, install) = crate::readiness::segmenter_pack(config)?;
        let passes_models = entry.env.iter().any(|v| v == MODELS_ENV);
        let env = passes_models
            .then(|| std::env::var_os(MODELS_ENV))
            .flatten();
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let models = models_dir(&install, env.as_deref(), home.as_deref());
        Segmenters::read(&install, models)
    }

    /// The catalogue in `install` (at its root, or under `src/rascal_tools/` in a checkout).
    pub fn read(install: &Path, models: PathBuf) -> Option<Segmenters> {
        let path = [
            install.join(CATALOGUE),
            install.join("src/rascal_tools").join(CATALOGUE),
        ]
        .into_iter()
        .find(|p| p.is_file())?;
        let text = std::fs::read_to_string(path).ok()?;
        let file: File = toml::from_str(&text).ok()?;
        Some(Segmenters {
            install: install.to_path_buf(),
            models,
            entries: file.segmenter,
        })
    }

    /// The pack's version, which versions its adapters: the install's provisioning
    /// `pyproject.toml` (or a checkout's), `[project] version`.
    pub fn pack_version(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.install.join("pyproject.toml")).ok()?;
        let table: toml::Table = toml::from_str(&text).ok()?;
        table
            .get("project")?
            .get("version")?
            .as_str()
            .map(str::to_string)
    }

    /// What a call of `remove_prompt` would run, for its estimate and invocation
    /// (`effective["x-rascal"].segmenter`): the model (the call's, else `[segment] model`, else
    /// the default), its current installation (null when not installed) and the adapters'
    /// version.
    pub fn effective(&self, call: Option<&str>, config: &Config) -> serde_json::Value {
        let model = call
            .map(str::to_string)
            .or_else(|| config.segment.as_ref().map(|s| s.model.clone()))
            .or_else(|| self.default_entry().map(|e| e.id.clone()));
        match model {
            Some(model) => serde_json::json!({
                "model": model,
                "installation": self.current(&model),
                "adapter_version": self.pack_version(),
            }),
            None => serde_json::Value::Null,
        }
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn default_entry(&self) -> Option<&Entry> {
        self.entries.iter().find(|e| e.default)
    }

    /// The model a call runs when it names none: `[segment] model` when it is catalogued, else
    /// the catalogue's default.
    pub fn default_model(&self, config: Option<&Config>) -> Option<String> {
        config
            .and_then(|c| c.segment.as_ref())
            .map(|s| s.model.clone())
            .filter(|m| self.entry(m).is_some())
            .or_else(|| self.default_entry().map(|e| e.id.clone()))
    }

    /// The model's current installation identity, when installed.
    pub fn current(&self, id: &str) -> Option<String> {
        if !plain(id) {
            return None;
        }
        let ident = std::fs::read_to_string(self.models.join(id).join("current")).ok()?;
        let ident = ident.trim().to_string();
        (plain(&ident) && self.models.join(id).join(&ident).is_dir()).then_some(ident)
    }

    /// Whether the model's current installation holds every file the catalogue names, each
    /// non-empty (the pack checks their digests before use; this is a local inspection).
    pub fn complete(&self, entry: &Entry) -> bool {
        let Some(ident) = self.current(&entry.id) else {
            return false;
        };
        let dir = self.models.join(&entry.id).join(ident);
        entry
            .files
            .iter()
            .chain(&entry.licence_files)
            .all(|f| std::fs::metadata(dir.join(f)).is_ok_and(|m| m.is_file() && m.len() > 0))
    }

    /// The installer's status for the model (`<models>/.status/<id>.json`), when there is one.
    pub fn status(&self, id: &str) -> Option<serde_json::Value> {
        if !plain(id) {
            return None;
        }
        let text =
            std::fs::read_to_string(self.models.join(".status").join(format!("{id}.json"))).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// The model's state: unavailable off its platforms, installing while a live installer
    /// says so, else installed or not.
    pub fn state(&self, entry: &Entry) -> State {
        if !entry.platforms.is_empty() && !entry.platforms.iter().any(|p| p == platform()) {
            return State::Unavailable;
        }
        let status = self.status(&entry.id);
        let installing = status.as_ref().is_some_and(|s| {
            s.get("state").and_then(|v| v.as_str()) == Some("installing")
                && s.get("pid")
                    .and_then(serde_json::Value::as_u64)
                    .is_some_and(alive)
        });
        if installing {
            State::Installing
        } else if self.complete(entry) {
            State::Installed
        } else {
            State::NotInstalled
        }
    }
}

/// This machine as the catalogue's `platforms` name it.
pub fn platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos-arm64",
        ("macos", "x86_64") => "macos-x86_64",
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-arm64",
        _ => "other",
    }
}

/// A name that is a plain file name (no separator, not hidden).
fn plain(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && !name.starts_with('.')
}

fn alive(pid: u64) -> bool {
    // `kill -0`: the process exists (or exists and is someone else's).
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// How a call's model was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Call,
    Config,
    Default,
}

/// The model a call runs: the call's, else `[segment] model`, else `default` (the pack's own
/// default, from its manifest). None when the pack takes no `model` and nothing names one; a
/// model named for a pack that takes none is refused.
pub fn resolve(
    call: Option<&str>,
    config: Option<&Config>,
    choices: Option<(&[String], Option<&str>)>,
) -> Result<Option<(String, Source)>> {
    let configured = config
        .and_then(|c| c.segment.as_ref())
        .map(|s| s.model.as_str());
    let Some((choices, default)) = choices else {
        if let Some(model) = call {
            return Err(Error::usage(format!(
                "model {model:?}: the segmenter pack's segment takes no model"
            )));
        }
        if configured.is_some() {
            return Err(Error::usage(
                "[segment] model is set, but the segmenter pack's segment takes no model",
            ));
        }
        return Ok(None);
    };
    let (model, source) = match (call, configured, default) {
        (Some(m), _, _) => (m, Source::Call),
        (None, Some(m), _) => (m, Source::Config),
        (None, None, Some(m)) => (m, Source::Default),
        (None, None, None) => return Ok(None),
    };
    if !choices.iter().any(|c| c == model) {
        let whose = match source {
            Source::Config => "[segment] model",
            _ => "model",
        };
        return Err(Error::usage(format!(
            "{whose} {model:?} is not a catalogued segmenter (have {})",
            choices.join(", ")
        )));
    }
    Ok(Some((model.to_string(), source)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn config(model: Option<&str>) -> Config {
        let mut text = String::from("config_version = 1\n");
        if let Some(m) = model {
            text.push_str(&format!("[segment]\nmodel = \"{m}\"\n"));
        }
        Config::parse(&text, "test").unwrap()
    }

    fn ids() -> Vec<String> {
        vec!["tiny".into(), "sam3".into()]
    }

    #[test]
    fn the_call_wins_then_the_config_then_the_default() {
        let choices = ids();
        let c = Some((choices.as_slice(), Some("tiny")));
        let r = |call, cfg: Option<&str>| resolve(call, Some(&config(cfg)), c).unwrap();
        assert_eq!(
            r(Some("sam3"), Some("tiny")),
            Some(("sam3".into(), Source::Call))
        );
        assert_eq!(r(None, Some("sam3")), Some(("sam3".into(), Source::Config)));
        assert_eq!(r(None, None), Some(("tiny".into(), Source::Default)));
    }

    #[test]
    fn an_unknown_model_is_refused_naming_where_it_came_from() {
        let choices = ids();
        let c = Some((choices.as_slice(), Some("tiny")));
        let e = resolve(Some("giraffe"), None, c).unwrap_err().to_string();
        assert!(
            e.contains("model \"giraffe\" is not a catalogued segmenter (have tiny, sam3)"),
            "{e}"
        );
        let e = resolve(None, Some(&config(Some("nope"))), c)
            .unwrap_err()
            .to_string();
        assert!(e.contains("[segment] model \"nope\""), "{e}");
    }

    #[test]
    fn a_pack_without_model_takes_none_and_refuses_one() {
        assert_eq!(resolve(None, Some(&config(None)), None).unwrap(), None);
        assert!(resolve(Some("sam3"), None, None).is_err());
        assert!(resolve(None, Some(&config(Some("sam3"))), None).is_err());
    }

    #[test]
    fn the_config_refuses_a_model_that_is_not_an_id() {
        let e = Config::parse("config_version = 1\n[segment]\nmodel = \"Sam 3\"\n", "t")
            .unwrap_err()
            .to_string();
        assert!(e.contains("is not a model id"), "{e}");
    }

    #[test]
    fn state_is_read_from_disk() {
        let dir = std::env::temp_dir().join(format!("rascal-segmenters-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let install = dir.join("install");
        let models = dir.join("models");
        std::fs::create_dir_all(&install).unwrap();
        std::fs::write(
            install.join(CATALOGUE),
            format!(
                "[[segmenter]]\nid = \"tiny\"\ntitle = \"T\"\nlicence = \"Apache-2.0\"\ndefault = true\nplatforms = [\"{}\"]\n\n[[segmenter]]\nid = \"far\"\ntitle = \"F\"\nlicence = \"MIT\"\nplatforms = [\"nowhere\"]\n",
                platform()
            ),
        )
        .unwrap();
        let s = Segmenters::read(&install, models.clone()).unwrap();
        let tiny = s.entry("tiny").unwrap().clone();
        assert_eq!(s.state(&tiny), State::NotInstalled);
        std::fs::create_dir_all(models.join("tiny/abc")).unwrap();
        std::fs::write(models.join("tiny/current"), "abc\n").unwrap();
        assert_eq!(s.state(&tiny), State::Installed);
        assert_eq!(s.current("tiny").as_deref(), Some("abc"));
        // A `current` naming a directory that is gone is not an installation.
        std::fs::write(models.join("tiny/current"), "gone").unwrap();
        assert_eq!(s.state(&tiny), State::NotInstalled);
        assert_eq!(s.state(s.entry("far").unwrap()), State::Unavailable);
        assert_eq!(s.default_entry().unwrap().id, "tiny");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
