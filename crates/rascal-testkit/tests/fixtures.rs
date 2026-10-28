//! The committed synthetic fixtures are exactly what their builders produce, so a fixture can
//! be regenerated and a test can trust it. `UPDATE_FIXTURES=1` rewrites them.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/images")
        .join(name)
}

#[test]
fn floor_fixture_matches_its_builder() {
    let built = rascal_testkit::images::floor(320, 200);
    let path = fixture("floor.png");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        built.save(&path).unwrap();
    }
    let committed = image::open(&path).unwrap().to_rgb8();
    assert_eq!(committed.dimensions(), built.dimensions());
    assert!(
        committed.as_raw() == built.as_raw(),
        "fixtures/images/floor.png differs from images::floor(320, 200); UPDATE_FIXTURES=1 regenerates it"
    );
}
