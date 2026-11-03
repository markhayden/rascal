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
//! Task 10.4: an operations tool's description: `profiles` with `operations`, the 24 built-in ops
//! as actions of kind `operation` with a `group` (in `vocabulary.group`) and typed `ports`, no
//! image or location slot, no spend, ready, no renders; `artifact_types` without `image`; the
//! table-manners fixture checks' rules (unique ports, declared types, the tool's prefix).
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

/// The seven recipe actions, each a `task`.
const TASKS: [&str; 7] = [
    "seam",
    "remove",
    "remove_prompt",
    "repair",
    "repair_area",
    "protect",
    "mask_rect",
];

/// The description's actions of one kind.
fn of_kind<'a>(doc: &'a serde_json::Value, kind: &str) -> Vec<&'a serde_json::Value> {
    doc["actions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["kind"] == kind)
        .collect()
}

/// The v0.4 shapes every description has, whatever the configuration.
fn assert_v04_shapes(doc: &serde_json::Value) {
    assert_eq!(doc["contract"], "0.4.0");
    assert_eq!(doc["profiles"], serde_json::json!(["images", "operations"]));
    // The core's word lists: the operations' groups, and the images profile's four.
    assert_eq!(
        doc["vocabulary"],
        serde_json::json!({ "group": ["find", "refine", "prep", "out", "inspect"],
                            "disposition": [], "confidence": [], "evidence": [],
                            "score_kind": ["owlv2-detection"] })
    );
    // Every type a port names but `image` (3.3): JSON, a bundle and the workspace-bound three.
    let types: Vec<(&str, &str, bool)> = doc["artifact_types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            assert_eq!(t["schema_versions"], serde_json::json!([1]), "{t}");
            assert!(!t["purpose"].as_str().unwrap().is_empty(), "{t}");
            (
                t["type"].as_str().unwrap(),
                t["portable"].as_str().unwrap(),
                t["preview"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        types,
        [
            ("location", "json", true),
            ("rascal:Mask", "bundle", true),
            ("rascal:BandProfile", "none", false),
            ("rascal:Series", "none", false),
            ("rascal:Plan", "none", false),
        ]
    );
    let actions = doc["actions"].as_array().unwrap();
    let names: Vec<&str> = actions
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    for (i, name) in names.iter().enumerate() {
        // The reserved names are MCP tools, never actions (table-manners 3, 5); no name twice.
        assert!(
            !["describe", "estimate", "export", "import"].contains(name),
            "{name}"
        );
        assert!(!names[..i].contains(name), "{name} is listed twice");
    }
    assert_eq!(actions.len(), TASKS.len() + 24);
    let mut tasks: Vec<&str> = of_kind(doc, "task")
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    tasks.sort_unstable();
    let mut expected = TASKS;
    expected.sort_unstable();
    assert_eq!(tasks, expected);
    assert_operations(doc);
    for action in of_kind(doc, "task") {
        let name = action["name"].as_str().unwrap();
        // Every recipe action is a whole job: a task, in no group, with no ports.
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

/// The 24 built-in ops as operations (task 10.4; table-manners 3, 4.7, 11.2), each against its
/// manifest.
fn assert_operations(doc: &serde_json::Value) {
    let registry = rascal_ops::registry().unwrap();
    let operations = of_kind(doc, "operation");
    let mut names: Vec<&str> = operations
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(names, registry.names(), "every built-in op, and no pack's");
    let types: Vec<&str> = doc["artifact_types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["type"].as_str().unwrap())
        .collect();
    let groups = doc["vocabulary"]["group"].as_array().unwrap();
    for action in operations {
        let name = action["name"].as_str().unwrap();
        let op = registry.get(name).unwrap();
        let manifest = op.manifest();
        assert_eq!(action["group"], op.group().name(), "{name}");
        assert!(groups.contains(&action["group"]), "{name}");
        // The name in words; the manifest's purpose with when to use it and when not.
        let title = action["title"].as_str().unwrap();
        assert_eq!(title.to_lowercase(), name, "{name}");
        let purpose = action["purpose"].as_str().unwrap();
        for text in [
            &manifest.purpose,
            &manifest.when_to_use,
            &manifest.when_not_to_use,
        ] {
            assert!(purpose.contains(text.as_str()), "{name}: {purpose}");
        }
        // Ports only: no image or location slot (4.7, 11.2), no spend, no configuration read,
        // no renders (an operation's picture is its preview).
        assert_eq!(action["images"], serde_json::json!([]), "{name}");
        assert_eq!(action["locations"], serde_json::json!([]), "{name}");
        assert_eq!(action["renders"], serde_json::json!([]), "{name}");
        assert_eq!(
            action["cost"],
            serde_json::json!({ "may_spend": false }),
            "{name}"
        );
        assert_eq!(
            action["readiness"],
            serde_json::json!({ "state": "ready", "requirements": [] }),
            "{name}"
        );
        assert_eq!(
            action["duration"]["typical_s"], manifest.cost.seconds,
            "{name}"
        );
        assert!(action["duration"]["max_s"].as_f64().unwrap() >= manifest.cost.seconds);
        let properties = action["params_schema"]["properties"].as_object().unwrap();
        assert!(!properties.contains_key("max_spend_usd"), "{name}");
        // The ports are the manifest's, typed by the contract's names; each name is unique on
        // its side, lowercase with underscores, and never a parameter's.
        for (side, declared) in [("in", &manifest.inputs), ("out", &manifest.outputs)] {
            let ports = action["ports"][side].as_array().unwrap();
            assert_eq!(ports.len(), declared.len(), "{name} {side}");
            for (i, (port, d)) in ports.iter().zip(declared.iter()).enumerate() {
                let port_name = port["name"].as_str().unwrap();
                assert_eq!(port_name, d.name, "{name}");
                assert!(
                    port_name
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                        && port_name.as_bytes()[0].is_ascii_lowercase(),
                    "{name}: {port_name}"
                );
                assert!(
                    !ports[..i].iter().any(|p| p["name"] == port["name"]),
                    "{name}: two {side} ports named {port_name}"
                );
                assert!(
                    !properties.contains_key(port_name),
                    "{name}: port {port_name} clashes with a parameter"
                );
                let t = port["type"].as_str().unwrap();
                assert_eq!(t, d.artifact_type.contract_type(), "{name}");
                assert!(t == "image" || types.contains(&t), "{name}: {t}");
                if let Some((prefix, _)) = t.split_once(':') {
                    assert_eq!(prefix, doc["tool"]["name"], "{name}: {t}");
                }
                assert_eq!(port["required"], d.required, "{name}");
                assert_eq!(port["many"].as_bool().unwrap_or(false), d.many, "{name}");
                assert_eq!(port["purpose"], d.purpose.as_str(), "{name}");
            }
        }
        // What it returns: the roles of the images its out-ports make, and a preview of every
        // artifact whose type has one; `found` when it makes a location.
        let out_types: Vec<&str> = manifest
            .outputs
            .iter()
            .map(|p| p.artifact_type.contract_type())
            .collect();
        let mut roles: Vec<String> = manifest
            .outputs
            .iter()
            .filter(|p| p.artifact_type.contract_type() == "image")
            .map(|p| p.name.clone())
            .collect();
        if manifest.outputs.iter().any(|p| p.artifact_type.preview()) {
            roles.push("preview".to_string());
        }
        assert_eq!(
            action["outputs"]["images"],
            serde_json::json!(roles),
            "{name}"
        );
        assert_eq!(
            action["outputs"]["locations"],
            if out_types.contains(&"location") {
                serde_json::json!(["found"])
            } else {
                serde_json::json!([])
            },
            "{name}"
        );
    }
    // A finder's list: `select` takes the instances' planes as one many port.
    let select = doc["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "select")
        .unwrap();
    assert_eq!(select["ports"]["in"][0]["name"], "plane");
    assert_eq!(select["ports"]["in"][0]["many"], true);
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
fn the_rect_action_says_its_mask_is_hard_edged() {
    // The rect recipe is a single rasterize step: its purpose claims no feather and names the
    // follow-up that softens it.
    let out = rascal(&["describe", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let rect = doc["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "mask_rect")
        .unwrap();
    assert_eq!(
        rect["purpose"],
        "A hard-edged rectangle as a mask; to soften its edge, feather it afterwards (the feather operation)."
    );
    let recipe =
        std::fs::read_to_string(repo_root().join("crates/rascal-mask/recipes/rect.toml")).unwrap();
    assert!(!recipe.contains("op = \"feather\""), "{recipe}");
}

#[test]
fn the_seam_action_says_it_masks_the_wrap_edge_only() {
    // The seam recipe takes only an image and masks its wrap edge: its purpose claims no other
    // line.
    let out = rascal(&["describe", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let seam = doc["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "seam")
        .unwrap();
    assert_eq!(
        seam["purpose"],
        "Masks the wrap edge of a panorama as a band, wide on smooth surfaces and thin on detail, with a tone ramp across it; with a filler, heals it."
    );
    assert_eq!(seam["locations"], serde_json::json!([]), "{seam}");
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
    for action in of_kind(&doc, "task") {
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
    // Every task lists the credential and the filler (each may heal with it); an operation
    // reads no configuration.
    for action in of_kind(&doc, "task") {
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
    for action in of_kind(&doc, "task") {
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
        // A file was given, so every task's default use reads it; it stands in the way. An
        // operation reads no configuration and stays ready.
        for action in of_kind(&doc, "operation") {
            assert_eq!(
                action["readiness"],
                serde_json::json!({ "state": "ready", "requirements": [] })
            );
        }
        for action in of_kind(&doc, "task") {
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

/// Task 10.4 (table-manners 4.7): each operation's `params_schema` accepts the manifest's
/// documented defaults (with the default constant set) and rejects an unknown parameter.
#[test]
fn each_operations_params_schema_takes_its_defaults_and_refuses_an_unknown_parameter() {
    let out = rascal(&["describe", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let registry = rascal_ops::registry().unwrap();
    for action in of_kind(&doc, "operation") {
        let name = action["name"].as_str().unwrap();
        let manifest = registry.get(name).unwrap().manifest();
        let schema = &action["params_schema"];
        let validator = jsonschema::validator_for(schema).unwrap();
        let mut defaults = serde_json::Map::new();
        for param in &manifest.params {
            if let Some(default) = &param.default {
                defaults.insert(param.name.clone(), default.clone());
            }
        }
        if let Some(first) = manifest.constants.first() {
            assert_eq!(schema["properties"]["constants"]["default"], first.as_str());
            defaults.insert("constants".to_string(), serde_json::json!(first));
        }
        // Every documented default is a value its own property takes.
        for (param, value) in &defaults {
            let property = jsonschema::validator_for(&schema["properties"][param]).unwrap();
            assert!(property.is_valid(value), "{name}.{param}: {value}");
        }
        // With what a call must give besides (a required parameter has no default), the whole
        // set validates, and an unknown parameter beside it never does.
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        if required.iter().all(|r| defaults.contains_key(*r)) {
            let call = serde_json::Value::Object(defaults.clone());
            assert!(validator.is_valid(&call), "{name}: {call}");
        }
        let mut unknown = defaults.clone();
        unknown.insert("zq_unknown".to_string(), serde_json::json!(1));
        assert!(
            !validator.is_valid(&serde_json::Value::Object(unknown)),
            "{name}"
        );
    }
}
