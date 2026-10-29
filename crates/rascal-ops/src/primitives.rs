//! The pixel primitives every op is built from (SPEC.md "Operations"): row smoothing, the 3×3
//! Sobel magnitude with edge replication, disc dilation and erosion, the exact Euclidean
//! distance transform, sRGB to CIE Lab (D65), and the standard normal CDF. Each is a pure
//! function on buffers; each is tested against brute force or tabulated values.

use image::{GrayImage, Luma};

/// `smooth_rows(v, σ)`: a Gaussian kernel truncated at `3σ`, renormalised over the samples it
/// covers at each position, applied along the row axis; `σ < 0.5` is the identity.
pub fn smooth_rows(values: &[f64], sigma: f64) -> Vec<f64> {
    if sigma < 0.5 || values.len() < 2 {
        return values.to_vec();
    }
    let radius = (3.0 * sigma).ceil() as usize;
    let kernel: Vec<f64> = (0..=radius)
        .map(|k| (-(k as f64).powi(2) / (2.0 * sigma * sigma)).exp())
        .collect();
    let n = values.len();
    (0..n)
        .map(|i| {
            let mut sum = 0.0;
            let mut weight = 0.0;
            let lo = i.saturating_sub(radius);
            let hi = (i + radius).min(n - 1);
            for j in lo..=hi {
                let w = kernel[i.abs_diff(j)];
                sum += w * values[j];
                weight += w;
            }
            sum / weight
        })
        .collect()
}

/// The luminance plane `L = 0.299 R + 0.587 G + 0.114 B` of an RGB image, as `f32`.
pub fn luminance(image: &image::RgbImage) -> Vec<f32> {
    image
        .pixels()
        .map(|p| 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]))
        .collect()
}

/// The 3×3 Sobel gradient magnitude of a plane (`width × height`, row-major), divided by 8,
/// with edge replication at the border.
pub fn sobel_magnitude(plane: &[f32], width: u32, height: u32) -> Vec<f32> {
    let (w, h) = (width as i64, height as i64);
    let at = |x: i64, y: i64| plane[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize];
    let mut out = vec![0.0f32; plane.len()];
    for y in 0..h {
        for x in 0..w {
            // Differences first, so a constant neighbourhood gives exactly zero (a sum of
            // signed terms would leave floating noise, and a zero gradient is a case the
            // measures test for).
            let gx = (at(x + 1, y - 1) - at(x - 1, y - 1))
                + 2.0 * (at(x + 1, y) - at(x - 1, y))
                + (at(x + 1, y + 1) - at(x - 1, y + 1));
            let gy = (at(x - 1, y + 1) - at(x - 1, y - 1))
                + 2.0 * (at(x, y + 1) - at(x, y - 1))
                + (at(x + 1, y + 1) - at(x + 1, y - 1));
            out[(y * w + x) as usize] = (gx * gx + gy * gy).sqrt() / 8.0;
        }
    }
    out
}

/// The offsets of a disc of radius `r` pixels: every `(dx, dy)` with `dx² + dy² ≤ r²`.
pub fn disc(radius: f64) -> Vec<(i64, i64)> {
    let r = radius.max(0.0);
    let ri = r.floor() as i64;
    let mut offsets = Vec::new();
    for dy in -ri..=ri {
        for dx in -ri..=ri {
            if ((dx * dx + dy * dy) as f64) <= r * r + 1e-9 {
                offsets.push((dx, dy));
            }
        }
    }
    offsets
}

/// Binary dilation by a disc: a pixel is set when any pixel within the disc was set.
pub fn dilate(plane: &GrayImage, radius: f64) -> GrayImage {
    morph(plane, radius, true)
}

/// Binary erosion by a disc: a pixel stays set only when every pixel within the disc (inside
/// the image) was set.
pub fn erode(plane: &GrayImage, radius: f64) -> GrayImage {
    morph(plane, radius, false)
}

fn morph(plane: &GrayImage, radius: f64, dilate: bool) -> GrayImage {
    let (w, h) = (plane.width() as i64, plane.height() as i64);
    let offsets = disc(radius);
    let set = |x: i64, y: i64| {
        x >= 0 && y >= 0 && x < w && y < h && plane.get_pixel(x as u32, y as u32)[0] != 0
    };
    GrayImage::from_fn(plane.width(), plane.height(), |x, y| {
        let (xi, yi) = (i64::from(x), i64::from(y));
        let hit = if dilate {
            offsets.iter().any(|(dx, dy)| set(xi + dx, yi + dy))
        } else {
            offsets.iter().all(|(dx, dy)| {
                let (px, py) = (xi + dx, yi + dy);
                // Outside the image counts as unset: erosion shrinks from the border too.
                px >= 0 && py >= 0 && px < w && py < h && set(px, py)
            })
        };
        Luma([if hit { 255 } else { 0 }])
    })
}

