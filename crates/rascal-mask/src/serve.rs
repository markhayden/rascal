//! `rascal serve --workspace <dir> [--config rascal.toml] [--concurrency n]`: the MCP server
//! over stdio. Nothing but the protocol goes to stdout; tracing goes to stderr (`RASCAL_LOG`).
//! With `[packs.*]` in the configuration, their tools are served too, journaling into the
//! same workspace.

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
    let server = Server::new(workspace, registry, args.concurrency)
        .with_packs(packs.clone())
        .with_dash(|root| crate::dash::write(root, &root.join("dash.html")));
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
