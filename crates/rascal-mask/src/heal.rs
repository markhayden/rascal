//! `rascal heal <recipe> <image> --filler <name> --config rascal.toml --out <dir>`: the fixed
//! typed graph of SPEC.md "Recipes" ("heal"), executed literally over the recipe's declared
//! outputs: apply, window, render, the filler call, paste, unwindow, put, the preservation
//! check and the measures. Every step is an op invocation in the journal; the filler call is
//! a journal note without its key.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::DynamicImage;
use rascal_core::Error;
use rascal_core::artifact::Artifact;
use rascal_core::config::{Config, FillerConfig};
use rascal_core::error::Result;
use rascal_core::frame::{Frame, FrameKind, FrameTable};
use rascal_core::image::{Image, Role};
use rascal_core::mask::Mask;
use rascal_core::op::{Inputs, Params, Status};
use rascal_core::recipe::Run;
use rascal_core::registry::OpRegistry;
use rascal_core::rendering::Profile;
use rascal_mask::filler::{self, FillRequest, FillerCall};

use crate::run::{self, RunArgs};

/// What `heal` was asked to do: a run plus the filler.
pub struct HealArgs {
    pub run: RunArgs,
    pub filler: String,
    pub no_dash: bool,
    /// A previous run whose recorded filler output is reused when the request is the same,
    /// so a healed run replays without the filler (G4).
    pub replay: Option<PathBuf>,
}

/// What `heal` did.
pub struct HealReport {
    pub status: Status,
    pub stopped_at: Option<String>,
    pub reason: Option<String>,
    pub result: Option<PathBuf>,
    pub dash: Option<PathBuf>,
    pub renders: Vec<(String, PathBuf)>,
}

/// The journal note prefix of a reusable fill: the request identity and the fill artifact.
const FILL_NOTE: &str = "fill stored: ";

/// A recorded fill, for replay: the request's identity, the fill's, and the filler that made
/// it (name and revision), so a replay never passes one filler's pixels off as another's.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StoredFill {
    request: String,
    fill: String,
    filler: String,
    #[serde(default)]
    revision: Option<String>,
}

