//! Task 9.5 (Wafflery H1; ADR 0020): `rascal playground` finds `wafflery`
//! (`WAFFLERY_BIN`, `PATH`, `~/.local/bin`), runs it with exactly `--tool rascal` without a
//! shell, and keeps its exit code (128 + N for a signal). A wrong override is 69 and never falls
//! through; missing, a release build says where to get it (69); the testkit's installer seam
//! drives the offer: no is 0 and changes nothing, yes installs and runs, a failed install is 74
//! and runs nothing, off a terminal is 69. Extra arguments are a usage error (64).
#![allow(clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-playground-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A shell script at `path`, executable.
fn script(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A fake `wafflery` that writes its arguments to `seen` and exits `code`.
fn fake(path: &Path, seen: &Path, code: i32) {
    script(
        path,
        &format!("printf '%s\\n' \"$@\" > '{}'\nexit {code}", seen.display()),
    );
}

/// `rascal playground <args>` with only the environment given: `PATH`, `HOME`, and the
/// override when set.
fn playground(args: &[&str], path: &str, home: &Path, explicit: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command
        .arg("playground")
        .args(args)
        .env_clear()
        .env("PATH", path)
        .env("HOME", home)
        .stdin(Stdio::null());
    if let Some(explicit) = explicit {
        command.env("WAFFLERY_BIN", explicit);
    }
    command.output().unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn the_program_gets_exactly_the_tool_and_its_exit_code_is_kept() {
    let dir = scratch("argv");
    let seen = dir.join("seen.txt");
    let home = dir.join("home");
    let bin = dir.join("bin");
    fake(&bin.join("wafflery"), &seen, 7);
    let out = playground(&[], bin.to_str().unwrap(), &home, None);
    assert_eq!(out.status.code(), Some(7), "{}", stderr(&out));
    assert_eq!(std::fs::read_to_string(&seen).unwrap(), "--tool\nrascal\n");
    // Extra arguments are a usage error, and nothing runs.
    std::fs::remove_file(&seen).unwrap();
    let out = playground(&["--port", "0"], bin.to_str().unwrap(), &home, None);
    assert_eq!(out.status.code(), Some(64));
    assert!(!seen.exists());
    // A signal is 128 + N.
    script(&bin.join("wafflery"), "kill -TERM $$");
    let out = playground(&[], bin.to_str().unwrap(), &home, None);
    assert_eq!(out.status.code(), Some(143), "{}", stderr(&out));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_override_comes_first_and_a_wrong_one_never_falls_through() {
    let dir = scratch("override");
    let home = dir.join("home");
    let on_path = dir.join("bin");
    let path_seen = dir.join("path.txt");
    fake(&on_path.join("wafflery"), &path_seen, 0);
    // An absolute path with spaces, run without a shell.
    let spaced = dir.join("with space/the program");
    let spaced_seen = dir.join("spaced.txt");
    fake(&spaced, &spaced_seen, 3);
    let out = playground(&[], on_path.to_str().unwrap(), &home, Some(&spaced));
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(&spaced_seen).unwrap(),
        "--tool\nrascal\n"
    );
    assert!(!path_seen.exists());
    // A symlink to the program is the program.
    let link = dir.join("link");
    std::os::unix::fs::symlink(&spaced, &link).unwrap();
    let out = playground(&[], on_path.to_str().unwrap(), &home, Some(&link));
    assert_eq!(out.status.code(), Some(3));
    // Relative, missing or not executable: 69, naming the variable, and PATH's is not run.
    let plain = dir.join("plain");
    std::fs::write(&plain, "not a program").unwrap();
    for wrong in [Path::new("relative/wafflery"), &dir.join("absent"), &plain] {
        let out = playground(&[], on_path.to_str().unwrap(), &home, Some(wrong));
        assert_eq!(out.status.code(), Some(69), "{}", wrong.display());
        assert!(stderr(&out).contains("WAFFLERY_BIN"), "{}", stderr(&out));
        assert!(!path_seen.exists(), "fell through for {}", wrong.display());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn path_comes_before_local_bin_and_missing_is_69_with_instructions() {
    let dir = scratch("lookup");
    let home = dir.join("home");
    let local_seen = dir.join("local.txt");
    fake(&home.join(".local/bin/wafflery"), &local_seen, 5);
    let path_seen = dir.join("path.txt");
    let bin = dir.join("bin");
    fake(&bin.join("wafflery"), &path_seen, 4);
    let out = playground(&[], bin.to_str().unwrap(), &home, None);
    assert_eq!(out.status.code(), Some(4));
    assert!(!local_seen.exists());
    let empty = dir.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let out = playground(&[], empty.to_str().unwrap(), &home, None);
    assert_eq!(out.status.code(), Some(5));
    assert_eq!(
        std::fs::read_to_string(&local_seen).unwrap(),
        "--tool\nrascal\n"
    );
    // Nowhere: a release build makes no offer; 69 and where to get it.
    let nobody = dir.join("nobody");
    let out = playground(&[], empty.to_str().unwrap(), &nobody, None);
    assert_eq!(out.status.code(), Some(69));
    assert!(stderr(&out).contains("not installed"), "{}", stderr(&out));
    assert!(!nobody.join(".local/bin").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The offer, through the testkit's installer seam.
#[cfg(feature = "testkit")]
#[test]
fn the_offer_installs_only_on_yes_and_a_failed_install_runs_nothing() {
    use std::io::Write;
    let dir = scratch("offer");
    let empty = dir.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let seen = dir.join("seen.txt");
    // The program the installer puts in place.
    let payload = dir.join("payload");
    fake(&payload, &seen, 9);
    let installer = dir.join("installer");
    script(
        &installer,
        &format!(
            "[ \"$1\" = --prefix ] || exit 9\n/bin/cp '{}' \"$2/wafflery\"",
            payload.display()
        ),
    );
    let failing = dir.join("failing-installer");
    script(&failing, "exit 1");
    let offer = |home: &Path, installer: &Path, answer: Option<&str>| {
        let mut args = vec!["playground", "--installer", installer.to_str().unwrap()];
        if answer.is_some() {
            args.push("--terminal");
        }
        let mut child = Command::new(env!("CARGO_BIN_EXE_rascal"))
            .args(&args)
            .env_clear()
            .env("PATH", &empty)
            .env("HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(answer) = answer {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(answer.as_bytes())
                .unwrap();
        }
        child.wait_with_output().unwrap()
    };
    // No, or no answer at all: 0, and nothing changes.
    for answer in ["n\n", ""] {
        let home = dir.join(format!("home-no-{}", answer.len()));
        let out = offer(&home, &installer, Some(answer));
        assert_eq!(out.status.code(), Some(0), "{answer:?}: {}", stderr(&out));
        assert!(stderr(&out).contains("[y/N]"));
        assert!(!home.join(".local/bin/wafflery").exists());
        assert!(!seen.exists());
    }
    // Off a terminal: 69 with instructions, no prompt.
    let home = dir.join("home-off");
    let out = offer(&home, &installer, None);
    assert_eq!(out.status.code(), Some(69));
    assert!(!stderr(&out).contains("[y/N]"));
    assert!(!home.join(".local/bin/wafflery").exists());
    // A failed installation: 74, and nothing is run.
    let home = dir.join("home-fail");
    let out = offer(&home, &failing, Some("y\n"));
    assert_eq!(out.status.code(), Some(74), "{}", stderr(&out));
    assert!(!seen.exists());
    // Yes: installed into ~/.local/bin and that path run, its exit code kept.
    let home = dir.join("home-yes");
    let out = offer(&home, &installer, Some("y\n"));
    assert_eq!(out.status.code(), Some(9), "{}", stderr(&out));
    assert!(home.join(".local/bin/wafflery").exists());
    assert_eq!(std::fs::read_to_string(&seen).unwrap(), "--tool\nrascal\n");
    let _ = std::fs::remove_dir_all(&dir);
}
