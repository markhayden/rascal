//! A band profile has a preview (CONTRACT 3.3, 4.7): fan's band drawn over its image with a chart
//! of the half-width per row beside it, so the picture is the image's height and wider than it.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use rascal_testkit::mcp_client::Client;

fn sky() -> PathBuf {
    std::fs::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/images/step-sky.png"),
    )
    .unwrap()
}

#[test]
fn a_served_fan_returns_its_band_profile_with_a_chart_preview() {
    let dir = std::env::temp_dir().join(format!("rascal-band-preview-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut command = rascal_testkit::mcp_client::serve_command(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "--workspace",
            dir.join("ws").to_str().unwrap(),
            "--runs",
            dir.join("runs").to_str().unwrap(),
        ])
        .env_remove("RASCAL_LOG");
    let mut client = Client::start(&mut command).unwrap();
    let line = client
        .call("line", serde_json::json!({ "image": { "path": sky().to_str().unwrap() }, "column": 0.5, "width_px": 2 }))
        .unwrap()
        .structured
        .unwrap();
    assert_eq!(line["status"], "completed", "{line:#}");
    let fan = client
        .call("fan", serde_json::json!({ "image": line["artifacts"]["in"][0]["artifact"], "line": line["artifacts"]["out"][0]["artifact"] }))
        .unwrap()
        .structured
        .unwrap();
    client.close().unwrap();
    assert_eq!(fan["status"], "completed", "{fan:#}");
    let band = &fan["artifacts"]["out"][0];
    assert_eq!(band["artifact"]["type"], "rascal:BandProfile", "{fan:#}");
    let (on, preview) = (&band["artifact"]["on"], &band["preview"]["image"]);
    assert_eq!(preview["height"], on["height"], "{fan:#}");
    assert!(
        preview["width"].as_u64().unwrap() > on["width"].as_u64().unwrap(),
        "the chart sits beside the image: {fan:#}"
    );
    let file = band["preview"]["path"].as_str().unwrap();
    let image = image::open(file).unwrap();
    assert_eq!(u64::from(image.width()), preview["width"].as_u64().unwrap());
    let _ = std::fs::remove_dir_all(&dir);
}
