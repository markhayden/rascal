//! The registry a run uses: every shipped op plus one op per tool of every configured pack
//! (`[packs.*]`), journaling into the run's workspace. Under `--replay` the packs are not
//! started: their tools and their answers come from the recorded run.

use std::path::Path;
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::config::Config;
use rascal_core::error::Result;
use rascal_core::registry::OpRegistry;
use rascal_core::workspace::Workspace;
use rascal_mcp::client::Pack;
use rascal_mcp::client::op::ops_for;
use rascal_mcp::client::replay::Replay;

/// The registry for `config` over `workspace`, and the packs behind it (kept alive by the
/// caller for as long as the registry is used; they stop their processes when dropped).
pub fn registry_for(
    config: Option<&Config>,
    workspace: &Arc<Workspace>,
    replay: Option<&Path>,
) -> Result<(OpRegistry, Vec<Arc<Pack>>)> {
    let mut registry = rascal_ops::registry()?;
    let mut packs = Vec::new();
    let Some(config) = config.filter(|c| !c.packs.is_empty()) else {
        return Ok((registry, packs));
    };
    let key_envs = config.key_envs();
    let replay = replay.map(Replay::open).transpose()?.map(Arc::new);
    for (name, entry) in &config.packs {
        let pack = match &replay {
            Some(replay) => Pack::replayed(name, entry.clone(), key_envs.clone(), replay.clone())?,
            None => Pack::new(name, entry.clone(), key_envs.clone())?,
        };
        let pack = Arc::new(pack);
        for op in ops_for(&pack, workspace)? {
            registry
                .register(op)
                .map_err(|e| Error::usage(format!("packs.{name}: {e}")))?;
        }
        packs.push(pack);
    }
    Ok((registry, packs))
}
