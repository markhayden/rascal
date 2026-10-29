//! Installing the Python pack from its bundle (SPEC.md "The Python pack and its bundle"): a
//! directory `scripts/build-bundle.sh` made for one platform, holding the provisioning
//! project (`pyproject.toml`, `uv.lock`), the pack wheel, a wheelhouse with every locked
//! dependency, `MODEL.md`, `artifacts.toml`, and `bundle.toml` with the SHA-256 of every
//! file. `setup` verifies every digest, copies exactly the listed files into the install
//! directory, and runs `uv sync --frozen --no-dev --offline --no-index --find-links
//! wheelhouse` with interpreter downloads forbidden, no cache and a baseline environment, so
//! the install either comes entirely from the bundle or fails. The launcher is
//! `<dir>/.venv/bin/rascal-tools`; launch never needs the network.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use rascal_core::Error;
use rascal_core::error::Result;
pub use rascal_core::pack::BundleManifest;
use sha2::{Digest, Sha256};

use crate::client::env::BASELINE;

/// The platform this binary runs on, in the bundle's vocabulary, when it is one a bundle
/// exists for.
pub fn host_platform() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("macos-arm64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
        _ => None,
    }
}

/// The lower-case `sha256:<hex>` of a file, streamed (a bundle may hold model weights).
pub fn sha256_file(path: &Path) -> Result<String> {
    let io = |source| Error::Io {
        path: path.to_path_buf(),
        source,
    };
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(io)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buffer).map_err(io)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!(
        "sha256:{}",
        rascal_core::identity::hex(&hasher.finalize())
    ))
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

/// What an install destination is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// The bundle's own directory: an install in place.
    InPlace,
    /// A directory that does not exist or is empty.
    Fresh,
    /// An earlier install of this pack, whose listed files and environment are replaced.
    Replace(BundleManifest),
}

/// The files the manifest must list for the install to be possible.
const REQUIRED: [&str; 2] = ["pyproject.toml", "uv.lock"];

/// What an install leaves beside the listed files, and what a replacement removes with them.
const INSTALL_EXTRAS: [&str; 2] = [".venv", "bundle.toml"];

