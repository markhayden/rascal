//! Task 5.3: the bundle's verification. A synthetic bundle with correct digests opens and
//! verifies; a changed byte is refused naming the file; a missing file is refused; a manifest
//! that escapes the bundle, lacks the project or has another version is refused; the host
//! platform check; the install directory read from a pack entry's command.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use rascal_mcp::provision::{Bundle, host_platform, install_dir_of, sha256_bytes};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-provision-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A bundle of small files with a manifest that lists them all, for `platform`.
fn synthetic(root: &Path, platform: &str) -> Vec<(String, Vec<u8>)> {
    let files = vec![
        ("MODEL.md".to_string(), b"# model\n".to_vec()),
        ("artifacts.toml".to_string(), b"artifact = []\n".to_vec()),
        (
            "pyproject.toml".to_string(),
            b"[project]\nname = \"x\"\n".to_vec(),
        ),
        ("uv.lock".to_string(), b"version = 1\n".to_vec()),
        (
            "wheel/rascal_tools-0.1.0-py3-none-any.whl".to_string(),
            vec![0x50, 0x4b, 1, 2, 3],
        ),
        (
            "wheelhouse/dep-1.0-py3-none-any.whl".to_string(),
            vec![0x50, 0x4b, 9],
        ),
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
    files
}

#[test]
fn a_sound_bundle_verifies_and_a_changed_byte_or_a_missing_file_is_refused_by_name() {
    let dir = scratch("verify");
    let platform = host_platform().unwrap_or("linux-x86_64");
    let files = synthetic(&dir, platform);
    let bundle = Bundle::open(&dir).unwrap();
    assert_eq!(bundle.manifest.files.len(), files.len());
    assert_eq!(bundle.manifest.pack, "rascal-tools");
    assert!(bundle.digest.starts_with("sha256:"));
    bundle.verify().unwrap();
    // The manifest's own digest is the bundle digest: the same bytes, the same digest.
    let again = Bundle::open(&dir).unwrap();
    assert_eq!(again.digest, bundle.digest);
    // One byte of a wheel changes.
    let wheel = dir.join("wheelhouse/dep-1.0-py3-none-any.whl");
    std::fs::write(&wheel, [0x50, 0x4b, 8]).unwrap();
    let err = bundle.verify().unwrap_err().to_string();
    assert!(
        err.contains("dep-1.0-py3-none-any.whl") && err.contains("does not match"),
        "{err}"
    );
    std::fs::remove_file(&wheel).unwrap();
    let err = bundle.verify().unwrap_err().to_string();
    assert!(
        err.contains("dep-1.0-py3-none-any.whl") && err.contains("missing"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_manifest_that_escapes_lacks_the_project_or_has_another_version_is_refused() {
    let dir = scratch("manifest");
    synthetic(&dir, "macos-arm64");
    let sound = std::fs::read_to_string(dir.join("bundle.toml")).unwrap();
    for (bad, needle) in [
        (
            sound.replace("bundle_version = 1", "bundle_version = 2"),
            "bundle_version 2",
        ),
        (
            sound.replace("\"uv.lock\"", "\"uv.lock.gone\""),
            "does not list uv.lock",
        ),
        (
            sound.replace("\"MODEL.md\"", "\"../MODEL.md\""),
            "not a plain relative path",
        ),
        (
            sound.replace("\"MODEL.md\"", "\"/etc/MODEL.md\""),
            "not a plain relative path",
        ),
        (
            sound.replace(
                "wheel = \"wheel/rascal_tools-0.1.0-py3-none-any.whl\"",
                "wheel = \"wheel/other.whl\"",
            ),
            "does not list wheel/other.whl",
        ),
    ] {
        std::fs::write(dir.join("bundle.toml"), bad).unwrap();
        let err = Bundle::open(&dir).unwrap_err().to_string();
        assert!(err.contains(needle), "{needle}: {err}");
    }
    // No manifest at all.
    std::fs::remove_file(dir.join("bundle.toml")).unwrap();
    let err = Bundle::open(&dir).unwrap_err().to_string();
    assert!(err.contains("not a bundle"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_platform_check_and_the_install_directory_of_a_pack_entry() {
    let dir = scratch("platform");
    let other = match host_platform() {
        Some("macos-arm64") => "linux-x86_64",
        _ => "macos-arm64",
    };
    synthetic(&dir, other);
    let bundle = Bundle::open(&dir).unwrap();
    let err = bundle.check_platform().unwrap_err().to_string();
    assert!(err.contains(other), "{err}");
    if let Some(host) = host_platform() {
        synthetic(&dir, host);
        Bundle::open(&dir).unwrap().check_platform().unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
    let command = |s: &str| vec![s.to_string(), "serve".to_string()];
    assert_eq!(
        install_dir_of(
            &command("/opt/rascal-tools/.venv/bin/rascal-tools"),
            "rascal-tools"
        ),
        Some(PathBuf::from("/opt/rascal-tools"))
    );
    assert_eq!(
        install_dir_of(
            &command("/opt/rascal-tools/.venv/bin/other"),
            "rascal-tools"
        ),
        None
    );
    assert_eq!(
        install_dir_of(&command("rascal-tools"), "rascal-tools"),
        None
    );
    assert_eq!(install_dir_of(&[], "rascal-tools"), None);
}
