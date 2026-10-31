//! `rascal playground [args…]`: starts the shared playground program over the owner's tools
//! (table-manners 7; ADR 0020). rascal carries no UI and no server for it: the command finds the
//! `playground` binary (`PLAYGROUND` when set, else `PATH`, else the data directory) and hands
//! over to it with `--tool rascal` and the arguments given, so one release of the playground
//! serves every tool.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the playground may be installed besides `PATH`: `$XDG_DATA_HOME/waffle/bin`, else
/// `~/.local/share/waffle/bin`.
fn data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .map(|d| d.join("waffle/bin"))
}

fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// The playground binary, when one is installed.
pub fn find() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("PLAYGROUND").map(PathBuf::from) {
        return executable(&explicit).then_some(explicit);
    }
    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    on_path
        .into_iter()
        .chain(data_dir())
        .map(|dir| dir.join("playground"))
        .find(|p| executable(p))
}

/// Runs the playground with `--tool rascal` and `args`, and returns its exit code; 69
/// (unavailable) with where to get it when none is installed.
pub fn run(args: &[String]) -> i32 {
    let Some(binary) = find() else {
        eprintln!(
            "rascal playground: the playground is not installed. It is a separate program shared by the waffle tools; install it so `playground` is on PATH (or in {}), or set PLAYGROUND to its path.",
            data_dir()
                .map(|d| d.display().to_string())
                .unwrap_or_else(|| "~/.local/share/waffle/bin".to_string())
        );
        return 69;
    };
    match Command::new(&binary)
        .arg("--tool")
        .arg("rascal")
        .args(args)
        .status()
    {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("rascal playground: {}: {error}", binary.display());
            69
        }
    }
}
