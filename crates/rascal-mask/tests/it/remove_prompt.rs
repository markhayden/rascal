//! Task 5.2's scenario through the fake pack: `rascal run remove-prompt <image> --prompt
//! "the chair" --config rascal.toml` binds the prompt into `segment`, selects every plane
//! and shapes the mask as `remove` does; `heal remove-prompt` heals it with the prompt kept
//! from the filler. Without `--prompt`, with `--prompt` on a recipe that takes none, or
//! without a pack, the run is refused naming what is missing.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rascal-remove-prompt-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rascal(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .env_remove("RASCAL_LOG")
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The fake pack behind a fake segmenter install (the launcher runs it, the model files are
/// there), so the call's readiness passes as it would for a real install (task 12.2).
fn config(dir: &Path) -> PathBuf {
    let path = dir.join("rascal.toml");
    let bin = env!("CARGO_BIN_EXE_rascal");
    let install =
        rascal_testkit::fake_install::write(&dir.join("pack"), &[bin, "fake-pack", "exact"], true)
            .unwrap();
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[packs.fake]\ncommand = {}\nrevision = \"fake 1\"\n[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\nrevision = \"fake exact\"\n",
            install.command_toml()
        ),
    )
    .unwrap();
    path
}

fn journal_step(dir: &Path, step: &str) -> serde_json::Value {
    let text = std::fs::read_to_string(dir.join("journal.jsonl")).unwrap();
    text.lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|v| v["step"] == step)
        .unwrap_or_else(|| panic!("no {step} step in the journal"))
}

