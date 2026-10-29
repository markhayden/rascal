//! `rascal workspace list <dir>` and `rascal workspace import <dir> <image>`, and the per-op
//! subcommands every op gets from its manifest: `rascal <op> --workspace <dir> --<port> <id>
//! [--<param> <value>] [--constants <set>]`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, ArgMatches, Command};
use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::frame::Projection;
use rascal_core::mask::Mask;
use rascal_core::op::{Inputs, ParamKind, Params};
use rascal_core::registry::OpRegistry;
use rascal_core::workspace::Workspace;

use crate::exit;

/// Lists the snapshot of a workspace.
pub fn list(dir: &Path, type_filter: Option<&str>, json: bool) -> Result<()> {
    let ws = Workspace::open(dir)?;
    let rows: Vec<_> = ws
        .list()?
        .into_iter()
        .filter(|(kind, _, _, _)| type_filter.is_none_or(|t| t.eq_ignore_ascii_case(kind)))
        .collect();
    if json {
        let value: Vec<serde_json::Value> = rows
            .iter()
            .map(|(kind, id, identity, frame)| serde_json::json!({ "type": kind, "id": id, "identity": identity, "frame": frame }))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
    } else {
        for (kind, id, identity, frame) in &rows {
            println!("{kind:<12} {id:<10} {frame:<34} {identity}");
        }
    }
    Ok(())
}

/// Imports an image into a workspace and prints its id.
pub fn import(dir: &Path, image: &Path, projection: Option<Projection>) -> Result<()> {
    let ws = Workspace::open(dir)?;
    let (id, frame, _) = ws.import(image, projection)?;
    println!("{id} {}", frame.id);
    Ok(())
}

/// Repairs a workspace and prints the report.
pub fn repair(dir: &Path, json: bool) -> Result<()> {
    let report = Workspace::repair(dir)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else {
        println!(
            "repaired {}: {} abandoned staging removed, {} orphans removed, {} missing entries dropped{}, journal rebuilt with {} lines",
            dir.display(),
            report.abandoned_staging_removed.len(),
            report.orphans_removed.len(),
            report.missing_dropped.len(),
            report
                .repaired_generation
                .map(|g| format!(" (generation {g})"))
                .unwrap_or_default(),
            report.journal_lines
        );
    }
    Ok(())
}

/// The per-op subcommands, one per manifest (built-in command names are skipped).
pub fn op_commands(registry: &OpRegistry, reserved: &[&str]) -> Vec<Command> {
    registry
        .manifests()
        .into_iter()
        .filter(|m| !reserved.contains(&m.name.as_str()))
        .map(|m| {
            let mut command = Command::new(m.name.clone())
                .about(m.purpose.clone())
                .long_about(format!(
                    "{}\n\nWhen to use: {}\nWhen not to use: {}\nOutput: {}",
                    m.purpose, m.when_to_use, m.when_not_to_use, m.output_guidance
                ))
                .arg(
                    Arg::new("workspace")
                        .long("workspace")
                        .value_name("DIR")
                        .value_parser(clap::value_parser!(PathBuf))
                        .required(true)
                        .help("The workspace the inputs are in and the outputs go to."),
                );
            for port in &m.inputs {
                let help = format!(
                    "{} ({}{})",
                    port.purpose,
                    port.artifact_type,
                    if port.required { ", required" } else { "" }
                );
                command = command.arg(
                    Arg::new(format!("port:{}", port.name))
                        .long(port.name.clone())
                        .value_name("ID")
                        .required(port.required)
                        .help(help),
                );
            }
            for param in &m.params {
                let help = format!(
                    "{}{}",
                    param.purpose,
                    param
                        .default
                        .as_ref()
                        .map(|d| format!(" (default {d})"))
                        .unwrap_or_default()
                );
                command = command.arg(
                    Arg::new(format!("param:{}", param.name))
                        .long(param.name.clone())
                        .value_name("VALUE")
                        .required(param.required)
                        .allow_hyphen_values(true)
                        .help(help),
                );
            }
            if !m.constants.is_empty() {
                command = command.arg(
                    Arg::new("constants")
                        .long("constants")
                        .value_name("SET")
                        .help(format!(
                            "The constant set: one of {} (default {})",
                            m.constants.join(", "),
                            m.constants[0]
                        )),
                );
            }
            command = command
                .arg(
                    Arg::new("strict")
                        .long("strict")
                        .action(ArgAction::SetTrue)
                        .help("Exit 1 when the op reports not_applicable or measured_nothing."),
                )
                .arg(
                    Arg::new("out")
                        .long("out")
                        .value_name("FILE")
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("Also write the op's image output to this file (PNG), overwriting it."),
                )
                .arg(
                    Arg::new("projection")
                        .long("projection")
                        .value_name("flat|equirectangular")
                        .help("How an image given as a path is read; without it an exactly 2:1 image is equirectangular."),
                );
            command
        })
        .collect()
}

/// Runs an op subcommand and returns the exit code.
pub fn run_op(registry: &OpRegistry, name: &str, matches: &ArgMatches) -> i32 {
    match run_op_inner(registry, name, matches) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("rascal {name}: {error}");
            exit::code_for(&error)
        }
    }
}

