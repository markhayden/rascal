//! The located retry of a checked remove (SPEC.md "Self-correcting remove", part 3, step 4):
//! from the places a checker named on a try's result, the planes the next try's mask is
//! rebuilt from. Each place is checked against the result's image reference, the input image is
//! cut exactly to the place's box padded by `segment.pad` (no added context), `segment` runs
//! there with the same prompt at half the threshold, and each plane it returns is written into
//! an empty raw-frame plane at the crop's offset and committed as a `plane` image; a place where
//! it finds nothing joins as itself, rasterized. An empty answer widens the mask instead: the
//! next try's `grow.px` doubled, once.

use std::collections::BTreeMap;
use std::sync::Arc;

use image::{DynamicImage, GenericImage, GrayImage};
use rascal_core::Error;
use rascal_core::artifact::Artifact;
use rascal_core::contract::table_manners::images::{ImageReference, Location as TmLocation};
use rascal_core::error::Result;
use rascal_core::frame::Frame;
use rascal_core::image::{Image, Role};
use rascal_core::op::{Inputs, Status};
use rascal_core::recipe::{Recipe, Run};
use rascal_core::registry::OpRegistry;
use rascal_mask::checker::call::Transition;

/// The steps of the shipped `remove-prompt` the loop names: the find, where the planes are
/// rebound, and the widening.
pub const SEGMENT: &str = "segment";

/// The crop pad for a model that takes no `pad` (`segment`'s default; SPEC.md "Choosing a
/// model").
pub const CROP_PAD: f64 = 0.05;
pub const SELECT: &str = "select";
pub const GROW: &str = "grow";

/// Where the retry runs: the try's run (its directory and the shared workspace), the registry,
/// the recipe as the run resolved it and its text inputs, and the input image in its raw frame.
pub struct Retry<'a> {
    pub run: &'a Run,
    pub registry: &'a OpRegistry,
    pub recipe: &'a Recipe,
    pub texts: &'a BTreeMap<String, String>,
    pub input: &'a Arc<Image>,
    pub raw: &'a Frame,
}

