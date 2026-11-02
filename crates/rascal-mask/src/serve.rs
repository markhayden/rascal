//! `rascal serve [--workspace <dir>] [--config rascal.toml] [--concurrency n] [--runs dir]
//! [--allow-dir <abs>]...`: the MCP server over stdio. Nothing but the protocol goes to stdout;
//! tracing goes to stderr (`RASCAL_LOG`). It starts in every configuration state (table-manners 3.1, 5): with no file,
//! a file that does not exist or one that does not load, its actions say what is missing and a
//! call that needs it is refused `not_ready`. It serves one MCP tool per action of the
//! description (the shipped recipes as tasks, the built-in ops as operations) plus `describe`
//! and `estimate`, and nothing else (CONTRACT 5): every call is a run in its own run directory
//! under `--runs`, the runs served as resources (ADR 0020). The workspace is the session's
//! store (CONTRACT 3.4, 4.7): artifact references resolve there by identity, and an operation's
//! outputs commit there. Each `[packs.*]` entry is started once at start to learn whether it
//! starts (its readiness requirement, 12.2) and then stopped; a pack's tools are not served, and
//! the segmenter is reached through `remove_prompt`. A caller's path is read only inside the
//! directories `--allow-dir` names, narrowed by `[serve] allow_dirs` (CONTRACT 5.2,
//! `rascal_core::paths`); with none, every path is refused and inline images still work.

use std::path::PathBuf;
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::config::ConfigState;
use rascal_core::error::Result;
use rascal_core::paths::{self, PathRule};
use rascal_core::workspace::Workspace;
use rascal_mcp::server::Server;

pub struct ServeArgs {
    /// The session's store, where references resolve and operations commit; the
    /// configuration's `[workspace] dir` (relative to the file), else
    /// `$XDG_DATA_HOME/rascal/workspace`, by default.
    pub workspace: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub concurrency: usize,
    /// Where the contract's action runs go (ADR 0020); `$XDG_DATA_HOME/rascal/runs` by default.
    pub runs: Option<PathBuf>,
    /// The directories callers' paths may name, as given (absolute, existing).
    pub allow_dirs: Vec<PathBuf>,
}

/// The workspace `serve` uses without `--workspace`: the configuration's `[workspace] dir`,
/// relative to the configuration file, else `workspace` beside the default runs root.
pub fn default_workspace(config: &ConfigState) -> PathBuf {
    let configured = config
        .config()
        .and_then(|c| c.workspace.as_ref())
        .and_then(|w| w.dir.as_deref())
        .map(PathBuf::from);
    match (configured, config.path()) {
        (Some(dir), _) if dir.is_absolute() => dir,
        (Some(dir), Some(file)) => file.parent().map(|p| p.join(&dir)).unwrap_or(dir),
        (Some(dir), None) => dir,
        (None, _) => crate::actions::data_home().join("rascal/workspace"),
    }
}

/// The allowed directories (CONTRACT 5.2): each `--allow-dir` resolved (a relative, missing or
/// non-directory one is a usage error, exit 64, before anything is served), narrowed by the
/// configuration's `[serve] allow_dirs`. A configuration that does not load allows no caller
/// path (fail closed: what it would narrow is unknown); `serve` still starts, and inline images
/// still work. The set, and why anything was dropped, go to stderr.
pub fn allowed(flags: &[PathBuf], state: &ConfigState) -> Result<PathRule> {
    let flags = paths::resolve_flags(flags)?;
    let configured = match state {
        ConfigState::Invalid { .. } => {
            eprintln!(
                "rascal serve: the configuration does not load, so what its [serve] allow_dirs would allow is unknown and no caller path is allowed"
            );
            Some(Vec::new())
        }
        _ => state
            .config()
            .and_then(|c| c.serve.as_ref())
            .and_then(|s| s.allow_dirs.as_ref())
            .map(|dirs| {
                let (resolved, dropped) = paths::resolve_configured(dirs);
                for dir in dropped {
                    eprintln!(
                        "rascal serve: serve.allow_dirs: {dir} is not a directory, so it allows nothing"
                    );
                }
                resolved
            }),
    };
    let allowed = paths::overlap(&flags, configured.as_deref());
    if allowed.is_empty() {
        eprintln!(
            "rascal serve: no directory is allowed, so every caller path is refused (inline images still work); give {} <absolute directory>",
            paths::FLAG
        );
    } else {
        let list: Vec<String> = allowed.iter().map(|d| d.display().to_string()).collect();
        eprintln!(
            "rascal serve: callers' paths are read only inside: {}",
            list.join(", ")
        );
    }
    Ok(PathRule::Allowed(allowed))
}

/// Serves until the client closes the connection.
pub fn serve(args: &ServeArgs) -> Result<()> {
    let state = ConfigState::read(args.config.as_deref());
    // The host's own flags are checked before anything is opened or answered.
    let paths = allowed(&args.allow_dirs, &state)?;
    if let ConfigState::Invalid { reason, .. } = &state {
        eprintln!(
            "rascal serve: the configuration does not load, so the actions that need it are not ready: {reason}"
        );
    }
    let config = state.config().cloned();
    let workspace_dir = args
        .workspace
        .clone()
        .unwrap_or_else(|| default_workspace(&state));
    let workspace = Arc::new(Workspace::open(&workspace_dir)?);
    // Each pack is started once to learn whether it starts (its readiness requirement reads
    // `invalid` when it does not, 12.2), then stopped: `serve` serves no pack tool.
    let failed_packs: std::collections::BTreeMap<String, String> = {
        let (_, packs, failed) =
            crate::packs::registry_tolerant(config.as_ref(), &workspace, None)?;
        drop(packs);
        failed
            .into_iter()
            .map(|(name, error)| {
                eprintln!(
                    "rascal serve: packs.{name} did not start, so the actions that need it are not ready: {error}"
                );
                (name, error.to_string())
            })
            .collect()
    };
    let actions = Arc::new(crate::actions::ContractActions {
        config_path: config
            .is_some()
            .then(|| state.path().map(PathBuf::from))
            .flatten(),
        config: config.clone(),
        config_state: state,
        failed_packs,
        answers: std::sync::Mutex::default(),
        runs: args
            .runs
            .clone()
            .unwrap_or_else(crate::actions::default_runs_root),
        ledger: std::sync::Mutex::default(),
        session: crate::actions::Session::start(),
        issued: std::sync::Mutex::default(),
        paths,
        store: workspace,
        ops: rascal_ops::registry()?,
    });
    let server = Server::new(actions, args.concurrency);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::op("serve", format!("runtime: {e}")))?;
    let served = runtime.block_on(server.serve_stdio());
    drop(runtime);
    served
}
