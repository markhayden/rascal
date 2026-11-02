//! The registry a run uses: every shipped op plus one op per tool of every configured pack
//! (`[packs.*]`), journaling into the run's workspace. Under `--replay` the packs are not
//! started: their tools and their answers come from the recorded run.

use std::collections::BTreeMap;
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
/// caller for as long as the registry is used; they stop their processes when dropped). A pack
/// that does not start fails the run.
pub fn registry_for(
    config: Option<&Config>,
    workspace: &Arc<Workspace>,
    replay: Option<&Path>,
) -> Result<(OpRegistry, Vec<Arc<Pack>>)> {
    let (registry, packs, _) = build(config, workspace, replay, false)?;
    Ok((registry, packs))
}

/// A registry, the packs behind it, and the packs that did not start with their error.
pub type Tolerated = (OpRegistry, Vec<Arc<Pack>>, BTreeMap<String, Error>);

/// As [`registry_for`], except that a pack that does not start leaves its tools out instead of
/// failing; each such pack is returned with its error (task 12.2). `serve` says so on stderr
/// and its readiness requirement reads `invalid`; a run notes it in its journal, so a call that
/// needs nothing from the pack still runs.
pub fn registry_tolerant(
    config: Option<&Config>,
    workspace: &Arc<Workspace>,
    replay: Option<&Path>,
) -> Result<Tolerated> {
    build(config, workspace, replay, true)
}

fn build(
    config: Option<&Config>,
    workspace: &Arc<Workspace>,
    replay: Option<&Path>,
    tolerant: bool,
) -> Result<Tolerated> {
    let mut registry = rascal_ops::registry()?;
    let mut packs = Vec::new();
    let mut failed = BTreeMap::new();
    let Some(config) = config.filter(|c| !c.packs.is_empty()) else {
        return Ok((registry, packs, failed));
    };
    let key_envs = config.key_envs();
    let replay = replay.map(Replay::open).transpose()?.map(Arc::new);
    for (name, entry) in &config.packs {
        let started = match &replay {
            Some(replay) => Pack::replayed(name, entry.clone(), key_envs.clone(), replay.clone()),
            None => Pack::new(name, entry.clone(), key_envs.clone()),
        }
        .map(Arc::new)
        .and_then(|pack| ops_for(&pack, workspace).map(|ops| (pack, ops)));
        let (pack, ops) = match started {
            Ok(started) => started,
            Err(error) if tolerant => {
                failed.insert(name.clone(), error);
                continue;
            }
            Err(error) => return Err(error),
        };
        for op in ops {
            registry
                .register(op)
                .map_err(|e| Error::usage(format!("packs.{name}: {e}")))?;
        }
        packs.push(pack);
    }
    Ok((registry, packs, failed))
}
