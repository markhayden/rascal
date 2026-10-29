//! The `rascal` binary. Commands arrive with the tasks that own them (PLAN.md); task 1.1
//! brings `run` and `render`.

mod exit;
mod export;
mod inspect;
mod ops;
mod render;
mod run;
mod workspace_cmd;

use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{Parser, Subcommand, ValueEnum};
use rascal_core::Error;
use rascal_core::frame::Projection;
use rascal_core::op::Status;

/// The find-and-mask step of an image pipeline.
#[derive(Parser, Debug)]
#[command(name = "rascal", version, about, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run a recipe on an image and write the run directory.
    Run {
        /// A shipped recipe by name (rect) or a recipe file.
        recipe: String,
        /// The image (JPEG or PNG).
        image: PathBuf,
        /// The run directory: must be new or empty.
        #[arg(long)]
        out: PathBuf,
        /// Bind inputs.location to a box: x0,y0,x1,y1 as fractions of the image. A value may
        /// start with a minus sign (a box reaching past the left edge of a flat image).
        #[arg(long, allow_hyphen_values = true, conflicts_with = "location")]
        bbox: Option<String>,
        /// Bind inputs.location to a location JSON file (any kind) in the image's frame.
        #[arg(long)]
        location: Option<PathBuf>,
        /// Bind inputs.stroke to a stroke location JSON file in the image's frame.
        #[arg(long)]
        stroke: Option<PathBuf>,
        /// Bind inputs.protect to a stored mask directory in the image's frame.
        #[arg(long)]
        protect: Option<PathBuf>,
        /// Bind inputs.mask to a stored mask directory in the image's frame.
        #[arg(long)]
        mask: Option<PathBuf>,
        /// Bind inputs.prompt through a configured pack (arrives with M5).
        #[arg(long)]
        prompt: Option<String>,
        /// The configuration file (rascal.toml).
        #[arg(long)]
        config: Option<PathBuf>,
        /// Keep artifacts and the journal in this workspace instead of a private one under --out.
        #[arg(long)]
        workspace: Option<PathBuf>,
        /// The image's projection; without it an exactly 2:1 image is taken as equirectangular.
        #[arg(long)]
        projection: Option<ProjectionArg>,
        /// Exit 1 when the recipe stops at a step that is not applicable.
        #[arg(long)]
        strict: bool,
    },
    /// A workspace: list what it holds, or import an image into it.
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// The op catalogue: every manifest as a client sees it.
    Ops {
        #[command(subcommand)]
        command: OpsCommand,
    },
    /// Inspect a stored mask: the record and zone statistics (--json), the overlay and the
    /// per-row chart (--out), or the mask rebuilt from its record and diffed (--replay).
    Inspect {
        /// The mask directory (`<workspace>/artifacts/<id>.mask/`).
        mask: PathBuf,
        /// Print the record, zone statistics and identity as JSON on stdout.
        #[arg(long)]
        json: bool,
        /// Write overlay.jpg, chart.svg and inspect.json into this directory (new or empty).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Rebuild the mask from its operation record and the stored inputs, and diff the planes.
        #[arg(long)]
        replay: bool,
    },
    /// Project a native envelope or a location onto the copied schemas (ADR 0009).
    Export {
        /// The journal (journal.jsonl) to take the envelope from.
        journal: Option<PathBuf>,
        /// The step whose envelope to export.
        #[arg(long)]
        step: Option<String>,
        /// A location JSON file to export standalone instead of an envelope.
        #[arg(long)]
        location: Option<PathBuf>,
        /// Validate against the copied schemas and write the projection.
        #[arg(long)]
        compat: bool,
        /// The file to write; overwritten if it exists.
        #[arg(long)]
        out: PathBuf,
    },
    /// Render a stored mask for one consumer.
    Render {
        /// The mask directory (`<id>.mask/`).
        mask: PathBuf,
        /// The profile: grey, binary, alpha, tinted, overlay.
        #[arg(long = "for")]
        profile: String,
        /// The image to draw on (alpha, tinted, overlay); must be in the mask's frame.
        #[arg(long)]
        image: Option<PathBuf>,
        /// binary: 255 where w >= threshold / 255 (1 to 255, default 127).
        #[arg(long)]
        threshold: Option<i64>,
        /// tinted and overlay: the tint as #rrggbb (default #ff00ff).
        #[arg(long)]
        colour: Option<String>,
        /// tinted: how far the core moves toward the colour (0 to 1, default 0.7).
        #[arg(long)]
        strength: Option<f64>,
        /// The file to write; overwritten if it exists. PNG, or JPEG for .jpg.
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum WorkspaceCommand {
    /// Every frame, location and artifact in the current generation.
    List {
        dir: PathBuf,
        /// Only this type: Frame, Location, Image, Mask, BandProfile, Series.
        #[arg(long = "type")]
        type_filter: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Remove abandoned staging and orphans, drop entries whose payload is missing, rebuild
    /// the journal from the records.
    Repair {
        dir: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Import an image as a canonical image in its raw frame; prints its id.
    Import {
        dir: PathBuf,
        image: PathBuf,
        #[arg(long)]
        projection: Option<ProjectionArg>,
    },
}

#[derive(Subcommand, Debug)]
enum OpsCommand {
    /// Every op: name, version and purpose, or every manifest with --json.
    List {
        #[arg(long)]
        json: bool,
    },
    /// One op's manifest.
    Describe {
        op: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ProjectionArg {
    Flat,
    Equirectangular,
}

impl From<ProjectionArg> for Projection {
    fn from(value: ProjectionArg) -> Self {
        match value {
            ProjectionArg::Flat => Projection::Flat,
            ProjectionArg::Equirectangular => Projection::Equirectangular,
        }
    }
}

/// Built-in command names that an op may not shadow.
const RESERVED: &[&str] = &[
    "run",
    "render",
    "inspect",
    "export",
    "ops",
    "workspace",
    "help",
];

fn main() {
    let registry = match rascal_ops::registry() {
        Ok(registry) => registry,
        Err(error) => {
            eprintln!("rascal: {error}");
            std::process::exit(exit::OPERATIONAL);
        }
    };
    use clap::{CommandFactory, FromArgMatches};
    let command = Cli::command()
        .subcommands(workspace_cmd::op_commands(&registry, RESERVED))
        .subcommand_help_heading("Commands (ops take --workspace; see `rascal ops list`)");
    let matches = match command.try_get_matches() {
        Ok(matches) => matches,
        Err(error) => match error.kind() {
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                let code = if error.print().is_err() {
                    exit::USAGE
                } else {
                    exit::OK
                };
                std::process::exit(code);
            }
            _ => {
                eprint!("{error}");
                std::process::exit(exit::USAGE);
            }
        },
    };
    // An op subcommand is not in the derive; it is dispatched by name.
    if let Some((name, sub)) = matches.subcommand()
        && !RESERVED.contains(&name)
        && registry.get(name).is_some()
    {
        std::process::exit(workspace_cmd::run_op(&registry, name, sub));
    }
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => {
            eprint!("{error}");
            std::process::exit(exit::USAGE);
        }
    };
    let code = match cli.command {
        Command::Run {
            recipe,
            image,
            out,
            bbox,
            location,
            stroke,
            protect,
            mask,
            prompt,
            config,
            workspace,
            projection,
            strict,
        } => {
            let args = run::RunArgs {
                recipe,
                image,
                out,
                bbox,
                location,
                stroke,
                protect,
                mask,
                prompt,
                config,
                workspace,
                projection: projection.map(Into::into),
                strict,
            };
            match run::run(&registry, &args) {
                Ok(report) => {
                    for (profile, path) in &report.renders {
                        println!("{profile}: {}", path.display());
                    }
                    if let Some(dir) = &report.mask_dir {
                        println!("mask: {}", dir.display());
                    }
                    match report.status {
                        Status::Ok => exit::OK,
                        status => {
                            let step = report.stopped_at.unwrap_or_default();
                            eprintln!(
                                "rascal run: step {step} reported {}: {}; nothing published by it",
                                serde_json::to_string(&status)
                                    .unwrap_or_default()
                                    .trim_matches('"'),
                                report.reason.unwrap_or_default()
                            );
                            if args.strict {
                                exit::NOT_APPLICABLE
                            } else {
                                exit::OK
                            }
                        }
                    }
                }
                Err(error) => {
                    eprintln!("rascal run: {error}");
                    exit::code_for(&error)
                }
            }
        }
        Command::Workspace { command } => {
            let result = match command {
                WorkspaceCommand::List {
                    dir,
                    type_filter,
                    json,
                } => workspace_cmd::list(&dir, type_filter.as_deref(), json),
                WorkspaceCommand::Import {
                    dir,
                    image,
                    projection,
                } => workspace_cmd::import(&dir, &image, projection.map(Into::into)),
                WorkspaceCommand::Repair { dir, json } => workspace_cmd::repair(&dir, json),
            };
            match result {
                Ok(()) => exit::OK,
                Err(error) => {
                    eprintln!("rascal workspace: {error}");
                    exit::code_for(&error)
                }
            }
        }
        Command::Ops { command } => ops::run(&registry, command),
        Command::Inspect {
            mask,
            json,
            out,
            replay,
        } => {
            let result = (|| -> rascal_core::error::Result<()> {
                if !json && out.is_none() && !replay {
                    return Err(Error::usage(
                        "inspect: give --json, --out <dir> or --replay",
                    ));
                }
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&inspect::inspect_json(&mask)?)
                            .unwrap_or_default()
                    );
                }
                if let Some(out) = &out {
                    for path in inspect::inspect_pages(&registry, &mask, out)? {
                        println!("{}", path.display());
                    }
                }
                if replay {
                    let report = inspect::inspect_replay(&registry, &mask)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).unwrap_or_default()
                    );
                    if !report.is_clean() {
                        return Err(Error::op(
                            "inspect",
                            format!(
                                "replay differs on {:?}",
                                report.differing_planes.keys().collect::<Vec<_>>()
                            ),
                        ));
                    }
                }
                Ok(())
            })();
            match result {
                Ok(()) => exit::OK,
                Err(error) => {
                    eprintln!("rascal inspect: {error}");
                    exit::code_for(&error)
                }
            }
        }
        Command::Export {
            journal,
            step,
            location,
            compat,
            out,
        } => match export::run(&journal, step.as_deref(), &location, compat, &out) {
            Ok(path) => {
                println!("{}", path.display());
                exit::OK
            }
            Err(error) => {
                eprintln!("rascal export: {error}");
                exit::code_for(&error)
            }
        },
        Command::Render {
            mask,
            profile,
            image,
            threshold,
            colour,
            strength,
            out,
        } => {
            let args = render::RenderArgs {
                mask,
                profile,
                image,
                threshold,
                colour,
                strength,
                out,
            };
            match render::render(&registry, &args) {
                Ok(path) => {
                    println!("{}", path.display());
                    exit::OK
                }
                Err(error) => {
                    eprintln!("rascal render: {error}");
                    exit::code_for(&error)
                }
            }
        }
    };
    std::process::exit(code);
}
