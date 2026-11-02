//! Task 10.4: every op carries its group (SPEC.md "Operations", the catalogue's table), which the
//! description gives each operation; every group has an op, and the constant sets are offered
//! with the first as the default.
#![allow(clippy::unwrap_used)]

use rascal_core::op::Group;

/// SPEC's table, but `segment`, which a pack serves.
const TABLE: [(&str, Group); 24] = [
    ("line", Group::Find),
    ("rect", Group::Find),
    ("stroke", Group::Find),
    ("rasterize", Group::Refine),
    ("tighten", Group::Refine),
    ("grow", Group::Refine),
    ("shrink", Group::Refine),
    ("close", Group::Refine),
    ("subtract", Group::Refine),
    ("select", Group::Refine),
    ("invert", Group::Refine),
    ("fan", Group::Refine),
    ("profile", Group::Refine),
    ("feather", Group::Refine),
    ("cut", Group::Prep),
    ("put", Group::Prep),
    ("ramp", Group::Prep),
    ("apply", Group::Prep),
    ("window", Group::Prep),
    ("unwindow", Group::Prep),
    ("render", Group::Out),
    ("paste", Group::Out),
    ("measure", Group::Out),
    ("inspect", Group::Inspect),
];

#[test]
fn every_op_is_in_its_specs_group_and_every_group_has_an_op() {
    let registry = rascal_ops::registry().unwrap();
    let mut names = registry.names();
    names.sort_unstable();
    let mut table: Vec<&str> = TABLE.iter().map(|(name, _)| *name).collect();
    table.sort_unstable();
    assert_eq!(names, table);
    for (name, group) in TABLE {
        assert_eq!(registry.get(name).unwrap().group(), group, "{name}");
    }
    for group in Group::ALL {
        assert!(TABLE.iter().any(|(_, g)| *g == group), "{}", group.name());
    }
    assert_eq!(
        Group::ALL.map(Group::name),
        ["find", "refine", "prep", "out", "inspect"]
    );
}

#[test]
fn the_params_schema_offers_the_constant_sets_with_the_first_the_default() {
    let registry = rascal_ops::registry().unwrap();
    for manifest in registry.manifests() {
        let schema = manifest.params_schema();
        let constants = &schema["properties"]["constants"];
        if manifest.constants.is_empty() {
            assert!(constants.is_null(), "{}", manifest.name);
            assert_eq!(schema, manifest.input_schema, "{}", manifest.name);
        } else {
            assert_eq!(constants["enum"], serde_json::json!(manifest.constants));
            assert_eq!(constants["default"], manifest.constants[0].as_str());
            // Nothing else changes: the parameters are the manifest's.
            let mut without = schema.clone();
            without["properties"]
                .as_object_mut()
                .unwrap()
                .remove("constants");
            assert_eq!(without, manifest.input_schema, "{}", manifest.name);
        }
    }
}
