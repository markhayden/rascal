//! Task 1.6: every recipe validation error with its step id, constant sets, defaults resolved
//! into the journal, and the recipe schema over the shipped recipes.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use image::DynamicImage;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::config::Config;
use rascal_core::frame::{Frame, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::location::Location;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};
use rascal_core::recipe::{Recipe, Run};
use rascal_core::registry::OpRegistry;
use rascal_core::schemas::SchemaDir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// An op with constant sets, for the tests: takes a mask, returns it.
struct Tuned;

impl Op for Tuned {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            "tuned",
            "1.0.0",
            "A probe with constant sets.",
            "In tests.",
            "Anywhere else.",
            "Returns its mask.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.0,
            },
            vec![Port {
                name: "mask".into(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask.".into(),
            }],
            vec![Port {
                name: "mask".into(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The mask.".into(),
            }],
            vec![
                Param::optional(
                    "gain",
                    "A number.",
                    ParamKind::Number,
                    serde_json::json!(2.5),
                )
                .bounded(0.0, 10.0),
            ],
        )
        .with_constants(&["tuned-v1", "tuned-v2"])
    }

    fn run(
        &self,
        _: &Context<'_>,
        inputs: &Inputs,
        _: &Params,
    ) -> rascal_core::error::Result<Output> {
        let mut outputs = BTreeMap::new();
        outputs.insert("mask".to_string(), inputs["mask"].clone());
        Ok(Output::ok("tuned", outputs))
    }
}

fn registry() -> OpRegistry {
    let mut registry = rascal_ops::registry().unwrap();
    registry.register(Box::new(Tuned)).unwrap();
    registry
}

const HEAD: &str = r#"
version = 1
name = "t"
use_case = "custom"
[inputs]
image = { type = "Image" }
location = { type = "Location" }
"#;

fn validate(body: &str) -> Result<Recipe, String> {
    let recipe = Recipe::parse(&format!("{HEAD}{body}"), "test").map_err(|e| e.to_string())?;
    recipe
        .validate(&registry())
        .map(|()| recipe)
        .map_err(|e| e.to_string())
}

const RASTERIZE: &str = r#"
[[step]]
id = "mask"
op = "rasterize"
inputs = { location = "inputs.location", image = "inputs.image" }
"#;

