//! M13 track C (SPEC.md "Self-correcting remove", part 3; PLAN.md 13.12): `heal remove-prompt`
//! asks the configured checker about its result. The checker is the testkit's scripted one
//! (`rascal fake-checker <script>`), the segmenter the fake pack, the filler the fake `exact`;
//! one test runs the real `squint` with its scripted provider when that binary is built.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-checker-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// What a test's configuration holds beyond the pack and the filler.
struct Setup<'a> {
    /// The script the fake checker answers from, written as `script.json`.
    script: Value,
    /// `[checker]` lines after `command` and `arguments`.
    checker: &'a str,
    /// `spend_usd_per_call` of the filler, when it is priced.
    filler_usd: Option<f64>,
}

impl Default for Setup<'_> {
    fn default() -> Self {
        Setup {
            script: json!({ "answers": [{ "verdict": "pass" }] }),
            checker: "spend_usd_per_call = 0.01\ntimeout_s = 30\nrevision = \"fake checker\"",
            filler_usd: None,
        }
    }
}

/// The fake pack behind a fake segmenter install, the fake `exact` filler and the scripted
/// checker with the spec's arguments; returns the configuration's path.
fn config(dir: &Path, setup: &Setup) -> PathBuf {
    let bin = env!("CARGO_BIN_EXE_rascal");
    let install =
        rascal_testkit::fake_install::write(&dir.join("pack"), &[bin, "fake-pack", "exact"], true)
            .unwrap();
    let script = dir.join("script.json");
    std::fs::write(&script, setup.script.to_string()).unwrap();
    let path = dir.join("rascal.toml");
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[packs.fake]\ncommand = {}\nrevision = \"fake 1\"\n\
             [fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\nrevision = \"fake exact\"\n{}\
             [checker]\ncommand = [{bin:?}, \"fake-checker\", {:?}]\n\
             arguments = {{ source = {{ path = \"{{source}}\" }}, candidate = {{ path = \"{{candidate}}\" }}, requirements = [{{ id = \"RQ1\", kind = \"must_not\", text = \"Any part of {{prompt}} is visible.\" }}] }}\n{}\n",
            install.command_toml(),
            setup
                .filler_usd
                .map(|usd| format!("spend_usd_per_call = {usd}\n"))
                .unwrap_or_default(),
            script.display().to_string(),
            setup.checker,
        ),
    )
    .unwrap();
    path
}

/// `rascal heal remove-prompt` of the chair image with the prompt "the chair" into `<dir>/run`,
/// plus `extra` flags.
fn heal(dir: &Path, config: &Path, extra: &[&str]) -> Output {
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let out = dir.join("run");
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args([
            "heal",
            "remove-prompt",
            image.to_str().unwrap(),
            "--prompt",
            "the chair",
            "--filler",
            "fake",
            "--config",
            config.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--no-dash",
        ])
        .args(extra)
        .env_remove("RASCAL_LOG")
        .env_remove("RASCAL_TEST_ABSENT_KEY")
        .output()
        .unwrap()
}

fn run_json(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("run/run.json")).unwrap()).unwrap()
}

/// How many times the scripted checker was called.
fn calls(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("script.calls"))
        .map(|t| t.trim().parse().unwrap())
        .unwrap_or(0)
}

/// What the scripted checker saw on its `n`th call (from 0).
fn seen(dir: &Path, n: usize) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(dir.join(format!("script.seen-{n}.json"))).unwrap(),
    )
    .unwrap()
}

fn notes(result: &Value) -> Vec<String> {
    result["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_string())
        .collect()
}

fn note_with<'a>(notes: &'a [String], prefix: &str) -> Vec<&'a str> {
    notes
        .iter()
        .filter_map(|n| n.strip_prefix(prefix))
        .collect()
}

