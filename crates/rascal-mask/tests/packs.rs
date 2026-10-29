//! Task 3.3: the pack client against the fake pack (`rascal fake-pack <behaviour>`), through
//! the shell (`rascal tools list|call`, `rascal run`) and through `rascal_mcp::client::Pack`
//! directly: every bad answer refused before anything is stored, the concurrency AND rule,
//! timeout and cancellation failing every in-flight call and shutting the pack down, a late
//! answer ignored, process death, stderr redacted, a relocated run replaying with the pack
//! absent, the span, the run page row and the `segment` export.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rascal_core::config::{Accepts, PackConfig};
use rascal_core::journal::Journal;
use rascal_mcp::client::Pack;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn floor() -> PathBuf {
    repo_root().join("fixtures/images/floor.png")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-packs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rascal_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command.args(args).env_remove("RASCAL_LOG");
    for (k, v) in env {
        command.env(k, v);
    }
    command.output().unwrap()
}

fn rascal(args: &[&str]) -> Output {
    rascal_env(args, &[])
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The first JSON document in a command's output (the envelope, before the trailing lines).
fn first_json(stdout: &str) -> serde_json::Value {
    let from = stdout.find('{').unwrap();
    serde_json::Deserializer::from_str(&stdout[from..])
        .into_iter::<serde_json::Value>()
        .next()
        .unwrap()
        .unwrap()
}

/// The fake's command line for a behaviour.
fn fake_command(behaviour: &str, concurrent: bool) -> Vec<String> {
    let mut command = vec![
        env!("CARGO_BIN_EXE_rascal").to_string(),
        "fake-pack".to_string(),
        behaviour.to_string(),
    ];
    if concurrent {
        command.push("--concurrent".to_string());
    }
    command
}

/// A configuration file with the fake as `[packs.fake]`.
fn config(dir: &Path, behaviour: &str, call_timeout_s: u64, extra: &str) -> PathBuf {
    let path = dir.join(format!("rascal-{behaviour}.toml"));
    let command: Vec<String> = fake_command(behaviour, false)
        .iter()
        .map(|c| format!("{c:?}"))
        .collect();
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[fillers.hosted]\nurl = \"https://example.invalid/x\"\nprofile = \"binary\"\nkey_env = \"HOSTED_FILLER_KEY\"\n[packs.fake]\ncommand = [{}]\nenv = [\"RASCAL_TOOLS_MODELS\", \"FAKE_SECRET_KEY\", \"HOSTED_FILLER_KEY\"]\nrevision = \"fake 1\"\ncall_timeout_s = {call_timeout_s}\n{extra}\n",
            command.join(", ")
        ),
    )
    .unwrap();
    path
}

fn pack_config(behaviour: &str, concurrent: bool, call_timeout_s: u64) -> PackConfig {
    PackConfig {
        command: fake_command(behaviour, concurrent),
        env: Vec::new(),
        accepts: Accepts::Base64,
        concurrent,
        revision: Some("fake 1".into()),
        startup_timeout_s: 30,
        call_timeout_s,
    }
}

fn pack(behaviour: &str, concurrent: bool, call_timeout_s: u64) -> Pack {
    Pack::new(
        "fake",
        pack_config(behaviour, concurrent, call_timeout_s),
        Vec::new(),
    )
    .unwrap()
    .with_grace(Duration::from_secs(3))
}

/// The arguments of a direct `segment` call through the client: a 4x4 image as `_harness`.
fn segment_args(delay_ms: u64) -> serde_json::Map<String, serde_json::Value> {
    let image =
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4, 4, image::Rgb([1, 2, 3])));
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    let args = serde_json::json!({
        "image": "img-1", "prompt": "x", "max_instances": 1, "delay_ms": delay_ms,
        "_harness": {
            "context_version": 1, "run_id": "test", "call_id": "segment-1",
            "images": [{ "port": "image", "id": "img-1", "identity": "sha256:0", "frame": "raw:4x4:flat",
                         "width": 4, "height": 4, "base64": rascal_core::panel::base64(&bytes),
                         "record": {} }],
            "frames": []
        }
    });
    args.as_object().unwrap().clone()
}

fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn image_count(ws: &Path) -> usize {
    let out = rascal(&[
        "workspace",
        "list",
        ws.to_str().unwrap(),
        "--type",
        "Image",
        "--json",
    ]);
    serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout)
        .unwrap()
        .len()
}

fn notes(journal: &Path, prefix: &str) -> Vec<String> {
    Journal::read(journal)
        .unwrap()
        .into_iter()
        .filter_map(|l| l.note)
        .filter(|n| n.starts_with(prefix))
        .collect()
}

// --- through the shell -------------------------------------------------------------------

#[test]
fn tools_list_prints_the_manifest_and_tools_call_stores_two_planes() {
    let dir = scratch("shell");
    let cfg = config(&dir, "exact", 30, "");
    let out = rascal(&["tools", "list", "--config", cfg.to_str().unwrap()]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let listed = text(&out.stdout);
    assert!(
        listed.contains("segment 0.1.0") && listed.contains("out region (Location[], required)"),
        "{listed}"
    );
    assert!(
        listed.contains("--prompt (String") && listed.contains("concurrent false"),
        "{listed}"
    );
    let out = rascal(&["tools", "list", "--config", cfg.to_str().unwrap(), "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["fake"][0]["name"], "segment");
    assert_eq!(json["fake"][0]["outputs"][1]["many"], true);

    let ws = dir.join("ws");
    let out = rascal(&[
        "tools",
        "call",
        "fake",
        "segment",
        "--config",
        cfg.to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
        "--image",
        floor().to_str().unwrap(),
        "--prompt",
        "the chair",
        "--json",
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    for name in ["plane_0:", "plane_1:", "region_0:", "region_1:"] {
        assert!(stdout.contains(name), "{stdout}");
    }
    let envelope: serde_json::Value = first_json(&stdout);
    assert_eq!(envelope["status"], "ok");
    assert_eq!(envelope["regions"][0]["label"], "the chair");
    assert_eq!(envelope["regions"][0]["location"]["kind"], "polygon");
    assert_eq!(envelope["images"].as_array().unwrap().len(), 2);
    assert!(
        envelope["images"][0]["identity"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert_eq!(envelope["artifacts"]["plane_1"]["artifact_type"], "Image");
    // The canonical image plus the two planes are in the workspace; the journal has the
    // manifest, the call and the stored call.
    assert_eq!(image_count(&ws), 3);
    let journal = ws.join("journal.jsonl");
    assert_eq!(notes(&journal, "pack manifest: ").len(), 1);
    let calls = notes(&journal, "pack call: ");
    assert_eq!(calls.len(), 1);
    let call: serde_json::Value =
        serde_json::from_str(calls[0].strip_prefix("pack call: ").unwrap()).unwrap();
    assert_eq!(call["status"], "ok");
    assert_eq!(call["tool"], "segment");
    assert!(
        call["request_bytes"].as_u64().unwrap() > 1000,
        "the harness carried the image"
    );
    assert_eq!(notes(&journal, "pack stored: ").len(), 1);
    // Three instances: six outputs.
    let out = rascal(&[
        "tools",
        "call",
        "fake",
        "segment",
        "--config",
        cfg.to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
        "--image",
        "img-1",
        "--max_instances",
        "3",
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("plane_2:"));
    // An unknown pack and an unknown tool are usage errors naming what exists.
    let out = rascal(&[
        "tools",
        "call",
        "nope",
        "segment",
        "--config",
        cfg.to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(text(&out.stderr).contains("the packs are fake"));
    let out = rascal(&[
        "tools",
        "call",
        "fake",
        "nope",
        "--config",
        cfg.to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(text(&out.stderr).contains("its tools are segment"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A behaviour whose answer must be refused: exit 3, the message names it, nothing stored.
fn refused(behaviour: &str, expect: &str) {
    let dir = scratch(behaviour);
    let cfg = config(&dir, behaviour, 30, "");
    let ws = dir.join("ws");
    let out = rascal(&[
        "tools",
        "call",
        "fake",
        "segment",
        "--config",
        cfg.to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
        "--image",
        floor().to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("answer rejected") && stderr.contains(expect),
        "{stderr}"
    );
    assert_eq!(image_count(&ws), 1, "only the canonical image is stored");
    let journal = ws.join("journal.jsonl");
    assert_eq!(
        notes(&journal, "pack call: ").len(),
        1,
        "the call itself is journaled"
    );
    assert!(notes(&journal, "pack stored: ").is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_schema_failure_is_refused() {
    refused("bad-schema", "does not parse");
}

#[test]
fn a_bounds_breach_is_refused() {
    refused("over-bounds", "more than 8 regions");
}

#[test]
fn a_block_mismatch_is_refused() {
    refused("block-mismatch", "3 image blocks for 2 images");
}

#[test]
fn a_foreign_frame_reference_is_refused() {
    refused("foreign-frame", "which the call did not pass");
}

#[test]
fn a_plane_of_the_wrong_size_is_refused() {
    refused("wrong-size", "but images[0] says");
}

#[test]
fn a_non_png_block_is_refused() {
    refused("non-png", "is image/jpeg, not image/png");
}

#[test]
fn stderr_is_journaled_with_credentials_redacted() {
    let dir = scratch("stderr");
    let cfg = config(&dir, "stderr", 30, "");
    let ws = dir.join("ws");
    let out = rascal(&[
        "tools",
        "call",
        "fake",
        "segment",
        "--config",
        cfg.to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
        "--image",
        floor().to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let lines = notes(&ws.join("journal.jsonl"), "pack stderr: ");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("token=[redacted]") && lines[0].contains("Bearer [redacted]"),
        "{}",
        lines[0]
    );
    assert!(!lines[0].contains("SECRET"), "{}", lines[0]);
    let whole = std::fs::read_to_string(ws.join("journal.jsonl")).unwrap();
    assert!(!whole.contains("SECRET-"), "no secret reaches the journal");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_pack_that_speaks_another_revision_or_lists_no_annotation_is_refused_at_start() {
    let dir = scratch("refused-start");
    for (behaviour, expect) in [
        ("old-version", "speaks revision 2025-06-18"),
        ("no-annotation", "has no rascal"),
    ] {
        let cfg = config(&dir, behaviour, 30, "");
        let out = rascal(&["tools", "list", "--config", cfg.to_str().unwrap()]);
        assert_eq!(
            out.status.code(),
            Some(3),
            "{behaviour}: {}",
            text(&out.stderr)
        );
        assert!(
            text(&out.stderr).contains(expect),
            "{behaviour}: {}",
            text(&out.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// --- through the client directly -----------------------------------------------------------

fn call_in_thread(pack: &Arc<Pack>, delay_ms: u64) -> std::thread::JoinHandle<Result<(), String>> {
    let pack = pack.clone();
    std::thread::spawn(move || {
        pack.call("segment", segment_args(delay_ms))
            .result
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

fn two_calls(pack: Arc<Pack>, delay_ms: u64) -> Duration {
    pack.tools().unwrap();
    let started = Instant::now();
    let a = call_in_thread(&pack, delay_ms);
    let b = call_in_thread(&pack, delay_ms);
    a.join().unwrap().unwrap();
    b.join().unwrap().unwrap();
    started.elapsed()
}

#[test]
fn serial_when_the_config_says_serial() {
    let mut config = pack_config("exact", true, 30);
    config.concurrent = false;
    let pack = Arc::new(Pack::new("fake", config, Vec::new()).unwrap());
    let elapsed = two_calls(pack, 400);
    assert!(elapsed >= Duration::from_millis(800), "{elapsed:?}");
}

#[test]
fn serial_when_the_manifest_says_serial() {
    let mut config = pack_config("exact", false, 30);
    config.concurrent = true;
    let pack = Arc::new(Pack::new("fake", config, Vec::new()).unwrap());
    let elapsed = two_calls(pack, 400);
    assert!(elapsed >= Duration::from_millis(800), "{elapsed:?}");
}

#[test]
fn concurrent_when_both_agree() {
    let mut config = pack_config("exact", true, 30);
    config.concurrent = true;
    let pack = Arc::new(Pack::new("fake", config, Vec::new()).unwrap());
    let elapsed = two_calls(pack, 400);
    assert!(elapsed < Duration::from_millis(750), "{elapsed:?}");
}

#[test]
fn a_timeout_fails_every_in_flight_call_and_shuts_the_pack_down() {
    let mut config = pack_config("exact", true, 1);
    config.concurrent = true;
    let pack = Arc::new(
        Pack::new("fake", config, Vec::new())
            .unwrap()
            .with_grace(Duration::from_secs(3)),
    );
    pack.tools().unwrap();
    let pid = pack.pid().unwrap();
    let started = Instant::now();
    let a = call_in_thread(&pack, 1500);
    let b = call_in_thread(&pack, 8000);
    let ea = a.join().unwrap().unwrap_err();
    let eb = b.join().unwrap().unwrap_err();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert!(
        ea.contains("no answer within call_timeout_s = 1")
            || ea.contains("another call on the pack failed"),
        "{ea}"
    );
    assert!(
        eb.contains("no answer within call_timeout_s = 1")
            || eb.contains("another call on the pack failed"),
        "{eb}"
    );
    assert!(
        ea.contains("no answer within") || eb.contains("no answer within"),
        "{ea} / {eb}"
    );
    assert!(!pack.is_live(), "the pack is down before any reuse");
    assert!(!alive(pid), "the process is gone");
    // The next call starts a fresh process and succeeds.
    let outcome = pack.call("segment", segment_args(0));
    outcome.result.unwrap();
    assert_eq!(outcome.call.status, "ok");
    assert_ne!(pack.pid().unwrap(), pid);
}

#[test]
fn a_late_answer_for_a_tombstoned_call_is_ignored() {
    // The fake's sleep ignores cancellation, so an answer 1.5 s in arrives after the 1 s
    // timeout tombstoned the call: it must go nowhere.
    let pack = pack("exact", false, 1);
    pack.tools().unwrap();
    let pid = pack.pid().unwrap();
    let outcome = pack.call("segment", segment_args(1500));
    assert_eq!(outcome.call.status, "timeout", "{}", outcome.call.outcome);
    assert!(!pack.is_live());
    assert!(!alive(pid));
    // The late answer went nowhere; the restarted pack answers the next call.
    let outcome = pack.call("segment", segment_args(0));
    outcome.result.unwrap();
    assert_eq!(outcome.call.status, "ok");
}

#[test]
fn process_death_is_an_operational_error_and_the_next_call_restarts() {
    let pack = pack("die", false, 10);
    pack.tools().unwrap();
    let pid = pack.pid().unwrap();
    let outcome = pack.call("segment", segment_args(0));
    let e = outcome.result.unwrap_err().to_string();
    assert!(e.contains("went away"), "{e}");
    assert_eq!(outcome.call.status, "died");
    assert!(!pack.is_live());
    assert!(!alive(pid));
    let outcome = pack.call("segment", segment_args(0));
    assert!(outcome.result.is_err());
    assert_ne!(
        pack.pid(),
        Some(pid),
        "a fresh process was started for the retry"
    );
}

#[test]
fn a_cancellation_fails_in_flight_calls_with_shutdown_completed_before_the_next_use() {
    let mut config = pack_config("exact", true, 30);
    config.concurrent = true;
    let pack = Arc::new(
        Pack::new("fake", config, Vec::new())
            .unwrap()
            .with_grace(Duration::from_secs(3)),
    );
    pack.tools().unwrap();
    let pid = pack.pid().unwrap();
    let a = call_in_thread(&pack, 5000);
    let b = call_in_thread(&pack, 5000);
    std::thread::sleep(Duration::from_millis(400));
    let started = Instant::now();
    pack.fail_in_flight("cancelled by the test");
    assert!(
        !pack.is_live(),
        "shutdown completed before fail_in_flight returned"
    );
    assert!(!alive(pid));
    let ea = a.join().unwrap().unwrap_err();
    let eb = b.join().unwrap().unwrap_err();
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "{:?}",
        started.elapsed()
    );
    for e in [&ea, &eb] {
        assert!(
            e.contains("another call on the pack failed") || e.contains("cancelled"),
            "{e}"
        );
    }
    let outcome = pack.call("segment", segment_args(0));
    outcome.result.unwrap();
    assert_ne!(pack.pid().unwrap(), pid);
}

// --- a recipe through the pack: replay, span, page, export -------------------------------

#[test]
fn a_relocated_run_replays_with_the_pack_absent_and_no_call_made() {
    let dir = scratch("replay");
    let cfg = config(&dir, "exact", 30, "");
    let recipe = dir.join("segment.toml");
    std::fs::write(&recipe, rascal_testkit::fake_pack::RECIPE).unwrap();
    let run1 = dir.join("run1");
    let out = rascal_env(
        &[
            "run",
            recipe.to_str().unwrap(),
            floor().to_str().unwrap(),
            "--out",
            run1.to_str().unwrap(),
            "--config",
            cfg.to_str().unwrap(),
        ],
        &[("RASCAL_LOG", "debug")],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let mask1 = text(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("mask: ").map(str::to_string))
        .unwrap();
    // The span: the pack call with its request size and status, on stderr only.
    let log = text(&out.stderr);
    assert!(
        log.contains("pack call") && log.contains("request_bytes=") && log.contains("status=ok"),
        "{log}"
    );
    assert!(!text(&out.stdout).contains("request_bytes"));
    // The run page has the pack call row.
    let page = dir.join("page.html");
    let out = rascal(&[
        "dash",
        run1.to_str().unwrap(),
        "--out",
        page.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let html = std::fs::read_to_string(&page).unwrap();
    assert!(
        html.contains("data-call-kind=\"pack\"") && html.contains("fake segment"),
        "no pack row"
    );
    // The compatibility export of the real segment envelope validates (export validates
    // before writing) and carries the polygon regions.
    let exported = dir.join("segment.json");
    let out = rascal(&[
        "export",
        "--compat",
        run1.join("journal.jsonl").to_str().unwrap(),
        "--step",
        "segment",
        "--out",
        exported.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let e: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&exported).unwrap()).unwrap();
    assert_eq!(e["regions"][0]["location"]["kind"], "polygon");
    assert_eq!(e["images"].as_array().unwrap().len(), 2);
    assert!(e.get("artifacts").is_none());

    // Relocate the run, point the config at a pack that does not exist, replay.
    let moved = dir.join("moved");
    let status = Command::new("cp")
        .args(["-R", run1.to_str().unwrap(), moved.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success());
    let absent = dir.join("rascal-absent.toml");
    std::fs::write(
        &absent,
        std::fs::read_to_string(&cfg)
            .unwrap()
            .replace("fake-pack", "no-such-pack")
            .replace(env!("CARGO_BIN_EXE_rascal"), "/nonexistent/rascal"),
    )
    .unwrap();
    let run2 = dir.join("run2");
    let out = rascal(&[
        "run",
        recipe.to_str().unwrap(),
        floor().to_str().unwrap(),
        "--out",
        run2.to_str().unwrap(),
        "--config",
        absent.to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "without --replay the absent pack fails: {}",
        text(&out.stderr)
    );
    assert!(text(&out.stderr).contains("could not start"));
    let run3 = dir.join("run3");
    let out = rascal(&[
        "run",
        recipe.to_str().unwrap(),
        floor().to_str().unwrap(),
        "--out",
        run3.to_str().unwrap(),
        "--config",
        absent.to_str().unwrap(),
        "--replay",
        moved.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let mask3 = text(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("mask: ").map(str::to_string))
        .unwrap();
    assert_eq!(
        Path::new(&mask1).file_name(),
        Path::new(&mask3).file_name(),
        "identical mask identity"
    );
    let journal3 = run3.join("journal.jsonl");
    assert!(
        notes(&journal3, "pack call: ").is_empty(),
        "no call was made"
    );
    assert!(notes(&journal3, "pack manifest: ").is_empty());
    assert_eq!(notes(&journal3, "pack call replayed").len(), 1);
    // A request the recording does not hold is an error naming it, not a call.
    let other = dir.join("other.toml");
    std::fs::write(
        &other,
        rascal_testkit::fake_pack::RECIPE.replace("the chair", "the lamp"),
    )
    .unwrap();
    let run4 = dir.join("run4");
    let out = rascal(&[
        "run",
        other.to_str().unwrap(),
        floor().to_str().unwrap(),
        "--out",
        run4.to_str().unwrap(),
        "--config",
        absent.to_str().unwrap(),
        "--replay",
        moved.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("no recorded call"),
        "{}",
        text(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
