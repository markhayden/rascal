//! `rascal ops list [--json]` and `rascal ops describe <op> [--json]`: the manifests as a client
//! sees them.

use rascal_core::registry::OpRegistry;

use crate::OpsCommand;
use crate::exit;

/// Runs an `ops` command and returns the exit code.
pub fn run(registry: &OpRegistry, command: OpsCommand) -> i32 {
    match command {
        OpsCommand::List { json } => {
            let manifests = registry.manifests();
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&manifests).unwrap_or_default()
                );
            } else {
                for m in &manifests {
                    println!("{:<12} {:<8} {}", m.name, m.version, m.purpose);
                }
            }
            exit::OK
        }
        OpsCommand::Describe { op, json } => match registry.require(&op) {
            Ok(found) => {
                let m = found.manifest();
                if json {
                    println!("{}", serde_json::to_string_pretty(&m).unwrap_or_default());
                } else {
                    println!("{} {}", m.name, m.version);
                    println!("{}", m.purpose);
                    println!();
                    println!("when to use: {}", m.when_to_use);
                    println!("when not to use: {}", m.when_not_to_use);
                    println!("output: {}", m.output_guidance);
                    println!("cost: {:?}, about {} s", m.cost.tier, m.cost.seconds);
                    println!();
                    println!("inputs:");
                    for p in &m.inputs {
                        println!(
                            "  {} ({}{}): {}",
                            p.name,
                            p.artifact_type,
                            if p.required { ", required" } else { "" },
                            p.purpose
                        );
                    }
                    println!("outputs:");
                    for p in &m.outputs {
                        println!("  {} ({}): {}", p.name, p.artifact_type, p.purpose);
                    }
                    if !m.params.is_empty() {
                        println!("parameters:");
                        for p in &m.params {
                            let default = p
                                .default
                                .as_ref()
                                .map(|d| format!(" (default {d})"))
                                .unwrap_or_default();
                            println!(
                                "  {} ({:?}{}{}): {}",
                                p.name,
                                p.kind,
                                if p.required { ", required" } else { "" },
                                default,
                                p.purpose
                            );
                        }
                    }
                }
                exit::OK
            }
            Err(error) => {
                eprintln!("rascal ops describe: {error}");
                exit::code_for(&error)
            }
        },
    }
}
