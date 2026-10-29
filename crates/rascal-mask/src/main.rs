//! The `rascal` binary. Commands arrive with the tasks that own them (PLAN.md); task 1.1
//! brings `run` and `render`.

mod dash;
mod exit;
mod export;
mod heal;
mod inspect;
mod measure;
mod ops;
mod packs;
mod paste;
mod render;
mod run;
mod serve;
mod tools;
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
        /// A recorded run (its directory or workspace) whose pack calls answer instead of the
        /// packs; no pack is started.
        #[arg(long)]
        replay: Option<PathBuf>,
    },
    /// Run a recipe and heal through a configured filler: the fixed graph with the
    /// preservation check and the measures; writes result.png and the run page.
    Heal {
        /// A shipped recipe by name (seam, rect) or a recipe file.
        recipe: String,
        /// The image (JPEG or PNG).
        image: PathBuf,
        /// The filler entry in the configuration.
        #[arg(long)]
        filler: String,
        /// The configuration file (rascal.toml) naming the filler.
        #[arg(long)]
        config: PathBuf,
        /// The run directory: must be new or empty.
        #[arg(long)]
        out: PathBuf,
        /// Bind inputs.location to a box: x0,y0,x1,y1 as fractions of the image.
        #[arg(long, allow_hyphen_values = true, conflicts_with = "location")]
        bbox: Option<String>,
        /// Bind inputs.location to a location JSON file.
        #[arg(long)]
        location: Option<PathBuf>,
        /// Bind inputs.stroke to a stroke location JSON file.
        #[arg(long)]
        stroke: Option<PathBuf>,
        /// Bind inputs.protect to a stored mask directory.
        #[arg(long)]
        protect: Option<PathBuf>,
        /// Bind inputs.mask to a stored mask directory.
        #[arg(long)]
        mask: Option<PathBuf>,
        /// The prompt for the filler, when it takes one.
        #[arg(long)]
        prompt: Option<String>,
        /// Keep artifacts and the journal in this workspace instead of a private one under --out.
        #[arg(long)]
        workspace: Option<PathBuf>,
        /// The image's projection; without it an exactly 2:1 image is taken as equirectangular.
        #[arg(long)]
        projection: Option<ProjectionArg>,
        /// Do not write the run page beside the journal.
        #[arg(long)]
        no_dash: bool,
        /// Reuse the filler output a previous run recorded for the same request (no filler call).
        #[arg(long)]
        replay: Option<PathBuf>,
        /// Exit 1 when a step is not applicable.
        #[arg(long)]
        strict: bool,
    },
    /// The MCP server over stdio: every op as a tool over a workspace (SPEC "The MCP server").
    Serve {
        /// The workspace the tools read and write.
        #[arg(long)]
        workspace: PathBuf,
        /// The configuration file (rascal.toml); checked, and used by later milestones' tools.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Calls that may run at once; each still commits in turn.
        #[arg(long, default_value_t = 1)]
        concurrency: usize,
    },
    /// The configured tool packs: list their tools, or call one from the shell.
    Tools {
        #[command(subcommand)]
        command: ToolsCommand,
    },
    #[cfg(feature = "testkit")]
    #[command(hide = true)]
    FakePack {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
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
    /// The run page: the journal's steps, timings, envelopes and artifacts as one HTML page.
    Dash {
        /// A run directory or workspace.
        #[arg(required_unless_present = "panels")]
        root: Option<PathBuf>,
        /// Write the page here (overwritten).
        #[arg(long, conflicts_with = "serve")]
        out: Option<PathBuf>,
        /// List the registered panels, one per line, and exit.
        #[arg(long, conflicts_with_all = ["out", "serve"])]
        panels: bool,
        /// Serve the page on loopback and print the URL; each request rebuilds it.
        #[arg(long)]
        serve: bool,
        /// The loopback port for --serve (0 picks a free one).
        #[arg(long, default_value_t = 0)]
        port: u16,
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
    /// The measures of a composite: continuity, tone step, transition, texture match, drift.
    Measure {
        /// The image to measure (the composite): a file, or an id with --workspace.
        #[arg(long, required_unless_present = "list")]
        after: Option<String>,
        /// The image before, for drift: a file, or an id with --workspace.
        #[arg(long)]
        before: Option<String>,
        /// The mask: a directory (`<id>.mask/`), or an id with --workspace.
        #[arg(long, required_unless_present = "list")]
        mask: Option<String>,
        /// Measure artifacts of this workspace by id, journaling the invocation there.
        #[arg(long)]
        workspace: Option<PathBuf>,
        /// The measures to run, comma-separated (default: all five).
        #[arg(long)]
        set: Option<String>,
        /// Print the envelope as JSON (the summaries are its measurements).
        #[arg(long)]
        json: bool,
        /// Exit 1 when the envelope is not ok (measured nothing, not applicable).
        #[arg(long)]
        strict: bool,
        /// List the measures, one per line, and exit.
        #[arg(long, conflicts_with_all = ["after", "before", "mask", "set", "json"])]
        list: bool,
    },
    /// The testkit's fake filler as a command (test builds only): `<behaviour> <image> [<mask>] <out>`.
    #[cfg(feature = "testkit")]
    #[command(hide = true)]
    FakeFiller {
        behaviour: String,
        paths: Vec<PathBuf>,
    },
    /// Composite a fill onto a base through a stored mask (feather, hard; poisson with M7).
    Paste {
        /// The base image, in the mask's frame.
        base: PathBuf,
        /// The fill, in the mask's frame and size.
        fill: PathBuf,
        /// The mask directory (`<id>.mask/`).
        mask: PathBuf,
        /// feather (default), hard, or poisson.
        #[arg(long, default_value = "feather")]
        mode: String,
        /// The file to write; overwritten if it exists. PNG, or JPEG for .jpg.
        #[arg(long)]
        out: PathBuf,
        /// Exit 1 when the paste is not applicable (poisson before M7).
        #[arg(long)]
        strict: bool,
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

#[derive(Subcommand, Debug)]
enum ToolsCommand {
    /// Start every configured pack and print its tools as a client sees them.
    List {
        /// The configuration file (rascal.toml) naming the packs.
        #[arg(long)]
        config: PathBuf,
        /// Print the manifests as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Call one pack tool: `rascal tools call <pack> <tool> --workspace ws --image img-1 --prompt x`;
    /// the tool's arguments are those of its op subcommand (`--help` after the tool lists them).
    Call {
        /// The pack, as named in the configuration.
        pack: String,
        /// The tool.
        tool: String,
        /// The configuration file (rascal.toml) naming the pack.
        #[arg(long)]
        config: PathBuf,
        /// The workspace the inputs are in and the outputs go to.
        #[arg(long)]
        workspace: PathBuf,
        /// The tool's own arguments.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// Built-in command names that an op may not shadow.
const RESERVED: &[&str] = &[
    "run",
    "heal",
    "serve",
    "render",
    "inspect",
    "export",
    "paste",
    "measure",
    "fake-filler",
    "fake-pack",
    "tools",
    "dash",
    "ops",
    "workspace",
    "help",
];

/// `RASCAL_LOG=<level or filter>` turns on tracing spans to stderr; off by default.
fn init_tracing() {
    if let Ok(filter) = std::env::var("RASCAL_LOG")
        && !filter.trim().is_empty()
    {
        use tracing_subscriber::EnvFilter;
        let filter = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("info"));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .try_init();
    }
}

fn main() {
    init_tracing();
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
            replay,
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
                replay,
            };
            match run::run(&args) {
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
        Command::Heal {
            recipe,
            image,
            filler,
            config,
            out,
            bbox,
            location,
            stroke,
            protect,
            mask,
            prompt,
            workspace,
            projection,
            no_dash,
            replay,
            strict,
        } => {
            let args = heal::HealArgs {
                run: run::RunArgs {
                    recipe,
                    image,
                    out,
                    bbox,
                    location,
                    stroke,
                    protect,
                    mask,
                    prompt,
                    config: Some(config),
                    workspace,
                    projection: projection.map(Into::into),
                    strict,
                    replay: replay.clone(),
                },
                filler,
                no_dash,
                replay,
            };
            match heal::heal(&args) {
                Ok(report) => {
                    for (profile, path) in &report.renders {
                        println!("{profile}: {}", path.display());
                    }
                    if let Some(result) = &report.result {
                        println!("result: {}", result.display());
                    }
                    if let Some(dash) = &report.dash {
                        println!("dash: {}", dash.display());
                    }
                    match report.status {
                        Status::Ok => exit::OK,
                        status => {
                            eprintln!(
                                "rascal heal: step {} reported {}: {}; nothing healed",
                                report.stopped_at.unwrap_or_default(),
                                serde_json::to_string(&status)
                                    .unwrap_or_default()
                                    .trim_matches('"'),
                                report.reason.unwrap_or_default()
                            );
                            if args.run.strict {
                                exit::NOT_APPLICABLE
                            } else {
                                exit::OK
                            }
                        }
                    }
                }
                Err(error) => {
                    eprintln!("rascal heal: {error}");
                    exit::code_for(&error)
                }
            }
        }
        Command::Serve {
            workspace,
            config,
            concurrency,
        } => match serve::serve(&serve::ServeArgs {
            workspace,
            config,
            concurrency,
        }) {
            Ok(()) => exit::OK,
            Err(error) => {
                eprintln!("rascal serve: {error}");
                exit::code_for(&error)
            }
        },
        Command::Tools { command } => {
            let result = match command {
                ToolsCommand::List { config, json } => {
                    tools::list(&config, json).map(|()| exit::OK)
                }
                ToolsCommand::Call {
                    pack,
                    tool,
                    config,
                    workspace,
                    args,
                } => tools::call(&pack, &tool, &config, &workspace, &args),
            };
            match result {
                Ok(code) => code,
                Err(error) => {
                    eprintln!("rascal tools: {error}");
                    exit::code_for(&error)
                }
            }
        }
        #[cfg(feature = "testkit")]
        Command::FakePack { args } => rascal_testkit::fake_pack::main(&args),
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
        Command::Dash {
            root,
            out,
            serve,
            port,
            panels,
        } => {
            let result = match (root, out, serve) {
                _ if panels => {
                    for name in dash::panels().names() {
                        println!("{name}");
                    }
                    Ok(())
                }
                (Some(root), Some(out), _) => {
                    dash::write(&root, &out).map(|p| println!("{}", p.display()))
                }
                (Some(root), None, true) => dash::serve(&root, port),
                _ => Err(Error::usage("dash: give --out <file> or --serve")),
            };
            match result {
                Ok(()) => exit::OK,
                Err(error) => {
                    eprintln!("rascal dash: {error}");
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
        Command::Measure {
            after,
            before,
            mask,
            workspace,
            set,
            json,
            strict,
            list,
        } => {
            if list {
                println!("{}", measure::list());
                exit::OK
            } else {
                let result = match &workspace {
                    Some(ws) => measure::measure_in_workspace(
                        &registry,
                        ws,
                        after.as_deref().unwrap_or_default(),
                        before.as_deref(),
                        mask.as_deref().unwrap_or_default(),
                        set.as_deref(),
                    ),
                    None => measure::measure(
                        &registry,
                        &measure::MeasureArgs {
                            before: before.map(PathBuf::from),
                            after: PathBuf::from(after.unwrap_or_default()),
                            mask: PathBuf::from(mask.unwrap_or_default()),
                            set,
                        },
                    ),
                };
                match result {
                    Ok(envelope) => {
                        if json {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&envelope).unwrap_or_default()
                            );
                        } else {
                            println!("{}", envelope.summary);
                            for m in &envelope.measurements {
                                println!("{:<28} {:>14.4} {}", m.name, m.value, m.unit);
                            }
                        }
                        if envelope.status == Status::Ok || !strict {
                            exit::OK
                        } else {
                            exit::NOT_APPLICABLE
                        }
                    }
                    Err(error) => {
                        eprintln!("rascal measure: {error}");
                        exit::code_for(&error)
                    }
                }
            }
        }
        #[cfg(feature = "testkit")]
        Command::FakeFiller { behaviour, paths } => {
            use rascal_testkit::fake_filler::{Behaviour, run_command};
            let Some(behaviour) = Behaviour::parse(&behaviour) else {
                eprintln!("rascal fake-filler: unknown behaviour {behaviour}");
                std::process::exit(exit::USAGE);
            };
            match paths.as_slice() {
                [image, out] => run_command(behaviour, image, None, out),
                [image, mask, out] => run_command(behaviour, image, Some(mask), out),
                _ => {
                    eprintln!("rascal fake-filler: <behaviour> <image> [<mask>] <out>");
                    exit::USAGE
                }
            }
        }
        Command::Paste {
            base,
            fill,
            mask,
            mode,
            out,
            strict,
        } => {
            let args = paste::PasteArgs {
                base,
                fill,
                mask,
                mode,
                out,
            };
            match paste::paste(&registry, &args) {
                Ok((path, Status::Ok, _)) => {
                    println!("{}", path.display());
                    exit::OK
                }
                Ok((_, status, reason)) => {
                    eprintln!(
                        "rascal paste: {}: {reason}; nothing written",
                        serde_json::to_string(&status)
                            .unwrap_or_default()
                            .trim_matches('"')
                    );
                    if strict {
                        exit::NOT_APPLICABLE
                    } else {
                        exit::OK
                    }
                }
                Err(error) => {
                    eprintln!("rascal paste: {error}");
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
