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
// 130 and 143: a `run` or `heal` cancelled by SIGINT or SIGTERM (table-manners 6); the signal
// watcher in `run_json` exits with them after writing `run.json`.

/// The exit code for a core error.
pub fn code_for(error: &Error) -> i32 {
    match error {
        Error::Usage { .. } | Error::InvalidArgs { .. } | Error::Recipe { .. } => USAGE,
        _ => OPERATIONAL,
    }
}
