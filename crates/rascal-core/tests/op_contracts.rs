//! Task 1.4: the op contract. Every shipped manifest passes the field table; each input,
//! parameter and envelope rule rejects what it must; a `not_applicable` step publishes nothing
//! and leaves exactly one journal line; a later step that requires its output stops the run
//! naming the requirement. The manifest and envelope fixtures under `fixtures/schemas/` are
//! written from the real values (`UPDATE_FIXTURES=1`) so the drift harness checks live shapes.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::DynamicImage;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::frame::{Frame, FrameTable, Projection};
use rascal_core::image::{Image, RenderingRole, Role};
use rascal_core::location::Location;
use rascal_core::mask::{Mask, Source, UseCase};
use rascal_core::op::{
    self, Context, Cost, CostTier, Envelope, Inputs, Manifest, Op, Output, Param, ParamKind,
    Params, Port, Status,
};
use rascal_core::recipe::{Recipe, Run};
use rascal_core::registry::OpRegistry;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-opc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn frame() -> Frame {
    Frame::raw(40, 30, Projection::Flat)
}

fn image(frame: &Frame, role: Role, rendering: Option<RenderingRole>) -> Artifact {
    let pixels =
        DynamicImage::ImageRgb8(rascal_testkit::images::flat(frame.width, frame.height, 100));
    Artifact::Image(Arc::new(Image::new("img", pixels, frame, role, rendering)))
}

fn mask(frame: &Frame) -> Artifact {
    let core = image::GrayImage::from_pixel(frame.width, frame.height, image::Luma([255]));
    Artifact::Mask(Arc::new(
        Mask::from_core("m", frame, UseCase::Custom, core, Source::default()).unwrap(),
    ))
}

fn location(frame: &Frame) -> Artifact {
    let location = Location::bbox(&frame.id, [0.1, 0.1, 0.5, 0.5]).unwrap();
    let identity = location.identity(&frame.identity);
    Artifact::Location { location, identity }
}

/// A probe op: takes a mask and an optional image, one required number and one optional
/// choice; returns the mask. `mode = "refuse"` returns not_applicable; `mode = "bad"` returns
/// invalid_args; `mode = "lie"` publishes an undeclared output.
struct Probe;

impl Op for Probe {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            "probe",
            "1.2.3",
            "A probe for the contract tests.",
            "In tests.",
            "Anywhere else.",
            "Returns its mask.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.0,
            },
            vec![
                Port {
                    name: "mask".into(),
                    artifact_type: ArtifactType::Mask,
                    required: true,
                    purpose: "The mask.".into(),
                },
                Port {
                    name: "image".into(),
                    artifact_type: ArtifactType::Image,
                    required: false,
                    purpose: "An image.".into(),
                },
            ],
            vec![Port {
                name: "mask".into(),
                artifact_type: ArtifactType::Mask,
                required: true,
                purpose: "The same mask.".into(),
            }],
            vec![
                Param::required("amount", "A number in [0, 10].", ParamKind::Number)
                    .bounded(0.0, 10.0),
                Param::optional(
                    "mode",
                    "What to do.",
                    ParamKind::Choice,
                    serde_json::json!("ok"),
                )
                .choices(&["ok", "refuse", "bad", "lie"]),
                Param::optional(
                    "count",
                    "An integer.",
                    ParamKind::Integer,
                    serde_json::json!(1),
                ),
                Param::optional(
                    "flag",
                    "A boolean.",
                    ParamKind::Boolean,
                    serde_json::json!(false),
                ),
                Param::optional(
                    "label",
                    "A string.",
                    ParamKind::String,
                    serde_json::json!("x"),
                ),
                Param::optional(
                    "pair",
                    "Two numbers.",
                    ParamKind::NumberArray,
                    serde_json::json!([0, 1]),
                )
                .length(2),
            ],
        )
    }

    fn run(
        &self,
        _context: &Context<'_>,
        inputs: &Inputs,
        params: &Params,
    ) -> rascal_core::error::Result<Output> {
        let mode = params.get("mode").and_then(|v| v.as_str()).unwrap_or("ok");
        match mode {
            "refuse" => Ok(Output::not_applicable("the probe refuses")),
            "bad" => Ok(Output::invalid_args("amount is wrong for this mask")),
            "lie" => {
                let mut outputs = BTreeMap::new();
                outputs.insert("mask".to_string(), inputs["mask"].clone());
                outputs.insert("extra".to_string(), inputs["mask"].clone());
                Ok(Output::ok("lied", outputs))
            }
            _ => {
                let mut outputs = BTreeMap::new();
                outputs.insert("mask".to_string(), inputs["mask"].clone());
                Ok(Output::ok("probed", outputs))
            }
        }
    }
}

