//! The artifact resource (CONTRACT 4.7; PLAN.md 10.7). `rascal://artifact/{identity}` reads
//! any artifact the session's store holds, found by its contract identity alone (ids play no
//! part, so a reopened store and one moved to another directory answer the same), as
//! `{contract, tool, artifact, preview, record}`: its reference, its preview for a type that
//! declares one (else null) and rascal's own record, which a host never reads.
//! `rascal://artifact/{identity}/preview` is that preview's PNG.
//!
//! The preview is `crate::preview::draw`, written once into rascal's cache
//! (`$XDG_CACHE_HOME/rascal/previews/<identity, : as ->.png`, outside the store and every run
//! directory) and located there by path. An identity the store does not hold, and `/preview` of
//! a type without one, is the `binding` refusal with `port: null`. A read takes no permit.

use std::path::{Path, PathBuf};

use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::contract::{self, table_manners};
use rascal_core::error::Result;
use rascal_core::registry::OpRegistry;
use rascal_core::workspace::{Resolved, Workspace};
use rascal_mcp::server::{CallError, ResourceBody};
use serde_json::{Value, json};
use table_manners::images::{ImageOut, Locator};
use table_manners::operations::{Binding, BindingProblem};

use crate::preview;

/// The URI every artifact reference's `resource` names (`contract::artifact_resource`).
pub const PREFIX: &str = "rascal://artifact/";
/// The resource template of an artifact.
pub const TEMPLATE: &str = "rascal://artifact/{identity}";
/// The resource template of an artifact's preview.
pub const PREVIEW_TEMPLATE: &str = "rascal://artifact/{identity}/preview";

/// What the refusal says an artifact read takes.
const EXPECTED: &str = "an artifact this store holds (any rascal type, v1)";
/// What it says `/preview` takes.
const EXPECTED_PREVIEW: &str =
    "an artifact this store holds whose type has a preview (location, rascal:Mask)";
/// What a person does when the store does not hold the identity.
const REMEDY: &str = "Import the artifact with the import tool (rascal import), or start rascal serve on the workspace that holds it, then read again.";
/// What a person does when the type has no preview.
const REMEDY_PREVIEW: &str = "This artifact's type has no preview; read its resource without /preview for its reference and record.";

/// Reads `uri` when it is an artifact resource: `None` when it is not one of its URIs.
pub fn read(
    store: &Workspace,
    ops: &OpRegistry,
    uri: &str,
) -> Option<std::result::Result<ResourceBody, CallError>> {
    let rest = uri.strip_prefix(PREFIX)?;
    let (identity, preview) = match rest.strip_suffix("/preview") {
        Some(identity) => (identity, true),
        None => (rest, false),
    };
    if identity.is_empty() || identity.contains('/') {
        return None;
    }
    Some(answer(store, ops, identity, preview))
}

fn answer(
    store: &Workspace,
    ops: &OpRegistry,
    identity: &str,
    want_preview: bool,
) -> std::result::Result<ResourceBody, CallError> {
    let expected = if want_preview {
        EXPECTED_PREVIEW
    } else {
        EXPECTED
    };
    let Some(resolved) = resolve(store, identity).map_err(store_error)? else {
        return Err(refusal(identity, expected, REMEDY));
    };
    if want_preview {
        if !resolved.artifact_type.preview() {
            return Err(refusal(identity, expected, REMEDY_PREVIEW));
        }
        let Some((path, _)) =
            cached_preview(store, ops, identity, &resolved).map_err(store_error)?
        else {
            return Err(refusal(identity, expected, REMEDY_PREVIEW));
        };
        let bytes = std::fs::read(&path).map_err(|e| store_error(Error::io(&path, e)))?;
        return Ok(ResourceBody::Blob {
            mime: "image/png".to_string(),
            bytes,
        });
    }
    let body = document(store, ops, identity, &resolved).map_err(store_error)?;
    Ok(ResourceBody::Text {
        mime: "application/json".to_string(),
        text: serde_json::to_string_pretty(&body)
            .map_err(|e| CallError::from(format!("the artifact resource: {e}")))?,
    })
}

