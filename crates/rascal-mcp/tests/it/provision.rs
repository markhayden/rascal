//! Task 5.3: the bundle's verification. A synthetic bundle with correct digests opens and
//! verifies; a changed byte is refused naming the file; a missing file is refused; a manifest
//! that escapes the bundle, lacks the project or has another version is refused; the host
//! platform check; the install directory read from a pack entry's command.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use rascal_mcp::provision::{
    Bundle, Destination, Prerequisites, host_platform, install_dir_of, sha256_bytes,
};

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
    // The manifest's own digest is the bundle digest.
    assert_eq!(
        bundle.digest,
        sha256_bytes(&std::fs::read(dir.join("bundle.toml")).unwrap())
    );
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
    assert_eq!(
        install_dir_of(&command(".venv/bin/rascal-tools"), "rascal-tools"),
        None,
        "a bare .venv names no directory"
    );
    assert_eq!(
        install_dir_of(&command("tools/.venv/bin/rascal-tools"), "rascal-tools"),
        Some(PathBuf::from("tools"))
    );
}

/// Prerequisites that cannot run uv: the install's copy and refusals happen before it.
fn no_uv() -> Prerequisites {
    Prerequisites {
        uv: PathBuf::from("/nonexistent/uv"),
        uv_version: "none".into(),
        python: PathBuf::from("/nonexistent/python"),
    }
}

fn walk(root: &Path) -> Vec<String> {
    fn go(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                go(root, &path, out);
            } else {
                out.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    let mut out = Vec::new();
    go(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn the_destination_is_checked_before_anything_is_written_and_exactly_the_listed_files_are_copied() {
    let dir = scratch("destination");
    let platform = host_platform().unwrap_or("linux-x86_64");
    let bundle_dir = dir.join("bundle");
    let files = synthetic(&bundle_dir, platform);
    std::fs::write(bundle_dir.join("stray.txt"), "not listed").unwrap();
    let bundle = Bundle::open(&bundle_dir).unwrap();
    // An empty path, a non-empty directory without a bundle.toml, another pack's install,
    // and an install with other files beside it: each refused by name, nothing written.
    let err = bundle
        .check_destination(Path::new(""))
        .unwrap_err()
        .to_string();
    assert!(err.contains("install directory is empty"), "{err}");
    let busy = dir.join("busy");
    std::fs::create_dir_all(&busy).unwrap();
    std::fs::write(busy.join("notes.txt"), "mine").unwrap();
    let err = bundle.check_destination(&busy).unwrap_err().to_string();
    assert!(err.contains("not an earlier install"), "{err}");
    let err = bundle.install(&busy, &no_uv()).unwrap_err().to_string();
    assert!(err.contains("not an earlier install"), "{err}");
    assert_eq!(
        walk(&busy),
        vec!["notes.txt".to_string()],
        "nothing was written"
    );
    let other = dir.join("other-pack");
    synthetic(&other, platform);
    let text = std::fs::read_to_string(other.join("bundle.toml")).unwrap();
    std::fs::write(
        other.join("bundle.toml"),
        text.replace("pack = \"rascal-tools\"", "pack = \"other-tools\""),
    )
    .unwrap();
    let err = bundle.check_destination(&other).unwrap_err().to_string();
    assert!(
        err.contains("install of other-tools, not rascal-tools"),
        "{err}"
    );
    let beside = dir.join("beside");
    synthetic(&beside, platform);
    std::fs::create_dir_all(beside.join("models")).unwrap();
    std::fs::write(beside.join("models/weights.bin"), "precious").unwrap();
    let err = bundle.check_destination(&beside).unwrap_err().to_string();
    assert!(err.contains("and also models"), "{err}");
    assert!(beside.join("models/weights.bin").is_file(), "untouched");
    // A fresh directory: the listed files and bundle.toml are copied, the stray file is not,
    // and only then does uv fail to start.
    let fresh = dir.join("fresh");
    assert_eq!(
        bundle.check_destination(&fresh).unwrap(),
        Destination::Fresh
    );
    let err = bundle.install(&fresh, &no_uv()).unwrap_err().to_string();
    assert!(err.contains("uv sync could not start"), "{err}");
    let mut expected: Vec<String> = files.iter().map(|(rel, _)| rel.clone()).collect();
    expected.push("bundle.toml".into());
    expected.sort();
    assert_eq!(walk(&fresh), expected);
    // An earlier install of this pack with a stale listed file and an environment: those go,
    // the new files come, and nothing else is touched.
    let earlier = dir.join("earlier");
    synthetic(&earlier, platform);
    std::fs::write(earlier.join("wheelhouse/old-0.1-py3-none-any.whl"), "old").unwrap();
    let text = std::fs::read_to_string(earlier.join("bundle.toml")).unwrap();
    std::fs::write(
        earlier.join("bundle.toml"),
        text + "\"wheelhouse/old-0.1-py3-none-any.whl\" = \"sha256:0000000000000000000000000000000000000000000000000000000000000000\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(earlier.join(".venv/bin")).unwrap();
    std::fs::write(earlier.join(".venv/bin/rascal-tools"), "#!/bin/sh").unwrap();
    assert!(matches!(
        bundle.check_destination(&earlier).unwrap(),
        Destination::Replace(_)
    ));
    let err = bundle.install(&earlier, &no_uv()).unwrap_err().to_string();
    assert!(err.contains("uv sync could not start"), "{err}");
    assert_eq!(
        walk(&earlier),
        expected,
        "the old wheel and the environment are gone, the new files are there"
    );
    // In place: nothing copied, the stray file stays.
    assert_eq!(
        bundle.check_destination(&bundle_dir).unwrap(),
        Destination::InPlace
    );
    let _ = std::fs::remove_dir_all(&dir);
}