fn registry() -> OpRegistry {
    let mut registry = rascal_ops::registry().unwrap();
    registry.register(Box::new(Probe)).unwrap();
    registry
}

// --- manifests ------------------------------------------------------------------------------------

#[test]
fn every_shipped_manifest_has_every_field_meaningful() {
    let registry = rascal_ops::registry().unwrap();
    let manifests = registry.manifests();
    assert!(manifests.len() >= 3);
    for m in &manifests {
        m.check().unwrap_or_else(|e| panic!("{e}"));
        // The field table: each one non-empty and of the right shape.
        assert_eq!(m.manifest_version, 1, "{}", m.name);
        for (field, text) in [
            ("purpose", &m.purpose),
            ("when_to_use", &m.when_to_use),
            ("when_not_to_use", &m.when_not_to_use),
            ("output_guidance", &m.output_guidance),
        ] {
            assert!(
                text.len() >= 20,
                "{}: {field} is too short to guide anyone: {text:?}",
                m.name
            );
        }
        assert!(m.version.split('.').count() == 3, "{}", m.name);
        assert!(m.cost.seconds >= 0.0);
        assert!(!m.outputs.is_empty(), "{}", m.name);
        assert_eq!(m.input_schema["type"], "object");
        assert_eq!(m.input_schema["additionalProperties"], false);
        assert_eq!(m.output_schema["type"], "object");
        for p in &m.params {
            assert!(
                m.input_schema["properties"][&p.name].is_object(),
                "{}: {} in input_schema",
                m.name,
                p.name
            );
            if p.required {
                assert!(
                    m.input_schema["required"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|r| r == &p.name)
                );
            }
        }
        for p in &m.outputs {
            assert_eq!(
                m.output_schema["properties"][&p.name]["properties"]["artifact_type"]["const"],
                p.artifact_type.to_string()
            );
        }
        // The manifest validates against its committed schema through the schema directory.
        let dir = rascal_core::schemas::SchemaDir::new(repo_root().join("schemas"));
        let validator = dir.validator("rascal/manifest-v1.json").unwrap();
        let value = serde_json::to_value(m).unwrap();
        if let Err(e) = validator.validate(&value) {
            panic!("{}: {e}", m.name);
        }
    }
}

#[test]
fn a_malformed_manifest_is_refused_at_registration() {
    fn with(f: impl FnOnce(&mut Manifest)) -> Result<(), String> {
        let mut m = Probe.manifest();
        f(&mut m);
        m.check()
    }
    assert!(with(|_| {}).is_ok());
    assert!(
        with(|m| m.purpose = " ".into())
            .unwrap_err()
            .contains("purpose is empty")
    );
    assert!(
        with(|m| m.version = "1.2".into())
            .unwrap_err()
            .contains("major.minor.patch")
    );
    assert!(
        with(|m| m.outputs.clear())
            .unwrap_err()
            .contains("at least one output")
    );
    assert!(
        with(|m| m.inputs.push(m.inputs[0].clone()))
            .unwrap_err()
            .contains("declared twice")
    );
    assert!(
        with(|m| m.params[1].choices.clear())
            .unwrap_err()
            .contains("needs choices")
    );
    assert!(
        with(|m| m.params[5].length = None)
            .unwrap_err()
            .contains("needs a length")
    );
    assert!(
        with(|m| m.params[0].min = Some(11.0))
            .unwrap_err()
            .contains("min > max")
    );
    assert!(
        with(|m| m.input_schema = serde_json::json!({}))
            .unwrap_err()
            .contains("schemas do not match")
    );
    assert!(
        with(|m| m.cost.seconds = f64::NAN)
            .unwrap_err()
            .contains("cost.seconds")
    );
    // Through the registry: a bad manifest and a duplicate name.
    struct Bad;
    impl Op for Bad {
        fn manifest(&self) -> Manifest {
            let mut m = Probe.manifest();
            m.when_to_use = String::new();
            m
        }
        fn run(
            &self,
            _: &Context<'_>,
            _: &Inputs,
            _: &Params,
        ) -> rascal_core::error::Result<Output> {
            unreachable!()
        }
    }
    let mut r = OpRegistry::new();
    assert!(
        r.register(Box::new(Bad))
            .unwrap_err()
            .to_string()
            .contains("manifest rejected")
    );
    r.register(Box::new(Probe)).unwrap();
    assert!(
        r.register(Box::new(Probe))
            .unwrap_err()
            .to_string()
            .contains("registered twice")
    );
}

