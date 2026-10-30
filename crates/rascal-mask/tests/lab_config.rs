//! Task 6.1: the lab's three files. The examples parse, resolve and validate against the
//! shipped ops; the corpus verifies against the fixtures and reports a tampered file by id;
//! a matrix without `protocol` is refused naming the field; a seed or name that differs from
//! the protocol's, an arm the protocol compares but the matrix lacks, an image missing from
//! the corpus, an override naming a missing step, parameter or constant set, are each refused
//! naming the thing; a recipe in which a finder's input derives from another finder's output
//! is rejected naming the binding, through a direct and an indirect chain.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rascal_core::artifact::ArtifactType;
use rascal_core::lab::corpus::{Corpus, Verification};
use rascal_core::lab::matrix::{Override, apply_override, reject_dependent_finders};
use rascal_core::lab::protocol::Protocol;
use rascal_core::op::{Context, Cost, CostTier, Inputs, Manifest, Op, Output, Params, Port};
use rascal_core::recipe::Recipe;
use rascal_mask::lab::resolve_matrix;

/// A stand-in for a pack tool: an image in, a plane out, never run here.
struct FinderStub(&'static str);

impl Op for FinderStub {
    fn manifest(&self) -> Manifest {
        let port = |name: &str, artifact_type: ArtifactType, required: bool| Port {
            name: name.to_string(),
            artifact_type,
            required,
            purpose: "a stub".to_string(),
            many: false,
        };
        Manifest::new(
            self.0,
            "0.1.0",
            "A finder stub for the lab's validation tests.",
            "Never.",
            "Always.",
            "A plane.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.1,
            },
            vec![
                port("image", ArtifactType::Image, true),
                port("plane", ArtifactType::Mask, false),
                port("mask", ArtifactType::Mask, false),
            ],
            vec![port("plane", ArtifactType::Mask, true)],
            vec![],
        )
    }

    fn run(&self, _: &Context<'_>, _: &Inputs, _: &Params) -> rascal_core::error::Result<Output> {
        Err(rascal_core::Error::op(self.0, "a stub never runs"))
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn examples() -> PathBuf {
    repo_root().join("examples/lab")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-lab-config-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A copy of the examples with the matrix text edited.
fn edited_matrix(name: &str, edit: impl Fn(String) -> String) -> PathBuf {
    let dir = scratch(name);
    for file in ["corpus.toml", "protocol.toml"] {
        std::fs::copy(examples().join(file), dir.join(file)).unwrap();
    }
    let text = std::fs::read_to_string(examples().join("matrix.toml")).unwrap();
    std::fs::write(dir.join("matrix.toml"), edit(text)).unwrap();
    dir.join("matrix.toml")
}

#[test]
fn the_examples_validate_and_the_arms_resolve_to_the_seam_recipe_with_overrides_applied() {
    let registry = rascal_ops::registry().unwrap();
    let validated = resolve_matrix(&examples().join("matrix.toml"), &registry, &[]).unwrap();
    assert_eq!(validated.matrix.name, "seam-demo");
    assert_eq!(validated.protocol.name, "seam-demo");
    assert!(validated.protocol_digest.starts_with("sha256:"));
    assert_eq!(validated.corpus.image.len(), 4);
    assert_eq!(validated.recipes.len(), 2);
    let thin = &validated.recipes[0];
    assert_eq!(thin.name, "seam");
    let fan = thin.step.iter().find(|s| s.id == "fan").unwrap();
    assert_eq!(fan.constants.as_deref(), Some("fan-thin-v1"));
    let ramp = &validated.recipes[1];
    let fan = ramp.step.iter().find(|s| s.id == "fan").unwrap();
    assert_eq!(fan.constants.as_deref(), Some("fan-v3"));
    // The protocol's defaults and rules.
    assert_eq!(validated.protocol.min_decided, 2);
    assert_eq!(validated.protocol.rules.preferred, [2, 3]);
    assert_eq!(
        validated.protocol.arms,
        ["fan-ramp".to_string(), "thin".to_string()]
    );
}

#[test]
fn the_corpus_verifies_against_the_fixtures_and_a_tampered_file_is_reported_by_id() {
    let corpus = Corpus::read(&examples().join("corpus.toml")).unwrap();
    let root = repo_root().join("fixtures/images");
    let found = corpus.verify(&root).unwrap();
    assert!(
        found.iter().all(|(_, v)| *v == Verification::Ok),
        "{found:?}"
    );
    // A copy of the root with one file changed and one missing.
    let dir = scratch("corpus");
    for image in &corpus.image {
        std::fs::copy(root.join(&image.file), dir.join(&image.file)).unwrap();
    }
    std::fs::write(dir.join("floor.png"), b"not the floor").unwrap();
    std::fs::remove_file(dir.join("chair-blob.png")).unwrap();
    let found: BTreeMap<&str, Verification> = corpus
        .verify(&dir)
        .unwrap()
        .into_iter()
        .map(|(i, v)| (i.id.as_str(), v))
        .collect();
    assert_eq!(found["sky-01"], Verification::Ok);
    assert!(matches!(
        found["structured-01"],
        Verification::Mismatch { .. }
    ));
    assert_eq!(found["structured-02"], Verification::Missing);
    // The root: given, else RASCAL_CORPUS, else ~/rascal/corpus.
    assert_eq!(Corpus::root(Some(Path::new("/x"))), PathBuf::from("/x"));
    assert!(
        Corpus::root(None).ends_with("rascal/corpus")
            || std::env::var_os("RASCAL_CORPUS").is_some()
    );
    // The manifest's own checks.
    for (bad, needle) in [
        ("version = 2\n", "version 2"),
        (
            "version = 1\n[[image]]\nid = \"a\"\nfile = \"../x.png\"\nsha256 = \"sha256:0000000000000000000000000000000000000000000000000000000000000000\"\ngroup = \"g\"\nlicence = \"l\"\n",
            "plain relative",
        ),
        (
            "version = 1\n[[image]]\nid = \"a\"\nfile = \"x.png\"\nsha256 = \"abc\"\ngroup = \"g\"\nlicence = \"l\"\n",
            "sha256:<64 hex",
        ),
        (
            "version = 1\n[[image]]\nid = \"a\"\nfile = \"x.png\"\nsha256 = \"sha256:0000000000000000000000000000000000000000000000000000000000000000\"\ngroup = \"g\"\nlicence = \"l\"\n[[image]]\nid = \"a\"\nfile = \"y.png\"\nsha256 = \"sha256:0000000000000000000000000000000000000000000000000000000000000000\"\ngroup = \"g\"\nlicence = \"l\"\n",
            "repeated",
        ),
    ] {
        let err = Corpus::parse(bad, "t").unwrap_err().to_string();
        assert!(err.contains(needle), "{needle}: {err}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_protocol_is_checked_for_its_arms_rules_and_groups() {
    let sound = std::fs::read_to_string(examples().join("protocol.toml")).unwrap();
    Protocol::parse(&sound, "t").unwrap();
    for (bad, needle) in [
        (
            sound.replace(
                "arms = [\"fan-ramp\", \"thin\"]",
                "arms = [\"thin\", \"thin\"]",
            ),
            "two distinct",
        ),
        (
            sound.replace("min_decided = 2", "min_decided = 0"),
            "min_decided",
        ),
        (
            sound.replace("preferred = [2, 3]", "preferred = [4, 3]"),
            "rules.preferred",
        ),
        (
            sound.replace("not_worse = [1, 3]", "not_worse = [1, 0]"),
            "rules.not_worse",
        ),
        (
            sound.replace(
                "images = [\"structured-01\", \"structured-02\"]",
                "images = [\"sky-01\"]",
            ),
            "more than one group",
        ),
        (
            sound.replace("name = \"structured\"", "name = \"sky\""),
            "repeated",
        ),
        (sound.replace("version = 1", "version = 3"), "version 3"),
    ] {
        let err = Protocol::parse(&bad, "t").unwrap_err().to_string();
        assert!(err.contains(needle), "{needle}: {err}");
    }
    for (bad, needle) in [
        (
            sound.replace("name = \"seam-demo\"", "name = \"\""),
            "name is empty",
        ),
        (
            sound[..sound.find("[[group]]").unwrap()].to_string(),
            "at least one group",
        ),
        (
            sound.replace("images = [\"sky-01\", \"sky-02\"]", "images = []"),
            "has no images",
        ),
    ] {
        let err = Protocol::parse(&bad, "t").unwrap_err().to_string();
        assert!(err.contains(needle), "{needle}: {err}");
    }
    // Defaults: min_decided 10 and the spec's fractions.
    let minimal = "version = 1\nname = \"x\"\nseed = 1\narms = [\"a\", \"b\"]\n[[group]]\nname = \"g\"\nimages = [\"i\"]\n";
    let p = Protocol::parse(minimal, "t").unwrap();
    assert_eq!(p.min_decided, 10);
    assert_eq!((p.rules.preferred, p.rules.not_worse), ([2, 3], [1, 3]));
}

#[test]
fn a_matrix_is_refused_naming_what_is_wrong() {
    let registry = rascal_ops::registry().unwrap();
    type Edit = Box<dyn Fn(String) -> String>;
    let cases: Vec<(&str, Edit, &str)> = vec![
        (
            "no-protocol",
            Box::new(|t| t.replace("protocol = \"protocol.toml\"\n", "")),
            "protocol",
        ),
        (
            "seed",
            Box::new(|t| t.replace("seed = 20261028", "seed = 7")),
            "seed 7 differs",
        ),
        (
            "name",
            Box::new(|t| t.replace("name = \"seam-demo\"", "name = \"other\"")),
            "one lab, one name",
        ),
        (
            "arm",
            Box::new(|t| t.replace("name = \"thin\"", "name = \"narrow\"")),
            "arm thin, which the matrix lacks",
        ),
        (
            "image",
            Box::new(|t| t.replace("id = \"sky-02\"", "id = \"sky-09\"")),
            "sky-09 is not in the corpus",
        ),
        (
            "missing-image",
            Box::new(|t| t.replace("[[image]]\nid = \"sky-02\"\n", "")),
            "names image sky-02, which the matrix lacks",
        ),
        (
            "step",
            Box::new(|t| t.replace("\"fan.constants\"", "\"fin.constants\"")),
            "no step fin",
        ),
        (
            "set",
            Box::new(|t| t.replace("\"fan-thin-v1\"", "\"fan-wide-v9\"")),
            "no constant set fan-wide-v9",
        ),
        (
            "param",
            Box::new(|t| {
                t.replace(
                    "overrides = { \"fan.constants\" = \"fan-thin-v1\" }",
                    "overrides = { \"fan.widest\" = 3 }",
                )
            }),
            "no parameter widest",
        ),
        (
            "key",
            Box::new(|t| t.replace("\"fan.constants\"", "\"fan\"")),
            "not <step>.<param>",
        ),
        (
            "spend",
            Box::new(|t| t.replace("spend_usd_per_call = 0.0", "spend_usd_per_call = -1.0")),
            "negative",
        ),
        (
            "recipe",
            Box::new(|t| {
                t.replace(
                    "recipe = \"seam\"\noverrides",
                    "recipe = \"nowhere.toml\"\noverrides",
                )
            }),
            "nowhere.toml",
        ),
        (
            "image-override",
            Box::new(|t| {
                t.replace(
                    "[[image]]\nid = \"sky-01\"\n",
                    "[[image]]\nid = \"sky-01\"\noverrides = { \"ramp.nope\" = true }\n",
                )
            }),
            "no parameter nope",
        ),
        (
            "empty-name",
            Box::new(|t| t.replace("name = \"seam-demo\"", "name = \"\"")),
            "name is empty",
        ),
        (
            "no-images",
            Box::new(|t| {
                let start = t.find("[[image]]").unwrap();
                let end = t.find("[[arm]]").unwrap();
                format!("{}{}", &t[..start], &t[end..])
            }),
            "at least one image",
        ),
        (
            "no-arms",
            Box::new(|t| t[..t.find("[[arm]]").unwrap()].to_string()),
            "at least one arm",
        ),
        (
            "repeated-image",
            Box::new(|t| t.replace("id = \"sky-02\"", "id = \"sky-01\"")),
            "repeated",
        ),
        (
            "repeated-arm",
            Box::new(|t| t.replace("name = \"fan-ramp\"", "name = \"thin\"")),
            "repeated",
        ),
        (
            "empty-filler",
            Box::new(|t| t.replace("filler = \"fake\"\nspend", "filler = \"\"\nspend")),
            "recipe and filler are required",
        ),
        (
            "nan-spend",
            Box::new(|t| t.replace("spend_usd_per_call = 0.0", "spend_usd_per_call = nan")),
            "not a number",
        ),
        (
            "bogus-input",
            Box::new(|t| {
                t.replace(
                    "[[image]]\nid = \"sky-01\"\n",
                    "[[image]]\nid = \"sky-01\"\ninputs = { bogus = \"x\" }\n",
                )
            }),
            "inputs.bogus is not one heal binds",
        ),
    ];
    for (name, edit, needle) in cases {
        let path = edited_matrix(name, edit);
        let err = resolve_matrix(&path, &registry, &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains(needle), "{name}: {err}");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
    // A per-image override that is sound is accepted, and an override's value reaches the step.
    let path = edited_matrix("sound-override", |t| {
        t.replace(
            "[[image]]\nid = \"sky-01\"\n",
            "[[image]]\nid = \"sky-01\"\noverrides = { \"line.width_px\" = 3.0 }\n",
        )
    });
    resolve_matrix(&path, &registry, &[]).unwrap();
    let mut recipe = Recipe::parse(include_str!("../recipes/seam.toml"), "seam").unwrap();
    apply_override(
        &mut recipe,
        &registry,
        "line.width_px",
        &Override::Float(3.0),
    )
    .unwrap();
    let line = recipe.step.iter().find(|s| s.id == "line").unwrap();
    assert_eq!(line.params["width_px"], toml::Value::Float(3.0));
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_finder_fed_by_another_finder_is_rejected_naming_the_binding() {
    let recipe = |steps: &str| -> Recipe {
        Recipe::parse(
            &format!(
                "version = 1\nname = \"t\"\nuse_case = \"remove\"\n[inputs]\nimage = {{ type = \"Image\" }}\n{steps}"
            ),
            "t",
        )
        .unwrap()
    };
    let finders = ["segment", "detect"];
    // Independent finders over the initial inputs: fine.
    let ok = recipe(
        "[[step]]\nid = \"a\"\nop = \"segment\"\ninputs = { image = \"inputs.image\" }\n[[step]]\nid = \"b\"\nop = \"detect\"\ninputs = { image = \"inputs.image\" }\n",
    );
    reject_dependent_finders(&ok, &finders).unwrap();
    // Direct: detect takes segment's plane.
    let direct = recipe(
        "[[step]]\nid = \"a\"\nop = \"segment\"\ninputs = { image = \"inputs.image\" }\n[[step]]\nid = \"b\"\nop = \"detect\"\ninputs = { image = \"inputs.image\", plane = \"a.plane\" }\n",
    );
    let err = reject_dependent_finders(&direct, &finders).unwrap_err();
    assert!(
        err.contains("finder b (detect) takes plane = a.plane") && err.contains("finder a"),
        "{err}"
    );
    // Indirect: a deterministic op between them carries the taint.
    let indirect = recipe(
        "[[step]]\nid = \"a\"\nop = \"segment\"\ninputs = { image = \"inputs.image\" }\n[[step]]\nid = \"g\"\nop = \"grow\"\ninputs = { mask = \"a.plane\" }\npx = 2\n[[step]]\nid = \"b\"\nop = \"detect\"\ninputs = { image = \"inputs.image\", mask = \"g.mask\" }\n",
    );
    let err = reject_dependent_finders(&indirect, &finders).unwrap_err();
    assert!(
        err.contains("finder b (detect) takes mask = g.mask")
            && err.contains("derives from finder a"),
        "{err}"
    );
    // A deterministic consumer of a finder is fine.
    let consumer = recipe(
        "[[step]]\nid = \"a\"\nop = \"segment\"\ninputs = { image = \"inputs.image\" }\n[[step]]\nid = \"g\"\nop = \"grow\"\ninputs = { mask = \"a.plane\" }\npx = 2\n",
    );
    reject_dependent_finders(&consumer, &finders).unwrap();
    // Without finders registered, nothing is a finder.
    reject_dependent_finders(&direct, &[]).unwrap();
    // Through the matrix: an arm whose recipe file chains finders is refused naming the arm.
    let dir = scratch("dependent");
    for file in ["corpus.toml", "protocol.toml"] {
        std::fs::copy(examples().join(file), dir.join(file)).unwrap();
    }
    std::fs::write(
        dir.join("chain.toml"),
        "version = 1\nname = \"chain\"\nuse_case = \"remove\"\n[inputs]\nimage = { type = \"Image\" }\n[[step]]\nid = \"a\"\nop = \"segment\"\ninputs = { image = \"inputs.image\" }\n[[step]]\nid = \"b\"\nop = \"detect\"\ninputs = { image = \"inputs.image\", plane = \"a.plane\" }\n",
    )
    .unwrap();
    let matrix = std::fs::read_to_string(examples().join("matrix.toml"))
        .unwrap()
        .replace(
            "recipe = \"seam\"\nfiller = \"fake\"",
            "recipe = \"chain.toml\"\nfiller = \"fake\"",
        );
    std::fs::write(dir.join("matrix.toml"), matrix).unwrap();
    // The registry must know the finders for the recipe to validate; the testkit's fake
    // pack ops stand in through a registry that names them.
    let mut registry = rascal_ops::registry().unwrap();
    for name in finders {
        registry.register(Box::new(FinderStub(name))).unwrap();
    }
    let err = resolve_matrix(&dir.join("matrix.toml"), &registry, &finders)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("arm fan-ramp") && err.contains("takes plane = a.plane"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
