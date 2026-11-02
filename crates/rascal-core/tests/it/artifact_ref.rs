//! Task 10.3 (CONTRACT 3.3, 3.4, 4.7): rascal's types by the contract's names, the artifact
//! reference of what a workspace holds, resolution by identity in a store (reopened and moved
//! alike), and binding a reference, with every problem a `binding` refusal names.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rascal_core::artifact::ArtifactType;
use rascal_core::binding::{self, Given};
use rascal_core::contract::{self, artifact_ref, table_manners};
use rascal_core::location::Location;
use rascal_core::op::{Inputs, Params};
use rascal_core::workspace::Workspace;
use table_manners::operations::{ArtifactRef, BindingProblem};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-artref-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

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

fn valid(id: &str, value: &serde_json::Value) {
    let (_, text) = table_manners::SCHEMAS
        .iter()
        .find(|(i, _)| *i == id)
        .unwrap();
    let schema: serde_json::Value = serde_json::from_str(text).unwrap();
    let validator = jsonschema::options()
        .with_retriever(Embedded)
        .build(&schema)
        .unwrap();
    let errors: Vec<String> = validator
        .iter_errors(value)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{id}: {errors:?}\n{value:#}");
}

fn step(
    ws: &Workspace,
    op: &str,
    inputs: &[(&str, &str)],
    params: &[(&str, toml::Value)],
) -> BTreeMap<String, String> {
    let registry = rascal_ops::registry().unwrap();
    let mut given: Inputs = BTreeMap::new();
    for (port, id) in inputs {
        given.insert((*port).to_string(), ws.load(id).unwrap());
    }
    let mut p = Params::new();
    for (k, v) in params {
        p.insert((*k).to_string(), v.clone());
    }
    let done = ws.invoke(op, op, &registry, &given, &p).unwrap();
    assert!(done.committed.is_some(), "{op}: {:?}", done.output.envelope);
    done.ids
}

/// What the store holds, by local id: the imported panorama, a bbox location on it, a Mask
/// rasterized from it in the raw frame, the cut canvas (an image rascal made), and the seam
/// steps' Mask windowed into a `crop:` frame.
struct Held {
    image: String,
    canvas: String,
    location: String,
    raw_mask: String,
    crop_mask: String,
}

fn store(dir: &Path) -> (Workspace, Held) {
    let ws = Workspace::open(dir).unwrap();
    let pano = root().join("fixtures/images/step-pano.png");
    let (image, frame, _) = ws.import(&pano, None).unwrap();
    let (location, _) = ws
        .add_location(Location::bbox(&frame.id, [0.2, 0.2, 0.4, 0.5]).unwrap())
        .unwrap();
    let raw_mask = step(
        &ws,
        "rasterize",
        &[("image", &image), ("location", &location)],
        &[],
    )["mask"]
        .clone();
    let canvas = step(
        &ws,
        "cut",
        &[("image", &image)],
        &[("strip", toml::Value::Float(0.25))],
    )["image"]
        .clone();
    let line = step(
        &ws,
        "line",
        &[("image", &canvas)],
        &[
            ("column", toml::Value::Float(0.5)),
            ("width_px", toml::Value::Float(2.0)),
        ],
    );
    let fan = step(
        &ws,
        "fan",
        &[("image", &canvas), ("line", &line["location"])],
        &[],
    );
    let band = step(&ws, "profile", &[("band", &fan["band"])], &[]);
    let ramp = step(
        &ws,
        "ramp",
        &[("image", &canvas), ("mask", &band["mask"])],
        &[],
    );
    let crop_mask = step(
        &ws,
        "window",
        &[("image", &canvas), ("mask", &ramp["mask"])],
        &[],
    )["mask"]
        .clone();
    (
        ws,
        Held {
            image,
            canvas,
            location,
            raw_mask,
            crop_mask,
        },
    )
}

/// Every local id the workspace gives out, so a test can show none leaks into a reference.
fn ids(ws: &Workspace) -> Vec<String> {
    let generation = ws.snapshot().unwrap();
    let mut ids: Vec<String> = generation.artifacts.iter().map(|a| a.id.clone()).collect();
    ids.extend(generation.locations.iter().map(|l| l.id.clone()));
    ids.extend(generation.plans.iter().map(|p| p.id.clone()));
    ids.extend(generation.frames.iter().map(|f| f.id.clone()));
    ids
}

#[test]
fn rascals_types_have_the_contracts_names_and_no_others() {
    for (t, name) in [
        (ArtifactType::Image, "image"),
        (ArtifactType::Location, "location"),
        (ArtifactType::Mask, "rascal:Mask"),
        (ArtifactType::BandProfile, "rascal:BandProfile"),
        (ArtifactType::Series, "rascal:Series"),
        (ArtifactType::Plan, "rascal:Plan"),
    ] {
        assert_eq!(t.contract_type(), name);
        assert_eq!(ArtifactType::from_contract(name), Some(t));
    }
    for other in ["squint:Mask", "Mask", "rascal:Thing", "rascal:mask", ""] {
        assert_eq!(ArtifactType::from_contract(other), None, "{other}");
    }
}

#[test]
fn a_reference_validates_names_its_root_image_and_no_id() {
    let dir = scratch("refs");
    let (ws, held) = store(&dir);
    let pano = root().join("fixtures/images/step-pano.png");
    let file = contract::file_sha256(&std::fs::read(&pano).unwrap());
    // The root image's reference without its pixel digest, which every store can give.
    let root_image = table_manners::images::ImageReference {
        pixel_sha256: None,
        ..contract::image_reference(&ws, &held.image).unwrap()
    };
    let generation = ws.snapshot().unwrap();
    let local_ids = ids(&ws);
    for (id, kind) in [
        (&held.image, "image"),
        (&held.location, "location"),
        (&held.raw_mask, "rascal:Mask"),
        (&held.crop_mask, "rascal:Mask"),
    ] {
        let reference = artifact_ref(&ws, id).unwrap();
        let json = serde_json::to_value(&reference).unwrap();
        valid("urn:table-manners:operations:artifact-ref", &json);
        assert_eq!(reference.artifact_type, kind);
        assert_eq!(reference.schema_version, 1);
        assert_eq!(reference.tool, "rascal");
        assert_eq!(
            reference.resource,
            format!("rascal://artifact/{}", reference.identity)
        );
        assert_eq!(
            reference.on.as_ref(),
            Some(&root_image),
            "{id}: made on the panorama"
        );
        let text = json.to_string();
        for local in &local_ids {
            assert!(
                !text.contains(&format!("\"{local}\"")),
                "{id}: {local} in {text}"
            );
        }
        // The frame is the frame's identity, never its id.
        let frame = reference.frame.as_deref().unwrap();
        assert!(
            generation.frames.iter().any(|f| f.identity == frame),
            "{frame}"
        );
    }
    let image = artifact_ref(&ws, &held.image).unwrap();
    assert_eq!(image.identity, format!("sha256:{file}"));
    let crop = artifact_ref(&ws, &held.crop_mask).unwrap();
    let crop_entry = generation.artifact(&held.crop_mask).unwrap();
    assert!(
        crop_entry.frame.starts_with("crop:"),
        "{}",
        crop_entry.frame
    );
    assert_eq!(crop.identity, crop_entry.identity);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_store_resolves_by_identity_reopened_and_moved() {
    let dir = scratch("resolve");
    let (ws, held) = store(&dir.join("ws"));
    // An image rascal made resolves by the digest of the PNG it wrote (task 10.6), as its
    // reference names it.
    let refs: Vec<(ArtifactType, ArtifactRef, String)> = [
        (ArtifactType::Image, &held.image),
        (ArtifactType::Image, &held.canvas),
        (ArtifactType::Location, &held.location),
        (ArtifactType::Mask, &held.raw_mask),
        (ArtifactType::Mask, &held.crop_mask),
    ]
    .into_iter()
    .map(|(t, id)| (t, artifact_ref(&ws, id).unwrap(), id.clone()))
    .collect();
    for (t, reference, id) in &refs {
        let found = ws.resolve(*t, &reference.identity).unwrap().unwrap();
        assert_eq!(&found.id, id);
        assert_eq!(found.artifact_type, *t);
    }
    // An identity the store does not hold, and a type that does not match, resolve to nothing.
    let mask = &refs[3].1;
    assert!(
        ws.resolve(ArtifactType::Location, &mask.identity)
            .unwrap()
            .is_none()
    );
    assert!(
        ws.resolve(ArtifactType::Mask, &format!("sha256:{}", "0".repeat(64)))
            .unwrap()
            .is_none()
    );
    // An image resolves by its file digest, not rascal's own identity.
    let own = ws
        .snapshot()
        .unwrap()
        .artifact(&held.image)
        .unwrap()
        .identity
        .clone();
    assert!(ws.resolve(ArtifactType::Image, &own).unwrap().is_none());
    // A new commit is seen: the index follows the generation.
    let (_, later) = ws
        .add_location(
            Location::bbox(
                &ws.snapshot().unwrap().artifact(&held.image).unwrap().frame,
                [0.5, 0.5, 0.6, 0.6],
            )
            .unwrap(),
        )
        .unwrap();
    let later = later.identity();
    assert!(
        ws.resolve(ArtifactType::Location, later.as_str())
            .unwrap()
            .is_some()
    );
    drop(ws);
    // Reopened (a restart), then moved to another directory and reopened.
    let check = |store: &Workspace| {
        for (t, reference, _) in &refs {
            let found = store.resolve(*t, &reference.identity).unwrap().unwrap();
            assert_eq!(&artifact_ref(store, &found.id).unwrap(), reference);
        }
    };
    check(&Workspace::open(&dir.join("ws")).unwrap());
    std::fs::rename(dir.join("ws"), dir.join("moved")).unwrap();
    check(&Workspace::open(&dir.join("moved")).unwrap());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn binding_names_every_problem_in_order() {
    let dir = scratch("bind");
    let (ws, held) = store(&dir);
    let mask = artifact_ref(&ws, &held.crop_mask).unwrap();
    let location = artifact_ref(&ws, &held.location).unwrap();
    let given = |port, expected, reference| Given {
        port,
        index: None,
        expected,
        reference,
    };
    let found = binding::bind(&ws, given("mask", ArtifactType::Mask, &mask))
        .unwrap()
        .unwrap();
    assert_eq!(found.id, held.crop_mask);

    let mut other_tool = mask.clone();
    other_tool.artifact_type = "squint:Thing".to_string();
    other_tool.tool = "squint".to_string();
    let mut made_elsewhere = mask.clone();
    made_elsewhere.tool = "squint".to_string();
    let mut newer = mask.clone();
    newer.schema_version = 2;
    let mut missing = mask.clone();
    missing.identity = format!("sha256:{}", "a".repeat(64));
    for (reference, problem, got) in [
        (
            &location,
            BindingProblem::WrongType,
            "location v1".to_string(),
        ),
        (
            &other_tool,
            BindingProblem::WrongType,
            "squint:Thing v1".to_string(),
        ),
        (
            &made_elsewhere,
            BindingProblem::WrongType,
            "rascal:Mask v1 made by squint".to_string(),
        ),
        (
            &newer,
            BindingProblem::WrongVersion,
            "rascal:Mask v2".to_string(),
        ),
        (
            &missing,
            BindingProblem::Unresolved,
            missing.identity.clone(),
        ),
    ] {
        let refused = binding::bind(&ws, given("mask", ArtifactType::Mask, reference))
            .unwrap()
            .unwrap_err();
        assert_eq!(refused.problem, problem);
        assert_eq!(refused.port.as_deref(), Some("mask"));
        assert_eq!(refused.expected, "rascal:Mask v1");
        assert_eq!(refused.got, got);
        assert_eq!(
            refused.remedy.is_some(),
            problem == BindingProblem::Unresolved
        );
        valid(
            "urn:table-manners:operations:binding",
            &serde_json::to_value(&refused).unwrap(),
        );
    }
    let unresolved = binding::bind(&ws, given("mask", ArtifactType::Mask, &missing))
        .unwrap()
        .unwrap_err();
    assert_eq!(
        unresolved.remedy.as_deref(),
        Some("Import its bundle with the import tool (rascal import), then call again.")
    );
    // Two bad references: both named, in the order given, with their indexes in a many port.
    let both = binding::bind_all(
        &ws,
        &[
            Given {
                port: "masks",
                index: Some(0),
                expected: ArtifactType::Mask,
                reference: &newer,
            },
            Given {
                port: "masks",
                index: Some(1),
                expected: ArtifactType::Mask,
                reference: &mask,
            },
            Given {
                port: "masks",
                index: Some(2),
                expected: ArtifactType::Mask,
                reference: &missing,
            },
        ],
    )
    .unwrap()
    .unwrap_err();
    let named: Vec<(Option<u32>, BindingProblem)> =
        both.iter().map(|b| (b.index, b.problem)).collect();
    assert_eq!(
        named,
        [
            (Some(0), BindingProblem::WrongVersion),
            (Some(2), BindingProblem::Unresolved)
        ]
    );
    // The remedies by type.
    assert!(binding::remedy(ArtifactType::Location).contains("location itself"));
    assert!(binding::remedy(ArtifactType::Image).contains("by path or inline"));
    for t in [
        ArtifactType::BandProfile,
        ArtifactType::Series,
        ArtifactType::Plan,
    ] {
        assert!(binding::remedy(t).contains("cannot move between workspaces"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The way `serve` brings a bound Mask into a run: its bundle, imported from memory into a
/// workspace that imported the same image, arrives with its frame chain and its identity, and
/// its reference there is the store's.
#[test]
fn a_mask_crosses_into_another_workspace_by_its_bundle_in_memory() {
    let dir = scratch("cross");
    let (ws, held) = store(&dir.join("store"));
    let reference = artifact_ref(&ws, &held.crop_mask).unwrap();
    let bundle = ws.export_mask(&held.crop_mask).unwrap();
    let run = Workspace::open(&dir.join("run")).unwrap();
    let pano = root().join("fixtures/images/step-pano.png");
    run.import(&pano, None).unwrap();
    let imported = run
        .import_mask_bytes(&bundle.bytes, "mask", "mask", Some(&bundle.sha256))
        .unwrap();
    assert_eq!(imported.identity, reference.identity);
    assert!(imported.frame.starts_with("crop:"), "{}", imported.frame);
    assert_eq!(artifact_ref(&run, &imported.id).unwrap(), reference);
    let _ = std::fs::remove_dir_all(&dir);
}
