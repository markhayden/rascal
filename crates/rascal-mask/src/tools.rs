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

/// A scratch directory removed on drop: created new (never an existing directory, which on a
/// shared temporary directory another user could have made and could write into between the
/// digest check and the install) and readable by its user only.
mod tempdir {
    use std::path::{Path, PathBuf};

    pub struct Scratch(PathBuf);

    impl Scratch {
        pub fn new(name: &str) -> std::io::Result<Scratch> {
            let stem = format!("rascal-setup-{name}-{}", std::process::id());
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
            for n in 0..1000u32 {
                let dir = std::env::temp_dir().join(format!("{stem}-{n}"));
                match builder.create(&dir) {
                    Ok(()) => return Ok(Scratch(dir)),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(e),
                }
            }
            Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("no new {stem}-<n> directory under the temporary directory"),
            ))
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

/// The largest archive a URL may deliver.
const MAX_DOWNLOAD: u64 = 4 << 30;

/// Downloads the archive at `url` (https or http; redirects followed, as a release asset
/// redirects to its storage host, since the digest and not the host is what is trusted), with
/// a timeout and a ceiling of `max` bytes held while the body streams, so a server that sends
/// no length, or a false one, cannot fill memory; the caller checks the bytes' digest.
fn download(url: &str, max: u64) -> Result<Vec<u8>> {
    let over = |got: String| {
        Error::op(
            "setup",
            format!("{url}: {got} is over the {max} byte ceiling; nothing was unpacked"),
        )
    };
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| Error::op("setup", format!("no runtime for the download: {e}")))?;
    // The TLS provider is process-wide; installing it twice is not an error here.
    let _ = rustls::crypto::ring::default_provider().install_default();
    rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3600))
            .build()
            .map_err(|e| Error::op("setup", format!("client: {e}")))?;
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|e| Error::op("setup", format!("{url}: {e}")))?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::op("setup", format!("{url}: HTTP {status}")));
        }
        if let Some(length) = response.content_length()
            && length > max
        {
            return Err(over(format!("a declared {length} bytes")));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| Error::op("setup", format!("{url}: {e}")))?
        {
            if (bytes.len() + chunk.len()) as u64 > max {
                return Err(over("the body".to_string()));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    })
}

/// The most an archive's files may hold together once unpacked: a full bundle is under a
/// gigabyte, so the ceiling stops a decompression bomb, not a bundle.
const MAX_UNPACKED: u64 = 16 << 30;

