//! The gradient-domain solve behind `paste --mode poisson` (SPEC.md "Operations", the paste
//! contract). The editable domain `Ω` is every pixel with `w > 0` and not protected. Per RGB
//! channel, the discrete Poisson equation on the 4-neighbour grid over `Ω` takes the **source
//! gradients of the fill** inside `Ω` and **Dirichlet values from the base** at every
//! in-image neighbour outside `Ω`, protected holes included; an image-border pixel with a
//! missing neighbour has one fewer term in its equation, and there is no wrap adjacency
//! (`paste` refuses a wrap frame before calling in). A connected component of `Ω` with no
//! neighbour outside `Ω` has nothing to anchor to and is reported instead of solved.
//!
//! Conjugate gradients in `f32` from the initial guess `fill`: the residual of the current
//! iterate is evaluated before every convergence test, including the first, and the solve is
//! converged when `‖r‖₂ ≤ tol · max(‖b‖₂, 1)` for the true residual `b − A x` (the recurrence's
//! residual gates the test, the true one confirms it); `max_iter` reached is an operational
//! error, never a fallback. The solution is clamped to `[0, 255]`; `paste` then blends it by
//! `w`, so the blend zone still fades. The three channels solve on three threads.

use image::{Rgb, RgbImage};
use rascal_core::Error;
use rascal_core::error::Result;
use rascal_core::mask::Mask;

/// `poisson-v1`: the convergence tolerance and the iteration ceiling.
pub const POISSON_V1: &str = include_str!("../constants/poisson-v1.toml");

/// The solver's settings: a constant set's values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// The relative tolerance on the residual: converged when `‖r‖₂ ≤ tol · max(‖b‖₂, 1)`.
    pub tol: f32,
    /// The iteration ceiling; reaching it is an error.
    pub max_iter: usize,
}

impl Settings {
    /// `poisson-v1`.
    pub fn v1() -> Settings {
        let table = POISSON_V1.parse::<toml::Table>().ok();
        let get = |key: &str| table.as_ref().and_then(|t| t.get(key).cloned());
        Settings {
            tol: get("tol").and_then(|v| v.as_float()).unwrap_or(1e-4) as f32,
            max_iter: get("max_iter")
                .and_then(|v| v.as_integer())
                .and_then(|v| usize::try_from(v).ok())
                .unwrap_or(2000),
        }
    }
}

/// What a solve came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Solve {
    /// The solution: the fill with every domain pixel replaced by the solve, clamped.
    Solved {
        solution: RgbImage,
        /// The number of domain pixels.
        domain_px: u64,
        /// The most iterations any channel took.
        iterations: usize,
    },
    /// A connected component of the domain has no neighbour outside it.
    Unanchored {
        /// The pixels in that component.
        component_px: u64,
    },
}

/// No unknown at this slot.
const NONE: u32 = u32::MAX;

/// The domain as a sparse system: one unknown per domain pixel, its 4-neighbour unknowns, and
/// the number of in-image neighbours (the diagonal).
struct Domain {
    width: usize,
    pixels: Vec<u32>,
    neighbours: Vec<[u32; 4]>,
    degree: Vec<f32>,
}

impl Domain {
    fn of(mask: &Mask) -> Domain {
        let (w, h) = mask.dimensions();
        let (width, height) = (w as usize, h as usize);
        let mut index = vec![NONE; width * height];
        let mut pixels = Vec::new();
        for y in 0..h {
            for x in 0..w {
                if mask.weight(x, y) > 0.0 && mask.protect.get_pixel(x, y)[0] == 0 {
                    let slot = y as usize * width + x as usize;
                    index[slot] = pixels.len() as u32;
                    pixels.push(slot as u32);
                }
            }
        }
        let mut neighbours = Vec::with_capacity(pixels.len());
        let mut degree = Vec::with_capacity(pixels.len());
        for &slot in &pixels {
            let slot = slot as usize;
            let (x, y) = (slot % width, slot / width);
            let mut around = [NONE; 4];
            let mut in_image = 0u8;
            let candidates = [
                (x > 0).then(|| slot - 1),
                (x + 1 < width).then(|| slot + 1),
                (y > 0).then(|| slot - width),
                (y + 1 < height).then(|| slot + width),
            ];
            for (i, candidate) in candidates.into_iter().enumerate() {
                if let Some(q) = candidate {
                    in_image += 1;
                    around[i] = index[q];
                }
            }
            neighbours.push(around);
            degree.push(f32::from(in_image));
        }
        Domain {
            width,
            pixels,
            neighbours,
            degree,
        }
    }

    /// The size of the component with no in-image neighbour outside the domain, if any. On
    /// a 4-connected grid a component of the domain that is not the whole image has a pixel
    /// whose neighbour lies outside the domain (a neighbour inside it would be in the same
    /// component), so the only unanchored component is the whole image.
    fn unanchored_component(&self, pixel_count: usize) -> Option<u64> {
        (self.pixels.len() == pixel_count).then_some(self.pixels.len() as u64)
    }

    /// `y = A x`: the degree on the diagonal, minus the domain neighbours.
    fn apply(&self, x: &[f32], y: &mut [f32]) {
        for (i, out) in y.iter_mut().enumerate() {
            let mut v = self.degree[i] * x[i];
            for &q in &self.neighbours[i] {
                if q != NONE {
                    v -= x[q as usize];
                }
            }
            *out = v;
        }
    }

