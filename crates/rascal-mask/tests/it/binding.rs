//! Task 10.3 (CONTRACT 3.4, 4.7): a served action's `mask` and `protect` arguments are Mask
//! references, resolved by identity in `serve`'s workspace (the session's store) before any
//! run. A bound Mask enters the run with its frame chain, so a mask in a derived frame needs no
//! `--workspace` from the run that made it; references that do not bind are the `-32602`
//! `binding` refusal, which writes no run and releases the call's `client_run_id`; a path where
//! a reference goes is plain invalid params; and the estimate binds a Mask by its identity.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rascal_core::contract::table_manners;
use rascal_testkit::mcp_client::{Client, Failure};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-binding-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rascal(args: &[&str]) -> Output {
    let out = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .env_remove("RASCAL_LOG")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn mask_line(out: &Output) -> PathBuf {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("mask: ").map(PathBuf::from))
        .unwrap()
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

fn valid(id: &str, value: &serde_json::Value) {
    let (_, text) = table_manners::SCHEMAS
        .iter()
        .find(|(i, _)| *i == id)
        .unwrap();
    let schema: serde_json::Value = serde_json::from_str(text).unwrap();
    let validator = jsonschema::options()
        .with_retriever(Embedded)
        .build(&schema)
        .unwrap();
    let errors: Vec<String> = validator
        .iter_errors(value)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{id}: {errors:?}\n{value:#}");
}

fn start(dir: &Path) -> Client {
    let mut command = rascal_testkit::mcp_client::serve_command(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "--workspace",
            dir.join("store").to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env("XDG_DATA_HOME", dir.join("data"))
        .env_remove("RASCAL_LOG");
    Client::start(&mut command).unwrap()
}

/// The run directories under the runs root.
fn runs(dir: &Path) -> Vec<PathBuf> {
    let mut runs: Vec<PathBuf> = std::fs::read_dir(dir.join("runs"))
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    runs.sort();
    runs
}

/// A seam mask (in a `cut:` frame) made on the panorama by the command line, its bundle
/// imported into the store with `rascal workspace import-mask` (10.5; the import tool is
/// 10.10), and its reference as the store gives it. Also the Mask the command line's `protect`
/// makes from it, by the stored directory with the making run's workspace (M12's path form).
struct Setup {
    pano: PathBuf,
    reference: serde_json::Value,
    protected_identity: String,
}

fn setup(dir: &Path) -> Setup {
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let made = dir.join("made");
    let seam = rascal(&[
        "run",
        "seam",
        pano.to_str().unwrap(),
        "--out",
        made.to_str().unwrap(),
    ]);
    let seam_mask = mask_line(&seam);
    let identity = rascal_core::mask::Mask::read(&seam_mask)
        .unwrap()
        .record
        .identity;
    assert!(
        rascal_core::mask::Mask::read(&seam_mask)
            .unwrap()
            .record
            .frame
            .starts_with("cut:")
    );
    let bundle = dir.join("seam.mask.tar");
    rascal(&[
        "workspace",
        "export-mask",
        made.to_str().unwrap(),
        &identity,
        "--out",
        bundle.to_str().unwrap(),
    ]);
    rascal(&[
        "workspace",
        "import-mask",
        dir.join("store").to_str().unwrap(),
        bundle.to_str().unwrap(),
    ]);
    let store = rascal_core::workspace::Workspace::open(&dir.join("store")).unwrap();
    let reference =
        serde_json::to_value(rascal_core::contract::artifact_ref(&store, &identity).unwrap())
            .unwrap();
    drop(store);
    let protected = rascal(&[
        "run",
        "protect",
        pano.to_str().unwrap(),
        "--mask",
        seam_mask.to_str().unwrap(),
        "--workspace",
        made.to_str().unwrap(),
        "--out",
        dir.join("cli-protect").to_str().unwrap(),
    ]);
    let protected_identity = rascal_core::mask::Mask::read(&mask_line(&protected))
        .unwrap()
        .record
        .identity;
    // The command line records the stored directory as the same reference the store gives.
    let recorded: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("cli-protect/run.json")).unwrap())
            .unwrap();
    assert_eq!(recorded["invocation"]["arguments"]["mask"], reference);
    Setup {
        pano,
        reference,
        protected_identity,
    }
}

/// The Mask a served run made, by its `run.json` body.
fn made_identity(result: &serde_json::Value) -> String {
    let dir = result["body"]["mask_dir"].as_str().unwrap();
    rascal_core::mask::Mask::read(Path::new(dir))
        .unwrap()
        .record
        .identity
}

/// The `binding` refusal's entries, the refusal valid against the contract's schema.
fn binding_refusal(
    outcome: Result<rascal_testkit::mcp_client::CallResult, Failure>,
) -> serde_json::Value {
    match outcome {
        Err(Failure::Rpc { code, data, .. }) => {
            assert_eq!(code, -32602);
            let data = data.unwrap();
            valid("urn:table-manners:core:refusal", &data);
            assert_eq!(data["reason"], "binding", "{data:#}");
            assert_eq!(data["run"], serde_json::Value::Null);
            data["bindings"].clone()
        }
        other => panic!("not refused: {other:?}"),
    }
}

#[test]
fn a_served_protect_binds_its_mask_by_reference() {
    let dir = scratch("protect");
    let s = setup(&dir);
    let mut client = start(&dir);
    let image = serde_json::json!({ "path": s.pano.to_str().unwrap() });

    let result = client
        .call(
            "protect",
            serde_json::json!({ "image": image, "mask": s.reference }),
        )
        .unwrap()
        .structured
        .unwrap();
    valid("urn:table-manners:core:result", &result);
    assert_eq!(result["status"], "completed", "{result:#}");
    // The Mask came with its frame chain: no --workspace from the run that made it, and the
    // same Mask as the path form with it.
    assert_eq!(made_identity(&result), s.protected_identity);
    // The invocation records the reference, which names no path.
    assert_eq!(result["invocation"]["arguments"]["mask"], s.reference);
    assert!(
        !result["invocation"]
            .to_string()
            .contains(dir.to_str().unwrap())
    );
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn references_that_do_not_bind_are_refused_before_any_run() {
    let dir = scratch("refused");
    let s = setup(&dir);
    let mut client = start(&dir);
    let image = serde_json::json!({ "path": s.pano.to_str().unwrap() });
    let call = |client: &mut Client, mask: serde_json::Value, id: &str| {
        client.call_with_meta(
            "protect",
            serde_json::json!({ "image": image, "mask": mask }),
            serde_json::json!({ "table-manners/client_run_id": id }),
        )
    };
    let before = runs(&dir);

    // Unresolved: an identity the store does not hold, with the import remedy; the correlation
    // id is released, so the corrected retry runs under it.
    let mut missing = s.reference.clone();
    missing["identity"] = serde_json::json!(format!("sha256:{}", "b".repeat(64)));
    missing["resource"] = serde_json::json!(format!("rascal://artifact/sha256:{}", "b".repeat(64)));
    let bindings = binding_refusal(call(&mut client, missing.clone(), "retry-1"));
    assert_eq!(
        bindings,
        serde_json::json!([{
            "port": "mask",
            "problem": "unresolved",
            "expected": "rascal:Mask v1",
            "got": missing["identity"],
            "remedy": "Import its bundle with the import tool (rascal import), then call again.",
        }])
    );
    assert_eq!(runs(&dir), before, "a refusal writes no run");
    let retried = call(&mut client, s.reference.clone(), "retry-1")
        .unwrap()
        .structured
        .unwrap();
    assert_eq!(retried["status"], "completed", "{retried:#}");
    assert_eq!(retried["run"]["client_run_id"], "retry-1");
    let before = runs(&dir);

    // Wrong type: a location's reference, and another tool's type; wrong version: 2.
    let mut location = s.reference.clone();
    location["type"] = serde_json::json!("location");
    let mut foreign = s.reference.clone();
    foreign["type"] = serde_json::json!("squint:Thing");
    foreign["tool"] = serde_json::json!("squint");
    let mut newer = s.reference.clone();
    newer["schema_version"] = serde_json::json!(2);
    for (i, (mask, problem, got)) in [
        (location, "wrong_type", "location v1"),
        (foreign, "wrong_type", "squint:Thing v1"),
        (newer, "wrong_version", "rascal:Mask v2"),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("bad-{i}");
        let bindings = binding_refusal(call(&mut client, mask, &id));
        assert_eq!(bindings.as_array().unwrap().len(), 1);
        assert_eq!(bindings[0]["port"], "mask");
        assert_eq!(bindings[0]["problem"], problem);
        assert_eq!(bindings[0]["got"], got);
        assert_eq!(bindings[0]["expected"], "rascal:Mask v1");
    }
    // `protect` of `remove`: the port is the argument's name.
    let remove = client.call(
        "remove",
        serde_json::json!({ "image": image, "protect": missing,
                            "location": { "kind": "bbox" } }),
    );
    let bindings = binding_refusal(remove);
    assert_eq!(bindings[0]["port"], "protect");
    assert_eq!(runs(&dir), before, "no refusal wrote a run");

    // A path where a reference goes is plain invalid params: no binding refusal, no run.
    let made_mask = dir.join("made/artifacts");
    match call(
        &mut client,
        serde_json::json!(made_mask.to_str().unwrap()),
        "path-1",
    ) {
        Err(Failure::Rpc {
            code,
            data,
            message,
        }) => {
            assert_eq!(code, -32602);
            assert_eq!(
                data,
                Some(serde_json::json!({ "class": "usage" })),
                "{message}"
            );
            assert!(
                message.contains("mask: not an artifact reference"),
                "{message}"
            );
        }
        other => panic!("a path for a reference: {other:?}"),
    }
    assert_eq!(runs(&dir), before);
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The estimate binds a Mask by its identity, as it bound a stored mask's identity before: the
/// same Mask under another rendering of its reference is the same binding, another Mask is not,
/// and the estimate never reads the store (an identity it does not hold is still priced).
#[test]
fn an_estimate_binds_a_mask_by_its_identity() {
    let dir = scratch("estimate");
    let s = setup(&dir);
    let mut client = start(&dir);
    let image = serde_json::json!({ "path": s.pano.to_str().unwrap() });
    let mut estimate = |mask: &serde_json::Value| {
        let out = client
            .call(
                "estimate",
                serde_json::json!({ "action": "protect",
                                    "arguments": { "image": image, "mask": mask } }),
            )
            .unwrap()
            .structured
            .unwrap();
        valid("urn:table-manners:core:estimate", &out);
        out["binding"].as_str().unwrap().to_string()
    };
    let bound = estimate(&s.reference);
    let mut without_on = s.reference.clone();
    without_on.as_object_mut().unwrap().remove("on");
    assert_eq!(estimate(&without_on), bound);
    let mut other = s.reference.clone();
    other["identity"] = serde_json::json!(format!("sha256:{}", "c".repeat(64)));
    assert_ne!(estimate(&other), bound);
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