impl Retry<'_> {
    /// The planes the places add, made for try `next`. `Ok(Err(reason))` when a place is not on
    /// the judged result (`candidate`): a check failure.
    pub fn locate(
        &self,
        next: u32,
        candidate: &ImageReference,
        places: &[TmLocation],
        transition: &mut Transition,
    ) -> Result<std::result::Result<Vec<Artifact>, String>> {
        let mut shapes = Vec::new();
        for place in places {
            // rascal holds one shape per location: a place in several parts is one per part.
            for part in &place.parts {
                let single = TmLocation {
                    parts: vec![part.clone()],
                    straddles_wrap: false,
                    ..place.clone()
                };
                match rascal_core::contract::location_in(&single, candidate, self.raw) {
                    Ok(location) => shapes.push(location),
                    Err(e) => return Ok(Err(format!("a place it named is not usable: {e}"))),
                }
            }
        }
        let segment = self.segment_params()?;
        let mut planes = Vec::new();
        for (i, location) in shapes.iter().enumerate() {
            let core = rascal_ops::rasterize::location_core(location, self.raw)
                .map_err(|e| Error::op("heal", format!("place {i}: {e}")))?;
            let Some(rect) = core.as_ref().and_then(support) else {
                self.run.note(format!(
                    "place {i} of try {} covers no pixel; it adds nothing",
                    next - 1
                ))?;
                continue;
            };
            let crop = padded(rect, segment.pad, self.raw.width, self.raw.height);
            transition.crops.push(crop);
            transition.crop_pad = Some(segment.pad);
            let found = self.segment_in(next, i, crop, &segment)?;
            let made = if found.is_empty() {
                let core = core.unwrap_or_default();
                vec![self.commit(next, i, crop, core, None)?]
            } else {
                let mut made = Vec::new();
                for plane in found {
                    let mut raw = GrayImage::new(self.raw.width, self.raw.height);
                    raw.copy_from(&plane.pixels.to_luma8(), crop[0], crop[1])
                        .map_err(|e| Error::op("heal", format!("place {i}: {e}")))?;
                    made.push(self.commit(next, i, crop, raw, Some(&plane.record.identity))?);
                }
                made
            };
            transition
                .planes
                .extend(made.iter().map(|a| a.identity().as_str().to_string()));
            planes.extend(made);
        }
        Ok(Ok(planes))
    }

    /// `segment`'s parameters as the run resolved them (its model and pinned installation
    /// included, from the step), with the threshold halved when the model uses one, and the crop
    /// pad: the model's `pad` when it uses one, else 0.05 (`segment`'s default), which is
    /// rascal's own and never passed to a model that does not take it.
    fn segment_params(&self) -> Result<Segment> {
        let step = self
            .recipe
            .step
            .iter()
            .find(|s| s.id == SEGMENT)
            .ok_or_else(|| Error::op("heal", "the recipe has no segment step to retry"))?;
        let mut params = self
            .recipe
            .step_params(SEGMENT, self.texts)
            .unwrap_or_default();
        let manifest = self.registry.require(&step.op)?.manifest();
        let resolved = |name: &str| {
            params
                .get(name)
                .and_then(number)
                .or_else(|| {
                    manifest
                        .params
                        .iter()
                        .find(|p| p.name == name)
                        .and_then(|p| p.default.as_ref())
                        .and_then(serde_json::Value::as_f64)
                })
                .ok_or_else(|| {
                    Error::op(
                        "heal",
                        format!("{} declares no {name} for the located retry", step.op),
                    )
                })
        };
        let op = self.registry.require(&step.op)?;
        let unused = op
            .inapplicable(&params)
            .map(|off| off.params)
            .unwrap_or_default();
        let pad = if unused.iter().any(|p| p == "pad") {
            CROP_PAD
        } else {
            resolved("pad")?
        };
        if !unused.iter().any(|p| p == "threshold") {
            let threshold = resolved("threshold")?;
            params.insert("threshold".into(), toml::Value::Float(threshold / 2.0));
        }
        Ok(Segment {
            op: step.op.clone(),
            params,
            pad,
        })
    }

    /// The planes `segment` finds in the input cut to `crop`; none when it finds nothing.
    fn segment_in(
        &self,
        next: u32,
        place: usize,
        crop: [u32; 4],
        segment: &Segment,
    ) -> Result<Vec<Arc<Image>>> {
        let frames = self.run.frames()?;
        let (image, frame) = rascal_ops::window::crop_exact(&frames, self.raw, self.input, crop)?;
        let (_, image) = self.run.workspace.add_external(
            image,
            &frame,
            "crop",
            serde_json::json!({ "try": next, "place": place, "bbox": crop }),
        )?;
        let mut inputs: Inputs = BTreeMap::new();
        inputs.insert("image".into(), image);
        let out = self.run.invoke(
            &format!("tries.{next}.{SEGMENT}.{place}"),
            &segment.op,
            self.registry,
            &inputs,
            &segment.params,
        )?;
        match out.envelope.status {
            Status::Ok => {
                let mut planes: Vec<(usize, Arc<Image>)> = out
                    .outputs
                    .into_iter()
                    .filter_map(|(name, artifact)| {
                        let n = name.strip_prefix("plane_")?.parse().ok()?;
                        match artifact {
                            Artifact::Image(image) => Some((n, image)),
                            _ => None,
                        }
                    })
                    .collect();
                planes.sort_by_key(|(n, _)| *n);
                Ok(planes.into_iter().map(|(_, p)| p).collect())
            }
            Status::NotApplicable => Ok(Vec::new()),
            status => Err(Error::op(
                "heal",
                format!(
                    "segment in place {place}: {status:?}: {}",
                    out.envelope.reason.unwrap_or(out.envelope.summary)
                ),
            )),
        }
    }

    /// Commits a raw-frame plane for place `place` of try `next`.
    fn commit(
        &self,
        next: u32,
        place: usize,
        crop: [u32; 4],
        plane: GrayImage,
        from: Option<&str>,
    ) -> Result<Artifact> {
        let image = Image::new(
            "place.plane",
            DynamicImage::ImageLuma8(plane),
            self.raw,
            Role::Plane,
            None,
        );
        let (_, artifact) = self.run.workspace.add_external(
            image,
            self.raw,
            "place",
            serde_json::json!({ "try": next, "place": place, "crop": crop, "from": from }),
        )?;
        Ok(artifact)
    }
}

