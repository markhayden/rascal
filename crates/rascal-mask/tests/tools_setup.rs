//! Task 5.3's command: `rascal tools setup` refuses a tampered bundle with exit 3 naming the
//! file, a missing bundle with exit 64, a bundle for another platform with exit 64, and asks
//! for a destination, refuses a pack command with no directory before `.venv` without
//! touching the current directory, and refuses a machine without uv before writing; with
//! `RASCAL_BUNDLE` naming a bundle built by `scripts/build-bundle.sh` for this machine, it
//! installs into a clean directory with no network and the installed pack answers
//! `echo_region` (skipped with a note outside CI, a failure in CI).
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rascal_mcp::provision::{host_platform, sha256_bytes};

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("rascal-tools-setup-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rascal(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command.args(args).env_remove("RASCAL_LOG");
    for (k, v) in env {
        command.env(k, v);
    }
    command.output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A bundle with correct digests for `platform`, whose files are not a real pack.
fn synthetic(root: &Path, platform: &str) {
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("MODEL.md", b"# model\n".to_vec()),
        ("pyproject.toml", b"[project]\nname = \"x\"\n".to_vec()),
        ("uv.lock", b"version = 1\n".to_vec()),
        ("wheel/rascal_tools-0.1.0-py3-none-any.whl", vec![1, 2, 3]),
        ("wheelhouse/dep-1.0-py3-none-any.whl", vec![4, 5]),
    ];
    let mut manifest = format!(
        "bundle_version = 1\npack = \"rascal-tools\"\npack_version = \"0.1.0\"\npython = \"3.12\"\nplatform = \"{platform}\"\nwheel = \"wheel/rascal_tools-0.1.0-py3-none-any.whl\"\n\n[files]\n"
    );
    for (rel, bytes) in &files {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        manifest.push_str(&format!("\"{rel}\" = \"{}\"\n", sha256_bytes(bytes)));
    }
    std::fs::write(root.join("bundle.toml"), manifest).unwrap();
}