fn run_op_inner(registry: &OpRegistry, name: &str, matches: &ArgMatches) -> Result<i32> {
    let manifest = registry.require(name)?.manifest();
    let dir: &PathBuf = matches
        .get_one::<PathBuf>("workspace")
        .ok_or_else(|| Error::usage("--workspace is required"))?;
    let ws = Workspace::open(dir)?;
    let mut inputs: Inputs = BTreeMap::new();
    for port in &manifest.inputs {
        if let Some(id) = matches.get_one::<String>(&format!("port:{}", port.name)) {
            let path = Path::new(id);
            let artifact = if port.artifact_type == ArtifactType::Image && path.is_file() {
                // A path names an image to import first; the import is journaled like any other.
                let projection =
                    matches
                        .get_one::<String>("projection")
                        .and_then(|p| match p.as_str() {
                            "flat" => Some(Projection::Flat),
                            "equirectangular" => Some(Projection::Equirectangular),
                            _ => None,
                        });
                ws.import(path, projection)?.2
            } else if port.artifact_type == ArtifactType::Mask && path.join("mask.toml").is_file() {
                Artifact::Mask(std::sync::Arc::new(Mask::read(path)?))
            } else {
                ws.load(id)?
            };
            inputs.insert(port.name.clone(), artifact);
        }
    }
    let mut params = Params::new();
    for param in &manifest.params {
        if let Some(text) = matches.get_one::<String>(&format!("param:{}", param.name)) {
            params.insert(param.name.clone(), parse_param(param.kind, text)?);
        }
    }
    if !manifest.constants.is_empty()
        && let Some(c) = matches.get_one::<String>("constants")
    {
        params.insert("constants".to_string(), toml::Value::String(c.clone()));
    }
    let strict = matches.get_flag("strict");
    let invocation = ws.invoke(name, name, registry, &inputs, &params)?;
    let status = invocation.output.envelope.status;
    for (port, id) in &invocation.ids {
        println!(
            "{port}: {id} {}",
            invocation
                .output
                .envelope
                .artifacts
                .get(port)
                .map(|a| a.identity.as_str())
                .unwrap_or("")
        );
    }
    if let Some(c) = &invocation.committed {
        println!("committed: generation {} seq {}", c.generation, c.seq);
    }
    if let Some(out) = matches.get_one::<PathBuf>("out") {
        let images: Vec<_> = invocation
            .output
            .outputs
            .values()
            .filter_map(|a| match a {
                Artifact::Image(image) => Some(image.clone()),
                _ => None,
            })
            .collect();
        match images.as_slice() {
            [image] => {
                let out = rascal_core::outdir::prepare(out, rascal_core::outdir::OutKind::File)?;
                rascal_core::canonical::write_png(&out, &image.pixels)?;
                println!("{}", out.display());
            }
            [] => {
                return Err(Error::usage(format!(
                    "{name} produced no image to write to --out"
                )));
            }
            _ => {
                return Err(Error::usage(format!(
                    "{name} produced several images; --out takes one"
                )));
            }
        }
    }
    match status {
        rascal_core::op::Status::Ok => Ok(exit::OK),
        other => {
            eprintln!(
                "rascal {name}: {}: {}",
                serde_json::to_string(&other)
                    .unwrap_or_default()
                    .trim_matches('"'),
                invocation.output.envelope.reason.unwrap_or_default()
            );
            Ok(if strict {
                exit::NOT_APPLICABLE
            } else {
                exit::OK
            })
        }
    }
}

/// A command-line value as the parameter's kind: numbers and booleans parsed, arrays as
/// comma-separated numbers, strings and choices as given.
fn parse_param(kind: ParamKind, text: &str) -> Result<toml::Value> {
    let bad = |what: &str| Error::usage(format!("'{text}' is not {what}"));
    Ok(match kind {
        ParamKind::Number => {
            if let Ok(i) = text.parse::<i64>() {
                toml::Value::Integer(i)
            } else {
                toml::Value::Float(text.parse::<f64>().map_err(|_| bad("a number"))?)
            }
        }
        ParamKind::Integer => {
            toml::Value::Integer(text.parse::<i64>().map_err(|_| bad("an integer"))?)
        }
        ParamKind::Boolean => {
            toml::Value::Boolean(text.parse::<bool>().map_err(|_| bad("true or false"))?)
        }
        ParamKind::String | ParamKind::Choice => toml::Value::String(text.to_string()),
        ParamKind::PointList => toml::Value::Array(
            text.split(';')
                .map(|point| {
                    let pair: Vec<toml::Value> = point
                        .split(',')
                        .map(|p| {
                            p.trim()
                                .parse::<f64>()
                                .map(toml::Value::Float)
                                .map_err(|_| bad("a list of x,y points separated by ';'"))
                        })
                        .collect::<Result<_>>()?;
                    if pair.len() != 2 {
                        return Err(bad("a list of x,y points separated by ';'"));
                    }
                    Ok(toml::Value::Array(pair))
                })
                .collect::<Result<_>>()?,
        ),
        ParamKind::NumberArray => toml::Value::Array(
            text.split(',')
                .map(|p| {
                    p.trim()
                        .parse::<f64>()
                        .map(toml::Value::Float)
                        .map_err(|_| bad("a list of numbers"))
                })
                .collect::<Result<_>>()?,
        ),
    })
}
