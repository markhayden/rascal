//! `rascal serve [--workspace <dir>] [--config rascal.toml] [--concurrency n] [--runs dir]`:
//! the MCP server over stdio. Nothing but the protocol goes to stdout; tracing goes to stderr
//! (`RASCAL_LOG`). It starts in every configuration state (table-manners 3.1, 5): with no file,
//! a file that does not exist or one that does not load, its actions say what is missing and a
//! call that needs it is refused `not_ready`. With `[packs.*]` in the configuration, their tools
//! are served too, journaling into the same workspace; a pack that does not start leaves its
//! tools out and says why on stderr. The contract's action tools (one per shipped recipe, and
//! `describe`) run each call in its own run directory under `--runs` and serve the runs as
//! resources (table-manners; ADR 0020).

use std::path::PathBuf;
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::config::ConfigState;
use rascal_core::error::Result;
use rascal_core::workspace::Workspace;
use rascal_mcp::server::Server;

pub struct ServeArgs {
    /// The op tools' workspace; the configuration's `[workspace] dir` (relative to the file),
    /// else `$XDG_DATA_HOME/rascal/workspace`, by default.
    pub workspace: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub concurrency: usize,
    /// Where the contract's action runs go (ADR 0020); `$XDG_DATA_HOME/rascal/runs` by default.
    pub runs: Option<PathBuf>,
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

/// Serves until the client closes the connection.
pub fn serve(args: &ServeArgs) -> Result<()> {
    let state = ConfigState::read(args.config.as_deref());
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
    let (registry, packs, failed_packs) =
        crate::packs::registry_tolerant(config.as_ref(), &workspace, None)?;
    let failed_packs: std::collections::BTreeMap<String, String> = failed_packs
        .into_iter()
        .map(|(name, error)| {
            eprintln!("rascal serve: packs.{name} did not start, so its tools are absent: {error}");
            (name, error.to_string())
        })
        .collect();
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
    });
    let server = Server::new(
        workspace,
        registry,
        args.concurrency,
        Some(Arc::new(|root: &std::path::Path| {
            crate::dash::write(root, &root.join("dash.html"))
        })),
        &packs,
    )?
    .with_actions(actions);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::op("serve", format!("runtime: {e}")))?;
    let served = runtime.block_on(server.serve_stdio());
    // The packs stop on this thread, after the runtime, so their escalation can block.
    drop(runtime);
    drop(packs);
    served
}
