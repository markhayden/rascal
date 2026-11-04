//! A manifest's exactly-one-of groups (rascal #7): `line` takes `column` or `path`, and its
//! input schema says so as a `oneOf`, so a client building a form from the description knows;
//! a group that names an unknown or required param is refused when the manifest is checked.
#![allow(clippy::unwrap_used)]

use rascal_core::artifact::ArtifactType;
use rascal_core::op::{Cost, CostTier, Manifest, Param, ParamKind, Port};

#[test]
fn line_declares_column_or_path_in_its_input_schema() {
    let registry = rascal_ops::registry().unwrap();
    let line = registry
        .manifests()
        .into_iter()
        .find(|m| m.name == "line")
        .unwrap();
    assert_eq!(
        line.exactly_one_of,
        vec![vec!["column".to_string(), "path".to_string()]]
    );
    assert_eq!(
        line.input_schema["oneOf"],
        serde_json::json!([{ "required": ["column"] }, { "required": ["path"] }])
    );
    assert!(line.check().is_ok());
}

fn manifest(params: Vec<Param>) -> Manifest {
    Manifest::new(
        "probe",
        "1.0.0",
        "p.",
        "w.",
        "n.",
        "o.",
        Cost {
            tier: CostTier::Cheap,
            seconds: 0.01,
        },
        vec![],
        vec![Port {
            name: "location".into(),
            artifact_type: ArtifactType::Location,
            required: true,
            purpose: "L.".into(),
            many: false,
        }],
        params,
    )
}

#[test]
fn a_group_naming_an_unknown_or_required_param_is_refused() {
    let unknown =
        manifest(vec![Param::absent("a", "A.", ParamKind::Number)]).exactly_one_of(&["a", "b"]);
    assert!(
        unknown
            .check()
            .unwrap_err()
            .contains("b, which is not a param")
    );
    let required = manifest(vec![
        Param::absent("a", "A.", ParamKind::Number),
        Param::required("b", "B.", ParamKind::Number),
    ])
    .exactly_one_of(&["a", "b"]);
    assert!(
        required
            .check()
            .unwrap_err()
            .contains("required on its own")
    );
}
