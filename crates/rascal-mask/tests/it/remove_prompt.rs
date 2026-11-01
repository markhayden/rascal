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

fn config(dir: &Path) -> PathBuf {
    let path = dir.join("rascal.toml");
    let bin = env!("CARGO_BIN_EXE_rascal");
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[packs.fake]\ncommand = [{bin:?}, \"fake-pack\", \"exact\"]\nrevision = \"fake 1\"\n[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\nrevision = \"fake exact\"\n"
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
    // Without a pack, segment is not an op the run knows; the error says what to configure.
    let result = rascal(&[
        "run",
        "remove-prompt",
        image.to_str().unwrap(),
        "--prompt",
        "the chair",
        "--out",
        dir.join("c").to_str().unwrap(),
    ]);
    assert_ne!(result.status.code(), Some(0));
    assert!(
        text(&result.stderr).contains("unknown op segment")
            && text(&result.stderr).contains("needs a configured pack"),
        "{}",
        text(&result.stderr)
    );
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
