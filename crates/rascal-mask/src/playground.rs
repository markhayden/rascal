//! `rascal playground`: hands off to Wafflery, the shared local program over the owner's
//! tools, by its launcher agreement (Wafflery H1; ADR 0020; decision 14). rascal carries no UI
//! and no server for it: the command finds `wafflery` (`WAFFLERY_BIN`, else `PATH`, else
//! `~/.local/bin`), runs it with exactly `--tool rascal`, and keeps its exit code. Wafflery
//! owns everything after that: the host, the browser, the library.
//!
//! When it is missing, a release build says where to get it and exits 69: the installer waits
//! for an approved release artifact. The offer to install is reachable only through the testkit's
//! seam (`--installer`), as squint's is, so the journey is tested with a fake installer.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The program's name.
const PROGRAM: &str = "wafflery";

/// The variable that names the program explicitly.
const OVERRIDE: &str = "WAFFLERY_BIN";

/// The program or its release is unavailable.
const UNAVAILABLE: i32 = 69;
/// The installation failed.
const IO_ERROR: i32 = 74;

/// The testkit's seam: an installer to offer, and whether to treat stdin as a terminal.
#[derive(Debug, Clone, Default)]
pub struct Seam {
    /// A program run as `<installer> --prefix <~/.local/bin>` that leaves `wafflery` there.
    pub installer: Option<PathBuf>,
    /// Take the answer from stdin as if it were a terminal.
    pub terminal: bool,
}

/// `~/.local/bin`, where the program is installed besides `PATH`.
fn local_bin() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/bin"))
}

/// An executable regular file (a symlink to one counts).
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Where the program is: `Ok(None)` when it is not installed; `Err` when `WAFFLERY_BIN` is set
/// but wrong, which never falls through to another program.
pub fn find() -> Result<Option<PathBuf>, String> {
    if let Some(explicit) = std::env::var_os(OVERRIDE) {
        let path = PathBuf::from(&explicit);
        if !path.is_absolute() {
            return Err(format!(
                "{OVERRIDE} is {}: it must be an absolute path",
                path.display()
            ));
        }
        if !executable(&path) {
            return Err(format!(
                "{OVERRIDE} is {}: not an executable file",
                path.display()
            ));
        }
        return Ok(Some(path));
    }
    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    Ok(on_path
        .into_iter()
        .chain(local_bin())
        .map(|dir| dir.join(PROGRAM))
        .find(|p| executable(p)))
}

/// Runs the program with exactly `--tool rascal`, without a shell, and returns its exit code
/// (128 + N when a signal ended it).
fn launch(program: &Path) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    match Command::new(program).args(["--tool", "rascal"]).status() {
        Ok(status) => status
            .code()
            .or_else(|| status.signal().map(|s| 128 + s))
            .unwrap_or(1),
        Err(error) => {
            eprintln!("rascal playground: {}: {error}", program.display());
            UNAVAILABLE
        }
    }
}

fn instructions() -> String {
    format!(
        "rascal playground: Wafflery is not installed. It is a separate program shared by the waffle tools; install it so `{PROGRAM}` is on PATH or in ~/.local/bin, or set {OVERRIDE} to its absolute path."
    )
}

/// Hands off to Wafflery and returns the exit code: its own; 69 when it is unavailable; with
/// the testkit's installer, the offer on a terminal (no: 0; an installation that fails: 74).
pub fn run(seam: &Seam) -> i32 {
    match find() {
        Err(reason) => {
            eprintln!("rascal playground: {reason}");
            UNAVAILABLE
        }
        Ok(Some(program)) => launch(&program),
        Ok(None) => {
            let Some(installer) = &seam.installer else {
                eprintln!("{}", instructions());
                return UNAVAILABLE;
            };
            let terminal = seam.terminal
                || (std::io::stdin().is_terminal() && std::io::stderr().is_terminal());
            if !terminal {
                eprintln!("{}", instructions());
                return UNAVAILABLE;
            }
            offer(installer)
        }
    }
}

/// Asks on stderr whether to install; no (or no answer) changes nothing and exits 0.
fn offer(installer: &Path) -> i32 {
    let Some(prefix) = local_bin() else {
        eprintln!("rascal playground: HOME is not set, so there is nowhere to install");
        return IO_ERROR;
    };
    eprint!(
        "Wafflery is a separate program shared by the waffle tools, and it is not installed.\nInstall it into {}? [y/N] ",
        prefix.display()
    );
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    if let Err(error) = std::io::stdin().lock().read_line(&mut answer) {
        eprintln!("rascal playground: reading the answer: {error}");
        return IO_ERROR;
    }
    if !matches!(answer.trim(), "y" | "Y" | "yes" | "Yes" | "YES") {
        eprintln!("rascal playground: not installed; nothing changed");
        return 0;
    }
    if let Err(error) = std::fs::create_dir_all(&prefix) {
        eprintln!("rascal playground: {}: {error}", prefix.display());
        return IO_ERROR;
    }
    let installed = Command::new(installer)
        .arg("--prefix")
        .arg(&prefix)
        .status();
    let program = prefix.join(PROGRAM);
    match installed {
        Ok(status) if status.success() && executable(&program) => launch(&program),
        Ok(status) if status.success() => {
            eprintln!(
                "rascal playground: the installer finished but left no program at {}",
                program.display()
            );
            IO_ERROR
        }
        Ok(status) => {
            eprintln!("rascal playground: the installer failed ({status})");
            IO_ERROR
        }
        Err(error) => {
            eprintln!("rascal playground: {}: {error}", installer.display());
            IO_ERROR
        }
    }
}