#[test]
fn setup_refuses_a_tampered_bundle_a_missing_one_another_platform_and_no_destination() {
    let dir = scratch("refusals");
    let host = host_platform().unwrap_or("linux-x86_64");
    let bundle = dir.join("bundle");
    synthetic(&bundle, host);
    let into = dir.join("into");
    // No destination.
    let out = rascal(
        &["tools", "setup", "--bundle", bundle.to_str().unwrap()],
        &[],
    );
    assert_eq!(out.status.code(), Some(64), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("--into"));
    // Tampered: exit 3, the file named, nothing installed.
    std::fs::write(bundle.join("wheelhouse/dep-1.0-py3-none-any.whl"), [4, 6]).unwrap();
    let out = rascal(
        &[
            "tools",
            "setup",
            "--bundle",
            bundle.to_str().unwrap(),
            "--into",
            into.to_str().unwrap(),
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
    let err = text(&out.stderr);
    assert!(
        err.contains("dep-1.0-py3-none-any.whl") && err.contains("tampered"),
        "{err}"
    );
    assert!(
        !into.exists(),
        "nothing is written before verification passes"
    );
    // Another platform: exit 64.
    let other = match host {
        "macos-arm64" => "linux-x86_64",
        _ => "macos-arm64",
    };
    let foreign = dir.join("foreign");
    synthetic(&foreign, other);
    let out = rascal(
        &[
            "tools",
            "setup",
            "--bundle",
            foreign.to_str().unwrap(),
            "--into",
            into.to_str().unwrap(),
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(64), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains(other));
    // Missing: exit 64.
    let out = rascal(
        &[
            "tools",
            "setup",
            "--bundle",
            dir.join("nowhere").to_str().unwrap(),
            "--into",
            into.to_str().unwrap(),
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(64), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("not a bundle"));
    // A config whose pack entry points nowhere installable, and one whose command is a bare
    // `.venv/bin/rascal-tools` (no directory before .venv): refused, and the current
    // directory is not written to.
    let cfg = dir.join("rascal.toml");
    for command in ["rascal-tools", ".venv/bin/rascal-tools"] {
        std::fs::write(
            &cfg,
            format!(
                "config_version = 1\n[packs.rascal-tools]\ncommand = [{command:?}, \"serve\"]\n"
            ),
        )
        .unwrap();
        let cwd = dir.join("cwd");
        std::fs::create_dir_all(&cwd).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_rascal"))
            .args([
                "tools",
                "setup",
                "--bundle",
                bundle.to_str().unwrap(),
                "--config",
                cfg.to_str().unwrap(),
            ])
            .current_dir(&cwd)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(64),
            "{command}: {}",
            text(&out.stderr)
        );
        assert!(
            text(&out.stderr).contains("--into"),
            "{command}: {}",
            text(&out.stderr)
        );
        assert_eq!(
            std::fs::read_dir(&cwd).unwrap().count(),
            0,
            "{command}: the current directory is untouched"
        );
    }
    // A relative directory in the command is taken from the configuration file's directory;
    // the destination is checked before uv is needed, so a machine without uv is refused
    // after the bundle verified and before anything is written. (The wheel is restored first.)
    std::fs::write(bundle.join("wheelhouse/dep-1.0-py3-none-any.whl"), [4, 5]).unwrap();
    std::fs::write(
        &cfg,
        "config_version = 1\n[packs.rascal-tools]\ncommand = [\"pack/.venv/bin/rascal-tools\", \"serve\"]\n",
    )
    .unwrap();
    let empty_path = dir.join("empty-path");
    std::fs::create_dir_all(&empty_path).unwrap();
    let out = rascal(
        &[
            "tools",
            "setup",
            "--bundle",
            bundle.to_str().unwrap(),
            "--config",
            cfg.to_str().unwrap(),
        ],
        &[("PATH", empty_path.to_str().unwrap())],
    );
    assert_eq!(out.status.code(), Some(64), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("uv is not on PATH"),
        "{}",
        text(&out.stderr)
    );
    assert!(
        text(&out.stdout).contains("files verified"),
        "verified before the prerequisites: {}",
        text(&out.stdout)
    );
    assert!(!dir.join("pack").exists(), "nothing written without uv");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_built_bundle_installs_offline_and_the_installed_pack_answers() {
    let Some(bundle) = std::env::var_os("RASCAL_BUNDLE") else {
        assert!(
            std::env::var_os("CI").is_none(),
            "RASCAL_BUNDLE is not set in CI; scripts/bundle-acceptance.sh sets it"
        );
        eprintln!("skipped: RASCAL_BUNDLE is not set (scripts/build-bundle.sh makes one)");
        return;
    };
    let bundle = PathBuf::from(bundle);
    let dir = scratch("install");
    let into = dir.join("pack");
    let cache = dir.join("uv-cache");
    let out = rascal(
        &[
            "tools",
            "setup",
            "--bundle",
            bundle.to_str().unwrap(),
            "--into",
            into.to_str().unwrap(),
        ],
        &[("UV_CACHE_DIR", cache.to_str().unwrap())],
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains("files verified") && stdout.contains("installed:"),
        "{stdout}"
    );
    let launcher = into.join(".venv/bin/rascal-tools");
    assert!(launcher.is_file());
    // The pack entry the command printed, verbatim, drives a call.
    let line = stdout
        .lines()
        .find(|l| l.starts_with("[packs.rascal-tools]: command"))
        .unwrap();
    let command = line.split_once(": ").unwrap().1;
    let cfg = dir.join("rascal.toml");
    std::fs::write(
        &cfg,
        format!("config_version = 1\n[packs.rascal-tools]\n{command}\n"),
    )
    .unwrap();
    let ws = dir.join("ws");
    let floor = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/images/floor.png");
    let out = rascal(
        &[
            "tools",
            "call",
            "rascal-tools",
            "echo_region",
            "--config",
            cfg.to_str().unwrap(),
            "--workspace",
            ws.to_str().unwrap(),
            "--image",
            floor.to_str().unwrap(),
            "--bbox",
            "0.25,0.5,0.75,1",
            "--json",
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(
        text(&out.stdout).contains("\"status\": \"ok\""),
        "{}",
        text(&out.stdout)
    );
    // A second setup into the same directory replaces the install; the config form finds it.
    let out = rascal(
        &[
            "tools",
            "setup",
            "--bundle",
            bundle.to_str().unwrap(),
            "--config",
            cfg.to_str().unwrap(),
        ],
        &[("UV_CACHE_DIR", cache.to_str().unwrap())],
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(
        text(&out.stdout).contains("already points here"),
        "{}",
        text(&out.stdout)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
