//! `window`: an integer crop around a mask's support, with context, into a new `crop:` frame
//! (SPEC.md "Operations", the window contract). Nothing is resampled: the image and the mask's
//! planes are cut by the same bbox, and `unwindow` writes the crop back into exactly it.

use std::collections::BTreeMap;
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::artifact::{Artifact, ArtifactType};
use rascal_core::error::Result;
use rascal_core::frame::{Frame, FrameKind, FrameTable};
use rascal_core::image::Image;
use rascal_core::mask::Mask;
use rascal_core::op::{
    Context, Cost, CostTier, Inputs, Manifest, Op, Output, Param, ParamKind, Params, Port,
};

/// The op.
pub struct Window;

const NAME: &str = "window";

/// `window-v1`: the context as a share of the support's width and height on each side.
pub const WINDOW_V1: &str = include_str!("../constants/window-v1.toml");

fn context_default() -> f64 {
    WINDOW_V1
        .parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("context").and_then(|v| v.as_float()))
        .unwrap_or(0.5)
}

impl Op for Window {
    fn manifest(&self) -> Manifest {
        Manifest::new(
            NAME,
            "1.0.0",
            "An integer crop around the mask's support, with context, as a new crop: frame holding the image and the mask.",
            "Before a filler whose max_side is smaller than the image, or to work on the region alone; the heal graph windows before the filler call and unwindows after.",
            "When the support itself is larger than max_side (scaled windows are a later milestone), or when the whole frame is wanted.",
            "An Image and a Mask in a new crop: frame whose bbox the frame records; nothing resampled. The support is always inside the window; context, squareness and the caps shrink only the context. Not applicable for an empty support or one larger than max_side, naming the sizes.",
            Cost {
                tier: CostTier::Cheap,
                seconds: 0.05,
            },
            vec![
                Port {
                    name: "image".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The image to window.".to_string(),
                },
                Port {
                    name: "mask".to_string(),
                    artifact_type: ArtifactType::Mask,
                    required: true,
                    purpose: "The mask whose support the window is built around.".to_string(),
                },
            ],
            vec![
                Port {
                    name: "image".to_string(),
                    artifact_type: ArtifactType::Image,
                    required: true,
                    purpose: "The crop of the image, in the crop: frame.".to_string(),
                },
                Port {
                    name: "mask".to_string(),
                    artifact_type: ArtifactType::Mask,
                    required: true,
                    purpose: "The mask's planes cropped by the same bbox, in the crop: frame.".to_string(),
                },
            ],
            vec![
                Param::absent(
                    "max_side",
                    "The largest side of the window in pixels; a support larger than it is not applicable. Unset: no cap but the image.",
                    ParamKind::Integer,
                )
                .bounded(1.0, 1e6),
                Param::optional(
                    "context",
                    "The context on each side of the support, as a share of its width and height.",
                    ParamKind::Number,
                    serde_json::json!(context_default()),
                )
                .bounded(0.0, 4.0),
            ],
        )
        .with_constants(&["window-v1"])
    }

    fn constant_set(&self, name: &str) -> Params {
        if name != "window-v1" {
            return Params::new();
        }
        [("context".to_string(), toml::Value::Float(context_default()))]
            .into_iter()
            .collect()
    }

