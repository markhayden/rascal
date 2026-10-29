//! Installing the Python pack from its bundle (SPEC.md "The Python pack and its bundle"): a
//! directory `scripts/build-bundle.sh` made for one platform, holding the provisioning
//! project (`pyproject.toml`, `uv.lock`), the pack wheel, a wheelhouse with every locked
//! dependency, `MODEL.md`, `artifacts.toml`, and `bundle.toml` with the SHA-256 of every
//! file. `setup` verifies every digest, copies exactly the listed files into the install
//! directory, and runs `uv sync --frozen --no-dev --offline --no-index --find-links
//! wheelhouse` with interpreter downloads forbidden and no cache, so the install either comes
//! entirely from the bundle or fails. The launcher is `<dir>/.venv/bin/rascal-tools`; launch
//! never needs the network.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use rascal_core::Error;
use rascal_core::error::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The manifest, `bundle.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest {
    pub bundle_version: u32,
    /// The pack's distribution name (`rascal-tools`).
    pub pack: String,
    pub pack_version: String,
    /// The interpreter line the bundle was built for (`3.12`).
    pub python: String,
    /// `macos-arm64` or `linux-x86_64`.
    pub platform: String,
    /// The pack wheel's path inside the bundle.
    pub wheel: String,
    /// Every file in the bundle by relative path, with its `sha256:<hex>` digest.
    pub files: BTreeMap<String, String>,
}

/// The platform this binary runs on, in the bundle's vocabulary, when it is one a bundle
/// exists for.
pub fn host_platform() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("macos-arm64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
        _ => None,
    }
}

/// The lower-case `sha256:<hex>` of a file.
pub fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(sha256_bytes(&bytes))
}

/// The lower-case `sha256:<hex>` of bytes.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    format!(
        "sha256:{}",
        rascal_core::identity::hex(&Sha256::digest(bytes))
    )
}

/// A bundle directory with its manifest read.
#[derive(Debug, Clone)]
pub struct Bundle {
    pub root: PathBuf,
    pub manifest: BundleManifest,
    /// The SHA-256 of `bundle.toml` itself: the bundle digest the lab records.
    pub digest: String,
}

/// The files the manifest must list for the install to be possible.
const REQUIRED: [&str; 2] = ["pyproject.toml", "uv.lock"];

impl Bundle {
    /// Reads `<root>/bundle.toml`; a missing or malformed manifest, a version other than 1, or
    /// one that does not list the wheel and the provisioning project is a usage error.
    pub fn open(root: &Path) -> Result<Bundle> {
        let path = root.join("bundle.toml");
        let text = std::fs::read(&path).map_err(|source| {
            Error::usage(format!(
                "{}: not a bundle ({source}); build one with scripts/build-bundle.sh",
                root.display()
            ))
        })?;
        let manifest: BundleManifest = toml::from_slice(&text).map_err(|source| Error::Toml {
            path: path.clone(),
            source: Box::new(source),
        })?;
        if manifest.bundle_version != 1 {
            return Err(Error::usage(format!(
                "{}: bundle_version {} is not 1",
                path.display(),
                manifest.bundle_version
            )));
        }
        for required in REQUIRED.iter().copied().chain([manifest.wheel.as_str()]) {
            if !manifest.files.contains_key(required) {
                return Err(Error::usage(format!(
                    "{}: the manifest does not list {required}",
                    path.display()
                )));
            }
        }
        for rel in manifest.files.keys() {
            if !is_plain_relative(rel) {
                return Err(Error::usage(format!(
                    "{}: {rel} is not a plain relative path inside the bundle",
                    path.display()
                )));
            }
        }
        Ok(Bundle {
            root: root.to_path_buf(),
            digest: sha256_bytes(&text),
            manifest,
        })
    }

    /// Every listed file is present and matches its digest; the first mismatch or absence is
    /// an operational error naming the file, and nothing is installed from such a bundle.
    pub fn verify(&self) -> Result<()> {
        for (rel, expected) in &self.manifest.files {
            let path = self.root.join(rel);
            if !path.is_file() {
                return Err(Error::op(
                    "setup",
                    format!("{}: listed in bundle.toml but missing", path.display()),
                ));
            }
            let actual = sha256_file(&path)?;
            if &actual != expected {
                return Err(Error::op(
                    "setup",
                    format!(
                        "{}: digest {actual} does not match bundle.toml's {expected}; the bundle is damaged or tampered with",
                        path.display()
                    ),
                ));
            }
        }
        Ok(())
    }

    /// The bundle is for this machine.
    pub fn check_platform(&self) -> Result<()> {
        match host_platform() {
            Some(host) if host == self.manifest.platform => Ok(()),
            Some(host) => Err(Error::usage(format!(
                "the bundle is for {} but this machine is {host}",
                self.manifest.platform
            ))),
            None => Err(Error::usage(format!(
                "no bundle exists for this machine ({} {}); the platforms are macos-arm64 and linux-x86_64",
                std::env::consts::OS,
                std::env::consts::ARCH
            ))),
        }
    }

    /// Copies exactly the listed files and `bundle.toml` into `into` (new or empty, an
    /// earlier install of this pack, which is replaced whole, or the bundle's own directory
    /// for an install in place), then syncs the environment offline from the wheelhouse and
    /// smokes the launcher.
    pub fn install(&self, into: &Path, prerequisites: &Prerequisites) -> Result<Installed> {
        let in_place = same_dir(&self.root, into);
        if !in_place {
            match std::fs::read_dir(into) {
                Ok(mut entries) => {
                    if entries.next().is_some() {
                        // A directory with a bundle.toml is an install of this pack: replace it.
                        if !into.join("bundle.toml").is_file() {
                            return Err(Error::usage(format!(
                                "{}: exists and is not empty, and is not an earlier install (no bundle.toml); choose a new directory",
                                into.display()
                            )));
                        }
                        std::fs::remove_dir_all(into).map_err(|source| Error::Io {
                            path: into.to_path_buf(),
                            source,
                        })?;
                        std::fs::create_dir_all(into).map_err(|source| Error::Io {
                            path: into.to_path_buf(),
                            source,
                        })?;
                    }
                }
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::create_dir_all(into).map_err(|source| Error::Io {
                        path: into.to_path_buf(),
                        source,
                    })?;
                }
                Err(source) => {
                    return Err(Error::Io {
                        path: into.to_path_buf(),
                        source,
                    });
                }
            }
            for rel in self
                .manifest
                .files
                .keys()
                .map(String::as_str)
                .chain(["bundle.toml"])
            {
                let target = into.join(rel);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                        path: parent.to_path_buf(),
                        source,
                    })?;
                }
                std::fs::copy(self.root.join(rel), &target).map_err(|source| Error::Io {
                    path: target.clone(),
                    source,
                })?;
            }
        }
        let output = Command::new(&prerequisites.uv)
            .args([
                "sync",
                "--frozen",
                "--no-dev",
                "--offline",
                "--no-index",
                "--find-links",
                "wheelhouse",
                "--no-cache",
                "--python",
                &self.manifest.python,
            ])
            .current_dir(into)
            .env("UV_PYTHON_DOWNLOADS", "never")
            .env("UV_OFFLINE", "1")
            .output()
            .map_err(|source| Error::op("setup", format!("uv sync could not start: {source}")))?;
        if !output.status.success() {
            return Err(Error::op(
                "setup",
                format!(
                    "uv sync failed ({}) in {}:\n{}",
                    output.status,
                    into.display(),
                    String::from_utf8_lossy(&output.stderr).trim_end()
                ),
            ));
        }
        let launcher = into.join(".venv").join("bin").join(&self.manifest.pack);
        let smoke = Command::new(&launcher)
            .arg("--version")
            .env("UV_OFFLINE", "1")
            .output()
            .map_err(|source| {
                Error::op(
                    "setup",
                    format!("{} did not start: {source}", launcher.display()),
                )
            })?;
        let version_line = String::from_utf8_lossy(&smoke.stdout).trim().to_string();
        let expected = format!("{} {}", self.manifest.pack, self.manifest.pack_version);
        if !smoke.status.success() || version_line != expected {
            return Err(Error::op(
                "setup",
                format!(
                    "{} --version printed {version_line:?}, expected {expected:?}",
                    launcher.display()
                ),
            ));
        }
        Ok(Installed {
            dir: into.to_path_buf(),
            launcher,
            version_line,
            bundle_digest: self.digest.clone(),
        })
    }
}

/// What a machine needs before a bundle installs: `uv`, and an interpreter of the bundle's
/// version that uv can find without downloading one.
#[derive(Debug, Clone)]
pub struct Prerequisites {
    pub uv: PathBuf,
    pub uv_version: String,
    pub python: PathBuf,
}

/// Finds `uv` on PATH and asks it for an interpreter of `python` with downloads forbidden; a
/// missing one is a usage error that says what to install.
pub fn prerequisites(python: &str) -> Result<Prerequisites> {
    let uv = PathBuf::from("uv");
    let version = Command::new(&uv).arg("--version").output().map_err(|_| {
        Error::usage("uv is not on PATH; install it (https://docs.astral.sh/uv/) and rerun setup")
    })?;
    if !version.status.success() {
        return Err(Error::usage("uv --version failed; reinstall uv"));
    }
    let find = Command::new(&uv)
        .args(["python", "find", python])
        .env("UV_PYTHON_DOWNLOADS", "never")
        .output()
        .map_err(|source| Error::usage(format!("uv python find could not start: {source}")))?;
    if !find.status.success() {
        return Err(Error::usage(format!(
            "no Python {python} interpreter found; install one (for example `uv python install {python}` on a connected machine) and rerun setup"
        )));
    }
    Ok(Prerequisites {
        uv,
        uv_version: String::from_utf8_lossy(&version.stdout).trim().to_string(),
        python: PathBuf::from(String::from_utf8_lossy(&find.stdout).trim()),
    })
}

/// A finished install.
#[derive(Debug, Clone)]
pub struct Installed {
    pub dir: PathBuf,
    /// `<dir>/.venv/bin/rascal-tools`: `command[0]` of the pack entry.
    pub launcher: PathBuf,
    pub version_line: String,
    pub bundle_digest: String,
}

/// The install directory a pack entry's command points at: its `command[0]` is
/// `<dir>/.venv/bin/<pack>`.
pub fn install_dir_of(command: &[String], pack: &str) -> Option<PathBuf> {
    let launcher = Path::new(command.first()?);
    let bin = launcher.parent()?;
    let venv = bin.parent()?;
    (launcher.file_name()?.to_str()? == pack
        && bin.file_name()?.to_str()? == "bin"
        && venv.file_name()?.to_str()? == ".venv")
        .then(|| venv.parent().map(Path::to_path_buf))
        .flatten()
}

fn is_plain_relative(rel: &str) -> bool {
    let path = Path::new(rel);
    !rel.is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
