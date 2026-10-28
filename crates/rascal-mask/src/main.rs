//! The `rascal` binary. Commands arrive with the tasks that own them (PLAN.md); at task 0.1 the
//! binary knows its name and version and the exit-code contract from SPEC.md "Commands".

use clap::Parser;
use clap::error::ErrorKind;

/// Process exit codes from SPEC.md "Commands".
mod exit {
    /// Everything succeeded.
    pub const OK: i32 = 0;
    /// A usage error: bad arguments, a missing command, an `--out` that is not allowed.
    pub const USAGE: i32 = 64;
}

/// The find-and-mask step of an image pipeline.
#[derive(Parser, Debug)]
#[command(name = "rascal", version, about, arg_required_else_help = true)]
struct Cli {}

fn main() {
    let code = match Cli::try_parse() {
        Ok(Cli {}) => exit::OK,
        Err(error) => match error.kind() {
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                // `print` writes help and version to stdout, as clap defines them.
                if error.print().is_err() {
                    exit::USAGE
                } else {
                    exit::OK
                }
            }
            _ => {
                // Every other parse failure is a usage error on stderr, including the help
                // shown when no command is given.
                eprint!("{error}");
                exit::USAGE
            }
        },
    };
    std::process::exit(code);
}
