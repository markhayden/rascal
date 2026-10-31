//! `rascal serve --workspace <dir> [--config rascal.toml] [--concurrency n]`: the MCP server
//! over stdio. Nothing but the protocol goes to stdout; tracing goes to stderr (`RASCAL_LOG`).
//! With `[packs.*]` in the configuration, their tools are served too, journaling into the
//! same workspace. The contract's action tools (one per shipped recipe, and `describe`) run
//! each call in its own run directory under `--runs` and serve the runs as resources
//! (table-manners; ADR 0020).

use std::path::PathBuf;
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::error::Result;
use rascal_core::workspace::Workspace;
use rascal_mcp::server::Server;

pub struct ServeArgs {
    pub workspace: PathBuf,
    pub config: Option<PathBuf>,
    pub concurrency: usize,
    /// Where the contract's action runs go (ADR 0020); `$XDG_DATA_HOME/rascal/runs` by default.
    pub runs: Option<PathBuf>,
}

/// Serves until the client closes the connection.
pub fn serve(args: &ServeArgs) -> Result<()> {
    let config = args
        .config
        .as_deref()
        .map(rascal_core::config::Config::read)
        .transpose()?;
    let workspace = Arc::new(Workspace::open(&args.workspace)?);
    let (registry, packs) = crate::packs::registry_for(config.as_ref(), &workspace, None)?;
    let actions = Arc::new(crate::actions::ContractActions {
        config_path: args.config.clone(),
        config: config.clone(),
        runs: args
            .runs
            .clone()
            .unwrap_or_else(crate::actions::default_runs_root),
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
