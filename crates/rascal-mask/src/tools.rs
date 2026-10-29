//! `rascal tools list --config rascal.toml`, `rascal tools call <pack> <tool> …` and
//! `rascal tools setup --bundle <dir>`: the configured packs from the shell. `list` starts
//! every pack and prints its tools as a client sees them; `call` starts one pack and runs one
//! tool as an op subcommand would, with the same arguments (`--image <id or path>`, the
//! tool's parameters, `--json`, `--out`); `setup` installs the Python pack from its bundle
//! with no network (`rascal_mcp::provision`).

use std::path::Path;
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::config::Config;
use rascal_core::error::Result;
use rascal_core::workspace::Workspace;
use rascal_mcp::client::Pack;
use rascal_mcp::provision::{Bundle, install_dir_of, prerequisites};

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

/// `rascal tools setup --bundle <dir> [--into <dir>] [--config rascal.toml [--pack <name>]]`:
/// the bundle verified, the platform checked, uv and the interpreter found, the files copied,
/// the environment synced offline, the launcher smoked, and the pack entry to write printed.
/// With `--config` and no `--into`, the install directory is the one the entry's command
/// points at (`<dir>/.venv/bin/rascal-tools`).
pub fn setup(
    bundle: &Path,
    into: Option<&Path>,
    config: Option<&Path>,
    pack: Option<&str>,
) -> Result<()> {
    let bundle = Bundle::open(bundle)?;
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
                    "[packs.{name}]: command[0] is not <dir>/.venv/bin/{}; give --into",
                    bundle.manifest.pack
                ))
            })?;
            Some((name.clone(), dir))
        }
        None => None,
    };
    let into = match (into, &configured) {
        (Some(into), _) => into.to_path_buf(),
        (None, Some((_, dir))) => dir.clone(),
        (None, None) => return Err(Error::usage("give --into <dir> or --config rascal.toml")),
    };
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
        Some((name, dir)) if dir == into => println!("[packs.{name}] already points here"),
        Some((name, _)) => println!("[packs.{name}]: set {command}"),
        None => println!("[packs.{}]: {command}", bundle.manifest.pack),
    }
    Ok(())
}
