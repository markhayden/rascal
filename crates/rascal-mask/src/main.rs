//! The `rascal` binary. Commands arrive with the tasks that own them (PLAN.md); task 1.1
//! brings `run` and `render`.

mod exit;
mod inspect;
mod ops;
mod render;
mod run;

use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{Parser, Subcommand, ValueEnum};
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
        #[arg(long, allow_hyphen_values = true)]
        bbox: Option<String>,
        /// The image's projection; without it an exactly 2:1 image is taken as equirectangular.
        #[arg(long)]
        projection: Option<ProjectionArg>,
        /// Exit 1 when the recipe stops at a step that is not applicable.
        #[arg(long)]
        strict: bool,
    },
    /// The op catalogue: every manifest as a client sees it.
    Ops {
        #[command(subcommand)]
        command: OpsCommand,
    },
    /// Inspect a stored mask: with --json, the record, zone statistics and identity.
    Inspect {
        /// The mask directory (`<id>.mask/`).
        mask: PathBuf,
        /// Print JSON on stdout.
        #[arg(long)]
        json: bool,
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

fn main() {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
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
    let registry = match rascal_ops::registry() {
        Ok(registry) => registry,
        Err(error) => {
            eprintln!("rascal: {error}");
            std::process::exit(exit::OPERATIONAL);
        }
    };
    let code = match cli.command {
        Command::Run {
            recipe,
            image,
            out,
            bbox,
            projection,
            strict,
        } => {
            let args = run::RunArgs {
                recipe,
                image,
                out,
                bbox,
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
        Command::Ops { command } => ops::run(&registry, command),
        Command::Inspect { mask, json } => {
            let result = if json {
                inspect::inspect_json(&mask).map(|value| {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&value).unwrap_or_default()
                    )
                })
            } else {
                inspect::inspect_pages(&mask)
            };
            match result {
                Ok(()) => exit::OK,
                Err(error) => {
                    eprintln!("rascal inspect: {error}");
                    exit::code_for(&error)
                }
            }
        }
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