#[test]
fn a_pass_on_the_first_try_ends_the_run_with_its_verdict() {
    let dir = scratch("pass");
    let config = config(&dir, &Setup::default());
    let o = heal(&dir, &config, &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let result = run_json(&dir);
    assert_eq!(result["status"], "completed", "{result:#}");
    assert_eq!(result["verdict"], json!({ "value": "pass", "where": [] }));
    assert_eq!(calls(&dir), 1, "a pass ends the run");
    // The try's check directory holds the source, the candidate and the arguments, and the
    // checker was allowed that directory alone, and ran in it.
    let check = dir.join("run/tries/1/check");
    for name in [
        "source.png",
        "candidate.png",
        "arguments.json",
        "result.json",
    ] {
        assert!(check.join(name).is_file(), "{name}");
    }
    assert_eq!(
        std::fs::read(check.join("candidate.png")).unwrap(),
        std::fs::read(dir.join("run/result.png")).unwrap(),
        "the candidate is the try's result"
    );
    let seen = seen(&dir, 0);
    assert_eq!(seen["allow_dir"], check.display().to_string());
    assert_eq!(seen["cwd"], check.display().to_string());
    let arguments = &seen["arguments"];
    assert_eq!(
        arguments["source"]["path"],
        check.join("source.png").display().to_string()
    );
    assert_eq!(
        arguments["candidate"]["path"],
        check.join("candidate.png").display().to_string()
    );
    assert_eq!(
        arguments["requirements"][0]["text"],
        "Any part of the chair is visible."
    );
    assert_eq!(arguments["max_spend_usd"], 0.01);
    // The journal: the call started, the call, and the stored answer with the request identity.
    let notes = notes(&result);
    assert_eq!(note_with(&notes, "checker call started: ").len(), 1);
    let call: Value = serde_json::from_str(note_with(&notes, "checker call: ")[0]).unwrap();
    assert_eq!(call["outcome"], "pass");
    assert_eq!(call["exit"], 0);
    let stored: Value = serde_json::from_str(note_with(&notes, "checker stored: ")[0]).unwrap();
    assert_eq!(stored["try"], 1);
    assert!(stored["request"].as_str().unwrap().starts_with("sha256:"));
    assert_eq!(stored["verdict"]["value"], "pass");
    assert_eq!(
        result["invocation"]["effective"]["x-rascal"]["check"],
        json!({ "on": true })
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_fail_is_an_answer_whatever_the_exit_code() {
    let dir = scratch("fail");
    let config = config(
        &dir,
        &Setup {
            script: json!({ "answers": [{ "verdict": "fail", "where": [[0.1, 0.1, 0.4, 0.3]] }] }),
            ..Setup::default()
        },
    );
    let o = heal(&dir, &config, &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let result = run_json(&dir);
    assert_eq!(result["status"], "completed");
    assert_eq!(result["verdict"]["value"], "fail");
    let place = &result["verdict"]["where"][0];
    assert_eq!(place["parts"][0]["bbox"], json!([0.1, 0.1, 0.4, 0.3]));
    // The place is on the result the run returns.
    let out = result["images"]["out"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["role"] == "result")
        .unwrap();
    assert_eq!(place["image"]["file_sha256"], out["image"]["file_sha256"]);
    let notes = notes(&result);
    let call: Value = serde_json::from_str(note_with(&notes, "checker call: ")[0]).unwrap();
    assert_eq!(call["exit"], 1, "the checker exited 1, and it was not read");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn each_check_failure_completes_the_run_flagged_without_a_verdict() {
    for (failure, says) in [
        ("status", "its run ended error: the provider answered 529"),
        ("no-verdict", "its completed result has no verdict"),
        ("garbage", "it printed no table-manners result"),
        ("oversize", "its answer is over 1048576 bytes"),
        ("timeout", "no answer within 1 s"),
        ("outside", "outside the allowed directories"),
    ] {
        let dir = scratch(&format!("failure-{failure}"));
        let answer = if failure == "timeout" {
            json!({ "verdict": "pass", "delay_ms": 5000 })
        } else {
            json!({ "verdict": "pass", "failure": failure })
        };
        let config = config(
            &dir,
            &Setup {
                script: json!({ "answers": [answer] }),
                checker: "spend_usd_per_call = 0.01\ntimeout_s = 1",
                ..Setup::default()
            },
        );
        let o = heal(&dir, &config, &[]);
        assert_eq!(o.status.code(), Some(0), "{failure}: {}", text(&o.stderr));
        let result = run_json(&dir);
        assert_eq!(result["status"], "completed", "{failure}: {result:#}");
        assert!(result.get("verdict").is_none(), "{failure}: {result:#}");
        assert!(
            dir.join("run/result.png").is_file(),
            "{failure}: the try is kept"
        );
        let notes = notes(&result);
        let flagged = note_with(&notes, "The check could not run: ");
        assert_eq!(flagged.len(), 1, "{failure}: {notes:#?}");
        assert!(flagged[0].contains(says), "{failure}: {}", flagged[0]);
        assert!(
            note_with(&notes, "checker stored: ").is_empty(),
            "{failure}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_missing_key_fills_once_unchecked_and_says_why() {
    let dir = scratch("key");
    let config = config(
        &dir,
        &Setup {
            checker: "spend_usd_per_call = 0.01\nkey_env = \"RASCAL_TEST_ABSENT_KEY\"",
            ..Setup::default()
        },
    );
    let o = heal(&dir, &config, &[]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "readiness stays ready: {}",
        text(&o.stderr)
    );
    let result = run_json(&dir);
    assert_eq!(result["status"], "completed");
    assert!(result.get("verdict").is_none());
    assert_eq!(calls(&dir), 0);
    assert!(!dir.join("run/tries").exists());
    assert!(
        notes(&result)
            .contains(&"Not checked: the checker needs RASCAL_TEST_ABSENT_KEY.".to_string()),
        "{result:#}"
    );
    assert_eq!(
        result["invocation"]["effective"]["x-rascal"]["check"],
        json!({ "on": false, "reason": "key_missing" })
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_check_fills_once_unchecked_and_says_why() {
    let dir = scratch("no-check");
    let config = config(&dir, &Setup::default());
    let o = heal(&dir, &config, &["--no-check"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let result = run_json(&dir);
    assert!(result.get("verdict").is_none());
    assert_eq!(calls(&dir), 0);
    assert!(
        notes(&result)
            .contains(&"Not checked: the check was turned off for this call.".to_string()),
        "{result:#}"
    );
    assert_eq!(result["invocation"]["arguments"]["check"], false);
    assert_eq!(
        result["invocation"]["effective"]["x-rascal"]["check"],
        json!({ "on": false, "reason": "turned_off" })
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_spend_sums_the_fill_and_the_check_after_a_pass_and_after_a_check_failure() {
    // A pass: the filler at its price (0.02, estimated) and the check at what it reports.
    let dir = scratch("spend-pass");
    let file = config(
        &dir,
        &Setup {
            script: json!({ "answers": [{ "verdict": "pass" }], "spend_usd": 0.004 }),
            filler_usd: Some(0.02),
            ..Setup::default()
        },
    );
    let o = heal(&dir, &file, &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let spend = &run_json(&dir)["spend"];
    assert_eq!(spend["usd"], 0.024, "{spend}");
    assert_eq!(spend["unresolved_usd"], 0.0);
    assert_eq!(spend["basis"], "estimated");
    let _ = std::fs::remove_dir_all(&dir);
    // A check that timed out: sent, never settled, so counted at its price as unresolved.
    let dir = scratch("spend-timeout");
    let file = config(
        &dir,
        &Setup {
            script: json!({ "answers": [{ "verdict": "pass", "delay_ms": 5000 }], "spend_usd": 0.004 }),
            checker: "spend_usd_per_call = 0.01\ntimeout_s = 1",
            filler_usd: Some(0.02),
        },
    );
    let o = heal(&dir, &file, &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let spend = &run_json(&dir)["spend"];
    assert_eq!(spend["usd"], 0.03, "{spend}");
    assert_eq!(spend["unresolved_usd"], 0.01);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The fake pack, filler and checker under `serve`, the session's workspace and runs apart.
fn serve(dir: &Path, config: &Path) -> rascal_testkit::mcp_client::Client {
    let mut command = rascal_testkit::mcp_client::serve_command(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "--workspace",
            dir.join("ws").to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env_remove("RASCAL_LOG");
    rascal_testkit::mcp_client::Client::start(&mut command).unwrap()
}

fn session(client: &mut rascal_testkit::mcp_client::Client) -> Value {
    let result = client
        .request("resources/read", json!({ "uri": "rascal://session" }))
        .unwrap();
    serde_json::from_str(result["contents"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn under_serve_the_checker_reads_both_images_and_the_unused_reservation_is_returned() {
    let dir = scratch("serve");
    let file = config(
        &dir,
        &Setup {
            script: json!({ "answers": [{ "verdict": "pass" }], "spend_usd": 0.004 }),
            filler_usd: Some(0.02),
            ..Setup::default()
        },
    );
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, format!("{text}[limits]\nsession_budget_usd = 1.0\n")).unwrap();
    let mut client = serve(&dir, &file);
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let out = client
        .call(
            "remove_prompt",
            json!({ "image": { "path": image.to_str().unwrap() }, "prompt": "the chair", "filler": "fake" }),
        )
        .unwrap();
    let s = out.structured.unwrap();
    assert_eq!(s["status"], "completed", "{s:#}");
    assert_eq!(
        s["verdict"]["value"], "pass",
        "the checker read both images: {s:#}"
    );
    assert_eq!(s["spend"]["usd"], 0.024);
    // Three tries were reserved (0.09); the run spent 0.024 and the rest came back.
    let ledger = session(&mut client);
    assert_eq!(ledger["reserved_usd"], 0.0, "{ledger:#}");
    assert_eq!(ledger["spent_usd"], 0.024, "{ledger:#}");
    let run_dir = PathBuf::from(s["run"]["dir"].as_str().unwrap());
    let check = run_dir.join("tries/1/check");
    assert_eq!(seen(&dir, 0)["allow_dir"], check.display().to_string());
    assert!(!run_dir.starts_with(dir.join("ws")));
    drop(client);
    // A checker that reaches outside --allow-dir is refused, and the run says so.
    std::fs::write(
        dir.join("script.json"),
        json!({ "answers": [{ "verdict": "pass", "failure": "outside" }] }).to_string(),
    )
    .unwrap();
    let mut client = serve(&dir, &file);
    let out = client
        .call(
            "remove_prompt",
            json!({ "image": { "path": image.to_str().unwrap() }, "prompt": "the chair", "filler": "fake" }),
        )
        .unwrap();
    let s = out.structured.unwrap();
    assert_eq!(s["status"], "completed", "{s:#}");
    assert!(s.get("verdict").is_none(), "{s:#}");
    assert!(
        notes(&s)
            .iter()
            .any(|n| n.starts_with("The check could not run: ")
                && n.contains("outside the allowed directories")),
        "{s:#}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The real squint built with its testkit (`cargo build --features squint-cli/testkit --bin
/// squint` in ../squint), when it is there.
fn squint() -> Option<PathBuf> {
    let bin = repo_root().join("../squint/target/debug/squint");
    let help = Command::new(&bin).args(["call", "--help"]).output().ok()?;
    text(&help.stdout)
        .contains("--provider-script")
        .then_some(bin)
}

#[test]
fn one_check_through_the_real_squint_with_its_scripted_provider() {
    let Some(squint) = squint() else {
        eprintln!(
            "skipped: no testkit squint at ../squint/target/debug/squint (cargo build --features squint-cli/testkit --bin squint in ../squint)"
        );
        return;
    };
    let squint_root = squint
        .parent()
        .unwrap()
        .join("../..")
        .canonicalize()
        .unwrap();
    for (script, value) in [
        ("requirement-held.json", "pass"),
        ("intent-must-not-violation.json", "fail"),
        ("requirement-cant-tell.json", "unsure"),
    ] {
        let dir = scratch(&format!("squint-{value}"));
        let file = config(&dir, &Setup::default());
        // The spec's checker, its command the real squint with its fixture config and script.
        let text_config = std::fs::read_to_string(&file).unwrap();
        let command = format!(
            "command = [{:?}, \"call\", \"source_fidelity\", \"--config\", {:?}, \"--provider-script\", {:?}]",
            squint.display().to_string(),
            squint_root
                .join("fixtures/config/minimal.toml")
                .display()
                .to_string(),
            squint_root
                .join("fixtures/scripts")
                .join(script)
                .display()
                .to_string(),
        );
        let replaced: Vec<String> = text_config
            .lines()
            .map(|l| {
                if l.starts_with("command = ") && l.contains("fake-checker") {
                    command.clone()
                } else if l.starts_with("spend_usd_per_call = 0.01") {
                    // The scripted provider prices a decision at about 0.15.
                    "spend_usd_per_call = 0.5".to_string()
                } else {
                    l.to_string()
                }
            })
            .collect();
        std::fs::write(&file, replaced.join("\n")).unwrap();
        let o = heal(&dir, &file, &[]);
        assert_eq!(o.status.code(), Some(0), "{script}: {}", text(&o.stderr));
        let result = run_json(&dir);
        assert_eq!(result["status"], "completed", "{script}: {result:#}");
        assert_eq!(result["verdict"]["value"], value, "{script}: {result:#}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