pub fn heal(args: &HealArgs) -> Result<HealReport> {
    let config_path = args
        .run
        .config
        .as_ref()
        .ok_or_else(|| Error::usage("heal needs --config <rascal.toml> naming the filler"))?;
    let config = Config::read(config_path)?;
    let entry: &FillerConfig = config.fillers.get(&args.filler).ok_or_else(|| {
        Error::usage(format!(
            "{}: no [fillers.{}]; the fillers are {}",
            config_path.display(),
            args.filler,
            config
                .fillers
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })?;
    let filler = filler::from_config(&args.filler, entry)?;
    let profile = Profile::parse(&entry.profile).ok_or_else(|| {
        Error::usage(format!(
            "fillers.{}: profile {}",
            args.filler, entry.profile
        ))
    })?;
    let role = profile.role().ok_or_else(|| {
        Error::usage(format!(
            "fillers.{}: {} is not a filler profile",
            args.filler, entry.profile
        ))
    })?;

    // R: the recipe's declared outputs, in the registry the run built (packs included).
    let run::Prepared {
        run,
        outcome,
        image: input_image,
        registry,
        packs: _packs,
    } = run::run_recipe(&args.run)?;
    let registry = &registry;
    if outcome.status != Status::Ok {
        return stopped(
            &run,
            args,
            outcome.status,
            outcome.stopped_at,
            outcome.reason,
            outcome.renders,
        );
    }
    let mask = match outcome.outputs.get("mask") {
        Some(Artifact::Mask(m)) => m.clone(),
        _ => return Err(Error::usage("heal needs the recipe's outputs.mask")),
    };
    let image_f = match outcome.outputs.get("image") {
        Some(Artifact::Image(i)) => i.clone(),
        _ => return Err(Error::usage("heal needs the recipe's outputs.image")),
    };
    let frames = run.frames()?;
    let frame_f = frames.require(&mask.record.frame)?.clone();
    let plan = match (frame_f.kind, outcome.outputs.get("plan")) {
        (FrameKind::Cut, Some(Artifact::Plan(p))) => Some(p.clone()),
        (FrameKind::Cut, _) => {
            return Err(Error::usage(
                "heal needs the recipe's outputs.plan when the mask's frame is a cut:",
            ));
        }
        _ => None,
    };
    let input_b = match &input_image {
        Artifact::Image(i) => i.clone(),
        _ => unreachable!("bound by run"),
    };

    // W = apply(I_F, M): the working image; a mask without a prescription leaves it as is.
    let working = {
        let mut inputs: Inputs = BTreeMap::new();
        inputs.insert("image".into(), Artifact::Image(image_f.clone()));
        inputs.insert("mask".into(), Artifact::Mask(mask.clone()));
        let out = run.invoke("apply", "apply", registry, &inputs, &Params::new())?;
        match (out.envelope.status, out.outputs.get("image")) {
            (Status::Ok, Some(Artifact::Image(w))) => w.clone(),
            (Status::NotApplicable, _) => image_f.clone(),
            (status, _) => {
                return Err(Error::op(
                    "heal",
                    format!("apply: {status:?}: {}", out.envelope.summary),
                ));
            }
        }
    };

    // (Wc, Mc) = window(W, M, max_side): the crop frame, or the identity.
    let (working_c, mask_c, crop_frame) = match entry.max_side {
        None => (working.clone(), mask.clone(), None),
        Some(max_side) => {
            let mut inputs: Inputs = BTreeMap::new();
            inputs.insert("image".into(), Artifact::Image(working.clone()));
            inputs.insert("mask".into(), Artifact::Mask(mask.clone()));
            let mut params = Params::new();
            params.insert("max_side".into(), toml::Value::Integer(i64::from(max_side)));
            let out = run.invoke("window", "window", registry, &inputs, &params)?;
            match out.envelope.status {
                Status::Ok => {}
                status => {
                    return stopped(
                        &run,
                        args,
                        status,
                        Some("window".into()),
                        out.envelope.reason,
                        outcome.renders,
                    );
                }
            }
            let (Some(Artifact::Image(wc)), Some(Artifact::Mask(mc))) =
                (out.outputs.get("image"), out.outputs.get("mask"))
            else {
                return Err(Error::op("heal", "window produced no image and mask"));
            };
            (wc.clone(), mc.clone(), out.frames.first().cloned())
        }
    };

    // Rend = render(Mc [, Wc], profile), and the grey rendering when the filler also wants it.
    let rendering = render(registry, &run, &mask_c, &working_c, profile)?;
    let grey = if entry.send_mask && profile != Profile::Grey {
        Some(render(registry, &run, &mask_c, &working_c, Profile::Grey)?)
    } else {
        None
    };

    // Fill = filler(...): the request as the matrix says; a recorded fill replays it.
    let request = FillRequest::build(
        role,
        entry.send_mask,
        &working_c,
        &rendering,
        grey.as_deref(),
        args.run.prompt.as_deref(),
    )?;
    let request_identity = request_identity(&request);
    let frame_c = crop_frame.clone().unwrap_or_else(|| frame_f.clone());
    let wanted = StoredFill {
        request: request_identity.clone(),
        fill: String::new(),
        filler: args.filler.clone(),
        revision: entry.revision.clone(),
    };
    let mut origin = serde_json::json!({ "filler": args.filler, "profile": entry.profile, "request": request_identity });
    let fill = match args
        .replay
        .as_deref()
        .map(|dir| recorded_fill(dir, &wanted))
        .transpose()?
        .flatten()
    {
        Some((pixels, stored)) => {
            if (pixels.width(), pixels.height()) != (request.width, request.height) {
                return Err(Error::op(
                    "heal",
                    format!(
                        "the recorded fill is {}x{} but the request is {}x{}",
                        pixels.width(),
                        pixels.height(),
                        request.width,
                        request.height
                    ),
                ));
            }
            origin["replayed_from"] =
                serde_json::json!(args.replay.as_deref().map(|d| d.display().to_string()));
            origin["recorded_fill"] = serde_json::json!(stored.fill);
            run.note(format!(
                "filler call replayed from the recorded fill {} of fillers.{} for request {request_identity}; the filler was not called",
                stored.fill, stored.filler
            ))?;
            pixels
        }
        None => {
            let scratch = run.dir.join("filler");
            std::fs::create_dir_all(&scratch).map_err(|e| Error::io(&scratch, e))?;
            let span = tracing::info_span!(
                "filler",
                filler = %args.filler,
                request_bytes = request.bytes(),
                width = request.width,
                height = request.height
            );
            let outcome = span.in_scope(|| filler.call(&request, &scratch));
            let _ = std::fs::remove_dir_all(&scratch);
            note_call(&run, &outcome.call)?;
            outcome.fill?
        }
    };
    let fill_image = Image::new("fill", fill, &frame_c, Role::Fill, None);
    let (_, fill_artifact) = run
        .workspace
        .add_external(fill_image, &frame_c, "filler", origin)?;
    run.note(format!(
        "{FILL_NOTE}{}",
        serde_json::to_string(&StoredFill {
            fill: fill_artifact.identity().as_str().to_string(),
            ..wanted
        })
        .unwrap_or_default()
    ))?;

    // P = paste(Wc, Fill, Mc, mode).
    let pasted = {
        let mut inputs: Inputs = BTreeMap::new();
        inputs.insert("base".into(), Artifact::Image(working_c.clone()));
        inputs.insert("fill".into(), fill_artifact.clone());
        inputs.insert("mask".into(), Artifact::Mask(mask_c.clone()));
        let mut params = Params::new();
        params.insert("mode".into(), toml::Value::String(entry.paste.clone()));
        let out = run.invoke("paste", "paste", registry, &inputs, &params)?;
        match (out.envelope.status, out.outputs.get("image")) {
            (Status::Ok, Some(Artifact::Image(p))) => p.clone(),
            (status, _) => {
                return stopped(
                    &run,
                    args,
                    status,
                    Some("paste".into()),
                    out.envelope.reason,
                    outcome.renders,
                );
            }
        }
    };

    // W' = unwindow(W, P) when windowed.
    let composite_f = match &crop_frame {
        None => pasted,
        Some(_) => {
            let mut inputs: Inputs = BTreeMap::new();
            inputs.insert("image".into(), Artifact::Image(working.clone()));
            inputs.insert("crop".into(), Artifact::Image(pasted));
            let out = run.invoke("unwindow", "unwindow", registry, &inputs, &Params::new())?;
            match (out.envelope.status, out.outputs.get("image")) {
                (Status::Ok, Some(Artifact::Image(w))) => w.clone(),
                (status, _) => {
                    return Err(Error::op(
                        "heal",
                        format!("unwindow: {status:?}: {}", out.envelope.summary),
                    ));
                }
            }
        }
    };

    // Out = put(inputs.image, W', plan) for a cut frame, else W'.
    let out_image = match &plan {
        None => composite_f.clone(),
        Some(plan) => {
            let mut inputs: Inputs = BTreeMap::new();
            inputs.insert("image".into(), Artifact::Image(input_b.clone()));
            inputs.insert("canvas".into(), Artifact::Image(composite_f.clone()));
            inputs.insert("plan".into(), Artifact::Plan(plan.clone()));
            let out = run.invoke("put", "put", registry, &inputs, &Params::new())?;
            match (out.envelope.status, out.outputs.get("image")) {
                (Status::Ok, Some(Artifact::Image(o))) => o.clone(),
                (status, _) => {
                    return Err(Error::op(
                        "heal",
                        format!("put: {status:?}: {}", out.envelope.summary),
                    ));
                }
            }
        }
    };

    // The preservation check in raw coordinates: every raw pixel whose mapped w = 0, and
    // every protected pixel, equals this invocation's input image.
    preservation_check(&input_b, &out_image, &mask, &frames, plan.as_deref())?;

    // The measures in F: the band set and drift with before = I_F and after = W'.
    {
        let mut inputs: Inputs = BTreeMap::new();
        inputs.insert("before".into(), Artifact::Image(image_f.clone()));
        inputs.insert("after".into(), Artifact::Image(composite_f.clone()));
        inputs.insert("mask".into(), Artifact::Mask(mask.clone()));
        let _ = run.invoke("measure", "measure", registry, &inputs, &Params::new())?;
    }

    let result = run.dir.join("result.png");
    rascal_core::canonical::write_png(&result, &out_image.pixels)?;
    run.write_tables()?;
    let dash = if args.no_dash {
        None
    } else {
        Some(crate::dash::write(&run.dir, &run.dir.join("dash.html"))?)
    };
    Ok(HealReport {
        status: Status::Ok,
        stopped_at: None,
        reason: None,
        result: Some(result),
        dash,
        renders: outcome.renders,
    })
}

/// A heal that stopped at a step that was not ok: nothing healed, but the tables and the run
/// page are written as for a complete run.
fn stopped(
    run: &Run,
    args: &HealArgs,
    status: Status,
    stopped_at: Option<String>,
    reason: Option<String>,
    renders: Vec<(String, PathBuf)>,
) -> Result<HealReport> {
    run.write_tables()?;
    let dash = if args.no_dash {
        None
    } else {
        Some(crate::dash::write(&run.dir, &run.dir.join("dash.html"))?)
    };
    Ok(HealReport {
        status,
        stopped_at,
        reason,
        result: None,
        dash,
        renders,
    })
}

fn render(
    registry: &OpRegistry,
    run: &Run,
    mask: &Arc<Mask>,
    image: &Arc<Image>,
    profile: Profile,
) -> Result<Arc<Image>> {
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("mask".into(), Artifact::Mask(mask.clone()));
    if profile.needs_image() {
        inputs.insert("image".into(), Artifact::Image(image.clone()));
    }
    let mut params = Params::new();
    params.insert(
        "for".into(),
        toml::Value::String(profile.name().to_string()),
    );
    let out = run.invoke(
        &format!("render.{}", profile.name()),
        "render",
        registry,
        &inputs,
        &params,
    )?;
    match (out.envelope.status, out.outputs.get("image")) {
        (Status::Ok, Some(Artifact::Image(r))) => Ok(r.clone()),
        (status, _) => Err(Error::op(
            "heal",
            format!(
                "render {}: {status:?}: {}",
                profile.name(),
                out.envelope.summary
            ),
        )),
    }
}

fn note_call(run: &Run, call: &FillerCall) -> Result<()> {
    tracing::debug!(filler = %call.filler, status = call.status, duration_ms = call.duration_ms, response_bytes = call.response_bytes, outcome = %call.outcome, "filler call finished");
    run.note(call.note())
}

/// `sha256:` over the request's bytes, so a replay recognises the same request.
fn request_identity(request: &FillRequest) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(&request.image_png);
    if let Some(mask) = &request.mask_png {
        hasher.update(b"mask");
        hasher.update(mask);
    }
    if let Some(prompt) = &request.prompt {
        hasher.update(b"prompt");
        hasher.update(prompt.as_bytes());
    }
    let hex: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("sha256:{hex}")
}

/// The fill a previous run stored for this request by the same filler and revision, when
/// its journal has one. `dir` is the directory holding that run's journal: the run directory,
/// or the workspace when the run used `--workspace`.
fn recorded_fill(dir: &Path, wanted: &StoredFill) -> Result<Option<(DynamicImage, StoredFill)>> {
    if !dir.join("journal.jsonl").is_file() {
        return Err(Error::usage(format!(
            "--replay {}: no journal there; give the run directory, or the workspace when the run used --workspace",
            dir.display()
        )));
    }
    let workspace = rascal_core::workspace::Workspace::open(dir)?;
    let lines = rascal_core::journal::Journal::read(&workspace.journal_path())?;
    for line in lines {
        let Some(text) = line.note.as_deref().and_then(|n| n.strip_prefix(FILL_NOTE)) else {
            continue;
        };
        let Ok(stored) = serde_json::from_str::<StoredFill>(text) else {
            continue;
        };
        if stored.request == wanted.request
            && stored.filler == wanted.filler
            && stored.revision == wanted.revision
            && let Artifact::Image(image) = workspace.load(&stored.fill)?
        {
            return Ok(Some((image.pixels.clone(), stored)));
        }
    }
    Ok(None)
}

/// Which raw pixels the mask reaches (`w > 0`) and which it protects, mapped through the
/// crop bboxes and the cut plan: a pixel of the mask's frame lands on one raw pixel, so a raw
/// pixel the mask never reaches has `w = 0`. One byte per raw pixel per plane.
pub(crate) fn raw_reach(
    mask: &Mask,
    frames: &FrameTable,
    plan: Option<&rascal_core::frame::Plan>,
    raw: &Frame,
) -> Result<(Vec<bool>, Vec<bool>)> {
    let (rw, rh) = (raw.width as usize, raw.height as usize);
    let mut reached = vec![false; rw * rh];
    let mut protected = vec![false; rw * rh];
    let mut frame = frames.require(&mask.record.frame)?.clone();
    // The chain of offsets from the mask's frame up to the raw frame.
    let mut crops: Vec<[u32; 4]> = Vec::new();
    let mut through_cut = false;
    while frame.kind != FrameKind::Raw {
        match frame.kind {
            FrameKind::Crop => crops.push(frame.bbox.ok_or_else(|| {
                Error::op("heal", format!("crop frame {} has no bbox", frame.id))
            })?),
            FrameKind::Cut => through_cut = true,
            FrameKind::Raw => {}
        }
        let parent = frame
            .parent
            .clone()
            .ok_or_else(|| Error::op("heal", "a derived frame without a parent"))?;
        frame = frames.require(&parent)?.clone();
    }
    if frame.id != raw.id {
        return Err(Error::op(
            "heal",
            format!(
                "the mask's frame descends from {}, not from the input's {}",
                frame.id, raw.id
            ),
        ));
    }
    let strip = match (through_cut, plan) {
        (true, Some(plan)) => plan.strip,
        (true, None) => {
            return Err(Error::op(
                "heal",
                "the mask is in a cut frame but no plan was given",
            ));
        }
        (false, _) => 0,
    };
    let (w, h) = mask.dimensions();
    for y in 0..h {
        for x in 0..w {
            let (mut px, mut py) = (x, y);
            for bbox in &crops {
                px += bbox[0];
                py += bbox[1];
            }
            if through_cut {
                // The canvas's left half is the parent's rightmost strip, its right half the
                // leftmost.
                px = if px < strip {
                    raw.width - strip + px
                } else {
                    px - strip
                };
            }
            if px >= raw.width || py >= raw.height {
                return Err(Error::op(
                    "heal",
                    format!(
                        "mask pixel {x},{y} maps outside the {}x{} raw frame",
                        raw.width, raw.height
                    ),
                ));
            }
            let i = py as usize * rw + px as usize;
            reached[i] = mask.weight(x, y) > 0.0;
            protected[i] = mask.protect.get_pixel(x, y)[0] != 0;
        }
    }
    Ok((reached, protected))
}

/// The preservation guarantee in raw coordinates: every raw pixel the mask never reaches and
/// every protected pixel equals the input, or the heal is an operational error naming the
/// first offender.
pub(crate) fn preservation_check(
    input: &Image,
    out: &Image,
    mask: &Mask,
    frames: &FrameTable,
    plan: Option<&rascal_core::frame::Plan>,
) -> Result<()> {
    let raw = frames.require(&input.record.frame)?;
    let (reached, protected) = raw_reach(mask, frames, plan, raw)?;
    // RGB views without a copy when the images already are RGB8.
    let before_owned;
    let before = match input.pixels.as_rgb8() {
        Some(rgb) => rgb,
        None => {
            before_owned = input.pixels.to_rgb8();
            &before_owned
        }
    };
    let after_owned;
    let after = match out.pixels.as_rgb8() {
        Some(rgb) => rgb,
        None => {
            after_owned = out.pixels.to_rgb8();
            &after_owned
        }
    };
    if before.dimensions() != after.dimensions() {
        return Err(Error::op(
            "heal",
            format!(
                "the result is {:?} but the input is {:?}",
                after.dimensions(),
                before.dimensions()
            ),
        ));
    }
    let w = raw.width as usize;
    let mut violations = 0u64;
    let mut first = None;
    for (x, y, p) in after.enumerate_pixels() {
        let i = y as usize * w + x as usize;
        if (!reached[i] || protected[i]) && p != before.get_pixel(x, y) {
            violations += 1;
            first.get_or_insert((x, y));
        }
    }
    if violations > 0 {
        let (x, y) = first.unwrap_or((0, 0));
        return Err(Error::op(
            "heal",
            format!(
                "preservation check failed: {violations} pixel(s) with w = 0 or protected changed, first at {x},{y}"
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use image::{GrayImage, Luma, Rgb, RgbImage};
    use rascal_core::frame::{Plan, Projection};
    use rascal_core::mask::{Source, UseCase};

    /// A raw 40x10 equirectangular frame, its cut frame (strip 4, canvas 8x10) and a crop of
    /// the canvas at a non-zero offset [2, 3, 7, 9): the mask lives in the crop.
    fn chain() -> (FrameTable, Frame, Plan, Mask, Image) {
        let raw = Frame::raw(40, 10, Projection::Equirectangular);
        let plan = Plan::new(&raw, 0.1).unwrap();
        assert_eq!(plan.strip, 4);
        let cut = Frame::cut(&raw, "abcdef012345", &plan).unwrap();
        let crop = Frame::crop(&cut, 1, [2, 3, 7, 9]).unwrap();
        let table = FrameTable {
            frames: vec![raw.clone(), cut, crop.clone()],
        };
        let mut core = GrayImage::new(5, 6);
        core.put_pixel(1, 1, Luma([255]));
        core.put_pixel(2, 2, Luma([255]));
        let mut mask = Mask::from_core(
            "m",
            &crop,
            UseCase::Seam,
            core,
            Source {
                locations: vec![],
                instances: vec![],
            },
        )
        .unwrap();
        let mut protect = GrayImage::new(5, 6);
        protect.put_pixel(4, 5, Luma([255]));
        mask.protect_union(&protect).unwrap();
        let input = Image::new(
            "in",
            DynamicImage::ImageRgb8(RgbImage::from_fn(40, 10, |x, y| Rgb([x as u8, y as u8, 7]))),
            &raw,
            Role::Canonical,
            None,
        );
        (table, raw, plan, mask, input)
    }

    /// The raw pixel a crop pixel lands on: crop offset, then the cut's two strips.
    fn raw_of(x: u32, y: u32) -> (u32, u32) {
        let (cx, cy) = (x + 2, y + 3);
        (if cx < 4 { 40 - 4 + cx } else { cx - 4 }, cy)
    }

    fn with_pixel(input: &Image, raw: &Frame, at: (u32, u32)) -> Image {
        let mut rgb = input.pixels.to_rgb8();
        rgb.put_pixel(at.0, at.1, Rgb([200, 200, 200]));
        Image::new(
            "out",
            DynamicImage::ImageRgb8(rgb),
            raw,
            Role::Composite,
            None,
        )
    }

    #[test]
    fn the_reach_maps_through_the_crop_offset_and_the_cut_strips() {
        let (table, raw, plan, mask, _) = chain();
        let (reached, protected) = raw_reach(&mask, &table, Some(&plan), &raw).unwrap();
        let i = |(x, y): (u32, u32)| (y * 40 + x) as usize;
        assert!(reached[i(raw_of(1, 1))], "core pixel (1,1) of the crop");
        assert!(reached[i(raw_of(2, 2))]);
        assert!(!reached[i(raw_of(0, 0))], "a crop pixel outside the core");
        assert!(protected[i(raw_of(4, 5))]);
        assert_eq!(reached.iter().filter(|r| **r).count(), 2);
        assert_eq!(protected.iter().filter(|p| **p).count(), 1);
        // (1,1) in the crop is (3,4) in the canvas: the left half, so the raw right strip.
        assert_eq!(raw_of(1, 1), (39, 4));
        // (2,2) is canvas (4,5): the right half, raw column 0.
        assert_eq!(raw_of(2, 2), (0, 5));
    }

    #[test]
    fn the_check_rejects_a_changed_unreached_or_protected_pixel_and_accepts_a_reached_one() {
        let (table, raw, plan, mask, input) = chain();
        // Unchanged: fine.
        preservation_check(&input, &input, &mask, &table, Some(&plan)).unwrap();
        // A changed reached pixel: fine.
        let ok = with_pixel(&input, &raw, raw_of(1, 1));
        preservation_check(&input, &ok, &mask, &table, Some(&plan)).unwrap();
        // A changed pixel the mask never reaches, in the middle of the panorama.
        let err = preservation_check(
            &input,
            &with_pixel(&input, &raw, (20, 5)),
            &mask,
            &table,
            Some(&plan),
        )
        .unwrap_err();
        assert!(err.to_string().contains("first at 20,5"), "{err}");
        // A changed pixel inside the crop but outside the core (w = 0).
        let err = preservation_check(
            &input,
            &with_pixel(&input, &raw, raw_of(0, 0)),
            &mask,
            &table,
            Some(&plan),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("preservation check failed"),
            "{err}"
        );
        // A changed protected pixel.
        let err = preservation_check(
            &input,
            &with_pixel(&input, &raw, raw_of(4, 5)),
            &mask,
            &table,
            Some(&plan),
        )
        .unwrap_err();
        assert!(err.to_string().contains("preservation"), "{err}");
        // A cut chain without its plan is an error, not a silent strip of zero.
        let err = raw_reach(&mask, &table, None, &raw).unwrap_err();
        assert!(err.to_string().contains("no plan"), "{err}");
    }
}