/// `segment` as the retry calls it.
struct Segment {
    op: String,
    params: rascal_core::op::Params,
    pad: f64,
}

fn number(value: &toml::Value) -> Option<f64> {
    value
        .as_float()
        .or_else(|| value.as_integer().map(|i| i as f64))
}

/// The box `[x0, y0, x1, y1)` of a plane's set pixels; none when none is set.
pub fn support(plane: &GrayImage) -> Option<[u32; 4]> {
    let mut rect: Option<[u32; 4]> = None;
    for (x, y, p) in plane.enumerate_pixels() {
        if p[0] != 0 {
            let r = rect.get_or_insert([x, y, x + 1, y + 1]);
            r[0] = r[0].min(x);
            r[1] = r[1].min(y);
            r[2] = r[2].max(x + 1);
            r[3] = r[3].max(y + 1);
        }
    }
    rect
}

/// `rect` with each side moved out by `pad` times the box's side on that axis, snapped outward
/// to whole pixels and clamped to the `width` by `height` frame.
pub fn padded(rect: [u32; 4], pad: f64, width: u32, height: u32) -> [u32; 4] {
    let [x0, y0, x1, y1] = rect.map(f64::from);
    let (dx, dy) = (pad * (x1 - x0), pad * (y1 - y0));
    let near = |v: f64| v.floor().max(0.0) as u32;
    let far = |v: f64, limit: u32| (v.ceil() as u32).min(limit);
    [
        near(x0 - dx),
        near(y0 - dy),
        far(x1 + dx, width),
        far(y1 + dy, height),
    ]
}

/// The planes `select` took for the first try: `segment.plane_<n>`, in order.
pub fn first_planes(bindings: &BTreeMap<String, Artifact>) -> Vec<Artifact> {
    let mut planes: Vec<(usize, Artifact)> = bindings
        .iter()
        .filter_map(|(name, a)| {
            let n = name
                .strip_prefix(SEGMENT)?
                .strip_prefix(".plane_")?
                .parse()
                .ok()?;
            Some((n, a.clone()))
        })
        .collect();
    planes.sort_by_key(|(n, _)| *n);
    planes.into_iter().map(|(_, a)| a).collect()
}

/// `recipe` with its `grow.px` doubled (as the run resolved it, or the op's default), and the
/// new value.
pub fn widened(recipe: &Recipe, registry: &OpRegistry) -> Result<(Recipe, f64)> {
    let mut out = recipe.clone();
    let step = out
        .step
        .iter_mut()
        .find(|s| s.id == GROW)
        .ok_or_else(|| Error::op("heal", "the recipe has no grow step to widen"))?;
    let px = match step.params.get("px").and_then(number) {
        Some(px) => px,
        None => registry
            .require(&step.op)?
            .manifest()
            .params
            .iter()
            .find(|p| p.name == "px")
            .and_then(|p| p.default.as_ref())
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| Error::op("heal", format!("{} declares no px", step.op)))?,
    };
    let doubled = 2.0 * px;
    step.params.insert("px".into(), toml::Value::Float(doubled));
    Ok((out, doubled))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_is_padded_outward_on_each_axis_and_clamped() {
        // 240x40 at (120, 160), 5%: 12 px across, 2 px down and up.
        assert_eq!(
            padded([120, 160, 360, 200], 0.05, 480, 320),
            [108, 158, 372, 202]
        );
        // A fraction of a pixel snaps outward.
        assert_eq!(padded([10, 10, 13, 13], 0.1, 100, 100), [9, 9, 14, 14]);
        // At the edge: clamped.
        assert_eq!(padded([0, 300, 40, 320], 0.5, 480, 320), [0, 290, 60, 320]);
        assert_eq!(padded([5, 5, 9, 7], 0.0, 480, 320), [5, 5, 9, 7]);
    }

    #[test]
    fn the_support_is_the_box_of_the_set_pixels() {
        let mut plane = GrayImage::new(8, 6);
        assert_eq!(support(&plane), None);
        plane.put_pixel(2, 1, image::Luma([255]));
        plane.put_pixel(5, 4, image::Luma([1]));
        assert_eq!(support(&plane), Some([2, 1, 6, 5]));
    }
}
