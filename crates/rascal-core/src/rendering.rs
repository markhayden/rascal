//! Renderings: what one consumer wants to see of a mask (SPEC.md "Renderings"). Each profile
//! declares its role (which filler field it fills), its output and its polarity. The functions
//! here are pure: a mask, maybe an image, a few parameters, pixels out. The `render` op in the
//! ops crate wraps them.

use image::{DynamicImage, GrayImage, Luma, Rgb, RgbImage, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::image::RenderingRole;
use crate::mask::Mask;

/// The rendering profiles. Named by a string parameter at every boundary, so the enum itself
/// has no schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    /// 8-bit `round(255 · w)`; 255 = paint. A `mask_input`.
    Grey,
    /// 8-bit, 255 where `w ≥ t / 255`; protected pixels are 0 whatever `t`. A `mask_input`.
    Binary,
    /// RGBA of the image, alpha 0 where `core` is set and not protected, 255 elsewhere;
    /// transparent = paint. An `image_input`.
    Alpha,
    /// RGB of the image with the unprotected core blended toward a colour; tint = paint. An
    /// `image_input`.
    Tinted,
    /// For the eye: the image, the tint by `w`, the core's edge as a white line.
    Overlay,
}

impl Profile {
    /// Every profile, in the spec's order.
    pub const ALL: [Profile; 5] = [
        Profile::Grey,
        Profile::Binary,
        Profile::Alpha,
        Profile::Tinted,
        Profile::Overlay,
    ];

    /// The name used on the command line and in recipes.
    pub fn name(self) -> &'static str {
        match self {
            Profile::Grey => "grey",
            Profile::Binary => "binary",
            Profile::Alpha => "alpha",
            Profile::Tinted => "tinted",
            Profile::Overlay => "overlay",
        }
    }

    /// By name.
    pub fn parse(name: &str) -> Option<Profile> {
        Profile::ALL.into_iter().find(|p| p.name() == name)
    }

    /// Which filler field the rendering fills; `None` for inspection.
    pub fn role(self) -> Option<RenderingRole> {
        match self {
            Profile::Grey | Profile::Binary => Some(RenderingRole::MaskInput),
            Profile::Alpha | Profile::Tinted => Some(RenderingRole::ImageInput),
            Profile::Overlay => None,
        }
    }

    /// True when the profile draws on the working image and needs one.
    pub fn needs_image(self) -> bool {
        !matches!(self, Profile::Grey | Profile::Binary)
    }

    /// Which value means "paint".
    pub fn polarity(self) -> &'static str {
        match self {
            Profile::Grey | Profile::Binary => "255 = paint",
            Profile::Alpha => "transparent = paint",
            Profile::Tinted => "tint = paint",
            Profile::Overlay => "for the eye only",
        }
    }
}

/// The parameters the profiles take; each has the spec's default.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderParams {
    /// `binary`: 255 where `w ≥ threshold / 255`; in `[1, 255]`.
    pub threshold: u8,
    /// `tinted` and `overlay`: the tint, default a magenta no photograph has.
    pub colour: [u8; 3],
    /// `tinted`: how far the core moves toward the colour, in `[0, 1]`.
    pub strength: f32,
}

impl Default for RenderParams {
    fn default() -> Self {
        RenderParams {
            threshold: 127,
            colour: [255, 0, 255],
            strength: 0.7,
        }
    }
}

/// Parses `#rrggbb` or `rrggbb`.
pub fn parse_colour(text: &str) -> Result<[u8; 3]> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::usage(format!(
            "colour must be #rrggbb, got '{text}'"
        )));
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
    Ok([channel(0), channel(2), channel(4)])
}

/// `grey`: 8-bit `round(255 · w)`.
pub fn grey(mask: &Mask) -> GrayImage {
    let (width, height) = mask.dimensions();
    GrayImage::from_fn(width, height, |x, y| {
        Luma([(255.0 * mask.weight(x, y)).round().clamp(0.0, 255.0) as u8])
    })
}