#[test]
fn remove_prompt_binds_the_prompt_into_segment_and_heals() {
    let dir = scratch("flow");
    let config = config(&dir);
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let out = dir.join("run");
    let result = rascal(&[
        "run",
        "remove-prompt",
        image.to_str().unwrap(),
        "--prompt",
        "the chair",
        "--config",
        config.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0), "{}", text(&result.stderr));
    assert!(out.join("remove-prompt-grey.png").is_file());
    let segment = journal_step(&out, "segment");
    assert_eq!(segment["op"], "segment");
    assert_eq!(segment["params"]["prompt"], "the chair");
    assert_eq!(segment["params"]["max_instances"], 8);
    let select = journal_step(&out, "select");
    // The fake answers max_instances planes; select took every one.
    assert_eq!(select["inputs"].as_object().unwrap().len(), 8);
    // Through heal: the prompt is the finder's, not the filler's.
    let healed = dir.join("heal");
    let result = rascal(&[
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
        healed.to_str().unwrap(),
        "--no-dash",
    ]);
    assert_eq!(result.status.code(), Some(0), "{}", text(&result.stderr));
    assert!(healed.join("result.png").is_file());
    let journal = std::fs::read_to_string(healed.join("journal.jsonl")).unwrap();
    assert!(
        journal.contains("\"prompt\":\"the chair\""),
        "the finder saw the prompt"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_prompt_is_required_by_remove_prompt_and_refused_by_remove() {
    let dir = scratch("refusals");
    let config = config(&dir);
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let result = rascal(&[
        "run",
        "remove-prompt",
        image.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
        "--out",
        dir.join("a").to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(64), "{}", text(&result.stderr));
    assert!(
        text(&result.stderr).contains("needs inputs.prompt: give --prompt"),
        "{}",
        text(&result.stderr)
    );
    let result = rascal(&[
        "run",
        "remove",
        image.to_str().unwrap(),
        "--bbox",
        "0.2,0.2,0.6,0.8",
        "--prompt",
        "the chair",
        "--out",
        dir.join("b").to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(64), "{}", text(&result.stderr));
    assert!(
        text(&result.stderr).contains("declares no inputs.prompt; --prompt does not apply"),
        "{}",
        text(&result.stderr)
    );
    // Without a configuration the call is not ready: refused before its run, naming what to
    // set up (task 12.2).
    let result = rascal(&[
        "run",
        "remove-prompt",
        image.to_str().unwrap(),
        "--prompt",
        "the chair",
        "--out",
        dir.join("c").to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(64), "{}", text(&result.stderr));
    assert!(
        text(&result.stderr).contains("not ready: config rascal.toml missing")
            && text(&result.stderr).contains("dependency packs.rascal-tools missing"),
        "{}",
        text(&result.stderr)
    );
    assert!(!dir.join("c").exists(), "no run directory");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn set_overrides_a_step_parameter_for_one_run_and_refuses_a_bad_one() {
    let dir = scratch("set");
    let config = config(&dir);
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let out = dir.join("run");
    let result = rascal(&[
        "run",
        "remove-prompt",
        image.to_str().unwrap(),
        "--prompt",
        "the chair",
        "--set",
        "segment.max_instances=1",
        "--set",
        "grow.px=9",
        "--config",
        config.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0), "{}", text(&result.stderr));
    let segment = journal_step(&out, "segment");
    assert_eq!(segment["params"]["max_instances"], 1);
    assert_eq!(journal_step(&out, "grow")["params"]["px"], 9.0);
    assert_eq!(
        journal_step(&out, "select")["inputs"]
            .as_object()
            .unwrap()
            .len(),
        1,
        "one plane selected"
    );
    let journal = std::fs::read_to_string(out.join("journal.jsonl")).unwrap();
    assert!(journal.contains("override from --set: segment.max_instances = 1"));
    assert!(journal.contains("override from --set: grow.px = 9"));
    for (set, expect) in [
        ("nonsense", "--set takes <step>.<param>=<value>"),
        ("threshold=0.4", "the key is <step>.<param>"),
        ("segment.bogus=1", "segment has no parameter bogus"),
        ("nope.px=1", "no step nope in the recipe"),
        ("fan.constants=fan-v9", "no step fan in the recipe"),
    ] {
        let result = rascal(&[
            "run",
            "remove-prompt",
            image.to_str().unwrap(),
            "--prompt",
            "the chair",
            "--set",
            set,
            "--config",
            config.to_str().unwrap(),
            "--out",
            dir.join("bad").to_str().unwrap(),
        ]);
        assert_eq!(
            result.status.code(),
            Some(64),
            "{set}: {}",
            text(&result.stderr)
        );
        assert!(
            text(&result.stderr).contains(expect),
            "{set}: {}",
            text(&result.stderr)
        );
        let _ = std::fs::remove_dir_all(dir.join("bad"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// --- choosing the segmenter (SPEC.md "Choosable segmenters", task 14.10) ------------------------

/// A config over the fake pack that declares `model` (`models`), with `[segment] model` when
/// given; the fake install has `tiny` installed and `sam3` catalogued but not installed.
fn models_config(dir: &Path, segment: Option<&str>) -> PathBuf {
    let path = dir.join("rascal.toml");
    let bin = env!("CARGO_BIN_EXE_rascal");
    let install =
        rascal_testkit::fake_install::write(&dir.join("pack"), &[bin, "fake-pack", "models"], true)
            .unwrap();
    let segment = segment
        .map(|m| format!("[segment]\nmodel = \"{m}\"\n"))
        .unwrap_or_default();
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n{segment}[packs.fake]\ncommand = {}\nrevision = \"fake 1\"\n",
            install.command_toml()
        ),
    )
    .unwrap();
    path
}

fn run_prompt(config: &Path, out: &Path, extra: &[&str]) -> Output {
    let image = repo_root().join("fixtures/images/chair-blob.png");
    let mut args = vec![
        "run",
        "remove-prompt",
        image.to_str().unwrap(),
        "--prompt",
        "the chair",
        "--config",
        config.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    rascal(&args)
}

#[test]
fn the_model_is_the_calls_else_the_configs_else_the_packs_default() {
    let dir = scratch("model-order");
    // No call, no [segment]: the pack's default, passed explicitly.
    let config = models_config(&dir, None);
    let out = dir.join("default");
    let r = run_prompt(&config, &out, &[]);
    assert_eq!(r.status.code(), Some(0), "{}", text(&r.stderr));
    assert_eq!(journal_step(&out, "segment")["params"]["model"], "tiny");
    // [segment] model names one that is not installed: refused before anything runs, naming the
    // command that installs it.
    let config = models_config(&dir.join("cfg"), Some("sam3"));
    let out = dir.join("configured");
    let r = run_prompt(&config, &out, &[]);
    assert_eq!(r.status.code(), Some(64), "{}", text(&r.stderr));
    let err = text(&r.stderr);
    assert!(
        err.contains("segmenter sam3 is not installed") && err.contains("rascal tools get sam3"),
        "{err}"
    );
    assert!(
        !out.join("journal.jsonl").is_file()
            || !std::fs::read_to_string(out.join("journal.jsonl"))
                .unwrap()
                .contains("\"step\":\"segment\"")
    );
    // The call wins over [segment].
    let out = dir.join("call");
    let r = run_prompt(&config, &out, &["--model", "tiny"]);
    assert_eq!(r.status.code(), Some(0), "{}", text(&r.stderr));
    assert_eq!(journal_step(&out, "segment")["params"]["model"], "tiny");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unknown_model_and_a_step_level_model_are_refused() {
    let dir = scratch("model-refused");
    let config = models_config(&dir, None);
    let r = run_prompt(&config, &dir.join("a"), &["--model", "giraffe"]);
    assert_eq!(r.status.code(), Some(64));
    assert!(
        text(&r.stderr)
            .contains("model \"giraffe\" is not a catalogued segmenter (have tiny, sam3)"),
        "{}",
        text(&r.stderr)
    );
    // An override cannot set it on the step: the only way in is inputs.model.
    let r = run_prompt(&config, &dir.join("b"), &["--set", "segment.model=tiny"]);
    assert_eq!(r.status.code(), Some(64));
    assert!(
        text(&r.stderr).contains("a step cannot set model itself"),
        "{}",
        text(&r.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_pack_without_model_keeps_working_and_refuses_one() {
    let dir = scratch("model-none");
    let config = config(&dir);
    let out = dir.join("plain");
    let r = run_prompt(&config, &out, &[]);
    assert_eq!(r.status.code(), Some(0), "{}", text(&r.stderr));
    assert!(
        journal_step(&out, "segment")["params"]
            .get("model")
            .is_none()
    );
    let r = run_prompt(&config, &dir.join("named"), &["--model", "tiny"]);
    assert_eq!(r.status.code(), Some(64));
    assert!(
        text(&r.stderr).contains("takes no model"),
        "{}",
        text(&r.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_parameter_the_model_does_not_use_is_left_out_and_refused_when_given() {
    let dir = scratch("model-applies");
    let config = models_config(&dir, None);
    // Install a fake sam3 so it can run.
    let models = dir.join("pack/models");
    std::fs::create_dir_all(models.join("sam3/abc")).unwrap();
    std::fs::write(models.join("sam3/current"), "abc\n").unwrap();
    let out = dir.join("sam3");
    let r = run_prompt(&config, &out, &["--model", "sam3"]);
    assert_eq!(r.status.code(), Some(0), "{}", text(&r.stderr));
    let params = &journal_step(&out, "segment")["params"];
    assert_eq!(params["model"], "sam3");
    assert!(
        params.get("pad").is_none(),
        "pad does not apply to sam3: {params}"
    );
    assert!(params.get("threshold").is_some());
    // Tiny keeps pad.
    let out = dir.join("tiny");
    let r = run_prompt(&config, &out, &[]);
    assert_eq!(r.status.code(), Some(0), "{}", text(&r.stderr));
    assert!(journal_step(&out, "segment")["params"].get("pad").is_some());
    // Given explicitly for sam3, even at its default, it is refused.
    let r = run_prompt(
        &config,
        &dir.join("given"),
        &["--model", "sam3", "--set", "segment.pad=0.05"],
    );
    assert_ne!(r.status.code(), Some(0));
    assert!(
        text(&r.stderr).contains("pad does not apply with model sam3"),
        "{}",
        text(&r.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_run_holds_its_installation_names_it_on_every_call_and_releases_it() {
    let dir = scratch("model-hold");
    let config = models_config(&dir, None);
    let models = dir.join("pack/models");
    let installation = rascal_testkit::fake_install::INSTALLATION;
    std::fs::write(
        models.join("tiny").join(installation).join("install.json"),
        "{\"model\":\"tiny\"}",
    )
    .unwrap();
    let out = dir.join("run");
    let r = run_prompt(&config, &out, &[]);
    assert_eq!(r.status.code(), Some(0), "{}", text(&r.stderr));
    // The segment call named the installation held at admission.
    assert_eq!(
        journal_step(&out, "segment")["params"]["installation"],
        installation
    );
    // The evidence stays with the run.
    assert_eq!(
        std::fs::read_to_string(out.join("segmenter-install.json")).unwrap(),
        "{\"model\":\"tiny\"}"
    );
    // The hold was released when the run ended.
    let held = models.join("tiny").join(installation).join(".held");
    let left: Vec<_> = std::fs::read_dir(&held)
        .map(|d| d.flatten().collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "holds left: {left:?}");
    // Replay answers from the recording with the model removed: the installation is not part
    // of a pack request's identity, and nothing is held or checked on disk.
    std::fs::remove_dir_all(models.join("tiny")).unwrap();
    let again = dir.join("replay");
    let r = run_prompt(&config, &again, &["--replay", out.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(0), "{}", text(&r.stderr));
    let _ = std::fs::remove_dir_all(&dir);
}