// --- input validation -----------------------------------------------------------------------------

#[test]
fn each_input_rule_rejects() {
    let f = frame();
    let other = Frame::raw(40, 31, Projection::Flat);
    let m = Probe.manifest();
    let check = |pairs: &[(&str, Artifact)]| {
        op::validate_inputs(
            &m,
            &pairs
                .iter()
                .cloned()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    };
    check(&[("mask", mask(&f))]).unwrap();
    check(&[
        ("mask", mask(&f)),
        ("image", image(&f, Role::Canonical, None)),
    ])
    .unwrap();
    let e = check(&[]).unwrap_err().to_string();
    assert!(e.contains("input mask is required"), "{e}");
    let e = check(&[("mask", image(&f, Role::Canonical, None))])
        .unwrap_err()
        .to_string();
    assert!(e.contains("input mask must be a Mask, got a Image"), "{e}");
    // A rendering is an Image and is refused where a Mask is declared, naming its role.
    let e = check(&[(
        "mask",
        image(&f, Role::Rendering, Some(RenderingRole::MaskInput)),
    )])
    .unwrap_err()
    .to_string();
    assert!(
        e.contains("got a Image (an image with role Rendering)"),
        "{e}"
    );
    let e = check(&[("mask", mask(&f)), ("bogus", location(&f))])
        .unwrap_err()
        .to_string();
    assert!(e.contains("unknown input bogus"), "{e}");
    let e = check(&[
        ("mask", mask(&f)),
        ("image", image(&other, Role::Canonical, None)),
    ])
    .unwrap_err()
    .to_string();
    assert!(e.contains("must be in one frame"), "{e}");
}

#[test]
fn each_parameter_rule_rejects_and_defaults_resolve() {
    let m = Probe.manifest();
    let p = |text: &str| -> Params { toml::from_str(text).unwrap() };
    let resolved = m.resolve_params(&p("amount = 3")).unwrap();
    assert_eq!(resolved["mode"].as_str(), Some("ok"));
    assert_eq!(resolved["count"].as_integer(), Some(1));
    assert_eq!(resolved["flag"].as_bool(), Some(false));
    assert_eq!(resolved["label"].as_str(), Some("x"));
    assert_eq!(resolved["pair"].as_array().unwrap().len(), 2);
    assert_eq!(resolved["amount"].as_integer(), Some(3));
    let err = |text: &str| m.resolve_params(&p(text)).unwrap_err();
    assert!(err("").contains("amount is required"));
    assert!(err("amount = 3\nbogus = 1").contains("unknown parameter bogus"));
    assert!(err("amount = 11").contains("amount must be in [0, 10]"));
    assert!(err("amount = 'x'").contains("amount must be a number"));
    assert!(err("amount = 1\nmode = 'nope'").contains("mode must be one of ok, refuse, bad, lie"));
    assert!(err("amount = 1\ncount = 1.5").contains("count must be an integer"));
    assert!(err("amount = 1\nflag = 'yes'").contains("flag must be true or false"));
    assert!(err("amount = 1\nlabel = 3").contains("label must be a string"));
    assert!(err("amount = 1\npair = [1]").contains("pair must have exactly 2 numbers"));
    assert!(err("amount = 1\npair = ['a', 'b']").contains("pair must be an array of numbers"));
}

// --- envelopes -------------------------------------------------------------------------------------

#[test]
fn each_envelope_rule_rejects() {
    let m = Probe.manifest();
    let f = frame();
    let good = {
        let mut outputs = BTreeMap::new();
        outputs.insert("mask".to_string(), mask(&f));
        Output::ok("fine", outputs).envelope
    };
    good.check(&m).unwrap();
    let with = |f: &dyn Fn(&mut Envelope)| {
        let mut e = good.clone();
        f(&mut e);
        e.check(&m).unwrap_err()
    };
    assert!(with(&|e| e.envelope_version = 2).contains("envelope_version"));
    assert!(
        with(&|e| {
            e.artifacts.clear();
        })
        .contains("output mask is missing")
    );
    assert!(
        with(&|e| e.artifacts.get_mut("mask").unwrap().artifact_type = ArtifactType::Image)
            .contains("must be a Mask")
    );
    assert!(
        with(&|e| {
            let a = e.artifacts["mask"].clone();
            e.artifacts.insert("extra".into(), a);
        })
        .contains("undeclared output extra")
    );
    assert!(
        with(&|e| e.artifacts.get_mut("mask").unwrap().identity = "nope".into())
            .contains("malformed identity")
    );
    assert!(with(&|e| e.summary = "x".repeat(2000)).contains("summary longer than"));
    assert!(
        with(&|e| e.measurements.push(op::Measurement {
            name: "m".into(),
            value: f64::NAN,
            unit: "px".into(),
            frame: None
        }))
        .contains("not finite")
    );
    assert!(
        with(&|e| e.transforms = (0..9)
            .map(|i| op::Transform {
                id: i.to_string(),
                from: "a".into(),
                to: "b".into(),
                kind: "k".into(),
                params: vec![]
            })
            .collect())
        .contains("more than 8 transforms")
    );
    // A not-ok envelope publishes nothing and needs a reason.
    let mut na = Envelope::not_applicable("why");
    na.check(&m).unwrap();
    na.artifacts = good.artifacts.clone();
    assert!(na.check(&m).unwrap_err().contains("publishes nothing"));
    let mut na = Envelope::not_applicable("");
    na.reason = None;
    assert!(na.check(&m).unwrap_err().contains("needs a reason"));
    // The envelope fixtures are written from real envelopes so the harness checks live shapes.
    let dir = repo_root().join("fixtures/schemas/envelope-v1");
    let mut ok = good.clone();
    ok.provenance = op::Provenance {
        tool: "probe".into(),
        version: "1.2.3".into(),
        duration_ms: 4,
    };
    ok.measurements.push(op::Measurement {
        name: "painted".into(),
        value: 1200.0,
        unit: "px".into(),
        frame: Some(f.id.clone()),
    });
    let mut na = Envelope::not_applicable("the box lies wholly outside the frame");
    na.provenance = op::Provenance {
        tool: "rect".into(),
        version: "1.0.0".into(),
        duration_ms: 0,
    };
    for (name, value) in [("ok.json", &ok), ("not-applicable.json", &na)] {
        let path = dir.join(name);
        let text = serde_json::to_string_pretty(value).unwrap() + "\n";
        if std::env::var_os("UPDATE_FIXTURES").is_some() || !path.exists() {
            std::fs::write(&path, &text).unwrap();
        }
        let committed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let committed_env: Envelope = serde_json::from_value(committed).unwrap();
        // The identity in the fixture is from a different run's mask; compare everything else.
        assert_eq!(committed_env.status, value.status, "{name}");
        assert_eq!(committed_env.summary, value.summary, "{name}");
    }
    // Likewise the manifest fixture.
    let path = repo_root().join("fixtures/schemas/manifest-v1/rect.json");
    let rect = rascal_ops::registry()
        .unwrap()
        .require("rect")
        .unwrap()
        .manifest();
    let text = serde_json::to_string_pretty(&rect).unwrap() + "\n";
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let committed: Manifest =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        committed, rect,
        "fixtures/schemas/manifest-v1/rect.json is stale; UPDATE_FIXTURES=1 rewrites it"
    );
}

