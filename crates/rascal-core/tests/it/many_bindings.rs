//! Task 4.2: a recipe binds a `many` output to a `many` input by prefix (`plane = "find.plane"`
//! expands to every `find.plane_<n>`), validation refuses the mismatched forms, and the
//! workspace delivers the artifacts as `<port>_<n>`. Task 13.11: a run from a step rebinds a many
//! port to the caller's artifacts, and a rebinding of the wrong type or frame is refused.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use image::{DynamicImage, GrayImage, Luma};
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::image::{Image, Role};
use rascal_core::op::Status;
use rascal_core::op::{Context, Cost, CostTier, Inputs, Manifest, Op, Output, Params, Port};
use rascal_core::recipe::{Rebind, Recipe, Run};
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
    fn group(&self) -> rascal_core::op::Group {
        rascal_core::op::Group::Find
    }

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
    fn group(&self) -> rascal_core::op::Group {
        rascal_core::op::Group::Refine
    }

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
        fn group(&self) -> rascal_core::op::Group {
            rascal_core::op::Group::Refine
        }

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
    let outcome = recipe("find.plane")
        .run(&with_one, inputs, &BTreeMap::new(), &run)
        .unwrap();
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

#[test]
fn an_optional_input_binds_only_an_optional_port_and_never_expands_into_a_single_one() {
    /// A consumer with a required `mask` port and an optional `extra` port.
    struct Consume;
    impl Op for Consume {
        fn group(&self) -> rascal_core::op::Group {
            rascal_core::op::Group::Refine
        }

        fn manifest(&self) -> Manifest {
            let mut extra = port("extra", ArtifactType::Image, false);
            extra.required = false;
            manifest(
                "consume",
                vec![port("image", ArtifactType::Image, false), extra],
                vec![port("image", ArtifactType::Image, false)],
            )
        }
        fn run(&self, context: &Context<'_>, inputs: &Inputs, _: &Params) -> Result<Output> {
            let Some(Artifact::Image(image)) = inputs.get("image") else {
                unreachable!()
            };
            let frame = context.frames.require(&image.record.frame)?;
            let seen: Vec<&String> = inputs.keys().collect();
            let mut outputs = BTreeMap::new();
            outputs.insert(
                "image".to_string(),
                Artifact::Image(std::sync::Arc::new(Image::new(
                    "consumed",
                    DynamicImage::ImageLuma8(GrayImage::from_pixel(
                        frame.width,
                        frame.height,
                        Luma([inputs.len() as u8]),
                    )),
                    frame,
                    Role::Working,
                    None,
                ))),
            );
            Ok(Output::ok(format!("{seen:?}"), outputs))
        }
    }
    let mut registry = OpRegistry::new();
    registry.register(Box::new(Consume)).unwrap();
    let parse = |inputs: &str, bindings: &str| {
        Recipe::parse(
            &format!(
                r#"version = 1
name = "opt"
use_case = "custom"
[inputs]
{inputs}
[[step]]
id = "consume"
op = "consume"
inputs = {{ {bindings} }}
[outputs]
image = "consume.image"
"#
            ),
            "test",
        )
        .unwrap()
    };
    // A required port may not bind an optional input; an optional port may.
    let bad = parse(
        "image = { type = \"Image\", optional = true }",
        "image = \"inputs.image\"",
    );
    let e = bad.validate(&registry).unwrap_err().to_string();
    assert!(
        e.contains("is required by consume, but inputs.image is an optional input"),
        "{e}"
    );
    let good = parse(
        "image = { type = \"Image\" }\nextra = { type = \"Image\", optional = true }",
        "image = \"inputs.image\", extra = \"inputs.extra\"",
    );
    good.validate(&registry).unwrap();
    // A hostile recipe declares `extra_0` beside the optional `extra`: the single port must
    // not pick it up when `extra` is left out; the op runs with `image` alone.
    let hostile = parse(
        "image = { type = \"Image\" }\nextra = { type = \"Image\", optional = true }\nextra_0 = { type = \"Image\" }",
        "image = \"inputs.image\", extra = \"inputs.extra\"",
    );
    hostile.validate(&registry).unwrap();
    let dir = scratch("optional");
    let floor = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/images/floor.png");
    let run = Run::open(&dir).unwrap();
    let (_, _, image) = run.workspace.import(&floor, None).unwrap();
    let mut inputs = BTreeMap::new();
    inputs.insert("image".to_string(), image.clone());
    inputs.insert("extra_0".to_string(), image);
    let outcome = hostile
        .run(&registry, inputs, &BTreeMap::new(), &run)
        .unwrap();
    assert_eq!(
        outcome.status,
        rascal_core::op::Status::Ok,
        "{:?}",
        outcome.reason
    );
    let Some(Artifact::Image(consumed)) = outcome.outputs.get("image") else {
        panic!()
    };
    assert_eq!(
        consumed.pixels.to_luma8().get_pixel(0, 0)[0],
        1,
        "only `image` arrived"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Two planes of the caller's in `image`'s frame, committed to the run's workspace.
fn planes(run: &Run, image: &Artifact, count: u32) -> Vec<Artifact> {
    let frames = run.frames().unwrap();
    let frame = frames.require(image.frame()).unwrap();
    (0..count)
        .map(|i| {
            let plane = GrayImage::from_fn(frame.width, frame.height, |x, _| {
                Luma([if x / 7 == i { 255 } else { 0 }])
            });
            let image = Image::new(
                &format!("given_{i}"),
                DynamicImage::ImageLuma8(plane),
                frame,
                Role::Plane,
                None,
            );
            run.workspace
                .add_external(image, frame, "given", serde_json::json!({ "plane": i }))
                .unwrap()
                .1
        })
        .collect()
}

fn rebind(step: &str, port: &str, artifacts: Vec<Artifact>) -> Rebind {
    Rebind {
        step: step.to_string(),
        port: port.to_string(),
        artifacts,
    }
}

#[test]
fn a_run_from_a_step_takes_the_bound_input_in_place_of_the_recipes() {
    let registry = registry();
    let recipe = recipe("find.plane");
    recipe.validate(&registry).unwrap();
    let dir = scratch("run-from");
    let floor = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/images/floor.png");
    let run = Run::open(&dir).unwrap();
    let (_, _, image) = run.workspace.import(&floor, None).unwrap();
    let mut inputs = BTreeMap::new();
    inputs.insert("image".to_string(), image.clone());
    let first = recipe
        .run(&registry, inputs, &BTreeMap::new(), &run)
        .unwrap();
    assert_eq!(first.status, Status::Ok, "{:?}", first.reason);

    let given = planes(&run, &image, 2);
    let again = recipe
        .run_from(
            &registry,
            &first,
            &rebind("gather", "plane", given.clone()),
            &run,
        )
        .unwrap();
    assert_eq!(again.status, Status::Ok, "{:?}", again.reason);
    let Some(Artifact::Image(gathered)) = again.outputs.get("image") else {
        panic!()
    };
    assert_eq!(
        gathered.pixels.to_luma8().get_pixel(0, 0)[0],
        2,
        "the two bound planes arrived, not find's three"
    );
    // find ran once; the second gather took exactly the bound planes, as plane_0 and plane_1.
    let lines = rascal_core::journal::Journal::read(&dir.join("journal.jsonl")).unwrap();
    let ops: Vec<&str> = lines.iter().filter_map(|l| l.op.as_deref()).collect();
    assert_eq!(
        ops,
        vec!["import", "find", "gather", "given", "given", "gather"]
    );
    let last = lines
        .iter()
        .rev()
        .find(|l| l.op.as_deref() == Some("gather"))
        .unwrap();
    let bound: Vec<(String, String)> = last
        .inputs
        .iter()
        .map(|(k, v)| (k.clone(), v.to_string()))
        .collect();
    let expected: Vec<(String, String)> = given
        .iter()
        .enumerate()
        .map(|(n, a)| (format!("plane_{n}"), a.identity().as_str().to_string()))
        .collect();
    assert_eq!(bound, expected);
    // The earlier run's bindings and texts carry over: the image input is still bound.
    assert!(again.bindings.contains_key("inputs.image"));
    assert!(again.bindings.contains_key("find.plane_0"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_rebinding_of_the_wrong_type_frame_count_step_or_port_is_refused_naming_the_step() {
    let registry = registry();
    let recipe = recipe("find.plane");
    let dir = scratch("run-from-refused");
    let images = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/images");
    let run = Run::open(&dir).unwrap();
    let (_, frame, image) = run
        .workspace
        .import(&images.join("floor.png"), None)
        .unwrap();
    let mut inputs = BTreeMap::new();
    inputs.insert("image".to_string(), image.clone());
    let first = recipe
        .run(&registry, inputs, &BTreeMap::new(), &run)
        .unwrap();
    let refused = |r: Rebind| {
        let e = recipe.run_from(&registry, &first, &r, &run).unwrap_err();
        assert!(
            matches!(&e, rascal_core::Error::Recipe { step, .. } if *step == r.step),
            "{e}"
        );
        e.to_string()
    };

    // A Location where the port takes Images.
    let location = rascal_core::location::Location::bbox(&frame.id, [0.0, 0.0, 0.5, 0.5]).unwrap();
    let (_, location) = run.workspace.add_location(location).unwrap();
    let e = refused(rebind("gather", "plane", vec![location]));
    assert!(
        e.contains("input plane must be a Image, but the bound plane_0 is a Location"),
        "{e}"
    );

    // A plane in another image's frame.
    let (_, _, chair) = run
        .workspace
        .import(&images.join("chair-blob.png"), None)
        .unwrap();
    let mut mixed = planes(&run, &image, 1);
    mixed.extend(planes(&run, &chair, 1));
    let e = refused(rebind("gather", "plane", mixed));
    assert!(
        e.contains(&format!(
            "input plane takes artifacts in frame {} (what find.plane gave it), but the bound plane_1 is in {}",
            image.frame(),
            chair.frame()
        )),
        "{e}"
    );

    // Nothing bound; a step or a port the recipe does not have.
    let e = refused(rebind("gather", "plane", vec![]));
    assert!(e.contains("input plane was bound to nothing"), "{e}");
    let e = refused(rebind("shape", "plane", planes(&run, &image, 1)));
    assert!(e.contains("no such step to run from"), "{e}");
    let e = refused(rebind("gather", "mask", planes(&run, &image, 1)));
    assert!(e.contains("op gather has no input mask to bind"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}
