//! The `rascal` binary. Commands arrive with the tasks that own them (PLAN.md); task 1.1
//! brings `run` and `render`.

mod exit;
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
        /// Bind inputs.location to a box: x0,y0,x1,y1 as fractions of the image.
        #[arg(long)]
        bbox: Option<String>,
        /// The image's projection; without it an exactly 2:1 image is taken as equirectangular.
        #[arg(long)]
        projection: Option<ProjectionArg>,
        /// Exit 1 when the recipe stops at a step that is not applicable.
        #[arg(long)]
        strict: bool,
    },
    /// Render a stored mask for one consumer.
    Render {
        /// The mask directory (`<id>.mask/`).
        mask: PathBuf,
        /// The profile: grey.
        #[arg(long = "for")]
        profile: String,
        /// The file to write; overwritten if it exists.
        #[arg(long)]
        out: PathBuf,
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
    let registry = rascal_ops::registry();
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
                                "rascal run: step {step} reported {}; nothing published",
                                serde_json::to_string(&status)
                                    .unwrap_or_default()
                                    .trim_matches('"')
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
        Command::Render { mask, profile, out } => {
            match run::render(&registry, &mask, &profile, &out) {
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
