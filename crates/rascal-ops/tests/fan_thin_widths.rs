//! Task 6.1: `fan-thin-v1` yields `thin` on every row of the step-sky fixture at three image
//! widths: with `wide_max 0.0` the effective upper width is `max(thin, wide_max · w) = thin`,
//! so no row widens at any width, while `fan-v1` widens rows at each.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use image::imageops::{FilterType, resize};
use rascal_ops::fan::{Settings, fan_profile};

fn fixture() -> image::RgbImage {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/images/step-sky.png");
    image::open(path).unwrap().to_rgb8()
}

#[test]
fn fan_thin_v1_is_thin_on_every_row_at_three_widths() {
    let sky = fixture();
    let thin = Settings::fan_thin_v1();
    let fan = Settings::fan_v1();
    for width in [400u32, 800, 1620] {
        let height = (u64::from(width) * u64::from(sky.height()) / u64::from(sky.width())) as u32;
        let image = resize(&sky, width, height, FilterType::Triangle);
        let p = fan_profile(&image, 0.5, &thin).unwrap();
        assert!(
            p.half.iter().all(|h| (h - p.thin).abs() < 1e-9),
            "{width} px: a row widened under fan-thin-v1"
        );
        let open = fan_profile(&image, 0.5, &fan).unwrap();
        assert!(
            open.half.iter().any(|h| *h > open.thin + 0.5),
            "{width} px: fan-v1 opens the band on the step-sky fixture"
        );
        assert!(
            (p.thin - open.thin).abs() < 1e-9,
            "{width} px: one thin width"
        );
    }
}