/// The artifact whose contract identity is `identity`, of whichever type holds it.
fn resolve(store: &Workspace, identity: &str) -> Result<Option<Resolved>> {
    if rascal_core::identity::Identity::parse(identity).is_none() {
        return Ok(None);
    }
    for artifact_type in ArtifactType::ALL {
        if let Some(found) = store.resolve(artifact_type, identity)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// The resource document (`urn:table-manners:operations:artifact`).
fn document(
    store: &Workspace,
    ops: &OpRegistry,
    identity: &str,
    resolved: &Resolved,
) -> Result<Value> {
    let reference = contract::artifact_ref(store, &resolved.id)?;
    let preview: Option<ImageOut> = if resolved.artifact_type.preview() {
        cached_preview(store, ops, identity, resolved)?.map(|(_, out)| out)
    } else {
        None
    };
    let record = record(&store.load(&resolved.id)?)?;
    Ok(json!({
        "contract": contract::CONTRACT_VERSION,
        "tool": {
            "name": rascal_core::artifact::TOOL,
            "version": env!("CARGO_PKG_VERSION"),
        },
        "artifact": reference,
        "preview": preview,
        "record": record,
    }))
}

/// rascal's own record of an artifact, as JSON (what a host never reads).
fn record(artifact: &Artifact) -> Result<Value> {
    let value = match artifact {
        Artifact::Image(i) => serde_json::to_value(&i.record),
        Artifact::Location { location, .. } => serde_json::to_value(location),
        Artifact::Mask(m) => serde_json::to_value(&m.record),
        Artifact::BandProfile(b) => serde_json::to_value(&**b),
        Artifact::Series(s) => serde_json::to_value(&**s),
        Artifact::Plan(p) => serde_json::to_value(&**p),
    };
    value.map_err(|e| Error::op("artifact resource", e.to_string()))
}

/// The preview's file in the cache and its image out, drawn the first time it is asked for;
/// `None` when the artifact has none.
fn cached_preview(
    store: &Workspace,
    ops: &OpRegistry,
    identity: &str,
    resolved: &Resolved,
) -> Result<Option<(PathBuf, ImageOut)>> {
    let frame = store.frames()?.require(&resolved.frame)?.clone();
    let dir = previews_dir()?;
    let path = dir.join(format!("{}.png", identity.replace(':', "-")));
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => {
            let Some((bytes, _)) = preview::draw(store, ops, &resolved.id)? else {
                return Ok(None);
            };
            write_once(&dir, &path, &bytes)?;
            bytes
        }
    };
    let out = preview::image_out(
        &bytes,
        &frame,
        Locator::Path(path.to_string_lossy().to_string()),
    )?;
    Ok(Some((path, out)))
}

/// `$XDG_CACHE_HOME/rascal/previews` (else `~/.cache/…`).
fn previews_dir() -> Result<PathBuf> {
    crate::dash::cache_root()
        .map(|root| root.join("rascal").join("previews"))
        .ok_or_else(|| {
            Error::usage("the artifact resource: no cache directory (set XDG_CACHE_HOME or HOME)")
        })
}

/// Writes `bytes` at `path` whole: a temporary name, then a rename.
fn write_once(dir: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| Error::io(&tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        Error::io(path, e)
    })
}

/// The `binding` refusal of a read (question 10): one `unresolved` entry, `port: null`.
fn refusal(identity: &str, expected: &str, remedy: &str) -> CallError {
    crate::operations::binding_refusal(vec![Binding {
        port: None,
        index: None,
        problem: BindingProblem::Unresolved,
        expected: expected.to_string(),
        got: identity.to_string(),
        remedy: Some(remedy.to_string()),
    }])
}

fn store_error(e: Error) -> CallError {
    CallError::from(format!("the store: {e}"))
}