    /// The right-hand side of one channel: the fill's Laplacian over in-image neighbours plus
    /// the base at every in-image neighbour outside the domain.
    fn rhs(&self, base: &[f32], fill: &[f32]) -> Vec<f32> {
        let height = base.len() / self.width;
        let mut b = Vec::with_capacity(self.pixels.len());
        for (i, &slot) in self.pixels.iter().enumerate() {
            let slot = slot as usize;
            let (x, y) = (slot % self.width, slot / self.width);
            let candidates = [
                (x > 0).then(|| slot - 1),
                (x + 1 < self.width).then(|| slot + 1),
                (y > 0).then(|| slot - self.width),
                (y + 1 < height).then(|| slot + self.width),
            ];
            let mut v = 0.0f32;
            for (k, candidate) in candidates.into_iter().enumerate() {
                if let Some(q) = candidate {
                    v += fill[slot] - fill[q];
                    if self.neighbours[i][k] == NONE {
                        v += base[q];
                    }
                }
            }
            b.push(v);
        }
        b
    }
}

fn dot(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| f64::from(x) * f64::from(y))
        .sum()
}

fn norm(a: &[f32]) -> f64 {
    dot(a, a).sqrt()
}

/// Why a channel did not converge: the iterations taken, the residual and the threshold.
#[derive(Debug, Clone, Copy)]
struct Stalled {
    iterations: usize,
    residual: f64,
    threshold: f64,
}

/// Conjugate gradients on one channel from the initial guess `x`; the iterations taken.
fn conjugate_gradients(
    domain: &Domain,
    b: &[f32],
    x: &mut [f32],
    settings: &Settings,
) -> std::result::Result<usize, Stalled> {
    let n = x.len();
    let threshold = f64::from(settings.tol) * norm(b).max(1.0);
    let mut ax = vec![0.0f32; n];
    let mut r = vec![0.0f32; n];
    let true_residual = |x: &[f32], ax: &mut [f32], r: &mut [f32]| {
        domain.apply(x, ax);
        for i in 0..n {
            r[i] = b[i] - ax[i];
        }
        norm(r)
    };
    let mut residual = true_residual(x, &mut ax, &mut r);
    let mut p = r.clone();
    let mut ap = vec![0.0f32; n];
    let mut rr = dot(&r, &r);
    let mut iterations = 0usize;
    loop {
        if residual <= threshold {
            // The recurrence's residual passed: confirm with the true residual of the iterate,
            // and restart from it when it does not agree.
            let confirmed = true_residual(x, &mut ax, &mut r);
            if confirmed <= threshold {
                return Ok(iterations);
            }
            residual = confirmed;
            p.copy_from_slice(&r);
            rr = dot(&r, &r);
        }
        if iterations >= settings.max_iter {
            return Err(Stalled {
                iterations,
                residual,
                threshold,
            });
        }
        domain.apply(&p, &mut ap);
        let pap = dot(&p, &ap);
        if pap <= 0.0 {
            return Err(Stalled {
                iterations,
                residual,
                threshold,
            });
        }
        let alpha = (rr / pap) as f32;
        for i in 0..n {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
        }
        let rr_next = dot(&r, &r);
        let beta = (rr_next / rr) as f32;
        for i in 0..n {
            p[i] = r[i] + beta * p[i];
        }
        rr = rr_next;
        residual = rr.sqrt();
        iterations += 1;
    }
}

/// The solve on three 8-bit RGB images of one size: `base`, `fill` and the mask's domain.
pub fn solve(base: &RgbImage, fill: &RgbImage, mask: &Mask, settings: &Settings) -> Result<Solve> {
    let (w, h) = mask.dimensions();
    if base.dimensions() != (w, h) || fill.dimensions() != (w, h) {
        return Err(Error::op(
            "paste",
            "poisson: base, fill and mask must share one size",
        ));
    }
    let domain = Domain::of(mask);
    if let Some(component_px) = domain.unanchored_component(w as usize * h as usize) {
        return Ok(Solve::Unanchored { component_px });
    }
    let channel = |image: &RgbImage, c: usize| -> Vec<f32> {
        image.pixels().map(|p| f32::from(p[c])).collect()
    };
    let results: Vec<std::result::Result<(Vec<f32>, usize), Stalled>> =
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..3)
                .map(|c| {
                    let domain = &domain;
                    scope.spawn(move || {
                        let base = channel(base, c);
                        let fill = channel(fill, c);
                        let b = domain.rhs(&base, &fill);
                        let mut x: Vec<f32> =
                            domain.pixels.iter().map(|&s| fill[s as usize]).collect();
                        conjugate_gradients(domain, &b, &mut x, settings).map(|it| (x, it))
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| match handle.join() {
                    Ok(result) => result,
                    Err(_) => Err(Stalled {
                        iterations: 0,
                        residual: f64::NAN,
                        threshold: f64::NAN,
                    }),
                })
                .collect()
        });
    let mut solution = fill.clone();
    let mut iterations = 0usize;
    for (c, result) in results.into_iter().enumerate() {
        let (x, taken) = result.map_err(|stalled| {
            Error::op(
                "paste",
                format!(
                    "poisson: channel {c} not converged after {} iteration{}: residual {:.3e} above tol · max(‖b‖, 1) = {:.3e}",
                    stalled.iterations,
                    if stalled.iterations == 1 { "" } else { "s" },
                    stalled.residual,
                    stalled.threshold
                ),
            )
        })?;
        iterations = iterations.max(taken);
        for (i, &slot) in domain.pixels.iter().enumerate() {
            let slot = slot as usize;
            let (px, py) = ((slot % domain.width) as u32, (slot / domain.width) as u32);
            let Rgb(mut rgb) = *solution.get_pixel(px, py);
            rgb[c] = x[i].round().clamp(0.0, 255.0) as u8;
            solution.put_pixel(px, py, Rgb(rgb));
        }
    }
    Ok(Solve::Solved {
        solution,
        domain_px: domain.pixels.len() as u64,
        iterations,
    })
}
