//! The default segmenter (SPEC.md "Choosable segmenters"): the catalogue ranks its models best
//! first, and a call that names no model, with no `[segment] model`, runs the best-ranked model
//! that is installed, complete and runs on this platform; with none installed, the best-ranked
//! one that runs here, which readiness reports missing with its setup. A model named by the call
//! or by `[segment] model` is honoured exactly, installed or not. `describe`, readiness and the
//! estimate's `x-rascal.segmenter` agree, each resolving once from the same catalogue.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

use rascal_core::config::Config;
use rascal_mask::segmenters::{self, Segmenters};
use rascal_testkit::fake_install::{self, CatalogueEntry};
use serde_json::Value;

/// The owner's preference, best first, written to the file in another order so the rank (and
/// not the file's order) decides.
const RANKED: [&str; 5] = ["sam3", "birefnet", "hqsam", "evfsam2", "tiny"];
const FILE_ORDER: [&str; 5] = ["tiny", "birefnet", "sam3", "evfsam2", "hqsam"];

struct Pack {
    dir: PathBuf,
    install: PathBuf,
    models: PathBuf,
}

impl Pack {
    /// A fake install whose catalogue ranks the five models, every one on this platform but
    /// those in `elsewhere`, with nothing installed.
    fn new(name: &str, elsewhere: &[&str]) -> Pack {
        let dir = std::env::temp_dir().join(format!(
            "rascal-default-segmenter-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let install = dir.join("pack");
        let bin = env!("CARGO_BIN_EXE_rascal");
        fake_install::write(&install, &[bin, "fake-pack", "exact"], false).unwrap();
        let entries: Vec<CatalogueEntry<'_>> = FILE_ORDER
            .iter()
            .map(|id| CatalogueEntry {
                id,
                rank: RANKED.iter().position(|r| r == id).unwrap() as u32 + 1,
                platforms: if elsewhere.contains(id) {
                    &["nowhere"]
                } else {
                    &[]
                },
            })
            .collect();
        fake_install::write_catalogue(&install, &entries).unwrap();
        let models = install.join("models");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::create_dir_all(dir.join("home")).unwrap();
        Pack {
            dir,
            install,
            models,
        }
    }

    fn install(&self, id: &str) {
        fake_install::install_model(&self.models, id, true).unwrap();
    }

    fn catalogue(&self) -> Segmenters {
        Segmenters::read(&self.install, self.models.clone()).unwrap()
    }

    /// `rascal.toml` naming the pack, and `[segment] model` when `model`.
    fn config(&self, model: Option<&str>) -> (PathBuf, Config) {
        let mut text = format!(
            "config_version = 1\n[packs.segmenter]\ncommand = [{:?}, \"serve\"]\n",
            self.install
                .join(".venv/bin/rascal-tools")
                .display()
                .to_string()
        );
        if let Some(m) = model {
            text.push_str(&format!("\n[segment]\nmodel = \"{m}\"\n"));
        }
        let path = self.dir.join("rascal.toml");
        std::fs::write(&path, &text).unwrap();
        (path, Config::parse(&text, "test").unwrap())
    }

    /// `remove_prompt` as `rascal describe --json` describes it.
    fn describe(&self, model: Option<&str>) -> Value {
        let (config, _) = self.config(model);
        let out = Command::new(env!("CARGO_BIN_EXE_rascal"))
            .args(["describe", "--json", "--config", config.to_str().unwrap()])
            .env_remove("RASCAL_TOOLS_MODELS")
            .env_remove("RASCAL_LOG")
            .env("HOME", self.dir.join("home"))
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
        doc["actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == "remove_prompt")
            .unwrap()
            .clone()
    }
}

impl Drop for Pack {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn default_of(pack: &Pack, model: Option<&str>) -> String {
    let (_, config) = pack.config(model);
    pack.catalogue().default_model(Some(&config)).unwrap()
}

/// The resolved default as `describe` shows it: the schema's default and first choice, and the
/// state of the one requirement that counts.
fn described(action: &Value) -> (String, String, String) {
    let model = &action["params_schema"]["properties"]["model"];
    assert_eq!(model["enum"][0], model["default"], "{model:#}");
    let default = model["default"].as_str().unwrap().to_string();
    let requirement = action["readiness"]["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == format!("segmenter:{default}"))
        .unwrap();
    (
        default,
        requirement["state"].as_str().unwrap().to_string(),
        action["readiness"]["state"].as_str().unwrap().to_string(),
    )
}

#[test]
fn nothing_installed_gives_the_best_ranked_model_missing() {
    let pack = Pack::new("none", &[]);
    assert_eq!(default_of(&pack, None), "sam3");
    let action = pack.describe(None);
    assert_eq!(
        described(&action),
        ("sam3".into(), "missing".into(), "needs_setup".into())
    );
    let sam3 = action["readiness"]["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "segmenter:sam3")
        .unwrap();
    assert!(
        sam3["setup"]
            .as_str()
            .unwrap()
            .contains("rascal tools get sam3"),
        "{sam3:#}"
    );
}

#[test]
fn only_tiny_installed_gives_tiny() {
    let pack = Pack::new("tiny", &[]);
    pack.install("tiny");
    assert_eq!(default_of(&pack, None), "tiny");
    assert_eq!(
        described(&pack.describe(None)),
        ("tiny".into(), "present".into(), "ready".into())
    );
}

#[test]
fn the_best_ranked_installed_model_wins() {
    let pack = Pack::new("two", &[]);
    pack.install("tiny");
    pack.install("birefnet");
    assert_eq!(default_of(&pack, None), "birefnet");
    assert_eq!(
        described(&pack.describe(None)),
        ("birefnet".into(), "present".into(), "ready".into())
    );
}

#[test]
fn an_incomplete_installation_is_skipped() {
    let pack = Pack::new("incomplete", &[]);
    fake_install::install_model(&pack.models, "sam3", false).unwrap();
    pack.install("tiny");
    assert_eq!(default_of(&pack, None), "tiny");
    assert_eq!(
        described(&pack.describe(None)),
        ("tiny".into(), "present".into(), "ready".into())
    );
}

#[test]
fn a_model_that_does_not_run_here_is_skipped() {
    let pack = Pack::new("elsewhere", &["sam3"]);
    assert_eq!(default_of(&pack, None), "birefnet");
    // Installed, but not for this platform: still skipped.
    pack.install("sam3");
    pack.install("tiny");
    assert_eq!(default_of(&pack, None), "tiny");
    let action = pack.describe(None);
    assert_eq!(
        described(&action),
        ("tiny".into(), "present".into(), "ready".into())
    );
    let choices = &action["params_schema"]["properties"]["model"]["enum"];
    assert!(
        !choices.as_array().unwrap().contains(&"sam3".into()),
        "{choices}"
    );
}

#[test]
fn an_explicit_config_model_not_installed_stays_that_model_and_is_missing() {
    let pack = Pack::new("config", &[]);
    pack.install("tiny");
    pack.install("birefnet");
    assert_eq!(default_of(&pack, Some("hqsam")), "hqsam");
    assert_eq!(
        described(&pack.describe(Some("hqsam"))),
        ("hqsam".into(), "missing".into(), "needs_setup".into())
    );
    let (_, config) = pack.config(Some("hqsam"));
    assert_eq!(
        pack.catalogue().effective(None, &config)["model"],
        "hqsam",
        "the estimate names the configured model too"
    );
}

#[test]
fn an_explicit_run_argument_wins_over_the_config() {
    let pack = Pack::new("call", &[]);
    pack.install("tiny");
    let (_, config) = pack.config(Some("hqsam"));
    let catalogue = pack.catalogue();
    assert_eq!(
        catalogue.effective(Some("evfsam2"), &config)["model"],
        "evfsam2"
    );
    let choices: Vec<String> = RANKED.iter().map(|s| s.to_string()).collect();
    let default = catalogue.default_model(None);
    assert_eq!(default.as_deref(), Some("tiny"));
    let resolve = |call| {
        segmenters::resolve(call, Some(&config), Some((&choices, default.as_deref())))
            .unwrap()
            .unwrap()
            .0
    };
    assert_eq!(resolve(Some("evfsam2")), "evfsam2");
    assert_eq!(resolve(None), "hqsam");
    // With neither, the default this machine resolves.
    let (_, bare) = pack.config(None);
    assert_eq!(catalogue.effective(None, &bare)["model"], "tiny");
}

#[test]
fn a_catalogue_without_one_preference_order_is_not_read() {
    let pack = Pack::new("ranks", &[]);
    let path = pack.install.join("segmenters.toml");
    let good = std::fs::read_to_string(&path).unwrap();
    for bad in [
        good.replace("rank = 2\n", "rank = 1\n"),
        good.replace("rank = 5\n", ""),
        good.replace("rank = 5\n", "rank = 6\n"),
    ] {
        std::fs::write(&path, &bad).unwrap();
        assert!(
            Segmenters::read(&pack.install, pack.models.clone()).is_none(),
            "{bad}"
        );
    }
}