/// Unpacks a `.tar.gz` of a bundle into `scratch` and returns the directory holding
/// `bundle.toml`: the archive's single top-level directory, or the scratch root. A bundle
/// holds files and directories only, so a link (which `verify` and the copy would follow out
/// of the archive), a device or a fifo is refused, as is a path that is absolute or climbs
/// out with `..` (which the unpacker would otherwise skip in silence), and files that
/// together exceed `max` bytes; each refusal names the entry.
fn unpack(bytes: &[u8], scratch: &Path, what: &str, max: u64) -> Result<PathBuf> {
    use std::path::Component;
    let unreadable = |e: std::io::Error| {
        Error::op(
            "setup",
            format!("{what}: not a bundle archive I can unpack: {e}"),
        )
    };
    let refuse = |entry: &Path, why: &str| {
        Error::usage(format!(
            "{what}: {}: {why}; nothing was installed",
            entry.display()
        ))
    };
    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    archive.set_overwrite(false);
    archive.set_preserve_permissions(false);
    let mut total = 0u64;
    for entry in archive.entries().map_err(unreadable)? {
        let mut entry = entry.map_err(unreadable)?;
        let kind = entry.header().entry_type();
        if kind.is_pax_global_extensions() || kind.is_pax_local_extensions() {
            continue;
        }
        let path = entry.path().map_err(unreadable)?.into_owned();
        if !(kind.is_file() || kind.is_dir()) {
            return Err(refuse(
                &path,
                "not a file or a directory (a bundle archive holds no links or special files)",
            ));
        }
        if !path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(refuse(&path, "a path outside the archive"));
        }
        total = total.saturating_add(entry.header().size().map_err(unreadable)?);
        if total > max {
            return Err(refuse(
                &path,
                &format!("the files pass the {max} byte ceiling for an unpacked bundle"),
            ));
        }
        if !entry.unpack_in(scratch).map_err(unreadable)? {
            return Err(refuse(&path, "a path outside the archive"));
        }
    }
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
            let bytes = download(bundle, MAX_DOWNLOAD)?;
            let got = sha256_hex(&bytes);
            if got != expected {
                return Err(Error::usage(format!(
                    "{bundle}: the archive's digest is {got}, not {expected}; nothing was unpacked"
                )));
            }
            let scratch =
                tempdir::Scratch::new("url").map_err(|e| Error::io(Path::new("<temp>"), e))?;
            let root = unpack(&bytes, scratch.path(), bundle, MAX_UNPACKED)?;
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
            let root = unpack(
                &bytes,
                scratch.path(),
                &path.display().to_string(),
                MAX_UNPACKED,
            )?;
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// One raw tar entry: the name's bytes as written (no validation), the type, the data and
    /// a link target.
    struct Raw<'a> {
        name: &'a [u8],
        kind: tar::EntryType,
        data: &'a [u8],
        link: Option<&'a str>,
    }

    fn file<'a>(name: &'a str, data: &'a [u8]) -> Raw<'a> {
        Raw {
            name: name.as_bytes(),
            kind: tar::EntryType::Regular,
            data,
            link: None,
        }
    }

    /// A `.tar.gz` of the raw entries, the names written into the header unchecked, as a
    /// hostile archive would carry them.
    fn archive(entries: &[Raw<'_>]) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(gz);
        for entry in entries {
            let mut header = tar::Header::new_gnu();
            header.as_old_mut().name[..entry.name.len()].copy_from_slice(entry.name);
            header.set_entry_type(entry.kind);
            header.set_size(entry.data.len() as u64);
            header.set_mode(if entry.kind.is_dir() { 0o755 } else { 0o644 });
            if let Some(link) = entry.link {
                header.set_link_name(link).unwrap();
            }
            header.set_cksum();
            builder.append(&header, entry.data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn scratch(name: &str) -> tempdir::Scratch {
        tempdir::Scratch::new(&format!("unit-{name}")).unwrap()
    }

    #[test]
    fn a_sound_archive_unpacks_to_its_top_level_directory() {
        let dir = scratch("sound");
        let bytes = archive(&[
            Raw {
                name: b"b/",
                kind: tar::EntryType::Directory,
                data: b"",
                link: None,
            },
            file("b/bundle.toml", b"bundle_version = 1\n"),
            file("b/wheel/x.whl", b"123"),
        ]);
        let root = unpack(&bytes, dir.path(), "t", 1 << 20).unwrap();
        assert_eq!(root, dir.path().join("b"));
        assert_eq!(std::fs::read(root.join("wheel/x.whl")).unwrap(), b"123");
    }

    #[test]
    fn a_link_a_special_entry_or_a_path_out_of_the_archive_is_refused_by_name() {
        let cases: Vec<(&str, Vec<Raw<'_>>, &str)> = vec![
            (
                "symlink",
                vec![
                    file("b/bundle.toml", b"x"),
                    Raw {
                        name: b"b/uv.lock",
                        kind: tar::EntryType::Symlink,
                        data: b"",
                        link: Some("/etc/hosts"),
                    },
                ],
                "b/uv.lock",
            ),
            (
                "hardlink",
                vec![
                    file("b/bundle.toml", b"x"),
                    Raw {
                        name: b"b/uv.lock",
                        kind: tar::EntryType::Link,
                        data: b"",
                        link: Some("b/bundle.toml"),
                    },
                ],
                "b/uv.lock",
            ),
            (
                "fifo",
                vec![Raw {
                    name: b"b/pipe",
                    kind: tar::EntryType::Fifo,
                    data: b"",
                    link: None,
                }],
                "b/pipe",
            ),
            (
                "parent",
                vec![file("b/bundle.toml", b"x"), file("b/../../escaped", b"x")],
                "b/../../escaped",
            ),
            (
                "absolute",
                vec![file("/tmp/rascal-escaped", b"x")],
                "/tmp/rascal-escaped",
            ),
        ];
        for (name, entries, named) in cases {
            let dir = scratch(name);
            let err = unpack(&archive(&entries), dir.path(), "t", 1 << 20)
                .map(|p| p.display().to_string())
                .unwrap_err()
                .to_string();
            assert!(err.contains(named), "{name}: {err}");
            assert!(err.contains("nothing was installed"), "{name}: {err}");
        }
        assert!(!Path::new("/tmp/rascal-escaped").exists());
    }

    #[test]
    fn an_archive_whose_files_exceed_the_ceiling_is_refused_before_the_overflowing_entry() {
        let dir = scratch("ceiling");
        let big = vec![7u8; 600];
        let bytes = archive(&[
            file("b/bundle.toml", b"x"),
            file("b/one", &big),
            file("b/two", &big),
        ]);
        let err = unpack(&bytes, dir.path(), "t", 1000)
            .unwrap_err()
            .to_string();
        assert!(err.contains("1000 byte ceiling"), "{err}");
        assert!(dir.path().join("b/one").exists());
        assert!(!dir.path().join("b/two").exists());
    }

    /// Serves `body` once on the loopback, with a `Content-Length` header or, without one,
    /// to the end of the connection, as a server that streams would.
    fn serve(body: Vec<u8>, with_length: bool) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/b.tar.gz", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Some(Ok(mut stream)) = listener.incoming().next() {
                let mut buffer = [0u8; 4096];
                let _ = stream.read(&mut buffer);
                let length = if with_length {
                    format!("Content-Length: {}\r\n", body.len())
                } else {
                    String::new()
                };
                let head = format!("HTTP/1.1 200 OK\r\n{length}Connection: close\r\n\r\n");
                let _ = stream.write_all(head.as_bytes());
                for chunk in body.chunks(256) {
                    let _ = stream.write_all(chunk);
                }
            }
        });
        url
    }

    #[test]
    fn a_download_past_the_ceiling_is_refused_with_or_without_a_length() {
        let body = vec![1u8; 4000];
        for with_length in [true, false] {
            let err = download(&serve(body.clone(), with_length), 1000)
                .map(|b| b.len())
                .unwrap_err()
                .to_string();
            assert!(err.contains("1000 byte ceiling"), "{with_length}: {err}");
        }
        assert_eq!(download(&serve(body.clone(), false), 4000).unwrap(), body);
    }

    #[test]
    fn a_scratch_directory_is_always_new_and_private_to_its_user() {
        let a = tempdir::Scratch::new("fresh").unwrap();
        let b = tempdir::Scratch::new("fresh").unwrap();
        assert_ne!(a.path(), b.path(), "two scratches never share a directory");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(a.path()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "{mode:o}");
        }
    }
}