#[test]
fn every_validation_error_names_its_step() {
    let cases: &[(&str, &str)] = &[
        ("", "step -: a recipe needs at least one step"),
        (
            "[[step]]\nid = \"inputs\"\nop = \"rasterize\"\n",
            "step inputs: step ids must be non-empty",
        ),
        (
            "[[step]]\nid = \"a.b\"\nop = \"rasterize\"\n",
            "step a.b: step ids must be non-empty, contain no '.'",
        ),
        (
            &format!("{RASTERIZE}{RASTERIZE}"),
            "step mask: duplicate step id",
        ),
        (
            "[[step]]\nid = \"m\"\nop = \"nope\"\n",
            "step m: unknown op nope",
        ),
        (
            "[[step]]\nid = \"m\"\nop = \"rasterize\"\ninputs = { bogus = \"inputs.image\" }\n",
            "step m: op rasterize has no input bogus",
        ),
        (
            "[[step]]\nid = \"m\"\nop = \"rasterize\"\ninputs = { location = \"later.location\", image = \"inputs.image\" }\n",
            "step m: input location is bound to later.location, which is not an earlier step's output or a declared input",
        ),
        (
            "[[step]]\nid = \"m\"\nop = \"rasterize\"\ninputs = { location = \"inputs.image\", image = \"inputs.image\" }\n",
            "step m: input location must be a Location, but inputs.image is a Image",
        ),
        (
            "[[step]]\nid = \"m\"\nop = \"rasterize\"\ninputs = { image = \"inputs.image\" }\n",
            "step m: input location is required by rasterize",
        ),
        (
            "[[step]]\nid = \"m\"\nop = \"rasterize\"\ninputs = { location = \"inputs.location\", image = \"inputs.image\" }\nbogus = 1\n",
            "step m: op rasterize: unknown parameter bogus",
        ),
        (
            "[[step]]\nid = \"m\"\nop = \"rasterize\"\ninputs = { location = \"inputs.location\", image = \"inputs.image\" }\nuse_case = \"nope\"\n",
            "step m: op rasterize: use_case must be one of",
        ),
        (
            "[[step]]\nid = \"m\"\nop = \"rect\"\ninputs = { image = \"inputs.image\" }\n",
            "step m: op rect: parameter bbox is required",
        ),
        (
            "[[step]]\nid = \"m\"\nop = \"rect\"\ninputs = { image = \"inputs.image\" }\nbbox = [1, 2]\n",
            "step m: op rect: bbox must have exactly 4 numbers",
        ),
        (
            &format!(
                "{RASTERIZE}[[step]]\nid = \"t\"\nop = \"tuned\"\ninputs = {{ mask = \"mask.mask\" }}\ngain = 11\n"
            ),
            "step t: op tuned: gain must be in [0, 10]",
        ),
        (
            &format!(
                "{RASTERIZE}[[step]]\nid = \"t\"\nop = \"tuned\"\ninputs = {{ mask = \"mask.mask\" }}\nconstants = \"tuned-v9\"\n"
            ),
            "step t: op tuned has no constant set tuned-v9; it has tuned-v1, tuned-v2",
        ),
        (
            &format!("{RASTERIZE}constants = \"x\"\n"),
            "step mask: op rasterize has no constant sets",
        ),
        (
            &format!("{RASTERIZE}[outputs]\nmask = \"inputs.image\"\n"),
            "step outputs: mask = inputs.image is a Image, expected a Mask",
        ),
        (
            &format!("{RASTERIZE}[outputs]\nmask = \"nothing.mask\"\n"),
            "step outputs: mask = nothing.mask does not name a step output or an input",
        ),
        (
            &format!("{RASTERIZE}[outputs]\nimage = \"mask.mask\"\n"),
            "step outputs: image = mask.mask is a Mask, expected a Image",
        ),
        (
            &format!("{RASTERIZE}[outputs]\nrender = [{{ for = \"grey\", out = \"g.png\" }}]\n"),
            "step outputs: render needs outputs.mask",
        ),
        (
            &format!(
                "{RASTERIZE}[outputs]\nmask = \"mask.mask\"\nrender = [{{ for = \"grey\", out = \"../g.png\" }}]\n"
            ),
            "step outputs: render out '../g.png' must be a plain file name",
        ),
        (
            &format!(
                "{RASTERIZE}[outputs]\nmask = \"mask.mask\"\nrender = [{{ for = \"sepia\", out = \"g.png\" }}]\n"
            ),
            "step outputs: render profile 'sepia' is not one of grey, binary, alpha, tinted, overlay",
        ),
        (
            &format!(
                "{RASTERIZE}[outputs]\nmask = \"mask.mask\"\nrender = [{{ for = \"tinted\", out = \"t.png\" }}]\n"
            ),
            "step outputs: render profile 'tinted' draws on the image: set outputs.image",
        ),
    ];
    for (body, expected) in cases {
        let error = validate(body)
            .err()
            .unwrap_or_else(|| panic!("should fail: {body}"));
        assert!(
            error.contains(expected),
            "for {body:?}\n  got:      {error}\n  expected: {expected}"
        );
        assert!(error.starts_with("recipe t, "), "{error}");
    }
    // Parse-level errors name what is wrong too.
    let e = Recipe::parse(
        "version = 2\nname = \"x\"\nuse_case = \"custom\"\n[[step]]\nid = 3\nop = \"rect\"\n",
        "test",
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("`id` must be a string"), "{e}");
    let e = Recipe::parse(
        "version = 1\nname = \"x\"\nuse_case = \"custom\"\nbogus = 1\n",
        "test",
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("bogus"), "{e}");
    let e = validate("version = 2\n").err().unwrap();
    assert!(e.contains("duplicate key") || e.contains("version"), "{e}");
}

#[test]
fn a_good_recipe_validates_with_constants_and_defaults() {
    let recipe = validate(&format!(
        "{RASTERIZE}[[step]]\nid = \"t\"\nop = \"tuned\"\ninputs = {{ mask = \"mask.mask\" }}\nconstants = \"tuned-v2\"\n[outputs]\nmask = \"t.mask\"\nimage = \"inputs.image\"\nrender = [{{ for = \"tinted\", out = \"t.png\" }}]\n"
    ))
    .unwrap();
    assert_eq!(recipe.step[1].constants.as_deref(), Some("tuned-v2"));
    // Serializes back to TOML with the parameters flattened on the step.
    let text = toml::to_string(&recipe).unwrap();
    assert!(text.contains("constants = \"tuned-v2\""), "{text}");
    let again = Recipe::parse(&text, "again").unwrap();
    assert_eq!(again, recipe);
}

