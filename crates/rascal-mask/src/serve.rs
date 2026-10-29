//! `rascal serve --workspace <dir> [--config rascal.toml] [--concurrency n]`: the MCP server
//! over stdio. Nothing but the protocol goes to stdout; tracing goes to stderr (`RASCAL_LOG`).

use std::path::PathBuf;

use rascal_core::Error;
use rascal_core::error::Result;
use rascal_core::registry::OpRegistry;
use rascal_core::workspace::Workspace;
use rascal_mcp::server::Server;

pub struct ServeArgs {
    pub workspace: PathBuf,
    pub config: Option<PathBuf>,
    pub concurrency: usize,
}

/// Serves until the client closes the connection.
pub fn serve(registry: OpRegistry, args: &ServeArgs) -> Result<()> {
    if let Some(path) = &args.config {
        rascal_core::config::Config::read(path)?;
    }
    let workspace = Workspace::open(&args.workspace)?;
    let server = Server::new(workspace, registry, args.concurrency)
        .with_dash(|root| crate::dash::write(root, &root.join("dash.html")));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::op("serve", format!("runtime: {e}")))?;
    runtime.block_on(server.serve_stdio())
}
