//! Decoding is bounded (a file claiming a huge image is refused before it is allocated) and a
//! grey image is kept grey.
#![allow(clippy::unwrap_used)]

use ::image::DynamicImage;
use rascal_core::frame::{Frame, Projection};
use rascal_core::image::{Image, MAX_SIDE, Role, decode_limited};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-img-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_decode_past_the_side_limit_is_refused_and_one_within_it_imports() {
    let dir = scratch("limits");
    let wide = dir.join("wide.png");
    ::image::GrayImage::new(MAX_SIDE + 1, 1)
        .save(&wide)
        .unwrap();
    let err = decode_limited(&wide).unwrap_err().to_string();
    assert!(err.contains("wide.png"), "{err}");
    assert!(Image::import(&wide, None).is_err());
    let fine = dir.join("fine.png");
    ::image::GrayImage::new(3, 2).save(&fine).unwrap();
    let (imported, frame, _) = Image::import(&fine, None).unwrap();
    assert_eq!((frame.width, frame.height), (3, 2));
    assert!(matches!(imported.pixels, DynamicImage::ImageLuma8(_)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_sixteen_bit_image_becomes_eight_bit_rgb_and_grey_stays_grey() {
    let f = Frame::raw(2, 2, Projection::Flat);
    let deep = DynamicImage::ImageRgb16(::image::ImageBuffer::from_pixel(
        2,
        2,
        ::image::Rgb([65535u16, 0, 32768]),
    ));
    let image = Image::new("d", deep, &f, Role::Canonical, None);
    assert!(matches!(image.pixels, DynamicImage::ImageRgb8(_)));
    assert_eq!(image.pixels.as_bytes()[..3], [255, 0, 128]);
    let grey = DynamicImage::ImageLuma8(::image::GrayImage::from_pixel(2, 2, ::image::Luma([7])));
    let image = Image::new("g", grey, &f, Role::Canonical, None);
    assert!(matches!(image.pixels, DynamicImage::ImageLuma8(_)));
}
