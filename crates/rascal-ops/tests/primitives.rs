//! Task 1.8a: every primitive against brute force or tabulated values.
#![allow(clippy::unwrap_used)]

use image::{GrayImage, Luma};
use rascal_ops::primitives::*;

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn smooth_rows_matches_a_brute_force_truncated_renormalised_gaussian() {
    let values: Vec<f64> = (0..50).map(|i| ((i * 7919) % 23) as f64).collect();
    for sigma in [0.5, 1.0, 2.5, 7.0] {
        let got = smooth_rows(&values, sigma);
        let radius = (3.0 * sigma).ceil() as i64;
        for i in 0..values.len() as i64 {
            let mut sum = 0.0;
            let mut weight = 0.0;
            for j in (i - radius)..=(i + radius) {
                if j < 0 || j >= values.len() as i64 {
                    continue;
                }
                let w = (-((i - j) as f64).powi(2) / (2.0 * sigma * sigma)).exp();
                sum += w * values[j as usize];
                weight += w;
            }
            assert!(
                close(got[i as usize], sum / weight, 1e-12),
                "sigma {sigma} at {i}"
            );
        }
    }
    assert_eq!(
        smooth_rows(&values, 0.49),
        values,
        "below half a pixel is the identity"
    );
    assert_eq!(smooth_rows(&[3.0], 5.0), vec![3.0]);
    let flat = vec![4.0; 20];
    assert!(
        smooth_rows(&flat, 3.0)
            .iter()
            .all(|v| close(*v, 4.0, 1e-12)),
        "a constant stays constant at the edges"
    );
}

#[test]
fn sobel_matches_brute_force_with_edge_replication() {
    let (w, h) = (9u32, 7u32);
    let plane: Vec<f32> = (0..w * h).map(|i| ((i * 31) % 17) as f32).collect();
    let got = sobel_magnitude(&plane, w, h);
    let at = |x: i64, y: i64| {
        plane[(y.clamp(0, h as i64 - 1) * w as i64 + x.clamp(0, w as i64 - 1)) as usize]
    };
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let gx = -at(x - 1, y - 1) - 2.0 * at(x - 1, y) - at(x - 1, y + 1)
                + at(x + 1, y - 1)
                + 2.0 * at(x + 1, y)
                + at(x + 1, y + 1);
            let gy = -at(x - 1, y - 1) - 2.0 * at(x, y - 1) - at(x + 1, y - 1)
                + at(x - 1, y + 1)
                + 2.0 * at(x, y + 1)
                + at(x + 1, y + 1);
            let expected = (gx * gx + gy * gy).sqrt() / 8.0;
            assert!(
                (got[(y * w as i64 + x) as usize] - expected).abs() < 1e-4,
                "({x},{y})"
            );
        }
    }
    // A vertical step of 8 levels gives a magnitude of 4 (= 8·4/8) on the step columns.
    let step: Vec<f32> = (0..w * h)
        .map(|i| if i % w < 4 { 0.0 } else { 8.0 })
        .collect();
    let got = sobel_magnitude(&step, w, h);
    assert!((got[(3 * w + 3) as usize] - 4.0).abs() < 1e-5);
    assert!((got[(3 * w + 4) as usize] - 4.0).abs() < 1e-5);
    assert!(got[(3 * w + 1) as usize].abs() < 1e-5);
    assert!(
        got[0].abs() < 1e-5,
        "edge replication: no false edge at the border"
    );
    assert!(
        (luminance(&image::RgbImage::from_pixel(2, 1, image::Rgb([255, 0, 0])))[0] - 76.245).abs()
            < 0.01
    );
}

#[test]
fn disc_dilate_and_erode_match_brute_force() {
    let mut plane = GrayImage::new(15, 11);
    plane.put_pixel(7, 5, Luma([255]));
    plane.put_pixel(2, 2, Luma([255]));
    plane.put_pixel(14, 10, Luma([255]));
    for radius in [0.0, 1.0, 1.5, 2.0, 3.0] {
        let d = dilate(&plane, radius);
        for y in 0..11i64 {
            for x in 0..15i64 {
                let expected = (0..11i64).any(|py| {
                    (0..15i64).any(|px| {
                        plane.get_pixel(px as u32, py as u32)[0] != 0
                            && (((px - x).pow(2) + (py - y).pow(2)) as f64)
                                <= radius * radius + 1e-9
                    })
                });
                assert_eq!(
                    d.get_pixel(x as u32, y as u32)[0] != 0,
                    expected,
                    "dilate r {radius} at ({x},{y})"
                );
            }
        }
        let e = erode(&d, radius);
        for y in 0..11i64 {
            for x in 0..15i64 {
                let expected = disc(radius).iter().all(|(dx, dy)| {
                    let (px, py) = (x + dx, y + dy);
                    px >= 0
                        && py >= 0
                        && px < 15
                        && py < 11
                        && d.get_pixel(px as u32, py as u32)[0] != 0
                });
                assert_eq!(
                    e.get_pixel(x as u32, y as u32)[0] != 0,
                    expected,
                    "erode r {radius} at ({x},{y})"
                );
            }
        }
    }
    assert_eq!(disc(0.0), vec![(0, 0)]);
    assert_eq!(disc(1.0).len(), 5);
    assert_eq!(disc(2.0).len(), 13);
    // Opening a lone pixel with a disc removes it; a block larger than the disc survives.
    let mut block = GrayImage::new(15, 11);
    for y in 3..9 {
        for x in 4..12 {
            block.put_pixel(x, y, Luma([255]));
        }
    }
    let opened = dilate(&erode(&block, 2.0), 2.0);
    assert!(opened.pixels().filter(|p| p[0] != 0).count() > 20);
    assert_eq!(
        dilate(&erode(&plane, 1.0), 1.0)
            .pixels()
            .filter(|p| p[0] != 0)
            .count(),
        0
    );
}

