//! `rascal tools list --config rascal.toml`, `rascal tools call <pack> <tool> …` and
//! `rascal tools setup --bundle <dir>`: the configured packs from the shell. `list` starts
//! every pack and prints its tools as a client sees them; `call` starts one pack and runs one
//! tool as an op subcommand would, with the same arguments (`--image <id or path>`, the
//! tool's parameters, `--json`, `--out`); `setup` installs the Python pack from its bundle
//! with no network (`rascal_mcp::provision`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::config::Config;
use rascal_core::error::Result;
use rascal_core::workspace::Workspace;
use rascal_mcp::client::Pack;
use rascal_mcp::provision::{Bundle, install_dir_of, prerequisites, same_dir};

/// Prints every configured pack's tools: the op view (name, version, purpose, ports,
/// parameters, bounds) or, with `--json`, the native manifests by pack.
pub fn list(config: &Path, json: bool) -> Result<()> {
    let config = Config::read(config)?;
    let key_envs = config.key_envs();
    let mut all = serde_json::Map::new();
    for (name, entry) in &config.packs {
        let pack = Pack::new(name, entry.clone(), key_envs.clone())?;
        let tools = pack.tools()?;
        if json {
            all.insert(
                name.clone(),
                serde_json::to_value(tools.iter().map(|t| &t.manifest).collect::<Vec<_>>())
                    .map_err(|e| Error::op("tools", e.to_string()))?,
            );
            continue;
        }
        println!(
            "[packs.{name}] {} ({} tools{})",
            entry.command.join(" "),
            tools.len(),
            if entry.concurrent { ", concurrent" } else { "" }
        );
        for tool in &tools {
            let m = &tool.manifest;
            println!("  {} {}: {}", m.name, m.version, m.purpose);
            for port in &m.inputs {
                println!(
                    "    in  {} ({}{})",
                    port.name,
                    port.artifact_type,
                    if port.required { ", required" } else { "" }
                );
            }
            for port in &m.outputs {
                println!(
                    "    out {} ({}{}{})",
                    port.name,
                    port.artifact_type,
                    if port.many { "[]" } else { "" },
                    if port.required { ", required" } else { "" }
                );
            }
            for param in &m.params {
                println!(
                    "    --{} ({:?}{})",
                    param.name,
                    param.kind,
                    param
                        .default
                        .as_ref()
                        .map(|d| format!(", default {d}"))
                        .unwrap_or_default()
                );
            }
            println!(
                "    returns up to {} regions, {} images; concurrent {}",
                m.returns.max_regions,
                m.returns.max_images,
                entry.concurrent && tool.compat.concurrent
            );
        }
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&all).map_err(|e| Error::op("tools", e.to_string()))?
        );
    }
    Ok(())
}

/// Calls one tool of one pack as an op subcommand: `args` are the subcommand's arguments
/// without `--workspace`, which is added. Returns the exit code.
pub fn call(
    pack: &str,
    tool: &str,
    config: &Path,
    workspace: &Path,
    args: &[String],
) -> Result<i32> {
    let mut config = Config::read(config)?;
    let names: Vec<String> = config.packs.keys().cloned().collect();
    config.packs.retain(|name, _| name == pack);
    if config.packs.is_empty() {
        return Err(Error::usage(format!(
            "no [packs.{pack}]; the packs are {}",
            if names.is_empty() {
                "none".to_string()
            } else {
                names.join(", ")
            }
        )));
    }
    let ws = Arc::new(Workspace::open(workspace)?);
    let (registry, packs) = crate::packs::registry_for(Some(&config), &ws, None)?;
    let served: Vec<String> = packs
        .iter()
        .find(|p| p.name() == pack)
        .ok_or_else(|| Error::op("tools", format!("packs.{pack} was not built")))?
        .tools()?
        .iter()
        .map(|t| t.manifest.name.clone())
        .collect();
    if !served.iter().any(|t| t == tool) {
        return Err(Error::usage(format!(
            "packs.{pack} has no tool {tool}; its tools are {}",
            served.join(", ")
        )));
    }
    let command = clap::Command::new("call")
        .subcommand_required(true)
        .subcommands(crate::workspace_cmd::op_commands(&registry, &[]));
    let mut argv: Vec<String> = vec!["call".to_string(), tool.to_string()];
    argv.extend(args.iter().cloned());
    argv.push("--workspace".to_string());
    argv.push(workspace.display().to_string());
    let matches = match command.try_get_matches_from(argv) {
        Ok(matches) => matches,
        Err(error) => {
            let _ = error.print();
            return Ok(match error.kind() {
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
                    crate::exit::OK
                }
                _ => crate::exit::USAGE,
            });
        }
    };
    let Some((name, sub)) = matches.subcommand() else {
        return Err(Error::usage("tools call: no tool"));
    };
    Ok(crate::workspace_cmd::run_op(&registry, name, sub))
}

