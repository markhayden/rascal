//! Text inputs: a recipe declares `prompt = { type = "Text" }`, a step's string parameter
//! names it as its value, and the run substitutes the text the caller gave; the journal
//! records the resolved value. Validation refuses text on a port, on a non-string parameter,
//! and an optional text behind a required parameter; a run refuses a missing required text
//! and text given for an artifact input.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use image::{DynamicImage, Rgb, RgbImage};
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::location::Location;
use rascal_core::recipe::{InputType, Recipe, Run};

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("rascal-text-inputs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const RECIPE: &str = r#"
version = 1
name = "tint"
use_case = "custom"

[inputs]
image = { type = "Image" }
location = { type = "Location" }
colour = { type = "Text" }
note = { type = "Text", optional = true }

[[step]]
id = "rasterize"
op = "rasterize"
inputs = { location = "inputs.location", image = "inputs.image" }
use_case = "custom"

[[step]]
id = "render"
op = "render"
inputs = { mask = "rasterize.mask", image = "inputs.image" }
for = "tinted"
colour = "inputs.colour"

[outputs]
mask = "rasterize.mask"
image = "inputs.image"
"#;

fn inputs(run: &Run) -> (BTreeMap<String, Artifact>, Frame) {
    let frame = Frame::raw(32, 24, Projection::Flat);
    let pixels = DynamicImage::ImageRgb8(RgbImage::from_pixel(32, 24, Rgb([80, 90, 100])));
    let image = Image::new("inputs.image", pixels, &frame, Role::Canonical, None);
    let (_, artifact) = run.workspace.add_canonical(image, &frame, "flat").unwrap();
    let location = Location::bbox(&frame.id, [0.25, 0.25, 0.75, 0.75]).unwrap();
    let (_, location) = run.workspace.add_location(location).unwrap();
    let mut inputs = BTreeMap::new();
    inputs.insert("image".to_string(), artifact);
    inputs.insert("location".to_string(), location);
    (inputs, frame)
}

fn texts(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn render_params(dir: &std::path::Path) -> serde_json::Value {
    let lines = rascal_core::journal::Journal::read(&dir.join("journal.jsonl")).unwrap();
    let line = lines
        .iter()
        .find(|l| l.step.as_deref() == Some("render"))
        .unwrap();
    serde_json::to_value(&line.params).unwrap()
}

#[test]
fn a_text_input_resolves_into_the_parameter_and_the_journal() {
    let registry = rascal_ops::registry().unwrap();
    let recipe = Recipe::parse(RECIPE, "t").unwrap();
    assert_eq!(recipe.inputs["colour"].kind, InputType::Text);
    assert_eq!(recipe.inputs["colour"].kind.to_string(), "Text");
    assert_eq!(recipe.inputs["image"].kind.to_string(), "Image");
    recipe.validate(&registry).unwrap();
    let dir = scratch("resolves");
    let run = Run::open(&dir).unwrap();
    let (inputs, _) = inputs(&run);
    let outcome = recipe
        .run(&registry, inputs, &texts(&[("colour", "#00ff00")]), &run)
        .unwrap();
    assert_eq!(outcome.status, rascal_core::op::Status::Ok);
    assert_eq!(render_params(&dir)["colour"], "#00ff00");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_required_text_and_text_for_an_artifact_are_refused() {
    let registry = rascal_ops::registry().unwrap();
    let recipe = Recipe::parse(RECIPE, "t").unwrap();
    let dir = scratch("missing");
    let run = Run::open(&dir).unwrap();
    let (inputs, _) = inputs(&run);
    let err = recipe
        .run(&registry, inputs.clone(), &BTreeMap::new(), &run)
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("input colour (Text) was not given"),
        "{err}"
    );
    let err = recipe
        .run(
            &registry,
            inputs.clone(),
            &texts(&[("colour", "#00ff00"), ("location", "x")]),
            &run,
        )
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("input location is a Location, but was given as text"),
        "{err}"
    );
    let err = recipe
        .run(
            &registry,
            inputs,
            &texts(&[("colour", "#00ff00"), ("bogus", "x")]),
            &run,
        )
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("input bogus is not one recipe tint declares"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_optional_text_left_out_leaves_the_parameter_to_its_default() {
    let registry = rascal_ops::registry().unwrap();
    let recipe = Recipe::parse(
        &RECIPE.replace("colour = \"inputs.colour\"", "colour = \"inputs.note\""),
        "t",
    )
    .unwrap();
    recipe.validate(&registry).unwrap();
    let dir = scratch("optional");
    let run = Run::open(&dir).unwrap();
    let (inputs, _) = inputs(&run);
    let outcome = recipe
        .run(&registry, inputs, &texts(&[("colour", "#00ff00")]), &run)
        .unwrap();
    assert_eq!(outcome.status, rascal_core::op::Status::Ok);
    assert_eq!(render_params(&dir)["colour"], "#ff00ff");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn validation_refuses_text_on_a_port_a_number_and_an_optional_behind_a_required() {
    let registry = rascal_ops::registry().unwrap();
    // Text bound to a port.
    let on_port = RECIPE.replace(
        "inputs = { mask = \"rasterize.mask\", image = \"inputs.image\" }",
        "inputs = { mask = \"rasterize.mask\", image = \"inputs.colour\" }",
    );
    let err = Recipe::parse(&on_port, "t")
        .unwrap()
        .validate(&registry)
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("a text input; text binds a string parameter"),
        "{err}"
    );
    // Text on a number parameter.
    let on_number = RECIPE.replace("colour = \"inputs.colour\"", "strength = \"inputs.colour\"");
    let err = Recipe::parse(&on_number, "t")
        .unwrap()
        .validate(&registry)
        .unwrap_err();
    assert!(
        err.to_string().contains(
            "parameter strength takes text input inputs.colour, but it is a Number parameter"
        ),
        "{err}"
    );
    // An optional text behind a required parameter.
    let behind_required = RECIPE.replace("for = \"tinted\"", "for = \"inputs.note\"");
    let err = Recipe::parse(&behind_required, "t")
        .unwrap()
        .validate(&registry)
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("takes text input inputs.note, but it is a Choice parameter"),
        "{err}"
    );
    let behind_required = RECIPE
        .replace("colour = { type = \"Text\" }", "colour = { type = \"Text\", optional = true }")
        .replace("inputs = { mask = \"rasterize.mask\", image = \"inputs.image\" }\nfor = \"tinted\"\ncolour = \"inputs.colour\"", "inputs = { mask = \"rasterize.mask\" }\nfor = \"inputs.colour\"");
    let err = Recipe::parse(&behind_required, "t")
        .unwrap()
        .validate(&registry)
        .unwrap_err();
    assert!(
        err.to_string().contains("takes text input inputs.colour"),
        "{err}"
    );
    // An unknown input type is a parse error.
    assert!(Recipe::parse(&RECIPE.replace("type = \"Text\"", "type = \"Prompt\""), "t").is_err());
}
