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
    /// The fake pack's behaviour.
    pack: &'a str,
}

impl Default for Setup<'_> {
    fn default() -> Self {
        Setup {
            script: json!({ "answers": [{ "verdict": "pass" }] }),
            checker: "spend_usd_per_call = 0.01\ntimeout_s = 30\nrevision = \"fake checker\"",
            filler_usd: None,
            pack: "exact",
        }
    }
}

/// The fake pack behind a fake segmenter install, the fake `exact` filler and the scripted
/// checker with the spec's arguments; returns the configuration's path.
fn config(dir: &Path, setup: &Setup) -> PathBuf {
    let bin = env!("CARGO_BIN_EXE_rascal");
    let install = rascal_testkit::fake_install::write(
        &dir.join("pack"),
        &[bin, "fake-pack", setup.pack],
        true,
    )
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
            ..Setup::default()
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

/// The journal's lines, parsed.
fn journal(dir: &Path) -> Vec<Value> {
    std::fs::read_to_string(dir.join("run/journal.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// The journal lines of op `op` (invocations, not notes), in order.
fn steps<'a>(journal: &'a [Value], op: &str) -> Vec<&'a Value> {
    journal.iter().filter(|l| l["op"] == op).collect()
}

fn sha256(path: &Path) -> String {
    rascal_core::contract::file_sha256(&std::fs::read(path).unwrap())
}

fn stored(result: &Value) -> Vec<Value> {
    note_with(&notes(result), "checker stored: ")
        .iter()
        .map(|t| serde_json::from_str(t).unwrap())
        .collect()
}

/// Two tries, a fail with one place and then a pass: the place is segmented again in a crop
/// of the input, at half the threshold, its planes join the first ones, the mask is rebuilt
/// through the shaping and filled again; try 1's files stay as they were.
#[test]
fn a_fail_then_a_pass_retries_inside_the_place() {
    let dir = scratch("retry");
    let config = config(
        &dir,
        &Setup {
            script: json!({ "answers": [
                { "verdict": "fail", "where": [[0.25, 0.5, 0.75, 0.625]] },
                { "verdict": "pass" }
            ] }),
            ..Setup::default()
        },
    );
    let o = heal(&dir, &config, &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let result = run_json(&dir);
    assert_eq!(result["status"], "completed", "{result:#}");
    assert_eq!(calls(&dir), 2);
    assert_eq!(result["verdict"]["value"], "pass");
    let journal = journal(&dir);
    // The retry's segment: in a crop frame, the same prompt, half the threshold.
    let segments = steps(&journal, "segment");
    assert_eq!(segments.len(), 2, "the first find and one per place");
    assert_eq!(segments[1]["params"]["prompt"], "the chair");
    assert_eq!(segments[1]["params"]["threshold"], 0.1);
    let selects = steps(&journal, "select");
    assert_eq!(selects.len(), 2);
    assert_eq!(selects[0]["inputs"].as_object().unwrap().len(), 8);
    assert_eq!(selects[1]["inputs"].as_object().unwrap().len(), 16);
    // The stored check of try 1 holds its transition: the place used and the planes added.
    let stored = stored(&result);
    assert_eq!(stored.len(), 2);
    let transition = &stored[0]["transition"];
    assert_eq!(
        transition["crops"],
        json!([[108, 158, 372, 202]]),
        "{transition:#}"
    );
    assert_eq!(transition["planes"].as_array().unwrap().len(), 8);
    assert!(stored[1].get("transition").is_none());
    // Each try's files are its own, and try 1's were not overwritten by try 2.
    for k in [1, 2] {
        let try_dir = dir.join(format!("run/tries/{k}"));
        assert!(try_dir.join("result.png").is_file(), "try {k}");
        assert_eq!(
            seen(&dir, k - 1)["candidate_sha256"],
            sha256(&try_dir.join("result.png")),
            "try {k}'s result is what was checked"
        );
    }
    assert_ne!(
        sha256(&dir.join("run/tries/1/result.png")),
        sha256(&dir.join("run/tries/2/result.png"))
    );
    assert!(dir.join("run/tries/2/remove-prompt-grey.png").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_where_widens_the_mask_once() {
    let dir = scratch("empty-where");
    let config = config(
        &dir,
        &Setup {
            script: json!({ "answers": [{ "verdict": "unsure", "where": [] }, { "verdict": "pass" }] }),
            ..Setup::default()
        },
    );
    let o = heal(&dir, &config, &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let journal = journal(&dir);
    assert_eq!(steps(&journal, "segment").len(), 1, "no place to segment");
    let grows = steps(&journal, "grow");
    assert_eq!(grows.len(), 2);
    assert_eq!(grows[0]["params"]["px"], 4.0);
    assert_eq!(grows[1]["params"]["px"], 8.0, "grow.px doubled");
    let stored = stored(&run_json(&dir));
    assert_eq!(stored[0]["transition"]["grow_px"], 8.0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn overlapping_places_each_add_their_planes() {
    let dir = scratch("overlap");
    let config = config(
        &dir,
        &Setup {
            script: json!({ "answers": [
                { "verdict": "fail", "where": [[0.2, 0.2, 0.6, 0.6], [0.4, 0.4, 0.8, 0.8]] },
                { "verdict": "pass" }
            ] }),
            ..Setup::default()
        },
    );
    let o = heal(&dir, &config, &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let journal = journal(&dir);
    assert_eq!(steps(&journal, "segment").len(), 3);
    let selects = steps(&journal, "select");
    assert_eq!(selects[1]["inputs"].as_object().unwrap().len(), 24);
    assert_eq!(run_json(&dir)["verdict"]["value"], "pass");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_place_on_another_image_is_a_check_failure() {
    let dir = scratch("foreign");
    let config = config(
        &dir,
        &Setup {
            script: json!({ "answers": [
                { "verdict": "fail", "where": [[0.2, 0.2, 0.6, 0.6]], "foreign": true },
                { "verdict": "pass" }
            ] }),
            ..Setup::default()
        },
    );
    let o = heal(&dir, &config, &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let result = run_json(&dir);
    assert_eq!(calls(&dir), 1, "no second try");
    assert!(result.get("verdict").is_none(), "{result:#}");
    let notes = notes(&result);
    let flagged = note_with(&notes, "The check could not run: ");
    assert_eq!(flagged.len(), 1, "{notes:#?}");
    assert!(flagged[0].contains("another image"), "{}", flagged[0]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The planes a run committed for its places (`place` records), as their set pixels' boxes.
fn place_planes(dir: &Path) -> Vec<[u32; 4]> {
    let ws = rascal_core::workspace::Workspace::open_read_only(&dir.join("run")).unwrap();
    ws.records()
        .unwrap()
        .iter()
        .filter(|r| r.op == "place")
        .map(|r| {
            let id = &r.envelope.artifacts["image"].identity;
            let rascal_core::artifact::Artifact::Image(image) = ws.load(id).unwrap() else {
                panic!("not an image");
            };
            assert_eq!(image.record.role, rascal_core::image::Role::Plane);
            assert_eq!(image.record.frame, "raw:480x320:flat");
            let grey = image.pixels.to_luma8();
            let set: Vec<(u32, u32)> = grey
                .enumerate_pixels()
                .filter(|(_, _, p)| p[0] != 0)
                .map(|(x, y, _)| (x, y))
                .collect();
            let x0 = set.iter().map(|p| p.0).min().unwrap();
            let y0 = set.iter().map(|p| p.1).min().unwrap();
            let x1 = set.iter().map(|p| p.0).max().unwrap() + 1;
            let y1 = set.iter().map(|p| p.1).max().unwrap() + 1;
            assert_eq!(set.len() as u32, (x1 - x0) * (y1 - y0), "a filled box");
            [x0, y0, x1, y1]
        })
        .collect()
}

/// A non-square place at an offset, unpadded: the fake's one instance in the crop (its box
/// [0.1, 0.1, 0.35, 0.25] of the crop, by pixel centres) lands on exactly those raw pixels.
#[test]
fn a_non_square_place_at_an_offset_maps_back_to_the_exact_raw_pixels() {
    let dir = scratch("offset");
    let config = config(
        &dir,
        &Setup {
            script: json!({ "answers": [
                { "verdict": "fail", "where": [[0.25, 0.5, 0.75, 0.625]] },
                { "verdict": "pass" }
            ] }),
            ..Setup::default()
        },
    );
    let o = heal(
        &dir,
        &config,
        &["--set", "segment.pad=0", "--set", "segment.max_instances=1"],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    // The place is x 120..360, y 160..200 of 480x320: a 240x40 crop at (120, 160).
    let inside = |i: u32, size: u32, a: f64, b: f64| {
        let c = (f64::from(i) + 0.5) / f64::from(size);
        c >= a && c < b
    };
    let xs: Vec<u32> = (0..240).filter(|&i| inside(i, 240, 0.1, 0.35)).collect();
    let ys: Vec<u32> = (0..40).filter(|&j| inside(j, 40, 0.1, 0.25)).collect();
    let expected = [
        120 + xs[0],
        160 + ys[0],
        120 + xs[xs.len() - 1] + 1,
        160 + ys[ys.len() - 1] + 1,
    ];
    assert_eq!(place_planes(&dir), vec![expected]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_place_where_segment_finds_nothing_joins_as_itself() {
    let dir = scratch("nothing");
    let config = config(
        &dir,
        &Setup {
            script: json!({ "answers": [
                { "verdict": "fail", "where": [[0.25, 0.5, 0.75, 0.625]] },
                { "verdict": "pass" }
            ] }),
            pack: "empty-crops",
            ..Setup::default()
        },
    );
    let o = heal(&dir, &config, &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(place_planes(&dir), vec![[120, 160, 360, 200]]);
    let journal = journal(&dir);
    assert_eq!(
        steps(&journal, "select")[1]["inputs"]
            .as_object()
            .unwrap()
            .len(),
        9
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn protect_is_preserved_in_every_try() {
    let dir = scratch("protect");
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let protect = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args([
            "run",
            "rect",
            image.to_str().unwrap(),
            "--bbox",
            "0.2,0.45,0.5,0.7",
            "--out",
            dir.join("protect-run").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(protect.status.success(), "{}", text(&protect.stderr));
    let mask_dir = text(&protect.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("mask: ").map(PathBuf::from))
        .unwrap();
    let config = config(
        &dir,
        &Setup {
            script: json!({ "answers": [
                { "verdict": "fail", "where": [[0.1, 0.4, 0.6, 0.8]] },
                { "verdict": "pass" }
            ] }),
            ..Setup::default()
        },
    );
    let o = heal(&dir, &config, &["--protect", mask_dir.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    assert_eq!(calls(&dir), 2);
    let input = image::open(&image).unwrap().to_rgb8();
    let protect = rascal_core::mask::Mask::read(&mask_dir).unwrap();
    for k in [1, 2] {
        let result = image::open(dir.join(format!("run/tries/{k}/result.png")))
            .unwrap()
            .to_rgb8();
        let mut covered = 0;
        for (x, y, p) in protect.core.enumerate_pixels() {
            if p[0] != 0 {
                covered += 1;
                assert_eq!(
                    result.get_pixel(x, y),
                    input.get_pixel(x, y),
                    "try {k} at {x},{y}"
                );
            }
        }
        assert!(covered > 0);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

fn roles(result: &Value) -> Vec<(String, String)> {
    result["images"]["out"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            (
                o["role"].as_str().unwrap().to_string(),
                o["label"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

/// A run whose every try is answered: `answers` in order.
fn tried(name: &str, answers: Value, extra: &[&str]) -> (PathBuf, Value) {
    let dir = scratch(name);
    let config = config(
        &dir,
        &Setup {
            script: json!({ "answers": answers, "spend_usd": 0.004 }),
            filler_usd: Some(0.02),
            ..Setup::default()
        },
    );
    let o = heal(&dir, &config, extra);
    assert_eq!(o.status.code(), Some(0), "{name}: {}", text(&o.stderr));
    let result = run_json(&dir);
    assert_eq!(result["status"], "completed", "{name}: {result:#}");
    (dir, result)
}

/// The tries run out without a pass: the kept try is `unsure` over `fail`, then the one with
/// fewer places, then the earlier; the result's verdict is the kept try's, its outputs lead,
/// every other try follows labelled, and the summary says what the check still sees.
#[test]
fn unsure_until_the_tries_run_out_keeps_the_try_the_selection_rule_picks() {
    let place = |x: f64| json!([x, 0.2, x + 0.2, 0.4]);
    // Unsure over fail, then fewer places: try 3.
    let (dir, result) = tried(
        "select-fewer",
        json!([
            { "verdict": "fail", "where": [place(0.1)] },
            { "verdict": "unsure", "where": [place(0.2), place(0.5)] },
            { "verdict": "unsure", "where": [place(0.3)] }
        ]),
        &[],
    );
    assert_eq!(calls(&dir), 3);
    assert_eq!(result["body"]["selected_try"], 3);
    assert_eq!(result["verdict"]["value"], "unsure");
    assert_eq!(result["verdict"]["where"].as_array().unwrap().len(), 1);
    assert!(
        notes(&result).contains(
            &"Kept try 3 of 3. The check still sees the chair in 1 place (unsure).".to_string()
        ),
        "{:#?}",
        notes(&result)
    );
    let tries = result["body"]["tries"].as_array().unwrap();
    assert_eq!(tries.len(), 3);
    for (i, t) in tries.iter().enumerate() {
        assert_eq!(t["try"], i + 1);
        assert!(t["mask"].as_str().unwrap().starts_with("sha256:"));
        assert_eq!(
            t["result"]["file_sha256"],
            sha256(&dir.join(format!("run/tries/{}/result.png", i + 1)))
        );
        assert_eq!(t["spend"]["usd"], 0.024, "{t}");
    }
    assert_eq!(tries[0]["verdict"]["value"], "fail");
    // Spend across the three tries: three fills at 0.02 and three checks at 0.004.
    assert_eq!(result["spend"]["usd"], 0.072);
    // The kept try's result, preview and mask first, then the others in try order.
    let expected: Vec<(String, String)> = [
        ("result", "Try 3 · result"),
        ("preview", "Try 3 · preview"),
        ("mask", "Try 3 · mask"),
        ("result_try_1", "Try 1 · result"),
        ("preview_try_1", "Try 1 · preview"),
        ("mask_try_1", "Try 1 · mask"),
        ("result_try_2", "Try 2 · result"),
        ("preview_try_2", "Try 2 · preview"),
        ("mask_try_2", "Try 2 · mask"),
    ]
    .iter()
    .map(|(r, l)| (r.to_string(), l.to_string()))
    .collect();
    assert_eq!(roles(&result), expected);
    let out = &result["images"]["out"][0];
    assert_eq!(
        out["image"]["file_sha256"],
        sha256(&dir.join("run/tries/3/result.png"))
    );
    assert_eq!(
        result["verdict"]["where"][0]["image"]["file_sha256"], out["image"]["file_sha256"],
        "the verdict's places are on the kept result"
    );
    // The run's Mask is the kept try's; the others are listed beside it.
    let masks: Vec<(&str, &str)> = result["artifacts"]["out"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["port"].as_str().unwrap(),
                e["artifact"]["identity"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        masks,
        vec![
            ("mask", tries[2]["mask"].as_str().unwrap()),
            ("mask_try_1", tries[0]["mask"].as_str().unwrap()),
            ("mask_try_2", tries[1]["mask"].as_str().unwrap()),
        ]
    );
    let _ = std::fs::remove_dir_all(&dir);
    // Equal verdicts and places: the earlier try.
    let (dir, result) = tried(
        "select-earlier",
        json!([
            { "verdict": "unsure", "where": [place(0.1), place(0.5)] },
            { "verdict": "fail", "where": [place(0.2)] },
            { "verdict": "unsure", "where": [place(0.3), place(0.6)] }
        ]),
        &[],
    );
    assert_eq!(result["body"]["selected_try"], 1);
    assert_eq!(
        roles(&result)[0],
        ("result".to_string(), "Try 1 · result".to_string())
    );
    assert_eq!(roles(&result)[3].0, "result_try_2");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_outer_verdict_is_a_pass_a_flagged_fail_or_absent() {
    let (dir, result) = tried(
        "outer-pass",
        json!([{ "verdict": "fail", "where": [[0.2, 0.2, 0.5, 0.5]] }, { "verdict": "pass" }]),
        &[],
    );
    assert_eq!(result["verdict"], json!({ "value": "pass", "where": [] }));
    assert_eq!(result["body"]["selected_try"], 2);
    assert!(notes(&result).contains(&"Kept try 2 of 2.".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
    let (dir, result) = tried(
        "outer-fail",
        json!([{ "verdict": "fail", "where": [[0.2, 0.2, 0.5, 0.5]] }]),
        &[],
    );
    assert_eq!(result["verdict"]["value"], "fail");
    assert_eq!(
        result["body"]["selected_try"], 1,
        "all fail alike: the earliest"
    );
    assert!(
        notes(&result)
            .iter()
            .any(|n| n.contains("The check still sees the chair in 1 place (fail).")),
        "{:#?}",
        notes(&result)
    );
    let _ = std::fs::remove_dir_all(&dir);
    let (dir, result) = tried(
        "outer-unchecked",
        json!([{ "verdict": "pass" }]),
        &["--no-check"],
    );
    assert!(result.get("verdict").is_none());
    assert!(result["body"].get("tries").is_none());
    assert!(
        result["images"]["out"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o.get("label").is_none())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unchanged_mask_stops_the_tries_early() {
    let (dir, result) = tried(
        "unchanged",
        json!([
            { "verdict": "unsure", "where": [] },
            { "verdict": "unsure", "where": [] },
            { "verdict": "pass" }
        ]),
        &[],
    );
    assert_eq!(calls(&dir), 2, "a third try would repeat the second");
    assert_eq!(result["body"]["tries"].as_array().unwrap().len(), 2);
    assert!(
        notes(&result)
            .iter()
            .any(|n| n.starts_with("Try 3 would repeat try 2")),
        "{:#?}",
        notes(&result)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cap_allows_one_and_then_two_tries() {
    for (cap, tries) in [("0.03", 1), ("0.06", 2), ("0.09", 3)] {
        let dir = scratch(&format!("cap-{tries}"));
        let file = config(
            &dir,
            &Setup {
                script: json!({ "answers": [{ "verdict": "fail", "where": [[0.2, 0.2, 0.5, 0.5]] }] }),
                filler_usd: Some(0.02),
                ..Setup::default()
            },
        );
        let text_config = std::fs::read_to_string(&file).unwrap();
        std::fs::write(
            &file,
            format!("{text_config}[limits]\ncall_cap_usd = {cap}\n"),
        )
        .unwrap();
        let o = heal(&dir, &file, &[]);
        assert_eq!(o.status.code(), Some(0), "{cap}: {}", text(&o.stderr));
        assert_eq!(calls(&dir), tries, "cap {cap}");
        let result = run_json(&dir);
        assert_eq!(result["body"]["tries"].as_array().unwrap().len(), tries);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// After a served run, an earlier try's Mask opens by its reference and its preview by the
/// run's image resource.
#[test]
fn an_earlier_try_s_mask_and_preview_open_by_reference_after_the_run() {
    let dir = scratch("earlier");
    let file = config(
        &dir,
        &Setup {
            script: json!({ "answers": [
                { "verdict": "fail", "where": [[0.2, 0.2, 0.5, 0.5]] },
                { "verdict": "pass" }
            ] }),
            ..Setup::default()
        },
    );
    let mut client = serve(&dir, &file);
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let out = client
        .call(
            "remove_prompt",
            json!({ "image": { "path": image.to_str().unwrap() }, "prompt": "the chair", "filler": "fake" }),
        )
        .unwrap();
    let s = out.structured.unwrap();
    assert_eq!(s["body"]["selected_try"], 2, "{s:#}");
    let first = s["artifacts"]["out"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["port"] == "mask_try_1")
        .unwrap()
        .clone();
    let resource = first["artifact"]["resource"].as_str().unwrap();
    let read = client
        .request("resources/read", json!({ "uri": resource }))
        .unwrap();
    assert!(!read["contents"].as_array().unwrap().is_empty(), "{read:#}");
    let id = s["run"]["id"].as_str().unwrap();
    let preview = client
        .request(
            "resources/read",
            json!({ "uri": format!("rascal://run/{id}/images/preview_try_1") }),
        )
        .unwrap();
    assert_eq!(
        preview["contents"][0]["mimeType"], "image/png",
        "{preview:#}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
