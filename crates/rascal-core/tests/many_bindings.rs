//! Task 4.2: a recipe binds a `many` output to a `many` input by prefix (`plane = "find.plane"`
//! expands to every `find.plane_<n>`), validation refuses the mismatched forms, and the
//! workspace delivers the artifacts as `<port>_<n>`.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use image::{DynamicImage, GrayImage, Luma};
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::image::{Image, Role};
use rascal_core::op::{Context, Cost, CostTier, Inputs, Manifest, Op, Output, Params, Port};
use rascal_core::recipe::{Recipe, Run};
use rascal_core::registry::OpRegistry;

fn port(name: &str, artifact_type: ArtifactType, many: bool) -> Port {
    Port {
        name: name.to_string(),
        artifact_type,
        required: true,
        purpose: "p".to_string(),
        many,
    }
}

fn manifest(name: &str, inputs: Vec<Port>, outputs: Vec<Port>) -> Manifest {
    Manifest::new(
        name,
        "1.0.0",
        "p",
        "w",
        "n",
        "o",
        Cost {
            tier: CostTier::Cheap,
            seconds: 0.0,
        },
        inputs,
        outputs,
        vec![],
    )
}

/// A finder: three planes from an image.
struct Find;

impl Op for Find {
    fn manifest(&self) -> Manifest {
        let mut m = manifest(
            "find",
            vec![port("image", ArtifactType::Image, false)],
            vec![port("plane", ArtifactType::Image, true)],
        );
        m.returns.max_images = 0;
        m
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, _: &Params) -> Result<Output> {
        let Some(Artifact::Image(image)) = inputs.get("image") else {
            unreachable!()
        };
        let frame = context.frames.require(&image.record.frame)?;
        let mut outputs = BTreeMap::new();
        for i in 0..3u32 {
            let plane = GrayImage::from_fn(frame.width, frame.height, |x, _| {
                Luma([if x / 10 == i { 255 } else { 0 }])
            });
            outputs.insert(
                format!("plane_{i}"),
                Artifact::Image(std::sync::Arc::new(Image::new(
                    &format!("plane_{i}"),
                    DynamicImage::ImageLuma8(plane),
                    frame,
                    Role::Plane,
                    None,
                ))),
            );
        }
        Ok(Output::ok("three planes", outputs))
    }
}

/// A gatherer: counts the planes it was given and returns one image naming the count.
struct Gather;

impl Op for Gather {
    fn manifest(&self) -> Manifest {
        manifest(
            "gather",
            vec![port("plane", ArtifactType::Image, true)],
            vec![port("image", ArtifactType::Image, false)],
        )
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, _: &Params) -> Result<Output> {
        let names: Vec<&String> = inputs.keys().collect();
        let Some(Artifact::Image(first)) = inputs.get("plane_0") else {
            unreachable!("{names:?}")
        };
        let frame = context.frames.require(&first.record.frame)?;
        let count = inputs.len() as u8;
        let mut outputs = BTreeMap::new();
        outputs.insert(
            "image".to_string(),
            Artifact::Image(std::sync::Arc::new(Image::new(
                "gathered",
                DynamicImage::ImageLuma8(GrayImage::from_pixel(
                    frame.width,
                    frame.height,
                    Luma([count]),
                )),
                frame,
                Role::Working,
                None,
            ))),
        );
        Ok(Output::ok(format!("{count} planes: {names:?}"), outputs))
    }
}

fn registry() -> OpRegistry {
    let mut r = OpRegistry::new();
    r.register(Box::new(Find)).unwrap();
    r.register(Box::new(Gather)).unwrap();
    r
}

fn recipe(gather_binding: &str) -> Recipe {
    Recipe::parse(
        &format!(
            r#"version = 1
name = "many"
use_case = "custom"
[inputs]
image = {{ type = "Image" }}
[[step]]
id = "find"
op = "find"
inputs = {{ image = "inputs.image" }}
[[step]]
id = "gather"
op = "gather"
inputs = {{ plane = "{gather_binding}" }}
[outputs]
image = "gather.image"
"#
        ),
        "test",
    )
    .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-many-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_many_output_binds_to_a_many_input_by_prefix_and_expands_at_run_time() {
    let registry = registry();
    recipe("find.plane").validate(&registry).unwrap();
    // One instance into a many input, or the whole output into a single input, are refused.
    let e = recipe("find.plane_1")
        .validate(&registry)
        .unwrap_err()
        .to_string();
    assert!(e.contains("takes every artifact of a many output"), "{e}");
    let mut single = recipe("find.plane");
    single.step[1]
        .inputs
        .insert("plane".into(), "find.plane".into());
    // (the gather port is many, so make a recipe whose consumer port is single instead)
    struct One;
    impl Op for One {
        fn manifest(&self) -> Manifest {
            manifest(
                "one",
                vec![port("plane", ArtifactType::Image, false)],
                vec![port("image", ArtifactType::Image, false)],
            )
        }
        fn run(&self, _: &Context<'_>, _: &Inputs, _: &Params) -> Result<Output> {
            unreachable!()
        }
    }
    let mut with_one = registry;
    with_one.register(Box::new(One)).unwrap();
    let mut r = recipe("find.plane");
    r.step[1].op = "one".into();
    let e = r.validate(&with_one).unwrap_err().to_string();
    assert!(
        e.contains("takes one artifact, but find.plane is a many output"),
        "{e}"
    );
    r.step[1]
        .inputs
        .insert("plane".into(), "find.plane_2".into());
    r.validate(&with_one).unwrap();

    // Run: gather sees plane_0, plane_1, plane_2 and nothing else.
    let dir = scratch("run");
    let floor = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/images/floor.png");
    let run = Run::open(&dir).unwrap();
    let (_, _, image) = run.workspace.import(&floor, None).unwrap();
    let mut inputs = BTreeMap::new();
    inputs.insert("image".to_string(), image);
    let outcome = recipe("find.plane").run(&with_one, inputs, &run).unwrap();
    assert_eq!(
        outcome.status,
        rascal_core::op::Status::Ok,
        "{:?}",
        outcome.reason
    );
    let Some(Artifact::Image(gathered)) = outcome.outputs.get("image") else {
        panic!()
    };
    assert_eq!(
        gathered.pixels.to_luma8().get_pixel(0, 0)[0],
        3,
        "three planes arrived"
    );
    let lines = rascal_core::journal::Journal::read(&dir.join("journal.jsonl")).unwrap();
    let gather = lines
        .iter()
        .find(|l| l.op.as_deref() == Some("gather"))
        .unwrap();
    let mut ports: Vec<&String> = gather.inputs.keys().collect();
    ports.sort();
    assert_eq!(ports, vec!["plane_0", "plane_1", "plane_2"]);
    let _ = std::fs::remove_dir_all(&dir);
}
