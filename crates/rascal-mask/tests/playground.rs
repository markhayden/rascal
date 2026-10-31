//! Task 8.7 (ADR 0020): `rascal playground` hands over to the shared playground with
//! `--tool rascal` and the arguments given, returns its exit code, and says where to install
//! it (exit 69) when there is none.
#![allow(clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn the_playground_gets_the_tool_and_the_arguments_and_its_exit_code_is_kept() {
    let dir = std::env::temp_dir().join(format!("rascal-playground-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let seen = dir.join("seen.txt");
    let script = bin.join("playground");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {:?}\nexit 7\n",
            seen.to_str().unwrap()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(["playground", "--port", "0", "open"])
        .env("PATH", &path)
        .env_remove("PLAYGROUND")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&seen).unwrap(),
        "--tool\nrascal\n--port\n0\nopen\n"
    );
    // None installed: where to get it, and 69.
    let out = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .arg("playground")
        .env("PATH", "/nonexistent")
        .env("HOME", &dir)
        .env_remove("XDG_DATA_HOME")
        .env_remove("PLAYGROUND")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(69));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not installed"));
    let _ = std::fs::remove_dir_all(&dir);
}