/// Where a bundle comes from: a directory, a `.tar.gz` archive, or the URL of one (the one
/// network step), with the archive's SHA-256 required for a URL and checked for a file when
/// given. An archive is unpacked under a scratch directory that lives as long as the install.
pub struct BundleSource {
    pub root: PathBuf,
    _scratch: Option<tempdir::Scratch>,
}

/// A scratch directory removed on drop.
mod tempdir {
    use std::path::{Path, PathBuf};

    pub struct Scratch(PathBuf);

    impl Scratch {
        pub fn new(name: &str) -> std::io::Result<Scratch> {
            let dir = std::env::temp_dir().join(format!(
                "rascal-setup-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or_default()
            ));
            std::fs::create_dir_all(&dir)?;
            Ok(Scratch(dir))
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

/// The SHA-256 of bytes as `sha256:<hex>`.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!(
        "sha256:{}",
        rascal_core::canonical::to_hex(&sha2::Sha256::digest(bytes))
    )
}

/// Downloads the archive at `url` (https or http, no redirects off the host's scheme), with
/// a size ceiling and a timeout; the bytes are checked against `expected` by the caller.
fn download(url: &str) -> Result<Vec<u8>> {
    const MAX_BYTES: usize = 4 << 30;
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| Error::op("setup", format!("no runtime for the download: {e}")))?;
    // The TLS provider is process-wide; installing it twice is not an error here.
    let _ = rustls::crypto::ring::default_provider().install_default();
    rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3600))
            .build()
            .map_err(|e| Error::op("setup", format!("client: {e}")))?;
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| Error::op("setup", format!("{url}: {e}")))?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::op("setup", format!("{url}: HTTP {status}")));
        }
        if let Some(length) = response.content_length()
            && length as usize > MAX_BYTES
        {
            return Err(Error::op(
                "setup",
                format!("{url}: {length} bytes is over the {MAX_BYTES} byte ceiling"),
            ));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| Error::op("setup", format!("{url}: {e}")))?;
        if bytes.len() > MAX_BYTES {
            return Err(Error::op(
                "setup",
                format!("{url}: over the {MAX_BYTES} byte ceiling"),
            ));
        }
        Ok(bytes.to_vec())
    })
}

