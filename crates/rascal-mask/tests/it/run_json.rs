//! Task 8.4 (ADR 0020): every `run` and `heal` writes `run.json`, the contract's result, last
//! and atomically: completed, invalid and error runs, each valid against table-manners' result
//! schema; a failed heal keeps the mask it made; an `--out` that held something is not touched.
//! Task 10.2 (table-manners 4.8): every one records its invocation as the matching action's
//! arguments, null only on an `invalid` run whose arguments did not resolve.
//! Task 10.4 (table-manners 4.7): rascal is an operations tool, so every result carries
//! `artifacts`, empty until operations run (10.6) and tasks list their Masks (10.8).
#![cfg(feature = "testkit")]
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

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-runjson-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
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

/// `run.json` from a run directory, validated against the contract's result schema.
fn run_json(dir: &Path) -> serde_json::Value {
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("run.json")).unwrap()).unwrap();
    let (_, text) = table_manners::SCHEMAS
        .iter()
        .find(|(i, _)| *i == "urn:table-manners:core:result")
        .unwrap();
    let schema: serde_json::Value = serde_json::from_str(text).unwrap();
    let validator = jsonschema::options()
        .with_retriever(Embedded)
        .build(&schema)
        .unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&value)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:?}\n{value:#}");
    assert!(!dir.join(".run.json.tmp").exists());
    assert_eq!(
        value["artifacts"],
        serde_json::json!({ "in": [], "out": [] }),
        "{value:#}"
    );
    value
}

fn config(dir: &Path, behaviour: &str) -> PathBuf {
    let path = dir.join(format!("{behaviour}.toml"));
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[fillers.fake]\ncommand = [{:?}, \"fake-filler\", \"{behaviour}\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\npaste = \"feather\"\ntimeout_s = 30\nrevision = \"fake\"\n",
            env!("CARGO_BIN_EXE_rascal")
        ),
    )
    .unwrap();
    path
}

