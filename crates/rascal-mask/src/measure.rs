//! `rascal measure --before a.png --after b.png --mask <dir> [--set …] [--json]`: the measures
//! on two image files in a stored mask's frame; `--list` names the measures.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::artifact::Artifact;
use rascal_core::error::Result;
use rascal_core::mask::Mask;
use rascal_core::op::{Envelope, Params};
use rascal_core::recipe::Run;
use rascal_core::registry::OpRegistry;

use crate::render::frame_of;

/// What `measure` was asked to do.
pub struct MeasureArgs {
    pub before: Option<PathBuf>,
    pub after: PathBuf,
    pub mask: PathBuf,
    pub set: Option<String>,
}

/// Runs the measures and returns the envelope.
pub fn measure(registry: &OpRegistry, args: &MeasureArgs) -> Result<Envelope> {
    let mask = Mask::read(&args.mask)?;
    let scratch = std::env::temp_dir().join(format!(
        "rascal-measure-{}-{}",
        std::process::id(),
        mask.identity().hex()
    ));
    std::fs::create_dir_all(&scratch).map_err(|e| Error::io(&scratch, e))?;
    let result = (|| {
        let run = Run::open(&scratch)?;
        let frame = frame_of(&mask);
        let mut inputs = BTreeMap::new();
        let mut ports = vec![("after", &args.after)];
        if let Some(before) = &args.before {
            ports.push(("before", before));
        }
        for (port, path) in ports {
            let (_, image_frame, image) = run.workspace.import(path, Some(frame.projection))?;
            if image_frame.id != frame.id {
                return Err(Error::usage(format!(
                    "{}: the {port} image is in frame {} but the mask is in {}",
                    path.display(),
                    image_frame.id,
                    frame.id
                )));
            }
            inputs.insert(port.to_string(), image);
        }
        inputs.insert("mask".to_string(), Artifact::Mask(Arc::new(mask.clone())));
        let mut params = Params::new();
        if let Some(set) = &args.set {
            params.insert("set".to_string(), toml::Value::String(set.clone()));
        }
        let output = run.invoke("measure", "measure", registry, &inputs, &params)?;
        Ok(output.envelope)
    })();
    let _ = std::fs::remove_dir_all(&scratch);
    result
}

/// The measure names, one per line, for `--list` and the docs check.
pub fn list() -> String {
    rascal_ops::measure::MEASURES.join("\n")
}

/// The workspace form: `after`, `before` and `mask` are ids (or identities) in `workspace`,
/// and the invocation is journaled there like any op's.
pub fn measure_in_workspace(
    registry: &OpRegistry,
    workspace: &std::path::Path,
    after: &str,
    before: Option<&str>,
    mask: &str,
    set: Option<&str>,
) -> Result<Envelope> {
    let ws = rascal_core::workspace::Workspace::open(workspace)?;
    let mut inputs = BTreeMap::new();
    inputs.insert("after".to_string(), ws.load(after)?);
    if let Some(before) = before {
        inputs.insert("before".to_string(), ws.load(before)?);
    }
    inputs.insert("mask".to_string(), ws.load(mask)?);
    let mut params = Params::new();
    if let Some(set) = set {
        params.insert("set".to_string(), toml::Value::String(set.to_string()));
    }
    let invocation = ws.invoke("measure", "measure", registry, &inputs, &params)?;
    Ok(invocation.output.envelope)
}