    fn run(&self, context: &Context<'_>, inputs: &Inputs, params: &Params) -> Result<Output> {
        let (Some(Artifact::Image(image)), Some(Artifact::Mask(mask))) =
            (inputs.get("image"), inputs.get("mask"))
        else {
            return Err(Error::op(NAME, "inputs image and mask are required"));
        };
        let parent = context.frames.require(&image.record.frame)?;
        let max_side = params
            .get("max_side")
            .and_then(|v| v.as_integer())
            .map(|m| m.max(1) as u32);
        let ctx = crate::float(params, "context").unwrap_or_else(context_default);
        let Some(support) = mask.support() else {
            return Ok(Output::not_applicable(
                "the mask's support is empty: no pixel has an effective weight above zero",
            ));
        };
        let bbox = match window_bbox(support, parent.width, parent.height, max_side, ctx) {
            Ok(bbox) => bbox,
            Err(TooLarge { support, max_side }) => {
                return Ok(Output::not_applicable(format!(
                    "the support is {}x{} and max_side is {max_side}; scaled windows are a later milestone",
                    support[2] - support[0],
                    support[3] - support[1]
                )));
            }
        };
        let frame = crop_frame(context.frames, parent, bbox)?;
        let [l, t, r, b] = bbox;
        let crop = image.pixels.crop_imm(l, t, r - l, b - t);
        let cropped_image = Image::new("window.image", crop, &frame, image.record.role, None);
        let cropped_mask = crop_mask(mask, &frame, bbox)?;
        let mut outputs = BTreeMap::new();
        outputs.insert(
            "image".to_string(),
            Artifact::Image(Arc::new(cropped_image)),
        );
        outputs.insert("mask".to_string(), Artifact::Mask(Arc::new(cropped_mask)));
        Ok(Output::ok(
            format!(
                "window [{l}, {t}, {r}, {b}) of {} ({}x{}) around a {}x{} support, into {}",
                parent.id,
                r - l,
                b - t,
                support[2] - support[0],
                support[3] - support[1],
                frame.id
            ),
            outputs,
        )
        .with_measurement(
            "window_px",
            f64::from((r - l) * (b - t)),
            "px",
            Some(&parent.id),
        )
        .with_frame(frame))
    }
}

/// A support larger than `max_side` on a side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooLarge {
    pub support: [u32; 4],
    pub max_side: u32,
}

/// The window contract, pure: the support expanded by `context` times its width and height
/// on each side (in pixels, rounded half up), made square at the larger side, capped to
/// `max_side` and then to the image, and slid inside the image, toward the smaller offset when
/// two placements tie. The support is always inside the window.
pub fn window_bbox(
    support: [u32; 4],
    width: u32,
    height: u32,
    max_side: Option<u32>,
    context: f64,
) -> std::result::Result<[u32; 4], TooLarge> {
    let [sx0, sy0, sx1, sy1] = support;
    let (sw, sh) = (sx1 - sx0, sy1 - sy0);
    if let Some(m) = max_side
        && (sw > m || sh > m)
    {
        return Err(TooLarge {
            support,
            max_side: m,
        });
    }
    let half_up = |v: f64| (v + 0.5).floor() as u32;
    let (cx, cy) = (
        half_up(context * f64::from(sw)),
        half_up(context * f64::from(sh)),
    );
    let side = (sw + 2 * cx).max(sh + 2 * cy);
    let side = max_side.map_or(side, |m| side.min(m));
    let (ww, wh) = (side.min(width), side.min(height));
    // Centre on the support; an odd remainder gives the smaller part to the near offset.
    let place = |s0: u32, s1: u32, w: u32, limit: u32| -> u32 {
        let extra = w - (s1 - s0);
        let x0 = i64::from(s0) - i64::from(extra / 2);
        x0.clamp(0, i64::from(limit - w)) as u32
    };
    let x0 = place(sx0, sx1, ww, width);
    let y0 = place(sy0, sy1, wh, height);
    Ok([x0, y0, x0 + ww, y0 + wh])
}

/// The crop frame for a bbox of `parent`: the existing frame when one with the same identity
/// is in the table, else a new `crop:<n>` with `n` past the crops already there.
pub fn crop_frame(frames: &FrameTable, parent: &Frame, bbox: [u32; 4]) -> Result<Frame> {
    let crops = frames
        .frames
        .iter()
        .filter(|f| f.kind == FrameKind::Crop)
        .count() as u64;
    let fresh = Frame::crop(parent, crops + 1, bbox)?;
    Ok(frames
        .frames
        .iter()
        .find(|f| f.identity == fresh.identity)
        .cloned()
        .unwrap_or(fresh))
}

/// The mask's three planes cut by the bbox into the crop frame; the use case and the source
/// carry over, the profile and the prescriptions do not (they were measured in the parent).
pub fn crop_mask(mask: &Mask, frame: &Frame, bbox: [u32; 4]) -> Result<Mask> {
    let [l, t, r, b] = bbox;
    let cut =
        |plane: &image::GrayImage| image::imageops::crop_imm(plane, l, t, r - l, b - t).to_image();
    let mut out = Mask::from_core(
        "window.mask",
        frame,
        mask.record.use_case,
        cut(&mask.core),
        mask.record.source.clone(),
    )?;
    out.set_blend(cut(&mask.blend))?;
    out.protect_union(&cut(&mask.protect))?;
    Ok(out)
}
