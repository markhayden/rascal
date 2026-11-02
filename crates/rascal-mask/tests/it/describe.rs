//! Task 8.3 (ADR 0020): `rascal describe --json` is the contract's description document, one
//! action per shipped recipe, valid against table-manners' schema, and stable (a fixture).
//! Task 12.1: at v0.3, with no `estimate` among the actions, `readiness` on every action (a
//! configured key a `credential` requirement, never its value), the dash render's `kind`, and
//! `max_spend_usd` above zero on every action.
//! Task 12.2: readiness without configuration. `describe` exits 0 with no file, a missing one
//! and an invalid one; each action's readiness is its default use (the mask alone unconfigured,
//! healing with the configured fillers otherwise).
//! Task 10.2: at v0.4, every action a `task` with no `group`, `vocabulary` the core's object
//! with the images profile's four lists, no `artifact_types`, no reserved name among the actions,
//! and the defaults a call runs with declared in `params_schema`.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rascal_core::contract::table_manners;

fn rascal(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .env_remove("RASCAL_LOG")
        .output()
        .unwrap()
}

/// The v0.4 shapes every description has, whatever the configuration.
fn assert_v04_shapes(doc: &serde_json::Value) {
    assert_eq!(doc["contract"], "0.4.0");
    assert_eq!(doc["profiles"], serde_json::json!(["images"]));
    // The core's word lists: the images profile's four, and no `group`, since no action has one.
    assert_eq!(
        doc["vocabulary"],
        serde_json::json!({ "disposition": [], "confidence": [], "evidence": [],
                            "score_kind": ["owlv2-detection"] })
    );
    // No operations, so no artifact types (3.3).
    assert!(doc.get("artifact_types").is_none());
    let actions = doc["actions"].as_array().unwrap();
    for action in actions {
        let name = action["name"].as_str().unwrap();
        // The reserved names are MCP tools, never actions (table-manners 3, 5).
        for reserved in ["describe", "estimate", "export", "import"] {
            assert_ne!(name, reserved);
        }
        // Every recipe action is a whole job: a task, in no group, with no ports.
        assert_eq!(action["kind"], "task", "{name}");
        assert!(action.get("group").is_none(), "{name}");
        assert!(action.get("ports").is_none(), "{name}");
        // The defaults a call runs with are declared, so its invocation can apply them (4.8).
        assert_eq!(
            action["params_schema"]["properties"]["strict"]["default"], false,
            "{name}"
        );
        let projection = &action["params_schema"]["properties"]["projection"]["default"];
        if name == "seam" {
            assert_eq!(projection, "equirectangular");
        } else {
            assert!(projection.is_null(), "{name}");
        }
        assert!(action["cost"].get("needs_env").is_none(), "{name}");
        let state = action["readiness"]["state"].as_str().unwrap();
        assert!(["ready", "needs_setup"].contains(&state), "{name}: {state}");
        // Every action takes a filler, so every action may be paid: it declares the cap.
        assert_eq!(
            action["params_schema"]["properties"]["max_spend_usd"]["type"], "number",
            "{name}"
        );
        assert_eq!(
            action["params_schema"]["properties"]["max_spend_usd"]["exclusiveMinimum"], 0,
            "{name}"
        );
        assert!(
            action["params_schema"]["properties"]["max_spend_usd"]
                .get("minimum")
                .is_none(),
            "{name}"
        );
        // No shipped recipe takes a list of images, so no slot is `many` (PLAN.md M12).
        for slot in action["images"].as_array().unwrap() {
            assert!(slot.get("many").is_none(), "{name}: {slot}");
        }
        // The two renders (3.2): the page served for a host to frame, and the one-file page.
        // Each runs the tool's own executable, so `command[0]` is `tool.name`.
        let renders = action["renders"].as_array().unwrap();
        assert_eq!(renders.len(), 2, "{name}");
        let dash = renders.iter().find(|r| r["name"] == "dash").unwrap();
        assert_eq!(dash["kind"], "server", "{name}");
        assert_eq!(dash["ready_timeout_s"], 10, "{name}");
        assert_eq!(
            dash["command"],
            serde_json::json!([
                "rascal",
                "dash",
                "{run.dir}",
                "--serve",
                "--port",
                "0",
                "--frame-ancestor",
                "{frame_ancestor}"
            ]),
            "{name}"
        );
        let page = renders.iter().find(|r| r["name"] == "page").unwrap();
        assert_eq!(page["kind"], "document", "{name}");
        assert!(page.get("ready_timeout_s").is_none(), "{name}");
        assert_eq!(
            page["command"],
            serde_json::json!(["rascal", "dash", "{run.dir}", "--document"]),
            "{name}"
        );
        for render in renders {
            assert_eq!(render["command"][0], doc["tool"]["name"], "{name}");
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Embedded;

impl jsonschema::Retrieve for Embedded {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let text = table_manners::SCHEMAS
            .iter()
            .find(|(id, _)| *id == uri.as_str())
            .map(|(_, text)| *text)
            .ok_or_else(|| format!("no embedded schema {uri}"))?;
        Ok(serde_json::from_str(text)?)
    }
}

fn valid(id: &str, value: &serde_json::Value) -> Vec<String> {
    let (_, text) = table_manners::SCHEMAS
        .iter()
        .find(|(i, _)| *i == id)
        .unwrap();
    let schema: serde_json::Value = serde_json::from_str(text).unwrap();
    let validator = jsonschema::options()
        .with_retriever(Embedded)
        .build(&schema)
        .unwrap();
    validator
        .iter_errors(value)
        .map(|e| e.to_string())
        .collect()
}

#[test]
fn describe_is_the_contracts_document_and_matches_its_fixture() {
    let out = rascal(&["describe", "--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let errors = valid("urn:table-manners:core:description", &doc);
    assert!(errors.is_empty(), "{errors:?}");
    assert_v04_shapes(&doc);
    // Unconfigured, an action's default use is the mask alone, which needs nothing; only
    // `remove_prompt` needs its pack, configured in a file that was not given.
    for action in doc["actions"].as_array().unwrap() {
        if action["name"] == "remove_prompt" {
            assert_eq!(
                action["readiness"],
                serde_json::json!({ "state": "needs_setup", "requirements": [
                    { "kind": "config", "name": "rascal.toml", "state": "missing",
                      "setup": "Start rascal with `--config <path>` naming the segmenter pack." },
                    { "kind": "dependency", "name": "packs.rascal-tools", "state": "missing",
                      "setup": "Run `rascal tools setup --bundle <bundle> --config <path>`." }
                ] })
            );
        } else {
            assert_eq!(
                action["readiness"],
                serde_json::json!({ "state": "ready", "requirements": [] }),
                "{}",
                action["name"]
            );
        }
    }
    let names: Vec<&str> = doc["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    for name in [
        "seam",
        "remove",
        "remove_prompt",
        "repair",
        "repair_area",
        "protect",
        "mask_rect",
    ] {
        assert!(names.contains(&name), "{names:?}");
    }
    // The fixture pins the document; the version is the crate's, so it is masked.
    let mut pinned = doc.clone();
    pinned["tool"]["version"] = serde_json::json!("<version>");
    let fixture = repo_root().join("fixtures/contract/describe.json");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(
            &fixture,
            serde_json::to_string_pretty(&pinned).unwrap() + "\n",
        )
        .unwrap();
    }
    let stored: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture).unwrap()).unwrap();
    assert_eq!(
        pinned, stored,
        "UPDATE_FIXTURES=1 regenerates fixtures/contract/describe.json"
    );
}

/// The configuration the serve tests use since M11: command fillers, two of them priced.
fn command_fillers(dir: &Path) -> PathBuf {
    let bin = env!("CARGO_BIN_EXE_rascal");
    let config = dir.join("m11.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[limits]\nsession_budget_usd = 1.0\n[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\ntimeout_s = 30\nrevision = \"fake\"\n\n[fillers.slow]\ncommand = [{bin:?}, \"fake-filler\", \"slow\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\ntimeout_s = 30\nrevision = \"slow\"\nspend_usd_per_call = 0.04\n"
        ),
    )
    .unwrap();
    config
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-describe-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_m11_configuration_describes_at_v04() {
    let dir = scratch("m11");
    let config = command_fillers(&dir);
    let out = rascal(&["describe", "--json", "--config", config.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let errors = valid("urn:table-manners:core:description", &doc);
    assert!(errors.is_empty(), "{errors:?}");
    assert_v04_shapes(&doc);
    let bin = env!("CARGO_BIN_EXE_rascal");
    for action in doc["actions"].as_array().unwrap() {
        assert_eq!(action["cost"]["may_spend"], true);
        // Command fillers read no key: the file, each filler and its program (one program for
        // both, named once), all there.
        let requirements = action["readiness"]["requirements"].as_array().unwrap();
        assert_eq!(
            requirements[..4],
            [
                serde_json::json!({ "kind": "config", "name": "rascal.toml", "state": "present" }),
                serde_json::json!({ "kind": "model", "name": "fillers.fake", "state": "present" }),
                serde_json::json!({ "kind": "dependency", "name": bin, "state": "present" }),
                serde_json::json!({ "kind": "model", "name": "fillers.slow", "state": "present" }),
            ],
            "{action:#}"
        );
        if action["name"] == "remove_prompt" {
            assert_eq!(action["readiness"]["state"], "needs_setup");
        } else {
            assert_eq!(requirements.len(), 4, "{action:#}");
            assert_eq!(action["readiness"]["state"], "ready");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_configured_filler_is_offered_and_may_spend_with_its_key_a_credential() {
    let dir = scratch("key");
    let config = dir.join("rascal.toml");
    std::fs::write(
        &config,
        "config_version = 1\n\n[fillers.remote]\nurl = \"https://example.invalid/fill\"\nprofile = \"grey\"\nkey_env = \"EXAMPLE_KEY\"\nspend_usd_per_call = 0.04\n",
    )
    .unwrap();
    let describe = |env: &[(&str, &str)]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
        command
            .args(["describe", "--json", "--config", config.to_str().unwrap()])
            .env_remove("RASCAL_LOG")
            .env_remove("EXAMPLE_KEY");
        for (name, value) in env {
            command.env(name, value);
        }
        let out = command.output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let errors = valid("urn:table-manners:core:description", &doc);
        assert!(errors.is_empty(), "{errors:?}");
        assert_v04_shapes(&doc);
        (doc, out)
    };
    let (doc, _) = describe(&[]);
    let seam = doc["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "seam")
        .unwrap();
    assert_eq!(
        seam["params_schema"]["properties"]["filler"]["enum"],
        serde_json::json!(["remote"])
    );
    assert_eq!(seam["cost"]["may_spend"], true);
    assert_eq!(seam["cost"]["basis"], "estimated");
    // The key's variable is a credential requirement, missing while it is unset, beside the
    // file and the filler.
    assert_eq!(
        seam["readiness"],
        serde_json::json!({
            "state": "needs_setup",
            "requirements": [
                { "kind": "config", "name": "rascal.toml", "state": "present" },
                { "kind": "model", "name": "fillers.remote", "state": "present" },
                { "kind": "credential", "name": "EXAMPLE_KEY", "state": "missing",
                  "setup": "Set `EXAMPLE_KEY` in the environment rascal is started with." }
            ]
        })
    );
    // Every action lists the credential and the filler (each may heal with it).
    for action in doc["actions"].as_array().unwrap() {
        let requirements = action["readiness"]["requirements"].as_array().unwrap();
        for (kind, name) in [("credential", "EXAMPLE_KEY"), ("model", "fillers.remote")] {
            assert!(
                requirements
                    .iter()
                    .any(|r| r["kind"] == kind && r["name"] == name),
                "{action:#}"
            );
        }
    }
    // Set, it is present (found, not checked), and its value is never printed.
    let sentinel = "zq-sentinel-key-value-zq";
    let (doc, out) = describe(&[("EXAMPLE_KEY", sentinel)]);
    for action in doc["actions"].as_array().unwrap() {
        if action["name"] != "remove_prompt" {
            assert_eq!(action["readiness"]["state"], "ready", "{action:#}");
        }
        let credential = action["readiness"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == "credential")
            .unwrap()
            .clone();
        assert_eq!(credential["state"], "present", "{action:#}");
    }
    assert!(!String::from_utf8_lossy(&out.stdout).contains(sentinel));
    assert!(!String::from_utf8_lossy(&out.stderr).contains(sentinel));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_configuration_file_that_is_missing_or_invalid_is_described_and_exits_0() {
    let dir = scratch("states");
    let absent = dir.join("absent.toml");
    let bad = dir.join("bad.toml");
    std::fs::write(
        &bad,
        "config_version = 1\n[fillers.f\nurl = \"zq-not-shown-zq\"\n",
    )
    .unwrap();
    for (path, state) in [(&absent, "missing"), (&bad, "invalid")] {
        let out = rascal(&["describe", "--json", "--config", path.to_str().unwrap()]);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let errors = valid("urn:table-manners:core:description", &doc);
        assert!(errors.is_empty(), "{errors:?}");
        assert_v04_shapes(&doc);
        // A file was given, so every action's default use reads it; it stands in the way.
        for action in doc["actions"].as_array().unwrap() {
            let config = &action["readiness"]["requirements"][0];
            assert_eq!(config["kind"], "config", "{action:#}");
            assert_eq!(config["name"], "rascal.toml");
            assert_eq!(config["state"], state, "{action:#}");
            assert_eq!(action["readiness"]["state"], "needs_setup");
        }
        let setup = doc["actions"][0]["readiness"]["requirements"][0]["setup"]
            .as_str()
            .unwrap()
            .to_string();
        if state == "invalid" {
            // The parse error, by place, without the file's text.
            assert!(
                setup.contains("line 2") && setup.contains("not valid TOML"),
                "{setup}"
            );
            assert!(!String::from_utf8_lossy(&out.stdout).contains("zq-not-shown-zq"));
        } else {
            assert!(setup.contains("does not exist"), "{setup}");
        }
        // The plain listing says the same and exits 0 too.
        let out = rascal(&["describe", "--config", path.to_str().unwrap()]);
        assert_eq!(out.status.code(), Some(0));
        assert!(
            String::from_utf8_lossy(&out.stdout).contains(&format!("config rascal.toml {state}")),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
