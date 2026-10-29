//! The focus of a pair (SPEC.md "The lab"): where the arms changed the image, so the pairs
//! page shows that region at full size and aims its look-around view at it. `Changes` marks
//! the columns and rows where a result differs from its source; the two sides' changes are
//! joined so both crops frame the same region; `Focus` is that region with context around it,
//! through the edge of a wrap frame and clamped at the edge of a bounded one.

use std::borrow::Cow;

use image::{DynamicImage, GenericImageView, Rgb, RgbImage};

use crate::Error;
use crate::error::Result;
use crate::frame::Topology;

/// Context around the changed region, per axis: half the region's extent on that axis, at
/// least `CONTEXT_MIN` and at most `CONTEXT_MAX` pixels, on each side.
pub const CONTEXT_MIN: u32 = 64;
pub const CONTEXT_MAX: u32 = 512;

/// The columns and rows where a result differs from its source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changes {
    width: u32,
    height: u32,
    columns: Vec<bool>,
    rows: Vec<bool>,
}

/// A region of a frame with context around it: `x` is its first column, and on a wrap frame
/// `x + width` may pass the frame's width, the columns continuing from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Focus {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

fn rgb(image: &DynamicImage) -> Cow<'_, RgbImage> {
    match image.as_rgb8() {
        Some(pixels) => Cow::Borrowed(pixels),
        None => Cow::Owned(image.to_rgb8()),
    }
}

impl Changes {
    /// The columns and rows where `after` differs from `before`; the sizes must agree.
    pub fn between(before: &DynamicImage, after: &DynamicImage) -> Result<Changes> {
        let (width, height) = before.dimensions();
        if after.dimensions() != (width, height) {
            return Err(Error::op(
                "focus",
                format!(
                    "the result is {}x{} but its source is {width}x{height}",
                    after.width(),
                    after.height()
                ),
            ));
        }
        let before = rgb(before);
        let after = rgb(after);
        let mut columns = vec![false; width as usize];
        let mut rows = vec![false; height as usize];
        let stride = 3 * width as usize;
        for (y, (a, b)) in before
            .as_raw()
            .chunks_exact(stride)
            .zip(after.as_raw().chunks_exact(stride))
            .enumerate()
        {
            if a == b {
                continue;
            }
            rows[y] = true;
            let (a, b) = (a.as_chunks::<3>().0, b.as_chunks::<3>().0);
            for (x, (pa, pb)) in a.iter().zip(b).enumerate() {
                if pa != pb {
                    columns[x] = true;
                }
            }
        }
        Ok(Changes {
            width,
            height,
            columns,
            rows,
        })
    }

    /// Adds another side's changes over the same frame.
    pub fn join(&mut self, other: &Changes) -> Result<()> {
        if (other.width, other.height) != (self.width, self.height) {
            return Err(Error::op(
                "focus",
                format!(
                    "the sides are {}x{} and {}x{}: not one image",
                    self.width, self.height, other.width, other.height
                ),
            ));
        }
        for (mine, theirs) in self.columns.iter_mut().zip(&other.columns) {
            *mine |= theirs;
        }
        for (mine, theirs) in self.rows.iter_mut().zip(&other.rows) {
            *mine |= theirs;
        }
        Ok(())
    }

    /// Whether any pixel changed.
    pub fn any(&self) -> bool {
        self.rows.iter().any(|&r| r)
    }

    /// The changed region with context, or `None` when nothing changed. On a wrap frame the
    /// columns form the tightest interval around the changes through the edge; when the
    /// context meets itself the region is the whole width.
    pub fn focus(&self, topology: Topology) -> Option<Focus> {
        if !self.any() {
            return None;
        }
        let (y, height) = bounded_span(&self.rows)?;
        let (y, height) = with_context(y, height, self.height);
        let (x, width) = match topology {
            Topology::Bounded => {
                let (x, width) = bounded_span(&self.columns)?;
                with_context(x, width, self.width)
            }
            Topology::Wrap => {
                let (x, width) = wrapped_span(&self.columns)?;
                let context = context_of(width);
                if width + 2 * context >= self.width {
                    (0, self.width)
                } else {
                    ((x + self.width - context) % self.width, width + 2 * context)
                }
            }
        };
        Some(Focus {
            x,
            y,
            width,
            height,
        })
    }
}

fn context_of(extent: u32) -> u32 {
    (extent / 2).clamp(CONTEXT_MIN, CONTEXT_MAX)
}

/// `start` and `extent` grown by the context and clamped to `[0, size)`.
fn with_context(start: u32, extent: u32, size: u32) -> (u32, u32) {
    let context = context_of(extent);
    let first = start.saturating_sub(context);
    let end = (start + extent + context).min(size);
    (first, end - first)
}

/// The first marked index and the count through the last.
fn bounded_span(marks: &[bool]) -> Option<(u32, u32)> {
    let first = marks.iter().position(|&m| m)?;
    let last = marks.iter().rposition(|&m| m)?;
    Some((first as u32, (last - first + 1) as u32))
}

/// The tightest circular interval holding every mark: the complement of the longest circular
/// run of unmarked indices.
fn wrapped_span(marks: &[bool]) -> Option<(u32, u32)> {
    let n = marks.len();
    if !marks.iter().any(|&m| m) {
        return None;
    }
    if marks.iter().all(|&m| m) {
        return Some((0, n as u32));
    }
    let mut best: Option<(usize, usize)> = None;
    let mut run: Option<(usize, usize)> = None;
    for i in 0..2 * n {
        if marks[i % n] {
            run = None;
            continue;
        }
        let (start, len) = match run {
            Some((start, len)) => (start, (len + 1).min(n)),
            None => (i, 1),
        };
        run = Some((start, len));
        if best.is_none_or(|(_, best_len)| len > best_len) {
            best = Some((start, len));
        }
    }
    let (gap_start, gap_len) = best?;
    Some((((gap_start + gap_len) % n) as u32, (n - gap_len) as u32))
}

impl Focus {
    /// The region's pixels, columns taken modulo the image width.
    pub fn crop(&self, image: &DynamicImage) -> RgbImage {
        let source = rgb(image);
        let width = source.width();
        let mut out = RgbImage::new(self.width, self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                let pixel: Rgb<u8> = *source.get_pixel((self.x + x) % width, self.y + y);
                out.put_pixel(x, y, pixel);
            }
        }
        out
    }

    /// The region's centre in frame pixels, the column modulo `frame_width`.
    pub fn centre(&self, frame_width: u32) -> (f64, f64) {
        let cx = (f64::from(self.x) + f64::from(self.width) / 2.0) % f64::from(frame_width);
        (cx, f64::from(self.y) + f64::from(self.height) / 2.0)
    }
}
