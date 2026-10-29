//! `rascal tools list --config rascal.toml` and `rascal tools call <pack> <tool> …`: the
//! configured packs from the shell. `list` starts every pack and prints its tools as a client
//! sees them; `call` starts one pack and runs one tool as an op subcommand would, with the
//! same arguments (`--image <id or path>`, the tool's parameters, `--json`, `--out`).

use std::path::Path;
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::config::Config;
use rascal_core::error::Result;
use rascal_core::workspace::Workspace;
use rascal_mcp::client::Pack;

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
    let served: Vec<String> = packs[0]
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