fn read_manifest(path: &Path) -> Result<(BundleManifest, Vec<u8>)> {
    let text = std::fs::read(path).map_err(|source| {
        Error::usage(format!(
            "{}: not a bundle ({source}); build one with scripts/build-bundle.sh",
            path.parent().unwrap_or(path).display()
        ))
    })?;
    let manifest: BundleManifest = toml::from_slice(&text).map_err(|source| Error::Toml {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;
    Ok((manifest, text))
}

impl Bundle {
    /// Reads `<root>/bundle.toml`; a missing or malformed manifest, a version other than 1, or
    /// one that does not list the wheel and the provisioning project is a usage error.
    pub fn open(root: &Path) -> Result<Bundle> {
        let path = root.join("bundle.toml");
        let (manifest, text) = read_manifest(&path)?;
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

    /// What `into` is, before anything is written: the bundle itself, a new or empty
    /// directory, or an earlier install of this pack (a directory whose `bundle.toml` names
    /// the same pack and which holds nothing beyond that install's files, its environment and
    /// its manifest). An empty path, any other non-empty directory, another pack's install,
    /// or an install with other files beside it is refused naming the reason.
    pub fn check_destination(&self, into: &Path) -> Result<Destination> {
        if into.as_os_str().is_empty() {
            return Err(Error::usage(
                "the install directory is empty; give --into <dir> or a pack command of the form <dir>/.venv/bin/rascal-tools",
            ));
        }
        if same_dir(&self.root, into) {
            return Ok(Destination::InPlace);
        }
        let entries = match std::fs::read_dir(into) {
            Ok(entries) => entries,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Destination::Fresh);
            }
            Err(source) => {
                return Err(Error::Io {
                    path: into.to_path_buf(),
                    source,
                });
            }
        };
        let names: BTreeSet<String> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .collect();
        if names.is_empty() {
            return Ok(Destination::Fresh);
        }
        let manifest_path = into.join("bundle.toml");
        if !manifest_path.is_file() {
            return Err(Error::usage(format!(
                "{}: exists and is not empty, and is not an earlier install (no bundle.toml); choose a new directory",
                into.display()
            )));
        }
        let (existing, _) = read_manifest(&manifest_path)?;
        if existing.pack != self.manifest.pack {
            return Err(Error::usage(format!(
                "{}: holds an install of {}, not {}; choose another directory",
                into.display(),
                existing.pack,
                self.manifest.pack
            )));
        }
        // Everything there must be the earlier install's: its listed files' top-level
        // entries, the environment and the manifest.
        let owned: BTreeSet<&str> = existing
            .files
            .keys()
            .filter_map(|rel| Path::new(rel).components().next())
            .filter_map(|c| c.as_os_str().to_str())
            .chain(INSTALL_EXTRAS)
            .collect();
        let others: Vec<&str> = names
            .iter()
            .map(String::as_str)
            .filter(|n| !owned.contains(n))
            .collect();
        if !others.is_empty() {
            return Err(Error::usage(format!(
                "{}: holds an earlier install of {} and also {}; move those aside before replacing it",
                into.display(),
                existing.pack,
                others.join(", ")
            )));
        }
        Ok(Destination::Replace(existing))
    }

    /// Copies exactly the listed files and `bundle.toml` into `into` (a fresh directory, an
    /// earlier install of this pack, whose own files are removed first, or the bundle's
    /// directory for an install in place), then syncs the environment offline from the
    /// wheelhouse with a baseline environment and smokes the launcher.
    pub fn install(&self, into: &Path, prerequisites: &Prerequisites) -> Result<Installed> {
        let destination = self.check_destination(into)?;
        let io = |path: &Path, source| Error::Io {
            path: path.to_path_buf(),
            source,
        };
        if let Destination::Replace(existing) = &destination {
            for rel in existing.files.keys() {
                let path = into.join(rel);
                if path.is_file() {
                    std::fs::remove_file(&path).map_err(|e| io(&path, e))?;
                }
            }
            for extra in INSTALL_EXTRAS {
                let path = into.join(extra);
                if path.is_dir() {
                    std::fs::remove_dir_all(&path).map_err(|e| io(&path, e))?;
                } else if path.is_file() {
                    std::fs::remove_file(&path).map_err(|e| io(&path, e))?;
                }
            }
            remove_empty_dirs(into)?;
        }
        if destination != Destination::InPlace {
            std::fs::create_dir_all(into).map_err(|e| io(into, e))?;
            for rel in self
                .manifest
                .files
                .keys()
                .map(String::as_str)
                .chain(["bundle.toml"])
            {
                let target = into.join(rel);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
                }
                std::fs::copy(self.root.join(rel), &target).map_err(|e| io(&target, e))?;
            }
        }
        let venv = into.join(".venv");
        let mut sync = Command::new(&prerequisites.uv);
        sync.args([
            "sync",
            "--frozen",
            "--no-dev",
            "--offline",
            "--no-index",
            "--find-links",
            "wheelhouse",
            "--no-cache",
            "--no-config",
            "--python",
            &self.manifest.python,
        ])
        .current_dir(into);
        baseline_env(&mut sync);
        sync.env("UV_PROJECT_ENVIRONMENT", &venv);
        let output = sync
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
        let launcher = venv.join("bin").join(&self.manifest.pack);
        let mut smoke = Command::new(&launcher);
        smoke.arg("--version");
        baseline_env(&mut smoke);
        let smoke = smoke.output().map_err(|source| {
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

/// The baseline environment (`PATH`, `HOME`, `LANG`, `LC_ALL`, `TMPDIR`) and the offline
/// settings, nothing else: no key, and no uv setting of the shell's that could relocate the
/// environment or reach an index.
fn baseline_env(command: &mut Command) {
    command.env_clear();
    for name in BASELINE {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command.env("UV_PYTHON_DOWNLOADS", "never");
    command.env("UV_OFFLINE", "1");
}

/// Removes directories under `root` that an earlier install's files left empty.
fn remove_empty_dirs(root: &Path) -> Result<()> {
    let entries = std::fs::read_dir(root).map_err(|source| Error::Io {
        path: root.to_path_buf(),
        source,
    })?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            remove_empty_dirs(&path)?;
            if std::fs::read_dir(&path)
                .map(|mut d| d.next().is_none())
                .unwrap_or(false)
            {
                std::fs::remove_dir(&path).map_err(|source| Error::Io { path, source })?;
            }
        }
    }
    Ok(())
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
/// `<dir>/.venv/bin/<pack>` with a non-empty `<dir>` (a bare `.venv/bin/<pack>` names no
/// directory).
pub fn install_dir_of(command: &[String], pack: &str) -> Option<PathBuf> {
    let launcher = Path::new(command.first()?);
    let bin = launcher.parent()?;
    let venv = bin.parent()?;
    let dir = venv.parent()?;
    (launcher.file_name()?.to_str()? == pack
        && bin.file_name()?.to_str()? == "bin"
        && venv.file_name()?.to_str()? == ".venv"
        && !dir.as_os_str().is_empty())
    .then(|| dir.to_path_buf())
}

fn is_plain_relative(rel: &str) -> bool {
    let path = Path::new(rel);
    !rel.is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}

/// Whether two paths name one directory, by canonical path when both exist.
pub fn same_dir(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