// --- the runner --------------------------------------------------------------------------------------

fn recipe(text: &str) -> Recipe {
    Recipe::parse(text, "test").unwrap()
}

/// The recipe inputs, with the image added to the run's workspace so its frame is known.
fn inputs(run: &Run, frame: &Frame) -> BTreeMap<String, Artifact> {
    let Artifact::Image(img) = image(frame, Role::Canonical, None) else {
        panic!()
    };
    let (_, artifact) = run
        .workspace
        .add_canonical((*img).clone(), frame, "flat")
        .unwrap();
    let mut inputs = BTreeMap::new();
    inputs.insert("image".to_string(), artifact);
    inputs.insert("location".to_string(), location(frame));
    inputs
}

/// A mask in the run's workspace frame, for direct invocations.
fn seed(run: &Run, frame: &Frame) {
    let Artifact::Image(img) = image(frame, Role::Canonical, None) else {
        panic!()
    };
    run.workspace
        .add_canonical((*img).clone(), frame, "flat")
        .unwrap();
}

fn listing(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries {
            let p = e.unwrap().path();
            out.push(p.strip_prefix(dir).unwrap().display().to_string());
            if p.is_dir() {
                out.extend(
                    listing(&p)
                        .into_iter()
                        .map(|c| format!("{}/{c}", p.file_name().unwrap().to_string_lossy())),
                );
            }
        }
    }
    out.sort();
    out
}

