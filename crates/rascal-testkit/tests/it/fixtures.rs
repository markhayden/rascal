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

#[test]
fn step_sky_fixture_matches_its_builder() {
    let built = rascal_testkit::images::step_sky(400, 300);
    let path = fixture("step-sky.png");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        built.save(&path).unwrap();
    }
    let committed = image::open(&path).unwrap().to_rgb8();
    assert!(
        committed.as_raw() == built.as_raw(),
        "fixtures/images/step-sky.png differs from images::step_sky(400, 300); UPDATE_FIXTURES=1 regenerates it"
    );
}

#[test]
fn step_pano_fixture_matches_its_builder() {
    let built = rascal_testkit::images::step_pano(800, 400);
    let path = fixture("step-pano.png");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        built.save(&path).unwrap();
    }
    let committed = image::open(&path).unwrap().to_rgb8();
    assert!(
        committed.as_raw() == built.as_raw(),
        "fixtures/images/step-pano.png differs from images::step_pano(800, 400); UPDATE_FIXTURES=1 regenerates it"
    );
}

#[test]
fn chair_blob_fixture_matches_its_builder() {
    let built = rascal_testkit::images::chair_blob(480, 320);
    let path = fixture("chair-blob.png");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        built.save(&path).unwrap();
    }
    let committed = image::open(&path).unwrap().to_rgb8();
    assert!(
        committed.as_raw() == built.as_raw(),
        "fixtures/images/chair-blob.png differs from images::chair_blob(480, 320); UPDATE_FIXTURES=1 regenerates it"
    );
}

#[test]
fn scratch_smooth_fixture_matches_its_builder() {
    let built = rascal_testkit::images::scratch_smooth(400, 300);
    let path = fixture("scratch-smooth.png");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        built.save(&path).unwrap();
    }
    let committed = image::open(&path).unwrap().to_rgb8();
    assert!(
        committed.as_raw() == built.as_raw(),
        "fixtures/images/scratch-smooth.png differs from images::scratch_smooth(400, 300); UPDATE_FIXTURES=1 regenerates it"
    );
}

#[test]
fn scratch_textured_fixture_matches_its_builder() {
    let built = rascal_testkit::images::scratch_textured(400, 300);
    let path = fixture("scratch-textured.png");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        built.save(&path).unwrap();
    }
    let committed = image::open(&path).unwrap().to_rgb8();
    assert!(
        committed.as_raw() == built.as_raw(),
        "fixtures/images/scratch-textured.png differs from images::scratch_textured(400, 300); UPDATE_FIXTURES=1 regenerates it"
    );
}