/// Unpacks a `.tar.gz` of a bundle into `scratch` and returns the directory holding
/// `bundle.toml`: the archive's single top-level directory, or the scratch root.
fn unpack(bytes: &[u8], scratch: &Path, what: &str) -> Result<PathBuf> {
    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    archive.set_overwrite(false);
    archive.set_preserve_permissions(false);
    archive.unpack(scratch).map_err(|e| {
        Error::op(
            "setup",
            format!("{what}: not a bundle archive I can unpack: {e}"),
        )
    })?;
    if scratch.join("bundle.toml").is_file() {
        return Ok(scratch.to_path_buf());
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(scratch)
        .map_err(|e| Error::io(scratch, e))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    match dirs.as_slice() {
        [only] if only.join("bundle.toml").is_file() => Ok(dirs.remove(0)),
        _ => Err(Error::usage(format!(
            "{what}: the archive holds no bundle.toml at its root or in a single top-level directory"
        ))),
    }
}

impl BundleSource {
    /// Resolves `--bundle`: a URL (needs `sha256`), an archive file (`sha256` checked when
    /// given), or a directory (no digest to check; `sha256` is refused).
    pub fn resolve(bundle: &str, sha256: Option<&str>) -> Result<BundleSource> {
        let expected = sha256.map(|d| {
            let d = d.trim().to_ascii_lowercase();
            if d.starts_with("sha256:") {
                d
            } else {
                format!("sha256:{d}")
            }
        });
        if bundle.starts_with("https://") || bundle.starts_with("http://") {
            let Some(expected) = expected else {
                return Err(Error::usage(
                    "--bundle <url> needs --sha256 <digest> (the .sha256 file beside the archive)",
                ));
            };
            println!("bundle: downloading {bundle}");
            let bytes = download(bundle)?;
            let got = sha256_hex(&bytes);
            if got != expected {
                return Err(Error::usage(format!(
                    "{bundle}: the archive's digest is {got}, not {expected}; nothing was unpacked"
                )));
            }
            let scratch =
                tempdir::Scratch::new("url").map_err(|e| Error::io(Path::new("<temp>"), e))?;
            let root = unpack(&bytes, scratch.path(), bundle)?;
            println!(
                "bundle: {} bytes downloaded, digest {got}, unpacked",
                bytes.len()
            );
            return Ok(BundleSource {
                root,
                _scratch: Some(scratch),
            });
        }
        let path = Path::new(bundle);
        if path.is_file() {
            let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
            let got = sha256_hex(&bytes);
            if let Some(expected) = &expected
                && &got != expected
            {
                return Err(Error::usage(format!(
                    "{}: the archive's digest is {got}, not {expected}; nothing was unpacked",
                    path.display()
                )));
            }
            let scratch =
                tempdir::Scratch::new("archive").map_err(|e| Error::io(Path::new("<temp>"), e))?;
            let root = unpack(&bytes, scratch.path(), &path.display().to_string())?;
            println!("bundle: {} unpacked, digest {got}", path.display());
            return Ok(BundleSource {
                root,
                _scratch: Some(scratch),
            });
        }
        if expected.is_some() {
            return Err(Error::usage(
                "--sha256 goes with an archive or a URL; a directory's files are verified by its bundle.toml",
            ));
        }
        Ok(BundleSource {
            root: path.to_path_buf(),
            _scratch: None,
        })
    }
}

/// `rascal tools setup --bundle <dir|archive|url> [--sha256 <digest>] [--into <dir>]
/// [--config rascal.toml [--pack <name>]]`: the bundle fetched and unpacked when it is an
/// archive or a URL, verified, the platform checked, the destination checked, uv and the
/// interpreter found, the files copied, the environment synced offline, the launcher smoked,
/// and the pack entry to write printed. With `--config` and no `--into`, the install directory
/// is the one the entry's command points at (`<dir>/.venv/bin/rascal-tools`, a relative
/// `<dir>` taken from the configuration file's directory).
pub fn setup(
    bundle: &str,
    sha256: Option<&str>,
    into: Option<&Path>,
    config: Option<&Path>,
    pack: Option<&str>,
) -> Result<()> {
    let source = BundleSource::resolve(bundle, sha256)?;
    let bundle = Bundle::open(&source.root)?;
    bundle.check_platform()?;
    let configured = match config {
        Some(path) => {
            let config = Config::read(path)?;
            let (name, entry) = match pack {
                Some(name) => config.packs.get_key_value(name).ok_or_else(|| {
                    Error::usage(format!("no [packs.{name}] in {}", path.display()))
                })?,
                None => {
                    let mut candidates: Vec<_> = config
                        .packs
                        .iter()
                        .filter(|(_, e)| {
                            install_dir_of(&e.command, &bundle.manifest.pack).is_some()
                        })
                        .collect();
                    match candidates.len() {
                        1 => candidates.remove(0),
                        0 => {
                            return Err(Error::usage(format!(
                                "{}: no pack entry's command is <dir>/.venv/bin/{}; give --into",
                                path.display(),
                                bundle.manifest.pack
                            )));
                        }
                        _ => {
                            return Err(Error::usage(format!(
                                "{}: several pack entries install {}; give --pack",
                                path.display(),
                                bundle.manifest.pack
                            )));
                        }
                    }
                }
            };
            let dir = install_dir_of(&entry.command, &bundle.manifest.pack).ok_or_else(|| {
                Error::usage(format!(
                    "[packs.{name}]: command[0] is not <dir>/.venv/bin/{} with a directory before .venv; give --into",
                    bundle.manifest.pack
                ))
            })?;
            let dir = if dir.is_absolute() {
                dir
            } else {
                path.parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_default()
                    .join(dir)
            };
            Some((name.clone(), dir))
        }
        None => None,
    };
    let into = match (into, &configured) {
        (Some(into), _) => into.to_path_buf(),
        (None, Some((_, dir))) => dir.clone(),
        (None, None) => return Err(Error::usage("give --into <dir> or --config rascal.toml")),
    };
    bundle.check_destination(&into)?;
    bundle.verify()?;
    println!(
        "bundle: {} {} for {}, Python {}, {} files verified, digest {}",
        bundle.manifest.pack,
        bundle.manifest.pack_version,
        bundle.manifest.platform,
        bundle.manifest.python,
        bundle.manifest.files.len(),
        bundle.digest
    );
    let prerequisites = prerequisites(&bundle.manifest.python)?;
    println!(
        "prerequisites: {} and {}",
        prerequisites.uv_version,
        prerequisites.python.display()
    );
    let installed = bundle.install(&into, &prerequisites)?;
    println!(
        "installed: {} ({})",
        installed.launcher.display(),
        installed.version_line
    );
    let command = format!(
        "command = [{:?}, \"serve\"]",
        installed.launcher.display().to_string()
    );
    match configured {
        Some((name, dir)) if same_dir(&dir, &into) => {
            println!("[packs.{name}] already points here")
        }
        Some((name, _)) => println!("[packs.{name}]: set {command}"),
        None => println!("[packs.{}]: {command}", bundle.manifest.pack),
    }
    Ok(())
}