/// The exact Euclidean distance transform of a binary plane: for every pixel, the distance in
/// pixels from its centre to the nearest set pixel's centre (0 on a set pixel; `f32::INFINITY`
/// when nothing is set). Felzenszwalb and Huttenlocher's two one-dimensional passes.
pub fn distance_transform(plane: &GrayImage) -> Vec<f32> {
    let (w, h) = (plane.width() as usize, plane.height() as usize);
    let inf = f64::INFINITY;
    let mut f: Vec<f64> = plane
        .pixels()
        .map(|p| if p[0] != 0 { 0.0 } else { inf })
        .collect();
    // Columns.
    let mut column = vec![0.0; h];
    let mut out = vec![0.0; h];
    for x in 0..w {
        for y in 0..h {
            column[y] = f[y * w + x];
        }
        edt_1d(&column, &mut out);
        for y in 0..h {
            f[y * w + x] = out[y];
        }
    }
    // Rows.
    let mut row = vec![0.0; w];
    let mut out = vec![0.0; w];
    for y in 0..h {
        row.copy_from_slice(&f[y * w..(y + 1) * w]);
        edt_1d(&row, &mut out);
        f[y * w..(y + 1) * w].copy_from_slice(&out);
    }
    f.into_iter()
        .map(|d| {
            if d.is_finite() {
                d.sqrt() as f32
            } else {
                f32::INFINITY
            }
        })
        .collect()
}

/// The squared-distance lower envelope of parabolas rooted at `f` (the 1-D step).
fn edt_1d(f: &[f64], out: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f64; n + 1];
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    let s_of = |q: usize, p: usize| -> f64 {
        if f[q].is_infinite() && f[p].is_infinite() {
            return f64::INFINITY;
        }
        if f[q].is_infinite() {
            return f64::INFINITY;
        }
        if f[p].is_infinite() {
            return f64::NEG_INFINITY;
        }
        ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * q as f64 - 2.0 * p as f64)
    };
    for q in 1..n {
        if f[q].is_infinite() {
            continue;
        }
        let mut s = s_of(q, v[k]);
        while k > 0 && s <= z[k] {
            k -= 1;
            s = s_of(q, v[k]);
        }
        if f[v[k]].is_infinite() && k == 0 {
            v[0] = q;
            z[0] = f64::NEG_INFINITY;
            z[1] = f64::INFINITY;
            continue;
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    if f[v[0]].is_infinite() {
        out.fill(f64::INFINITY);
        return;
    }
    let mut k = 0usize;
    for (q, slot) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let d = q as f64 - v[k] as f64;
        *slot = d * d + f[v[k]];
    }
}

/// sRGB 8-bit to CIE Lab under D65.
pub fn srgb_to_lab(rgb: [u8; 3]) -> [f64; 3] {
    let linear = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (linear(rgb[0]), linear(rgb[1]), linear(rgb[2]));
    let x = (0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = (0.0193339 * r + 0.1191920 * g + 0.9503041 * b) / 1.08883;
    let f = |t: f64| {
        if t > 216.0 / 24389.0 {
            t.cbrt()
        } else {
            (24389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// The standard normal cumulative distribution function.
pub fn normal_cdf(x: f64) -> f64 {
    0.5 * erfc(-x / std::f64::consts::SQRT_2)
}

/// The complementary error function: the Maclaurin series of `erf` up to `|x| = 3` (it
/// converges fast there) and the continued fraction of `erfc` beyond (Lentz's method). Both
/// agree with tabulated values to better than 1e-12.
pub fn erfc(x: f64) -> f64 {
    let ax = x.abs();
    let value = if ax <= 3.0 {
        // erf(x) = 2/√π · Σ (-1)^n x^(2n+1) / (n! (2n+1))
        let mut term = ax;
        let mut sum = ax;
        let x2 = ax * ax;
        let mut n = 0.0;
        while term.abs() > 1e-17 * sum.abs().max(1e-300) && n < 200.0 {
            n += 1.0;
            term *= -x2 / n;
            sum += term / (2.0 * n + 1.0);
        }
        1.0 - 2.0 / std::f64::consts::PI.sqrt() * sum
    } else {
        // erfc(x) = exp(-x²)/√π · 1/(x + 1/(2x + 2/(x + 3/(2x + ...)))) as a continued fraction
        // in the form 1/(x+ (1/2)/(x+ 1/(x+ (3/2)/(x+ ...)))).
        let tiny = 1e-300;
        let mut f = ax;
        let mut c = ax;
        let mut d = 0.0;
        for k in 1..300 {
            let a = k as f64 / 2.0;
            d = ax + a * d;
            d = if d.abs() < tiny { tiny } else { d };
            c = ax + a / c;
            c = if c.abs() < tiny { tiny } else { c };
            d = 1.0 / d;
            let delta = c * d;
            f *= delta;
            if (delta - 1.0).abs() < 1e-16 {
                break;
            }
        }
        (-ax * ax).exp() / (std::f64::consts::PI.sqrt() * f)
    };
    if x < 0.0 { 2.0 - value } else { value }
}