/// `binary`: 255 where `w ≥ t / 255`, protected pixels 0 whatever `t`. `t` in `[1, 255]`.
pub fn binary(mask: &Mask, threshold: u8) -> Result<GrayImage> {
    if threshold == 0 {
        return Err(Error::usage(
            "threshold must be in [1, 255]; 0 would paint everything",
        ));
    }
    let t = f32::from(threshold) / 255.0;
    let (width, height) = mask.dimensions();
    Ok(GrayImage::from_fn(width, height, |x, y| {
        let protected = mask.protect.get_pixel(x, y)[0] != 0;
        Luma([if !protected && mask.weight(x, y) >= t {
            255
        } else {
            0
        }])
    }))
}

/// `alpha`: RGBA of the image, alpha 0 where the core is set and not protected.
pub fn alpha(mask: &Mask, image: &DynamicImage) -> Result<RgbaImage> {
    same_size(mask, image)?;
    let rgba = image.to_rgba8();
    let (width, height) = mask.dimensions();
    Ok(RgbaImage::from_fn(width, height, |x, y| {
        let p = rgba.get_pixel(x, y);
        let paint = mask.core.get_pixel(x, y)[0] != 0 && mask.protect.get_pixel(x, y)[0] == 0;
        Rgba([p[0], p[1], p[2], if paint { 0 } else { 255 }])
    }))
}

/// `tinted`: RGB of the image with the unprotected core blended toward `colour` by `strength`.
pub fn tinted(
    mask: &Mask,
    image: &DynamicImage,
    colour: [u8; 3],
    strength: f32,
) -> Result<RgbImage> {
    same_size(mask, image)?;
    if !(0.0..=1.0).contains(&strength) {
        return Err(Error::usage(format!(
            "strength must be in [0, 1], got {strength}"
        )));
    }
    let rgb = image.to_rgb8();
    let (width, height) = mask.dimensions();
    Ok(RgbImage::from_fn(width, height, |x, y| {
        let p = rgb.get_pixel(x, y);
        let paint = mask.core.get_pixel(x, y)[0] != 0 && mask.protect.get_pixel(x, y)[0] == 0;
        if paint {
            Rgb(mix(p.0, colour, strength))
        } else {
            *p
        }
    }))
}

/// `overlay`: the image tinted by `w` (half strength), the core's edge as a white line.
pub fn overlay(mask: &Mask, image: &DynamicImage, colour: [u8; 3]) -> Result<RgbImage> {
    same_size(mask, image)?;
    let rgb = image.to_rgb8();
    let (width, height) = mask.dimensions();
    let core = |x: i64, y: i64| {
        x >= 0
            && y >= 0
            && x < i64::from(width)
            && y < i64::from(height)
            && mask.core.get_pixel(x as u32, y as u32)[0] != 0
    };
    Ok(RgbImage::from_fn(width, height, |x, y| {
        let (xi, yi) = (i64::from(x), i64::from(y));
        if core(xi, yi)
            && !(core(xi - 1, yi) && core(xi + 1, yi) && core(xi, yi - 1) && core(xi, yi + 1))
        {
            return Rgb([255, 255, 255]);
        }
        let p = rgb.get_pixel(x, y);
        let w = mask.weight(x, y);
        if w > 0.0 {
            Rgb(mix(p.0, colour, 0.5 * w))
        } else {
            *p
        }
    }))
}

fn mix(base: [u8; 3], colour: [u8; 3], amount: f32) -> [u8; 3] {
    let channel = |b: u8, c: u8| {
        ((1.0 - amount) * f32::from(b) + amount * f32::from(c))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    [
        channel(base[0], colour[0]),
        channel(base[1], colour[1]),
        channel(base[2], colour[2]),
    ]
}

fn same_size(mask: &Mask, image: &DynamicImage) -> Result<()> {
    if mask.dimensions() != (image.width(), image.height()) {
        return Err(Error::usage(format!(
            "the image is {}x{} but the mask is {}x{}",
            image.width(),
            image.height(),
            mask.dimensions().0,
            mask.dimensions().1
        )));
    }
    Ok(())
}
