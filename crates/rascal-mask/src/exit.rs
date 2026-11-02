//! Process exit codes from SPEC.md "Commands", and the mapping from errors to them.

use rascal_core::Error;

/// Everything succeeded.
pub const OK: i32 = 0;
/// The op reported `not_applicable` or `measured_nothing` and `--strict` was given.
pub const NOT_APPLICABLE: i32 = 1;
/// An operational error: a filler failed, a pack died, IO, non-convergence, a failed check.
pub const OPERATIONAL: i32 = 3;
/// A usage error: bad arguments, a missing command, an `--out` that is not allowed.
pub const USAGE: i32 = 64;
/// A `run`, `heal` or `call` cancelled by SIGINT (table-manners 6); the signal watchers in
/// `run_json` and `call` exit with it after the run's `run.json` is written.
pub const INTERRUPTED: i32 = 130;
/// The same, cancelled by SIGTERM.
pub const TERMINATED: i32 = 143;

/// The exit code for a core error.
pub fn code_for(error: &Error) -> i32 {
    match error {
        Error::Usage { .. }
        | Error::Input { .. }
        | Error::InvalidArgs { .. }
        | Error::Recipe { .. }
        | Error::StaleEstimate { .. } => USAGE,
        _ => OPERATIONAL,
    }
}