#[test]
fn resolved_defaults_and_the_constant_set_appear_in_the_journal() {
    let registry = registry();
    let frame = Frame::raw(40, 30, Projection::Flat);
    let recipe = validate(&format!(
        "{RASTERIZE}[[step]]\nid = \"t\"\nop = \"tuned\"\ninputs = {{ mask = \"mask.mask\" }}\n[[step]]\nid = \"u\"\nop = \"tuned\"\ninputs = {{ mask = \"t.mask\" }}\nconstants = \"tuned-v2\"\ngain = 1\n[outputs]\nmask = \"u.mask\"\n"
    ))
    .unwrap();
    let dir = std::env::temp_dir().join(format!("rascal-recipe-journal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let run = Run::open(&dir).unwrap();
    let pixels = DynamicImage::ImageRgb8(rascal_testkit::images::flat(40, 30, 90));
    let (_, artifact) = run
        .workspace
        .add_canonical(
            Image::new("i", pixels, &frame, Role::Canonical, None),
            &frame,
            "flat",
        )
        .unwrap();
    let mut inputs = BTreeMap::new();
    inputs.insert("image".to_string(), artifact);
    let location = Location::bbox(&frame.id, [0.1, 0.1, 0.5, 0.5]).unwrap();
    let identity = location.identity(&frame.identity);
    inputs.insert(
        "location".to_string(),
        Artifact::Location { location, identity },
    );
    let outcome = recipe.run(&registry, inputs, &run).unwrap();
    assert_eq!(outcome.status, rascal_core::op::Status::Ok);
    let lines = rascal_core::journal::Journal::read(&dir.join("journal.jsonl")).unwrap();
    let by_step = |id: &str| {
        lines
            .iter()
            .find(|l| l.step.as_deref() == Some(id))
            .unwrap()
            .params
            .clone()
            .unwrap()
    };
    assert_eq!(
        by_step("mask")["use_case"],
        "custom",
        "the rasterize default is recorded"
    );
    assert_eq!(by_step("t")["gain"], 2.5, "the tuned default is recorded");
    assert_eq!(
        by_step("t")["constants"],
        "tuned-v1",
        "the first constant set is the default"
    );
    assert_eq!(by_step("u")["gain"], 1);
    assert_eq!(by_step("u")["constants"], "tuned-v2");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_shipped_recipes_validate_against_the_schema_and_the_registry() {
    let dir = SchemaDir::new(repo_root().join("schemas"));
    let validator = dir.validator("rascal/recipe-v1.json").unwrap();
    let recipes = repo_root().join("crates/rascal-mask/recipes");
    let mut count = 0;
    for entry in std::fs::read_dir(&recipes).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "toml") {
            let recipe = Recipe::read(&path).unwrap();
            recipe
                .validate(&rascal_ops::registry().unwrap())
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let value = serde_json::to_value(&recipe).unwrap();
            if let Err(e) = validator.validate(&value) {
                panic!("{}: {e}", path.display());
            }
            count += 1;
        }
    }
    assert!(count >= 1);
    // The schema refuses what the parser refuses.
    for bad in [
        serde_json::json!({ "version": 1, "name": "x", "use_case": "nope" }),
        serde_json::json!({ "version": 1, "name": "x", "use_case": "custom", "step": [{ "id": "inputs", "op": "rect" }] }),
        serde_json::json!({ "version": 1, "name": "x", "use_case": "custom", "step": [{ "id": "a.b", "op": "rect" }] }),
        serde_json::json!({ "version": 1, "name": "x", "use_case": "custom", "outputs": { "render": [{ "for": "sepia", "out": "g.png" }] } }),
        serde_json::json!({ "version": 1, "name": "x", "use_case": "custom", "bogus": 1 }),
    ] {
        assert!(validator.validate(&bad).is_err(), "{bad}");
    }
}

#[test]
fn the_config_file_parses_and_refuses_unknown_sections() {
    let c = Config::parse("config_version = 1\n", "t").unwrap();
    assert!(c.workspace.is_none() && c.corpus.is_none());
    let c = Config::parse(
        "config_version = 1\n[workspace]\ndir = \"ws\"\n[corpus]\nmanifest = \"corpus.toml\"\n",
        "t",
    )
    .unwrap();
    assert_eq!(c.workspace.unwrap().dir.as_deref(), Some("ws"));
    assert!(Config::parse("config_version = 2\n", "t").is_err());
    let e = Config::parse("config_version = 1\n[fillers.x]\ncommand = [\"a\"]\n", "t")
        .unwrap_err()
        .to_string();
    assert!(e.contains("fillers"), "{e}");
}
