//! Synthetic images with known content, built deterministically so a committed fixture can be
//! checked against its builder and a test can know the answer before it runs.

use image::{Rgb, RgbImage};

/// A small deterministic generator (a linear congruential step), so noise is reproducible
/// across platforms without a dependency.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }
}

/// A textured floor: warm planks with a grain, darker joins every 40 columns, a gentle
/// vertical light falloff and a little noise. Nothing in it is smooth enough to be mistaken
/// for sky, and the planks give `fan` something to measure later.
pub fn floor(width: u32, height: u32) -> RgbImage {
    let mut lcg = Lcg(0x5eed_f100_12ab_cd01);
    let mut out = RgbImage::new(width, height);
    for y in 0..height {
        let light = 1.0 - 0.25 * (y as f32 / height.max(1) as f32);
        for x in 0..width {
            let join = x % 40 < 2;
            let grain = ((x as f32 * 0.37 + y as f32 * 0.11).sin() * 0.5 + 0.5) * 18.0;
            let noise = (lcg.next() % 11) as f32 - 5.0;
            let (r, g, b) = if join {
                (62.0, 44.0, 30.0)
            } else {
                (168.0 + grain, 124.0 + grain * 0.8, 84.0 + grain * 0.5)
            };
            let px = |v: f32| ((v + noise) * light).round().clamp(0.0, 255.0) as u8;
            out.put_pixel(x, y, Rgb([px(r), px(g), px(b)]));
        }
    }
    out
}

/// A flat mid-grey field: the simplest canvas for a mask test.
pub fn flat(width: u32, height: u32, value: u8) -> RgbImage {
    RgbImage::from_pixel(width, height, Rgb([value, value, value]))
}

/// The step-sky canvas: a smooth sky over a textured floor, with a tone step across the centre
/// column (the left side a little brighter), as a cut panorama canvas presents a seam. `fan`
/// opens on the sky rows, where the sides differ and nothing is textured, and stays at its
/// minimum on the floor rows.
pub fn step_sky(width: u32, height: u32) -> RgbImage {
    let horizon = height * 3 / 5;
    let mut out = RgbImage::new(width, height);
    let floor = floor(width, height);
    for y in 0..height {
        for x in 0..width {
            let step = if x < width / 2 { 10.0 } else { 0.0 };
            let px = if y < horizon {
                let t = y as f32 / horizon.max(1) as f32;
                let base = [120.0 + 70.0 * t, 160.0 + 45.0 * t, 220.0 + 10.0 * t];
                [base[0] + step, base[1] + step, base[2] + step]
            } else {
                let p = floor.get_pixel(x, y);
                [
                    f32::from(p[0]) + step,
                    f32::from(p[1]) + step,
                    f32::from(p[2]) + step,
                ]
            };
            out.put_pixel(
                x,
                y,
                Rgb([
                    px[0].round().clamp(0.0, 255.0) as u8,
                    px[1].round().clamp(0.0, 255.0) as u8,
                    px[2].round().clamp(0.0, 255.0) as u8,
                ]),
            );
        }
    }
    out
}

/// The step panorama: an equirectangular image whose wrap edge carries a tone step, as a
/// panorama stitched with a colour mismatch does. The left edge columns are a little darker
/// than the right edge columns, so the seam shows once `cut` puts the two edges side by side;
/// the sky is smooth and the ground textured, so `fan` opens on the sky rows only.
pub fn step_pano(width: u32, height: u32) -> RgbImage {
    let horizon = height * 3 / 5;
    let floor = floor(width, height);
    RgbImage::from_fn(width, height, |x, y| {
        // A gentle lateral gradient that is continuous across the wrap, plus a step on the
        // left edge strip only.
        let step = if x < width / 2 { -10.0 } else { 0.0 };
        let px = if y < horizon {
            let t = y as f32 / horizon.max(1) as f32;
            [
                120.0 + 70.0 * t + step,
                160.0 + 45.0 * t + step,
                220.0 + 10.0 * t + step,
            ]
        } else {
            let p = floor.get_pixel(x, y);
            [
                f32::from(p[0]) + step,
                f32::from(p[1]) + step,
                f32::from(p[2]) + step,
            ]
        };
        Rgb([
            px[0].round().clamp(0.0, 255.0) as u8,
            px[1].round().clamp(0.0, 255.0) as u8,
            px[2].round().clamp(0.0, 255.0) as u8,
        ])
    })
}