#[test]
fn a_completed_heal_names_its_input_its_mask_and_its_result() {
    let dir = scratch("heal");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let out = rascal(&[
        "heal",
        "seam",
        pano.to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        config(&dir, "exact").to_str().unwrap(),
        "--out",
        dir.join("run").to_str().unwrap(),
        "--no-dash",
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r = run_json(&dir.join("run"));
    assert_eq!(r["status"], "completed");
    assert_eq!(r["action"], "seam");
    assert_eq!(r["error"], serde_json::Value::Null);
    assert_eq!(r["tool"]["name"], "rascal");
    let digest = rascal_core::contract::file_sha256(&std::fs::read(&pano).unwrap());
    assert_eq!(
        r["images"]["in"][0]["image"]["file_sha256"],
        digest.as_str()
    );
    assert_eq!(r["images"]["in"][0]["image"]["topology"], "wrap");
    let roles: Vec<&str> = r["images"]["out"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["mask", "result"]);
    let result = &r["images"]["out"][1];
    assert_eq!(result["image"]["width"], 800);
    assert_eq!(
        result["image"]["topology"], "wrap",
        "the result is the panorama"
    );
    assert!(Path::new(result["path"].as_str().unwrap()).is_file());
    assert_eq!(r["body"]["filler"], "fake");
    assert_eq!(r["body"]["outcome"], "ok");
    // The heal as the action's call: the image by its reference, the filler, the defaults.
    assert_eq!(r["invocation"]["action"], "seam");
    assert_eq!(
        r["invocation"]["arguments"],
        serde_json::json!({
            "image": { "expect": r["images"]["in"][0]["image"] },
            "filler": "fake",
            "projection": "equirectangular",
            "strict": false,
        }),
        "{r:#}"
    );
    assert_eq!(
        r["invocation"]["requirements"],
        serde_json::json!(["rascal.toml", "fillers.fake", "rascal"])
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A filler at 0.04 USD a call, for the spend tests (task 9.1); `command` replaces the fake.
fn priced(dir: &Path, behaviour: &str, command: Option<&str>) -> PathBuf {
    let path = dir.join(format!("priced-{behaviour}.toml"));
    let program = command.unwrap_or(env!("CARGO_BIN_EXE_rascal"));
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[fillers.fake]\ncommand = [{program:?}, \"fake-filler\", \"{behaviour}\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\npaste = \"feather\"\ntimeout_s = 30\nrevision = \"fake\"\nspend_usd_per_call = 0.04\n"
        ),
    )
    .unwrap();
    path
}

fn heal_with(config: &Path, out: &Path) -> Output {
    let pano = repo_root().join("fixtures/images/step-pano.png");
    rascal(&[
        "heal",
        "seam",
        pano.to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        config.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--no-dash",
    ])
}

/// Task 9.1: a call that may have been charged is never reported as zero. A filler that ran and
/// failed is unresolved at its price; one that could not start cost nothing.
#[test]
fn a_failed_call_keeps_its_spend_unresolved_and_an_unsent_one_is_free() {
    let dir = scratch("spend-failed");
    let out = heal_with(&priced(&dir, "failing", None), &dir.join("failing"));
    assert_eq!(out.status.code(), Some(3));
    let r = run_json(&dir.join("failing"));
    assert_eq!(r["status"], "error");
    assert_eq!(r["spend"]["usd"], 0.04, "{r:#}");
    assert_eq!(r["spend"]["unresolved_usd"], 0.04);
    assert!(
        r["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("unresolved"))
    );
    // A program that is there (so the call is ready, task 12.2) but cannot start, its
    // interpreter missing: not sent.
    let missing = dir.join("not-a-program");
    std::fs::write(&missing, b"#!/nonexistent/interpreter\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&missing, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let out = heal_with(
        &priced(&dir, "exact", Some(missing.to_str().unwrap())),
        &dir.join("unsent"),
    );
    assert_eq!(out.status.code(), Some(3));
    let r = run_json(&dir.join("unsent"));
    assert_eq!(r["spend"]["usd"], 0.0, "{r:#}");
    assert_eq!(r["spend"]["unresolved_usd"], 0.0);
    // A call that answered is known: the price, nothing unresolved.
    let out = heal_with(&priced(&dir, "exact", None), &dir.join("known"));
    assert_eq!(out.status.code(), Some(0));
    let r = run_json(&dir.join("known"));
    assert_eq!(r["spend"]["usd"], 0.04);
    assert_eq!(r["spend"]["unresolved_usd"], 0.0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_filler_is_an_error_that_keeps_the_mask() {
    let dir = scratch("failing");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let out = rascal(&[
        "heal",
        "seam",
        pano.to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        config(&dir, "failing").to_str().unwrap(),
        "--out",
        dir.join("run").to_str().unwrap(),
        "--no-dash",
    ]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r = run_json(&dir.join("run"));
    assert_eq!(r["status"], "error");
    assert!(r["error"]["reason"].as_str().unwrap().len() > 3);
    let mask = r["images"]["out"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["role"] == "mask");
    assert!(mask.is_some(), "the mask survives the failed fill: {r:#}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_invalid_request_is_invalid_and_a_used_location_is_listed() {
    let dir = scratch("invalid");
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let chairs = repo_root().join("fixtures/locations/chairs.json");
    let ok = rascal(&[
        "run",
        "remove",
        image.to_str().unwrap(),
        "--location",
        chairs.to_str().unwrap(),
        "--out",
        dir.join("ok").to_str().unwrap(),
    ]);
    assert_eq!(ok.status.code(), Some(0));
    let r = run_json(&dir.join("ok"));
    assert_eq!(r["locations"]["used"].as_array().unwrap().len(), 1);
    assert_eq!(
        r["locations"]["used"][0]["parts"][0]["bbox"],
        serde_json::json!([0.27, 0.21, 0.53, 0.79])
    );
    // rascal's own location file is written as the contract location the run used.
    assert_eq!(r["invocation"]["action"], "remove");
    assert_eq!(
        r["invocation"]["arguments"]["location"],
        r["locations"]["used"][0]
    );
    assert_eq!(r["invocation"]["arguments"]["projection"], "flat");
    let bad = rascal(&[
        "run",
        "remove",
        image.to_str().unwrap(),
        "--location",
        repo_root()
            .join("fixtures/contract/locations/bbox-squint-finding.json")
            .to_str()
            .unwrap(),
        "--out",
        dir.join("bad").to_str().unwrap(),
    ]);
    assert_eq!(bad.status.code(), Some(64));
    let r = run_json(&dir.join("bad"));
    assert_eq!(r["status"], "invalid");
    assert_eq!(r["error"]["code"], "input");
    assert_eq!(r["error"]["retryable"], false);
    // A location on another image: the arguments did not resolve, so no invocation.
    assert_eq!(r["invocation"], serde_json::Value::Null);
    // Equal requests have equal identities; a different location does not.
    let again = rascal(&[
        "run",
        "remove",
        image.to_str().unwrap(),
        "--location",
        chairs.to_str().unwrap(),
        "--out",
        dir.join("again").to_str().unwrap(),
    ]);
    assert_eq!(again.status.code(), Some(0));
    assert_eq!(
        run_json(&dir.join("again"))["input_identity"],
        run_json(&dir.join("ok"))["input_identity"]
    );
    assert_ne!(
        r["input_identity"],
        run_json(&dir.join("ok"))["input_identity"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_out_that_held_something_is_left_untouched() {
    let dir = scratch("occupied");
    let occupied = dir.join("occupied");
    std::fs::create_dir_all(&occupied).unwrap();
    std::fs::write(occupied.join("keep.txt"), "mine").unwrap();
    let out = rascal(&[
        "run",
        "seam",
        repo_root()
            .join("fixtures/images/step-pano.png")
            .to_str()
            .unwrap(),
        "--out",
        occupied.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(!occupied.join("run.json").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A signal mid-fill (the slow fake sleeps three seconds): the run is `cancelled`, the exit
/// code is the shell's (143 for SIGTERM, 130 for SIGINT), and `run.json` names what it reached.
#[test]
fn a_signal_cancels_the_run_and_says_so() {
    let dir = scratch("signal");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let slow = priced(&dir, "slow", None);
    for (signal, code) in [("TERM", 143), ("INT", 130)] {
        let run = dir.join(format!("run-{signal}"));
        let mut child = Command::new(env!("CARGO_BIN_EXE_rascal"))
            .args([
                "heal",
                "seam",
                pano.to_str().unwrap(),
                "--filler",
                "fake",
                "--config",
                slow.to_str().unwrap(),
                "--out",
                run.to_str().unwrap(),
                "--no-dash",
            ])
            .env_remove("RASCAL_LOG")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        // Wait until the fill is under way: the call is journaled as started.
        let started = std::time::Instant::now();
        while !std::fs::read_to_string(run.join("journal.jsonl"))
            .is_ok_and(|j| j.contains("filler call started"))
        {
            assert!(
                started.elapsed().as_secs() < 30,
                "the run never reached the fill"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let kill = Command::new("kill")
            .args([format!("-{signal}"), child.id().to_string()])
            .status()
            .unwrap();
        assert!(kill.success());
        let status = child.wait().unwrap();
        assert_eq!(status.code(), Some(code), "SIG{signal}");
        let r = run_json(&run);
        assert_eq!(r["status"], "cancelled", "SIG{signal}");
        assert_eq!(r["error"], serde_json::Value::Null);
        assert_eq!(r["body"]["signal"], format!("SIG{signal}"));
        // Stopped mid-call: the charge is unresolved, counted at the price (task 9.1).
        assert_eq!(r["spend"]["usd"], 0.04, "SIG{signal}: {r:#}");
        assert_eq!(r["spend"]["unresolved_usd"], 0.04);
        assert_eq!(r["images"]["in"].as_array().unwrap().len(), 1);
        // Cancelled, it still records what it was asked (4.8).
        assert_eq!(r["invocation"]["action"], "seam", "SIG{signal}: {r:#}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Flags that do not resolve end the run `invalid` (exit 64) before it starts, so no signal can
/// cancel a run that has no invocation (4.8 allows a null one only on `invalid`). The image is a
/// FIFO: it is no file, so the flags do not resolve, and a run that went on regardless would
/// block reading it, a window in which a signal used to write `cancelled` with a null
/// invocation. The command must end by itself; if it hangs, it is signalled and the assertions
/// below say what it wrote.
#[test]
fn flags_that_do_not_resolve_never_start_a_run_a_signal_could_cancel() {
    let dir = scratch("unresolved-signal");
    let fifo = dir.join("pano.png");
    let made = Command::new("mkfifo").arg(&fifo).status().unwrap();
    assert!(made.success());
    let slow = priced(&dir, "slow", None);
    for command in ["run", "heal"] {
        let run = dir.join(format!("run-{command}"));
        let mut args = vec![command, "seam", fifo.to_str().unwrap()];
        if command == "heal" {
            args.extend(["--filler", "fake", "--no-dash", "--config"]);
            args.push(slow.to_str().unwrap());
        }
        args.extend(["--out", run.to_str().unwrap()]);
        let mut child = Command::new(env!("CARGO_BIN_EXE_rascal"))
            .args(&args)
            .env_remove("RASCAL_LOG")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed().as_secs() >= 20 {
                let _ = Command::new("kill")
                    .args(["-TERM", &child.id().to_string()])
                    .status();
                break child.wait().unwrap();
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        let r = run_json(&run);
        assert_eq!(r["status"], "invalid", "{command}: {r:#}");
        assert_eq!(r["invocation"], serde_json::Value::Null);
        assert_eq!(status.code(), Some(64), "{command}");
        // Nothing ran: the run directory holds its result and nothing else.
        let entries: Vec<_> = std::fs::read_dir(&run)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, ["run.json"], "{command}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Task 10.2: a command line's flags as the action's arguments: `--bbox` a contract location on
/// the image (the one the run used), `--set` the `set` object with its values typed, `--strict`
/// and `--projection` as given, and no path anywhere.
#[test]
fn a_command_lines_invocation_is_the_actions_arguments() {
    let dir = scratch("cli-invocation");
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let out = rascal(&[
        "run",
        "remove",
        image.to_str().unwrap(),
        "--bbox",
        "0.27,0.21,0.53,0.79",
        "--set",
        "grow.px=8",
        "--set",
        "feather.sigma_px=2.5",
        "--projection",
        "flat",
        "--strict",
        "--out",
        dir.join("run").to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r = run_json(&dir.join("run"));
    let invocation = &r["invocation"];
    assert_eq!(invocation["action"], "remove");
    assert_eq!(
        invocation["arguments"],
        serde_json::json!({
            "image": { "expect": r["images"]["in"][0]["image"] },
            "location": r["locations"]["used"][0],
            "set": { "grow.px": 8, "feather.sigma_px": 2.5 },
            "projection": "flat",
            "strict": true,
        }),
        "{r:#}"
    );
    assert_eq!(invocation["requirements"], serde_json::json!([]));
    assert_eq!(
        invocation["effective"]["config_identity"],
        rascal_core::config::Config::builtin().identity().as_str()
    );
    assert!(!invocation.to_string().contains(dir.to_str().unwrap()));
    assert!(!invocation.to_string().contains("fixtures/images"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Task 8.6: with `[cache] dir`, a repeated heal is answered from the cache: `cache: hit`,
/// no spend, the filler not called; the first paid the filler's price (`cache: miss`).
#[test]
fn a_repeated_heal_is_a_cache_hit_that_spends_nothing() {
    let dir = scratch("cache");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let config = dir.join("priced.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[cache]\ndir = {:?}\n[fillers.fake]\ncommand = [{:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\nrevision = \"fake\"\nspend_usd_per_call = 0.04\n",
            dir.join("fills").to_str().unwrap(),
            env!("CARGO_BIN_EXE_rascal")
        ),
    )
    .unwrap();
    let heal = |name: &str| {
        let out = rascal(&[
            "heal",
            "seam",
            pano.to_str().unwrap(),
            "--filler",
            "fake",
            "--config",
            config.to_str().unwrap(),
            "--out",
            dir.join(name).to_str().unwrap(),
            "--no-dash",
        ]);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        run_json(&dir.join(name))
    };
    let first = heal("first");
    assert_eq!(first["cache"], "miss");
    assert_eq!(first["spend"]["usd"], 0.04);
    let second = heal("second");
    assert_eq!(second["cache"], "hit");
    assert_eq!(second["spend"]["usd"], 0.0);
    assert_eq!(second["input_identity"], first["input_identity"]);
    let journal = std::fs::read_to_string(dir.join("second/journal.jsonl")).unwrap();
    assert!(journal.contains("fill from the cache"), "{journal}");
    assert!(
        !journal.contains("filler call: "),
        "the filler was not called"
    );
    // The healed images are the same pixels.
    assert_eq!(
        second["images"]["out"][1]["image"]["file_sha256"],
        first["images"]["out"][1]["image"]["file_sha256"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A run.json with what changes per run masked: times, the run's id and paths, temporary paths
/// in any string, so a stored fixture pins everything else.
fn masked(mut value: serde_json::Value, scratch: &Path) -> serde_json::Value {
    /// `"duration_ms":<digits>` inside a string (a journal note) as `"duration_ms":0`.
    fn durations(s: &str) -> String {
        let key = "\"duration_ms\":";
        let mut out = String::new();
        let mut rest = s;
        while let Some(i) = rest.find(key) {
            out.push_str(&rest[..i + key.len()]);
            rest = rest[i + key.len()..].trim_start_matches(|c: char| c.is_ascii_digit());
            out.push('0');
        }
        out.push_str(rest);
        out
    }
    fn walk(v: &mut serde_json::Value, scratch: &str) {
        match v {
            serde_json::Value::String(s) if s.contains(scratch) || s.contains("duration_ms") => {
                *s = durations(&s.replace(scratch, "<path>"));
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(|i| walk(i, scratch)),
            serde_json::Value::Object(map) => map.values_mut().for_each(|i| walk(i, scratch)),
            _ => {}
        }
    }
    let scratch = scratch.to_str().unwrap().to_string();
    walk(&mut value, &scratch);
    let root = repo_root().canonicalize().unwrap();
    walk(&mut value, root.to_str().unwrap());
    for key in ["started", "finished"] {
        value[key] = serde_json::json!("<time>");
    }
    value["duration_ms"] = serde_json::json!(0);
    value["run"]["id"] = serde_json::json!("<id>");
    // The configuration names the fake filler by this checkout's path, so its identity is
    // machine-specific; the built-in defaults' is the same everywhere.
    let builtin = rascal_core::config::Config::builtin().identity();
    if let Some(identity) = value.pointer_mut("/invocation/effective/config_identity")
        && identity.is_string()
        && identity != builtin.as_str()
    {
        *identity = serde_json::json!(format!("sha256:{}0001", "c".repeat(60)));
    }
    value["run"]["resource"] = serde_json::json!("rascal://run/<id>/run.json");
    value["tool"]["version"] = serde_json::json!("<version>");
    value
}

/// The conformance fixtures (table-manners 7): one run.json per status rascal produces from the
/// command line, masked, compared with `fixtures/contract/results/`; `UPDATE_FIXTURES=1`
/// rewrites them. Each is valid against the contract's result schema.
#[test]
fn the_conformance_fixtures_hold() {
    let dir = scratch("fixtures").canonicalize().unwrap();
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let exact = config(&dir, "exact");
    let failing = config(&dir, "failing");
    let heal = |name: &str, cfg: &Path| {
        rascal(&[
            "heal",
            "seam",
            pano.to_str().unwrap(),
            "--filler",
            "fake",
            "--config",
            cfg.to_str().unwrap(),
            "--out",
            dir.join(name).to_str().unwrap(),
            "--no-dash",
        ])
    };
    assert_eq!(heal("completed", &exact).status.code(), Some(0));
    assert_eq!(heal("error", &failing).status.code(), Some(3));
    let invalid = rascal(&[
        "run",
        "remove",
        image.to_str().unwrap(),
        "--location",
        repo_root()
            .join("fixtures/contract/locations/bbox-squint-finding.json")
            .to_str()
            .unwrap(),
        "--out",
        dir.join("invalid").to_str().unwrap(),
    ]);
    assert_eq!(invalid.status.code(), Some(64));
    let found = rascal(&[
        "run",
        "remove",
        image.to_str().unwrap(),
        "--location",
        repo_root()
            .join("fixtures/locations/chairs.json")
            .to_str()
            .unwrap(),
        "--out",
        dir.join("completed-remove").to_str().unwrap(),
    ]);
    assert_eq!(found.status.code(), Some(0));
    let stored = repo_root().join("fixtures/contract/results");
    for name in ["completed", "completed-remove", "error", "invalid"] {
        let value = masked(run_json(&dir.join(name)), &dir);
        let path = stored.join(format!("{name}.json"));
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::create_dir_all(&stored).unwrap();
            std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap() + "\n").unwrap();
        }
        let expected: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            value,
            expected,
            "{name}: UPDATE_FIXTURES=1 regenerates {}",
            path.display()
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
