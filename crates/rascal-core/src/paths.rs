//! The path rule (CONTRACT 5.2; PLAN.md 10.1): `rascal serve` reads a caller's path only inside
//! the directories the session allows, and [`PathRule::admit`] is the one way it does. A person's
//! own command line (`rascal run`, `heal`, `estimate` and the rest) takes any path they name.
//!
//! The allowed set is fixed when `serve` starts: each `--allow-dir` must be absolute and name an
//! existing directory ([`resolve_flags`]), each is resolved once (`.`, `..` and every symlink),
//! and the configuration's `[serve] allow_dirs` narrows them ([`overlap`]), never widens them.
//! With no `--allow-dir` the set is empty and every caller path is refused.
//!
//! A caller path is admitted when it is absolute and its canonical form (every symlink resolved,
//! the final component included) exists, is a regular file and lies under an allowed directory,
//! component by component. It is then read through the canonical path, never the
//! given one. Every failure is the one refusal [`refusal`], whose text says nothing about the
//! file: a missing file, a directory, a symlink out of the tree and a path outside it read the
//! same. A check-then-read race (a symlink swapped between the check and the read) is outside
//! what this guards: the caller is on the host's own stdio, and the rule keeps an agent from
//! naming files such as a key (docs/contract.md).

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// The flag that allows a directory.
pub const FLAG: &str = "--allow-dir";

/// Whose paths a command reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathRule {
    /// A person's own command line: any path they name (CONTRACT 5.2).
    Own,
    /// `rascal serve`: only inside these directories, canonical and fixed for the session. Empty
    /// refuses every path.
    Allowed(Vec<PathBuf>),
}

/// The one refusal of a caller path, the same whatever the file is.
pub fn refusal(slot: &str) -> Error {
    Error::Input {
        message: format!(
            "{slot}: this path is not readable here (only files inside the directories rascal serve allows; CONTRACT 5.2)"
        ),
    }
}

impl PathRule {
    /// The path a slot's value names, as the command reads it: for [`PathRule::Own`] the path
    /// as given (an image must be an absolute path to a file, as table-manners 10.1 asks); for
    /// [`PathRule::Allowed`] its canonical form, admitted, or [`refusal`].
    pub fn admit(&self, slot: &str, given: &Path) -> Result<PathBuf> {
        match self {
            PathRule::Own => {
                if !given.is_absolute() {
                    return Err(Error::usage(format!(
                        "{slot}: {} is not an absolute path (CONTRACT.md 10.1)",
                        given.display()
                    )));
                }
                if !given.is_file() {
                    return Err(Error::usage(format!(
                        "{slot}: {} is not a file",
                        given.display()
                    )));
                }
                Ok(given.to_path_buf())
            }
            PathRule::Allowed(dirs) => admitted(given, dirs).ok_or_else(|| refusal(slot)),
        }
    }

    /// The allowed directories; none for a person's own command line.
    pub fn dirs(&self) -> &[PathBuf] {
        match self {
            PathRule::Own => &[],
            PathRule::Allowed(dirs) => dirs,
        }
    }
}

/// The canonical path when it is admitted under `dirs`: a regular file inside.
fn admitted(given: &Path, dirs: &[PathBuf]) -> Option<PathBuf> {
    if !given.is_absolute() {
        return None;
    }
    let canonical = inside(given, dirs)?;
    std::fs::metadata(&canonical)
        .ok()?
        .is_file()
        .then_some(canonical)
}

/// The canonical form of `path` when it exists and lies under one of `dirs`.
fn inside(path: &Path, dirs: &[PathBuf]) -> Option<PathBuf> {
    let canonical = std::fs::canonicalize(path).ok()?;
    dirs.iter()
        .any(|dir| canonical.starts_with(dir))
        .then_some(canonical)
}

/// Each `--allow-dir`, resolved: it must be absolute and name an existing directory, else a
/// usage error naming the flag (it is the host's own argument, so it may say why).
pub fn resolve_flags(flags: &[PathBuf]) -> Result<Vec<PathBuf>> {
    flags
        .iter()
        .map(|flag| {
            if !flag.is_absolute() {
                return Err(Error::usage(format!(
                    "{FLAG} {}: not an absolute path",
                    flag.display()
                )));
            }
            let canonical = std::fs::canonicalize(flag).map_err(|_| {
                Error::usage(format!("{FLAG} {}: no such directory", flag.display()))
            })?;
            if !canonical.is_dir() {
                return Err(Error::usage(format!(
                    "{FLAG} {}: not a directory",
                    flag.display()
                )));
            }
            Ok(canonical)
        })
        .collect()
}

/// The configured directories resolved the same way; one that does not resolve allows nothing
/// and is returned in the second list, for the start line to name.
pub fn resolve_configured(configured: &[String]) -> (Vec<PathBuf>, Vec<String>) {
    let mut resolved = Vec::new();
    let mut dropped = Vec::new();
    for dir in configured {
        match std::fs::canonicalize(dir) {
            Ok(canonical) if canonical.is_dir() => resolved.push(canonical),
            _ => dropped.push(dir.clone()),
        }
    }
    (resolved, dropped)
}

/// The allowed set: the flags' directories, narrowed by the configured ones when the
/// configuration lists any (`None`: no `[serve] allow_dirs`). Where a flag directory and a
/// configured one overlap, the deeper of the two is allowed (a flag directory inside a
/// configured one stays; a configured directory inside a flag one replaces it); anything else
/// goes, so a configured directory with no flag over it allows nothing. Sorted, with no
/// directory inside another.
pub fn overlap(flags: &[PathBuf], configured: Option<&[PathBuf]>) -> Vec<PathBuf> {
    let mut allowed: Vec<PathBuf> = match configured {
        None => flags.to_vec(),
        Some(configured) => flags
            .iter()
            .flat_map(|flag| {
                configured.iter().filter_map(move |dir| {
                    if dir.starts_with(flag) {
                        Some(dir.clone())
                    } else if flag.starts_with(dir) {
                        Some(flag.clone())
                    } else {
                        None
                    }
                })
            })
            .collect(),
    };
    allowed.sort();
    allowed.dedup();
    let outer = allowed.clone();
    allowed.retain(|dir| !outer.iter().any(|o| o != dir && dir.starts_with(o)));
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn the_overlap_keeps_the_deeper_directory() {
        let flags = [p("/a"), p("/b/c")];
        assert_eq!(overlap(&flags, None), flags.to_vec());
        // A configured subdirectory of a flag directory replaces it; a flag directory inside a
        // configured one stays; a configured directory under no flag allows nothing.
        let configured = [p("/a/x"), p("/b"), p("/z")];
        assert_eq!(
            overlap(&flags, Some(&configured)),
            vec![p("/a/x"), p("/b/c")]
        );
        assert!(overlap(&flags, Some(&[])).is_empty());
        assert!(overlap(&[], Some(&configured)).is_empty());
        // Component-wise: /a/bc is not under /a/b.
        assert!(overlap(&[p("/a/b")], Some(&[p("/a/bc")])).is_empty());
        // No directory inside another.
        assert_eq!(overlap(&[p("/a"), p("/a/b")], None), vec![p("/a")]);
    }

    #[test]
    fn the_refusal_names_only_the_slot() {
        assert_eq!(
            refusal("image").to_string(),
            "image: this path is not readable here (only files inside the directories rascal serve allows; CONTRACT 5.2)"
        );
    }
}
