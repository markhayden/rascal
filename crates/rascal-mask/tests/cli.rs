//! The binary's command-line contract, run against the built `rascal` executable.

use std::process::Command;

fn rascal() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
}

#[test]
fn version_prints_name_and_version() {
    let out = rascal().arg("--version").output().unwrap();
    assert!(out.status.success(), "status: {}", out.status);
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("rascal {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn help_is_on_stdout_and_succeeds() {
    let out = rascal().arg("--help").output().unwrap();
    assert!(out.status.success(), "status: {}", out.status);
    assert!(String::from_utf8_lossy(&out.stdout).contains("Usage"));
    assert!(out.stderr.is_empty());
}

#[test]
fn no_arguments_is_a_usage_error() {
    let out = rascal().output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(64),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Usage"));
}

#[test]
fn unknown_command_is_a_usage_error() {
    let out = rascal().arg("no-such-command").output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(64),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
