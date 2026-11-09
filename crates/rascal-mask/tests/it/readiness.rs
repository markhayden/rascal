//! Task 12.2 (table-manners 3.1): readiness by local inspection. `remove_prompt` follows a
//! segmenter install (its launcher, its model files where the pack looks, the models variable
//! as an `env` requirement); the model directory resolves as the pack's does (the table both
//! suites check); `rascal heal` refuses a call that is not ready before its run; and describing
//! starts no pack and sends no request.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rascal_testkit::fake_filler::{Behaviour, spawn};
use rascal_testkit::fake_install::{self, MODEL_FILES};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-readiness-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `rascal` with `HOME` at `home` (so the pack's default cache is empty) and the models variable
/// unset unless `env` sets it.
fn rascal(home: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command
        .args(args)
        .env_remove("RASCAL_LOG")
        .env_remove("RASCAL_TOOLS_MODELS")
        .env("HOME", home);
    for (name, value) in env {
        command.env(name, value);
    }
    command.output().unwrap()
}

/// `remove_prompt`'s readiness from `rascal describe --json --config <config>`.
fn remove_prompt(home: &Path, config: &Path, env: &[(&str, &str)]) -> serde_json::Value {
    let out = rascal(
        home,
        &["describe", "--json", "--config", config.to_str().unwrap()],
        env,
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    doc["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "remove_prompt")
        .unwrap()["readiness"]
        .clone()
}

fn pack_config(dir: &Path, launcher: &Path, env: &[&str]) -> PathBuf {
    let path = dir.join("rascal.toml");
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[packs.segmenter]\ncommand = [{:?}, \"serve\"]\nenv = {env:?}\n",
            launcher.display().to_string()
        ),
    )
    .unwrap();
    path
}

fn files(readiness: &serde_json::Value) -> Vec<(String, String)> {
    readiness["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "file")
        .map(|r| {
            (
                r["name"].as_str().unwrap().to_string(),
                r["state"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn requirement<'a>(readiness: &'a serde_json::Value, kind: &str) -> Option<&'a serde_json::Value> {
    readiness["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == kind)
}

#[test]
fn remove_prompt_follows_an_install() {
    let dir = scratch("install");
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let bin = env!("CARGO_BIN_EXE_rascal");
    let install_dir = dir.join("pack");
    let launcher = install_dir.join(".venv/bin/rascal-tools");
    let config = pack_config(&dir, &launcher, &[]);

    // No launcher: the pack is missing, with the setup to install it.
    let readiness = remove_prompt(&home, &config, &[]);
    assert_eq!(readiness["state"], "needs_setup");
    let pack = requirement(&readiness, "dependency").unwrap();
    assert_eq!(pack["name"], "packs.segmenter");
    assert_eq!(pack["state"], "missing");
    assert!(
        pack["setup"]
            .as_str()
            .unwrap()
            .contains("rascal tools setup --bundle"),
        "{pack}"
    );

    // The launcher without its model files: each file missing, naming where it was looked for.
    fake_install::write(&install_dir, &[bin, "fake-pack", "exact"], false).unwrap();
    let readiness = remove_prompt(&home, &config, &[]);
    assert_eq!(readiness["state"], "needs_setup");
    assert_eq!(
        requirement(&readiness, "dependency").unwrap()["state"],
        "present"
    );
    let missing = files(&readiness);
    assert_eq!(missing.len(), MODEL_FILES.len());
    assert!(missing.iter().all(|(_, s)| s == "missing"), "{missing:?}");
    let setup = requirement(&readiness, "file").unwrap()["setup"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        setup.contains(&home.join(".cache/rascal-tools").display().to_string())
            && setup.contains("rascal-tools get tiny"),
        "{setup}"
    );

    // Every file in the install's models/: ready.
    fake_install::write_models(&install_dir.join("models")).unwrap();
    let readiness = remove_prompt(&home, &config, &[]);
    assert_eq!(readiness["state"], "ready", "{readiness:#}");
    let present = files(&readiness);
    let names: Vec<&str> = present.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, MODEL_FILES);
    assert!(present.iter().all(|(_, s)| s == "present"), "{present:?}");
    // An empty file is not a model file.
    let third = fake_install::model_file(&install_dir.join("models"), MODEL_FILES[2]);
    std::fs::write(&third, b"").unwrap();
    let readiness = remove_prompt(&home, &config, &[]);
    assert_eq!(readiness["state"], "needs_setup");
    std::fs::write(&third, b"fake weights").unwrap();
    // The files without `current` naming them are no installation (a flat directory of model
    // files, as before M14, is not read either).
    std::fs::remove_file(install_dir.join("models/tiny/current")).unwrap();
    let readiness = remove_prompt(&home, &config, &[]);
    assert_eq!(readiness["state"], "needs_setup");
    fake_install::write_models(&install_dir.join("models")).unwrap();

    // Another model in the catalogue and in artifacts.toml, not installed: only the default's
    // files are required, so remove_prompt stays ready (PLAN.md 14.2).
    let mut artifacts = std::fs::read_to_string(install_dir.join("artifacts.toml")).unwrap();
    artifacts.push_str("\n[[artifact]]\nname = \"other.onnx\"\nurl = \"https://example.invalid/other.onnx\"\nsha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\nlicence = \"MIT\"\n");
    std::fs::write(install_dir.join("artifacts.toml"), artifacts).unwrap();
    let mut catalogue = std::fs::read_to_string(install_dir.join("segmenters.toml")).unwrap();
    catalogue.push_str("\n[[segmenter]]\nid = \"other\"\nfiles = [\"other.onnx\"]\n");
    std::fs::write(install_dir.join("segmenters.toml"), catalogue).unwrap();
    let readiness = remove_prompt(&home, &config, &[]);
    assert_eq!(readiness["state"], "ready", "{readiness:#}");
    assert_eq!(files(&readiness).len(), MODEL_FILES.len());
    // No catalogue: the install is broken, and says so.
    std::fs::remove_file(install_dir.join("segmenters.toml")).unwrap();
    let readiness = remove_prompt(&home, &config, &[]);
    assert_eq!(readiness["state"], "needs_setup");
    assert_eq!(
        files(&readiness),
        vec![("segmenters.toml".to_string(), "missing".to_string())]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_models_variable_is_an_env_requirement_when_the_pack_receives_it() {
    let dir = scratch("models-env");
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let bin = env!("CARGO_BIN_EXE_rascal");
    let install =
        fake_install::write(&dir.join("pack"), &[bin, "fake-pack", "exact"], false).unwrap();
    let config = pack_config(
        &dir,
        &install.launcher,
        &["RASCAL_TOOLS_MODELS", "PACK_SECRET_KEY"],
    );
    let elsewhere = dir.join("elsewhere");
    fake_install::write_models(&elsewhere).unwrap();
    let elsewhere_str = elsewhere.to_str().unwrap();

    // Set: present, and the files are looked for there.
    let readiness = remove_prompt(
        &home,
        &config,
        &[
            ("RASCAL_TOOLS_MODELS", elsewhere_str),
            ("PACK_SECRET_KEY", "zq-hidden-zq"),
        ],
    );
    assert_eq!(readiness["state"], "ready", "{readiness:#}");
    let env: Vec<&serde_json::Value> = readiness["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "env")
        .collect();
    // A secret-shaped name is never an `env` requirement (the pack never receives it).
    assert_eq!(
        env,
        [&serde_json::json!({ "kind": "env", "name": "RASCAL_TOOLS_MODELS", "state": "present" })]
    );
    assert!(!readiness.to_string().contains("zq-hidden-zq"));

    // Unset, and the fallbacks hold no model files: missing, beside the missing files.
    let readiness = remove_prompt(&home, &config, &[]);
    assert_eq!(readiness["state"], "needs_setup");
    let env = requirement(&readiness, "env").unwrap();
    assert_eq!(env["name"], "RASCAL_TOOLS_MODELS");
    assert_eq!(env["state"], "missing");
    assert!(files(&readiness).iter().all(|(_, s)| s == "missing"));

    // Unset, and the install's models/ holds them: nothing stands in the way, so it is omitted.
    fake_install::write_models(&install.dir.join("models")).unwrap();
    let readiness = remove_prompt(&home, &config, &[]);
    assert_eq!(readiness["state"], "ready", "{readiness:#}");
    assert!(requirement(&readiness, "env").is_none(), "{readiness:#}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_models_directory_resolves_as_the_packs_does() {
    // The same table `tools/python/tests/test_models_dir.py` checks against the pack.
    let table: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("fixtures/readiness/models-dir.json")).unwrap(),
    )
    .unwrap();
    let cases = table["cases"].as_array().unwrap();
    assert!(cases.len() >= 6);
    for (i, case) in cases.iter().enumerate() {
        let root = scratch(&format!("table-{i}"));
        let install = root.join("install");
        std::fs::create_dir_all(install.join(".venv")).unwrap();
        if case["beside"] == true {
            std::fs::create_dir_all(install.join("models")).unwrap();
        }
        let home = root.join("home");
        let variable = root.join("from-env");
        let env: Option<std::ffi::OsString> = match case["env"].as_str() {
            None => None,
            Some("") => Some("".into()),
            Some(_) => Some(variable.clone().into()),
        };
        let expected = match case["expect"].as_str().unwrap() {
            "env" => variable.clone(),
            "beside" => install.join("models"),
            "cache" => home.join(".cache/rascal-tools"),
            other => panic!("unknown expectation {other}"),
        };
        assert_eq!(
            rascal_mask::readiness::models_dir(&install, env.as_deref(), Some(&home)),
            expected,
            "{case}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[test]
fn heal_refuses_a_call_that_is_not_ready_before_its_run() {
    let dir = scratch("heal-refused");
    let (url, seen) = spawn(Behaviour::Exact, Some("k".to_string()));
    let config = dir.join("rascal.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[fillers.remote]\nurl = \"{url}\"\nprofile = \"grey\"\nkey_env = \"READINESS_UNSET_KEY\"\n"
        ),
    )
    .unwrap();
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let out_dir = dir.join("run");
    let out = rascal(
        &dir,
        &[
            "heal",
            "seam",
            pano.to_str().unwrap(),
            "--filler",
            "remote",
            "--config",
            config.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
        ],
        &[],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(64), "{stderr}");
    assert!(
        stderr.contains("not ready: credential READINESS_UNSET_KEY missing"),
        "{stderr}"
    );
    assert!(!out_dir.exists(), "no run directory");
    assert!(seen.lock().unwrap().is_empty(), "no request");
    // A filler the configuration does not have is not ready either.
    let out = rascal(
        &dir,
        &[
            "heal",
            "seam",
            pano.to_str().unwrap(),
            "--filler",
            "nowhere",
            "--config",
            config.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
        ],
        &[],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(64), "{stderr}");
    assert!(stderr.contains("model fillers.nowhere missing"), "{stderr}");
    assert!(!out_dir.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn describing_starts_no_pack_and_sends_no_request() {
    let dir = scratch("no-side-effects");
    let bin = env!("CARGO_BIN_EXE_rascal");
    // The fake pack's `mutate` writes its marker when it starts.
    let marker = dir.join("started");
    let install = fake_install::write(
        &dir.join("pack"),
        &[bin, "fake-pack", "mutate", marker.to_str().unwrap()],
        true,
    )
    .unwrap();
    let (url, seen) = spawn(Behaviour::Exact, Some("k".to_string()));
    let config = dir.join("rascal.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[packs.segmenter]\ncommand = {}\n[fillers.remote]\nurl = \"{url}\"\nprofile = \"grey\"\nkey_env = \"READINESS_KEY\"\nspend_usd_per_call = 0.04\n",
            install.command_toml()
        ),
    )
    .unwrap();
    for args in [
        vec!["describe", "--json", "--config", config.to_str().unwrap()],
        vec!["describe", "--config", config.to_str().unwrap()],
    ] {
        let out = rascal(&dir, &args, &[("READINESS_KEY", "k")]);
        assert_eq!(out.status.code(), Some(0));
    }
    let readiness = remove_prompt(&dir, &config, &[("READINESS_KEY", "k")]);
    assert_eq!(readiness["state"], "ready", "{readiness:#}");
    assert!(!marker.exists(), "describing started the pack");
    assert!(seen.lock().unwrap().is_empty(), "describing sent a request");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Task 13.10 (ADR 0023, table-manners 3.1): with a filler and a `[checker]`, `remove_prompt`
/// takes a `check` setting, declares `answers: verdict`, and lists the checker's program and key
/// as optional requirements for checking the result. A missing key leaves it `ready`: the remove
/// runs and fills, unchecked. Without a checker, none of this appears.
#[test]
fn a_checkers_missing_key_is_optional_and_leaves_remove_prompt_ready() {
    let dir = scratch("checker");
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let bin = env!("CARGO_BIN_EXE_rascal");
    let install_dir = dir.join("pack");
    fake_install::write(&install_dir, &[bin, "fake-pack", "exact"], false).unwrap();
    fake_install::write_models(&install_dir.join("models")).unwrap();
    let launcher = install_dir.join(".venv/bin/rascal-tools");
    let pack = std::fs::read_to_string(pack_config(&dir, &launcher, &[])).unwrap();
    let filler = format!(
        "[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\n"
    );
    let checker = format!(
        "[checker]\ncommand = [{bin:?}, \"--version\"]\narguments = {{ source = {{ path = \"{{source}}\" }}, candidate = {{ path = \"{{candidate}}\" }} }}\nkey_env = \"CHECKER_TEST_KEY\"\nspend_usd_per_call = 0.01\n"
    );
    let describe = |config: &str| -> serde_json::Value {
        let path = dir.join("rascal.toml");
        std::fs::write(&path, config).unwrap();
        let out = rascal(
            &home,
            &["describe", "--json", "--config", path.to_str().unwrap()],
            &[],
        );
        let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        doc["actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == "remove_prompt")
            .unwrap()
            .clone()
    };

    let with = describe(&format!("{pack}{filler}{checker}"));
    assert_eq!(with["readiness"]["state"], "ready", "{with:#}");
    assert_eq!(with["answers"], "verdict");
    assert_eq!(
        with["params_schema"]["properties"]["check"]["type"],
        "boolean"
    );
    assert_eq!(
        with["params_schema"]["properties"]["check"]["default"],
        true
    );
    let requirements = with["readiness"]["requirements"].as_array().unwrap();
    let key = requirements
        .iter()
        .find(|r| r["name"] == "CHECKER_TEST_KEY")
        .unwrap();
    assert_eq!(key["kind"], "credential");
    assert_eq!(key["state"], "missing");
    assert_eq!(key["optional"], true);
    assert_eq!(key["for"], "checking the result");
    let program = requirements
        .iter()
        .find(|r| r["name"] == "checker")
        .unwrap();
    assert_eq!(
        (program["state"].as_str(), program["optional"].as_bool()),
        (Some("present"), Some(true))
    );

    // No filler configured: nothing to check, so no checker requirement and no `check`.
    let no_filler = describe(&format!("{pack}{checker}"));
    assert!(
        no_filler["readiness"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["name"] != "CHECKER_TEST_KEY")
    );

    // No checker: no `check`, no `answers`, no optional requirement.
    let without = describe(&format!("{pack}{filler}"));
    assert!(
        without["params_schema"]["properties"]
            .get("check")
            .is_none()
    );
    assert!(without.get("answers").is_none());
    assert!(
        without["readiness"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r.get("optional").is_none())
    );
    // Other actions never take `check`.
    let _ = std::fs::remove_dir_all(&dir);
}

/// Task 13.10: `check` is an argument of `remove_prompt` only with a checker configured; the
/// estimate records whether the result will be checked (`effective["x-rascal"].check`) and
/// prices the tries: with a 0.04 filler and a 0.01 check, three tries are 0.15, and a cap of
/// 0.12 leaves two (0.10). With the check turned off, one try is the filler's 0.04.
#[test]
fn the_estimate_prices_the_tries_and_records_the_check() {
    let dir = scratch("checker-estimate");
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let bin = env!("CARGO_BIN_EXE_rascal");
    let install_dir = dir.join("pack");
    fake_install::write(&install_dir, &[bin, "fake-pack", "exact"], false).unwrap();
    fake_install::write_models(&install_dir.join("models")).unwrap();
    let launcher = install_dir.join(".venv/bin/rascal-tools");
    let pack = std::fs::read_to_string(pack_config(&dir, &launcher, &[])).unwrap();
    let filler = format!(
        "[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\nspend_usd_per_call = 0.04\n"
    );
    let checker = format!(
        "[checker]\ncommand = [{bin:?}, \"--version\"]\narguments = {{ source = {{ path = \"{{source}}\" }}, candidate = {{ path = \"{{candidate}}\" }} }}\nkey_env = \"CHECKER_TEST_KEY\"\nspend_usd_per_call = 0.01\n"
    );
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let estimate = |config: &str, extra: serde_json::Value| -> (Option<i32>, serde_json::Value) {
        let path = dir.join("rascal.toml");
        std::fs::write(&path, config).unwrap();
        let mut arguments = serde_json::json!({
            "image": { "path": image }, "prompt": "chair", "filler": "fake",
        });
        for (k, v) in extra.as_object().unwrap() {
            arguments[k] = v.clone();
        }
        let request = dir.join("request.json");
        std::fs::write(
            &request,
            serde_json::json!({ "action": "remove_prompt", "arguments": arguments }).to_string(),
        )
        .unwrap();
        let out = rascal(
            &home,
            &[
                "estimate",
                "--json",
                "--config",
                path.to_str().unwrap(),
                request.to_str().unwrap(),
            ],
            &[("CHECKER_TEST_KEY", "set")],
        );
        let doc = serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null);
        (out.status.code(), doc)
    };
    let all = format!("{pack}{filler}{checker}");

    let (code, e) = estimate(&all, serde_json::json!({}));
    assert_eq!(code, Some(0), "{e:#}");
    assert_eq!(
        e["effective"]["x-rascal"]["check"],
        serde_json::json!({ "on": true })
    );
    assert_eq!(e["worst_case_usd"], 0.15);

    let (_, capped) = estimate(&all, serde_json::json!({ "max_spend_usd": 0.12 }));
    assert_eq!(capped["worst_case_usd"], 0.1, "two tries fit 0.12");
    assert_eq!(capped["allowed"], true);

    let (_, off) = estimate(&all, serde_json::json!({ "check": false }));
    assert_eq!(
        off["effective"]["x-rascal"]["check"],
        serde_json::json!({ "on": false, "reason": "turned_off" })
    );
    assert_eq!(off["worst_case_usd"], 0.04);

    // Without a checker, `check` is not an argument of remove_prompt.
    let (code, _) = estimate(
        &format!("{pack}{filler}"),
        serde_json::json!({ "check": false }),
    );
    assert_eq!(code, Some(64), "check without a checker is a usage error");
    let _ = std::fs::remove_dir_all(&dir);
}