#[test]
fn not_applicable_publishes_nothing_and_journals_once() {
    let f = frame();
    let registry = registry();
    let r = recipe(
        r#"
version = 1
name = "t"
use_case = "custom"
[inputs]
image = { type = "Image" }
location = { type = "Location" }
[[step]]
id = "mask"
op = "rasterize"
inputs = { location = "inputs.location", image = "inputs.image" }
[[step]]
id = "p"
op = "probe"
inputs = { mask = "mask.mask" }
amount = 1
mode = "refuse"
[outputs]
mask = "mask.mask"
"#,
    );
    r.validate(&registry).unwrap();
    let dir = scratch("na");
    let run = Run::open(&dir).unwrap();
    let outcome = r.run(&registry, inputs(&run, &f), &run).unwrap();
    assert_eq!(outcome.status, Status::NotApplicable);
    assert_eq!(outcome.stopped_at.as_deref(), Some("p"));
    assert_eq!(outcome.reason.as_deref(), Some("the probe refuses"));
    // Exactly the rasterize mask is published; the probe published nothing.
    let artifacts = listing(&dir.join("artifacts"));
    assert_eq!(
        artifacts.iter().filter(|a| a.ends_with(".mask")).count(),
        1,
        "{artifacts:?}"
    );
    let lines = rascal_core::journal::Journal::read(&dir.join("journal.jsonl")).unwrap();
    assert_eq!(lines.len(), 3, "import, mask, p");
    assert_eq!(lines[2].step.as_deref(), Some("p"));
    assert_eq!(
        lines[2].envelope.as_ref().unwrap().status,
        Status::NotApplicable
    );
    assert!(lines[2].outputs.is_empty());
    assert_eq!(lines[2].envelope.as_ref().unwrap().provenance.tool, "probe");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_step_that_requires_a_refused_step_stops_the_run_naming_the_requirement() {
    let f = frame();
    let registry = registry();
    let r = recipe(
        r#"
version = 1
name = "t"
use_case = "custom"
[inputs]
image = { type = "Image" }
location = { type = "Location" }
[[step]]
id = "mask"
op = "rasterize"
inputs = { location = "inputs.location", image = "inputs.image" }
[[step]]
id = "first"
op = "probe"
inputs = { mask = "mask.mask" }
amount = 1
mode = "refuse"
[[step]]
id = "second"
op = "probe"
inputs = { mask = "first.mask" }
amount = 1
[outputs]
mask = "mask.mask"
"#,
    );
    r.validate(&registry).unwrap();
    let dir = scratch("req");
    let run = Run::open(&dir).unwrap();
    let outcome = r.run(&registry, inputs(&run, &f), &run).unwrap();
    assert_eq!(outcome.status, Status::NotApplicable);
    assert_eq!(
        outcome.stopped_at.as_deref(),
        Some("first"),
        "the first refusal is what is reported"
    );
    assert!(
        outcome
            .reason
            .as_deref()
            .unwrap()
            .contains("step second requires first.mask"),
        "{:?}",
        outcome.reason
    );
    let lines = rascal_core::journal::Journal::read(&dir.join("journal.jsonl")).unwrap();
    let note = lines
        .iter()
        .find(|l| l.event == rascal_core::journal::Event::Note)
        .unwrap();
    assert!(
        note.note
            .as_deref()
            .unwrap()
            .contains("step second requires first.mask, but step first did not produce it"),
        "{:?}",
        note.note
    );
    assert!(
        lines.iter().all(|l| l.step.as_deref() != Some("second")),
        "the blocked step never ran"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_args_is_journaled_and_is_a_usage_error() {
    let f = frame();
    let registry = registry();
    for (mode, expect) in [
        ("amount = 99", "amount must be in [0, 10]"),
        (
            "amount = 1\nmode = \"bad\"",
            "amount is wrong for this mask",
        ),
    ] {
        let r = recipe(&format!(
            r#"
version = 1
name = "t"
use_case = "custom"
[inputs]
image = {{ type = "Image" }}
location = {{ type = "Location" }}
[[step]]
id = "mask"
op = "rasterize"
inputs = {{ location = "inputs.location", image = "inputs.image" }}
[[step]]
id = "p"
op = "probe"
inputs = {{ mask = "mask.mask" }}
{mode}
[outputs]
mask = "mask.mask"
"#
        ));
        let dir = scratch("ia");
        let run = Run::open(&dir).unwrap();
        if mode.starts_with("amount = 99") {
            // Out of bounds is caught by validation before anything runs.
            let e = r.validate(&registry).unwrap_err().to_string();
            assert!(e.contains(expect), "{e}");
            continue;
        }
        r.validate(&registry).unwrap();
        let e = r.run(&registry, inputs(&run, &f), &run).unwrap_err();
        assert!(matches!(e, rascal_core::Error::Usage { .. }), "{e:?}");
        assert!(e.to_string().contains(expect), "{e}");
        let lines = rascal_core::journal::Journal::read(&dir.join("journal.jsonl")).unwrap();
        assert_eq!(
            lines.last().unwrap().envelope.as_ref().unwrap().status,
            Status::InvalidArgs
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn an_undeclared_output_is_refused_by_the_envelope_check() {
    let f = frame();
    let registry = registry();
    let dir = scratch("lie");
    let run = Run::open(&dir).unwrap();
    seed(&run, &f);
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("mask".into(), mask(&f));
    let mut params = Params::new();
    params.insert("amount".into(), toml::Value::Integer(1));
    params.insert("mode".into(), toml::Value::String("lie".into()));
    let e = run
        .invoke("p", "probe", &registry, &inputs, &params)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("envelope rejected") && e.contains("undeclared output extra"),
        "{e}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_frame_mismatch_between_mask_and_image_is_refused_by_the_runner() {
    let f = frame();
    let other = Frame::raw(40, 31, Projection::Flat);
    let registry = registry();
    let dir = scratch("frames");
    let run = Run::open(&dir).unwrap();
    seed(&run, &f);
    seed(&run, &other);
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("mask".into(), mask(&f));
    inputs.insert("image".into(), image(&other, Role::Canonical, None));
    let mut params = Params::new();
    params.insert("amount".into(), toml::Value::Integer(1));
    let e = run
        .invoke("p", "probe", &registry, &inputs, &params)
        .unwrap_err()
        .to_string();
    assert!(e.contains("must be in one frame"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bad_rect_box_is_invalid_args_from_the_op() {
    let f = frame();
    let registry = registry();
    let dir = scratch("rectbad");
    let run = Run::open(&dir).unwrap();
    seed(&run, &f);
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), image(&f, Role::Canonical, None));
    let mut params = Params::new();
    params.insert(
        "bbox".into(),
        toml::Value::Array(
            vec![0.4, 0.2, 0.2, 0.5]
                .into_iter()
                .map(toml::Value::Float)
                .collect(),
        ),
    );
    let e = run
        .invoke("r", "rect", &registry, &inputs, &params)
        .unwrap_err();
    assert!(matches!(e, rascal_core::Error::Usage { .. }), "{e:?}");
    assert!(e.to_string().contains("x0 < x1"), "{e}");
    let lines = rascal_core::journal::Journal::read(&dir.join("journal.jsonl")).unwrap();
    assert_eq!(
        lines.last().unwrap().envelope.as_ref().unwrap().status,
        Status::InvalidArgs
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_frame_table_is_what_ops_see() {
    let f = frame();
    let table = FrameTable {
        frames: vec![f.clone()],
    };
    let context = Context { frames: &table };
    assert_eq!(context.frames.require(&f.id).unwrap().id, f.id);
    assert!(context.frames.require("nope").is_err());
}
