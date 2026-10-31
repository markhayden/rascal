//! Task 8.3 (ADR 0020): `rascal describe --json` is the contract's description document, one
//! action per shipped recipe, valid against table-manners' schema, and stable (a fixture).
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

#[test]
fn a_configured_filler_is_offered_and_may_spend_with_its_key_named() {
    let dir = std::env::temp_dir().join(format!("rascal-describe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("rascal.toml");
    std::fs::write(
        &config,
        "config_version = 1\n\n[fillers.remote]\nurl = \"https://example.invalid/fill\"\nprofile = \"grey\"\nkey_env = \"EXAMPLE_KEY\"\nspend_usd_per_call = 0.04\n",
    )
    .unwrap();
    let out = rascal(&["describe", "--json", "--config", config.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(valid("urn:table-manners:core:description", &doc).is_empty());
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
    assert_eq!(
        seam["cost"]["needs_env"],
        serde_json::json!(["EXAMPLE_KEY"])
    );
    // The value of a key is never printed, only its name.
    let _ = std::fs::remove_dir_all(&dir);
}
