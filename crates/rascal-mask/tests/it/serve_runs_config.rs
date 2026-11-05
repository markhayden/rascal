//! `[serve] runs` with `[workspace] dir`: a configuration alone names where `call` (and `serve`)
//! keep the store and the runs, relative to the file, so a host can launch `serve --config <file>`
//! and a terminal's `call --config <file>` lands in the same places. `--runs` still wins.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("rascal-runs-config-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

fn image() -> PathBuf {
    std::fs::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/images/floor.png"),
    )
    .unwrap()
}

/// `rascal call rect` with only `--config`: the image by path, allowed through its folder.
fn call(dir: &Path, extra: &[&str]) -> std::process::Output {
    let arguments = dir.join("args.json");
    std::fs::write(
        &arguments,
        format!(
            "{{\"image\":{{\"path\":{:?}}},\"bbox\":[0.25,0.25,0.75,0.75]}}",
            image().to_str().unwrap()
        ),
    )
    .unwrap();
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args([
            "call",
            "rect",
            "--arguments",
            arguments.to_str().unwrap(),
            "--config",
            dir.join("rascal.toml").to_str().unwrap(),
            "--allow-dir",
            image().parent().unwrap().to_str().unwrap(),
        ])
        .args(extra)
        .env("XDG_DATA_HOME", dir.join("xdg"))
        .env_remove("RASCAL_LOG")
        .output()
        .unwrap()
}

fn runs_in(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| {
                    e.path().is_dir()
                        && e.file_name().to_string_lossy().starts_with("run-")
                        && !e.file_name().to_string_lossy().contains('.')
                })
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn the_configuration_names_the_workspace_and_the_runs_relative_to_itself() {
    let dir = scratch("relative");
    std::fs::write(
        dir.join("rascal.toml"),
        "config_version = 1\n[workspace]\ndir = \"workspace\"\n[serve]\nruns = \"runs\"\n",
    )
    .unwrap();
    let output = call(&dir, &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        runs_in(&dir.join("runs")),
        1,
        "the run lands under [serve] runs"
    );
    assert!(
        dir.join("workspace").is_dir(),
        "the store is [workspace] dir"
    );
    assert_eq!(
        runs_in(&dir.join("xdg/rascal/runs")),
        0,
        "nothing goes to the default runs root"
    );
}

#[test]
fn the_runs_flag_wins_over_the_configuration() {
    let dir = scratch("flag");
    std::fs::write(
        dir.join("rascal.toml"),
        "config_version = 1\n[serve]\nruns = \"runs\"\n",
    )
    .unwrap();
    let output = call(&dir, &["--runs", dir.join("flagged").to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(runs_in(&dir.join("flagged")), 1);
    assert_eq!(runs_in(&dir.join("runs")), 0);
}
