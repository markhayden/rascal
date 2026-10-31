//! table-manners (ADR 0020): rascal reproduces the contract's pixel vectors through its own
//! import, and the references it gives validate against the contract's schema.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use rascal_core::contract::{self, image_reference, table_manners};
use rascal_core::frame::Projection;
use rascal_core::workspace::Workspace;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-contract-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the contract's `urn:` ids from the crate's embedded schemas, never the network.
struct Embedded;

impl jsonschema::Retrieve for Embedded {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let text = table_manners::SCHEMAS
            .iter()
            .find(|(id, _)| *id == uri.as_str())
            .map(|(_, text)| *text)
            .ok_or_else(|| format!("no embedded schema {uri}"))?;
        Ok(serde_json::from_str(text)?)
    }
}

fn validator(id: &str) -> jsonschema::Validator {
    let (_, text) = table_manners::SCHEMAS
        .iter()
        .find(|(i, _)| *i == id)
        .unwrap();
    let schema: serde_json::Value = serde_json::from_str(text).unwrap();
    jsonschema::options()
        .with_retriever(Embedded)
        .build(&schema)
        .unwrap()
}

#[test]
fn every_pixel_vector_reproduces_through_rascals_import() {
    let dir = root().join("fixtures/contract/pixels");
    let digests: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("digests.json")).unwrap()).unwrap();
    let vectors = digests["vectors"].as_array().unwrap();
    assert!(vectors.len() >= 12, "{} vectors", vectors.len());
    let ws_dir = scratch("vectors");
    let ws = Workspace::open(&ws_dir).unwrap();
    let schema = validator("urn:table-manners:images:image-reference");
    for v in vectors {
        let file = v["file"].as_str().unwrap();
        let (id, _, _) = ws.import(&dir.join(file), Some(Projection::Flat)).unwrap();
        let reference = image_reference(&ws, &id).unwrap();
        assert_eq!(
            reference.file_sha256,
            v["file_sha256"].as_str().unwrap(),
            "{file}"
        );
        assert_eq!(
            reference.pixel_sha256.as_deref(),
            v["pixel_sha256"].as_str(),
            "{file}: {}",
            v["note"]
        );
        assert_eq!(
            u64::from(reference.width),
            v["width"].as_u64().unwrap(),
            "{file}"
        );
        assert_eq!(
            u64::from(reference.height),
            v["height"].as_u64().unwrap(),
            "{file}"
        );
        let json = serde_json::to_value(&reference).unwrap();
        assert!(schema.is_valid(&json), "{file}: {json}");
    }
    let _ = std::fs::remove_dir_all(&ws_dir);
}

#[test]
fn an_import_names_the_callers_file_and_a_made_image_names_the_png_rascal_wrote() {
    let ws_dir = scratch("made");
    let ws = Workspace::open(&ws_dir).unwrap();
    let pano = root().join("fixtures/images/step-pano.png");
    let bytes = std::fs::read(&pano).unwrap();
    let (id, frame, _) = ws.import(&pano, None).unwrap();
    let reference = image_reference(&ws, &id).unwrap();
    assert_eq!(reference.file_sha256, contract::file_sha256(&bytes));
    // A 2:1 image is taken as equirectangular, which always wraps.
    assert_eq!(frame.projection, Projection::Equirectangular);
    assert_eq!(
        serde_json::to_value(reference.topology).unwrap(),
        serde_json::json!("wrap")
    );
    // The same bytes inline: one image, one reference.
    let (id2, _, _) = ws.import_bytes(&bytes, "inline", None).unwrap();
    assert_eq!(image_reference(&ws, &id2).unwrap(), reference);
    // An image with no file behind it names the PNG the workspace wrote.
    let flat = rascal_core::frame::Frame::raw(4, 2, Projection::Flat);
    let made = rascal_core::image::Image::new(
        "made",
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4, 2, image::Rgb([9, 8, 7]))),
        &flat,
        rascal_core::image::Role::Canonical,
        None,
    );
    let (made_id, _) = ws.add_canonical(made, &flat, "built").unwrap();
    let entry = ws.snapshot().unwrap().artifact(&made_id).unwrap().clone();
    let written = std::fs::read(ws_dir.join(&entry.path)).unwrap();
    assert_eq!(
        image_reference(&ws, &made_id).unwrap().file_sha256,
        contract::file_sha256(&written)
    );
    let _ = std::fs::remove_dir_all(&ws_dir);
}
