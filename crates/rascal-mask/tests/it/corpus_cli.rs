//! Task 6.1's CLI scenario: `rascal corpus verify examples/lab/corpus.toml` reports a missing
//! file by id when the root does not hold the corpus, and passes with `--root fixtures/images`;
//! a changed file is reported as changed with both digests.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rascal(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command.args(args).env_remove("RASCAL_CORPUS");
    for (k, v) in env {
        command.env(k, v);
    }
    command.output().unwrap()
}

#[test]
fn corpus_verify_reports_a_missing_file_by_id_and_passes_on_the_fixtures() {
    let manifest = repo_root().join("examples/lab/corpus.toml");
    let dir = std::env::temp_dir().join(format!("rascal-corpus-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // The default root under a HOME without a corpus: every image is missing, by id.
    let out = rascal(
        &["corpus", "verify", manifest.to_str().unwrap()],
        &[("HOME", dir.to_str().unwrap())],
    );
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("missing  sky-01") && stdout.contains("rascal/corpus"),
        "{stdout}"
    );
    assert!(stdout.contains("0 of 4 images verified"), "{stdout}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("4 images missing or changed"));
    // RASCAL_CORPUS names the root.
    let fixtures = repo_root().join("fixtures/images");
    let out = rascal(
        &["corpus", "verify", manifest.to_str().unwrap()],
        &[("RASCAL_CORPUS", fixtures.to_str().unwrap())],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("4 of 4 images verified"));
    // --root too; and a changed file is reported with both digests.
    for image in [
        "step-sky.png",
        "step-pano.png",
        "floor.png",
        "chair-blob.png",
    ] {
        std::fs::copy(fixtures.join(image), dir.join(image)).unwrap();
    }
    std::fs::write(dir.join("floor.png"), b"changed").unwrap();
    let out = rascal(
        &[
            "corpus",
            "verify",
            manifest.to_str().unwrap(),
            "--root",
            dir.to_str().unwrap(),
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(3));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("changed  structured-01") && stdout.contains("manifest sha256:40ec7298"),
        "{stdout}"
    );
    assert!(stdout.contains("ok       sky-01"), "{stdout}");
    assert!(stdout.contains("3 of 4 images verified"), "{stdout}");
    // A manifest that is not TOML is an operational error (as any unreadable file is).
    let out = rascal(
        &["corpus", "verify", dir.join("floor.png").to_str().unwrap()],
        &[],
    );
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("not valid TOML"));
    // A manifest that fails its own checks is a usage error.
    std::fs::write(dir.join("bad.toml"), "version = 2\n").unwrap();
    let out = rascal(
        &["corpus", "verify", dir.join("bad.toml").to_str().unwrap()],
        &[],
    );
    assert_eq!(
        out.status.code(),
        Some(64),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