#[test]
fn the_distance_transform_is_exact_against_brute_force() {
    let (w, h) = (23u32, 17u32);
    let mut plane = GrayImage::new(w, h);
    for (x, y) in [(3u32, 4u32), (20, 2), (11, 11), (0, 16), (22, 16), (12, 12)] {
        plane.put_pixel(x, y, Luma([255]));
    }
    let got = distance_transform(&plane);
    for y in 0..h {
        for x in 0..w {
            let mut best = f32::INFINITY;
            for py in 0..h {
                for px in 0..w {
                    if plane.get_pixel(px, py)[0] != 0 {
                        let d = (((px as f32 - x as f32).powi(2))
                            + ((py as f32 - y as f32).powi(2)))
                        .sqrt();
                        best = best.min(d);
                    }
                }
            }
            assert!(
                (got[(y * w + x) as usize] - best).abs() < 1e-4,
                "({x},{y}): got {} expected {best}",
                got[(y * w + x) as usize]
            );
        }
    }
    let empty = GrayImage::new(4, 3);
    assert!(distance_transform(&empty).iter().all(|d| d.is_infinite()));
    let full = GrayImage::from_pixel(4, 3, Luma([255]));
    assert!(distance_transform(&full).iter().all(|d| *d == 0.0));
    // A single column set: the distance is the horizontal offset.
    let mut column = GrayImage::new(9, 3);
    for y in 0..3 {
        column.put_pixel(4, y, Luma([255]));
    }
    let d = distance_transform(&column);
    for x in 0..9 {
        assert!((d[x as usize] - (x as f32 - 4.0).abs()).abs() < 1e-5);
    }
}

#[test]
fn srgb_to_lab_matches_tabulated_values() {
    // Reference values under D65 for sRGB primaries and greys.
    let cases: [([u8; 3], [f64; 3]); 6] = [
        ([255, 255, 255], [100.0, 0.0, 0.0]),
        ([0, 0, 0], [0.0, 0.0, 0.0]),
        ([255, 0, 0], [53.24, 80.09, 67.20]),
        ([0, 255, 0], [87.73, -86.18, 83.18]),
        ([0, 0, 255], [32.30, 79.19, -107.86]),
        ([128, 128, 128], [53.59, 0.0, 0.0]),
    ];
    for (rgb, expected) in cases {
        let lab = srgb_to_lab(rgb);
        for i in 0..3 {
            assert!(
                close(lab[i], expected[i], 0.05),
                "{rgb:?}: got {lab:?} expected {expected:?}"
            );
        }
    }
}

#[test]
fn the_normal_cdf_matches_tabulated_values() {
    let cases = [
        (0.0, 0.5),
        (1.0, 0.841_344_746_068_543),
        (-1.0, 0.158_655_253_931_457),
        (1.959_963_984_540_054, 0.975),
        (2.0, 0.977_249_868_051_821),
        (3.0, 0.998_650_101_968_370),
        (-3.0, 0.001_349_898_031_630),
        (4.0, 0.999_968_328_758_167),
        (-5.0, 2.866_515_718_791_94e-7),
        (6.0, 0.999_999_999_013_412),
    ];
    for (x, expected) in cases {
        let got = normal_cdf(x);
        assert!(
            close(got, expected, 1e-10),
            "cdf({x}) = {got}, expected {expected}"
        );
    }
    assert!(close(erfc(0.5), 0.479_500_122_186_953, 1e-12));
    assert!(close(erfc(3.5), 7.430_983_723_414_13e-7, 1e-15));
    assert!(close(erfc(-2.0), 1.995_322_265_018_953, 1e-12));
}
