//! `rascal paste <base> <fill> <mask> --mode <feather|hard|poisson> --out <file>`: a stored
//! mask composites a fill onto a base, both image files in the mask's frame, to a file
//! destination (overwritten; PNG, or JPEG for `.jpg`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::artifact::Artifact;
use rascal_core::error::Result;
use rascal_core::mask::Mask;
use rascal_core::op::{Params, Status};
use rascal_core::outdir::{self, OutKind};
use rascal_core::recipe::Run;
use rascal_core::registry::OpRegistry;

use crate::render::{frame_of, write_by_extension};

/// What `paste` was asked to do.
pub struct PasteArgs {
    pub base: PathBuf,
    pub fill: PathBuf,
    pub mask: PathBuf,
    pub mode: String,
    pub out: PathBuf,
}

/// Pastes and writes the composite; the envelope's status decides the exit code.
pub fn paste(registry: &OpRegistry, args: &PasteArgs) -> Result<(PathBuf, Status, String)> {
    let mask = Mask::read(&args.mask)?;
    let out = outdir::prepare(&args.out, OutKind::File)?;
    let scratch = std::env::temp_dir().join(format!(
        "rascal-paste-{}-{}",
        std::process::id(),
        mask.identity().hex()
    ));
    std::fs::create_dir_all(&scratch).map_err(|e| Error::io(&scratch, e))?;
    let result = (|| {
        let run = Run::open(&scratch)?;
        let frame = frame_of(&mask);
        let mut inputs = BTreeMap::new();
        for (port, path) in [("base", &args.base), ("fill", &args.fill)] {
            let (_, image_frame, image) = run.workspace.import(path, Some(frame.projection))?;
            if image_frame.id != frame.id {
                return Err(Error::usage(format!(
                    "{}: the {port} is in frame {} but the mask is in {}",
                    path.display(),
                    image_frame.id,
                    frame.id
                )));
            }
            inputs.insert(port.to_string(), image);
        }
        inputs.insert("mask".to_string(), Artifact::Mask(Arc::new(mask.clone())));
        let mut params = Params::new();
        params.insert("mode".to_string(), toml::Value::String(args.mode.clone()));
        let output = run.invoke("paste", "paste", registry, &inputs, &params)?;
        match output.outputs.get("image") {
            Some(Artifact::Image(image)) => {
                write_by_extension(&out, &image.pixels)?;
                Ok((output.envelope.status, output.envelope.summary))
            }
            _ => Ok((
                output.envelope.status,
                output.envelope.reason.unwrap_or(output.envelope.summary),
            )),
        }
    })();
    let _ = std::fs::remove_dir_all(&scratch);
    result.map(|(status, text)| (out, status, text))
}
