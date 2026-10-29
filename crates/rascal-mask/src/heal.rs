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

/// A recorded fill, for replay.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StoredFill {
    request: String,
    fill: String,
}

pub fn heal(registry: &OpRegistry, args: &HealArgs) -> Result<HealReport> {
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

    // R: the recipe's declared outputs.
    let (run, outcome, input_image) = run::run_recipe(registry, &args.run)?;
    if outcome.status != Status::Ok {
        return Ok(HealReport {
            status: outcome.status,
            stopped_at: outcome.stopped_at,
            reason: outcome.reason,
            result: None,
            dash: None,
            renders: outcome.renders,
        });
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
                    return Ok(HealReport {
                        status,
                        stopped_at: Some("window".into()),
                        reason: out.envelope.reason,
                        result: None,
                        dash: None,
                        renders: outcome.renders,
                    });
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
    let fill = match args
        .replay
        .as_deref()
        .map(|dir| recorded_fill(dir, &request_identity))
        .transpose()?
        .flatten()
    {
        Some(pixels) => {
            run.note(format!(
                "filler call replayed from the recorded fill for request {request_identity}; the filler was not called"
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
            let result = span.in_scope(|| filler.call(&request, &scratch));
            let _ = std::fs::remove_dir_all(&scratch);
            match result {
                Ok((pixels, call)) => {
                    note_call(&run, &call)?;
                    pixels
                }
                Err(e) => {
                    let call = FillerCall {
                        filler: args.filler.clone(),
                        target: entry
                            .url
                            .clone()
                            .or_else(|| entry.command.as_ref().map(|c| c.join(" ")))
                            .unwrap_or_default(),
                        request_width: request.width,
                        request_height: request.height,
                        request_bytes: request.bytes(),
                        response_bytes: 0,
                        duration_ms: 0,
                        status: -1,
                        outcome: e.to_string(),
                    };
                    note_call(&run, &call)?;
                    return Err(e);
                }
            }
        }
    };
    let fill_image = Image::new("fill", fill, &frame_c, Role::Fill, None);
    let (_, fill_artifact) = run.workspace.add_external(
        fill_image,
        &frame_c,
        "filler",
        serde_json::json!({ "filler": args.filler, "profile": entry.profile, "request": request_identity }),
    )?;
    run.note(format!(
        "{FILL_NOTE}{}",
        serde_json::to_string(&StoredFill {
            request: request_identity.clone(),
            fill: fill_artifact.identity().as_str().to_string(),
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
                return Ok(HealReport {
                    status,
                    stopped_at: Some("paste".into()),
                    reason: out.envelope.reason,
                    result: None,
                    dash: None,
                    renders: outcome.renders,
                });
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

/// The fill a previous run stored for this request, when its journal has one.
fn recorded_fill(dir: &Path, request: &str) -> Result<Option<DynamicImage>> {
    let workspace = rascal_core::workspace::Workspace::open(dir)?;
    let lines = rascal_core::journal::Journal::read(&workspace.journal_path())?;
    for line in lines {
        let Some(text) = line.note.as_deref().and_then(|n| n.strip_prefix(FILL_NOTE)) else {
            continue;
        };
        let Ok(stored) = serde_json::from_str::<StoredFill>(text) else {
            continue;
        };
        if stored.request == request
            && let Artifact::Image(image) = workspace.load(&stored.fill)?
        {
            return Ok(Some(image.pixels.clone()));
        }
    }
    Ok(None)
}

/// The mask's effective weight and protect mapped to the raw frame: a pixel of the mask's
/// frame lands on one raw pixel through the crop bbox and the cut plan, so a raw pixel the
/// mask never reaches has `w = 0`.
fn raw_weights(
    mask: &Mask,
    frames: &FrameTable,
    plan: Option<&rascal_core::frame::Plan>,
    raw: &Frame,
) -> Result<(Vec<f32>, Vec<bool>)> {
    let (rw, rh) = (raw.width as usize, raw.height as usize);
    let mut weights = vec![0.0f32; rw * rh];
    let mut protected = vec![false; rw * rh];
    let mut frame = frames.require(&mask.record.frame)?.clone();
    // The chain of offsets from the mask's frame up to the raw frame.
    let mut crops: Vec<[u32; 4]> = Vec::new();
    let mut through_cut = false;
    while frame.kind != FrameKind::Raw {
        match frame.kind {
            FrameKind::Crop => crops.push(frame.bbox.unwrap_or([0, 0, 0, 0])),
            FrameKind::Cut => through_cut = true,
            FrameKind::Raw => {}
        }
        let parent = frame
            .parent
            .clone()
            .ok_or_else(|| Error::op("heal", "a derived frame without a parent"))?;
        frame = frames.require(&parent)?.clone();
    }
    let strip = plan.map(|p| p.strip).unwrap_or(0);
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
            let i = py as usize * rw + px as usize;
            weights[i] = mask.weight(x, y);
            protected[i] = mask.protect.get_pixel(x, y)[0] != 0;
        }
    }
    Ok((weights, protected))
}

fn preservation_check(
    input: &Image,
    out: &Image,
    mask: &Mask,
    frames: &FrameTable,
    plan: Option<&rascal_core::frame::Plan>,
) -> Result<()> {
    let raw = frames.require(&input.record.frame)?;
    let (weights, protected) = raw_weights(mask, frames, plan, raw)?;
    let before = input.pixels.to_rgb8();
    let after = out.pixels.to_rgb8();
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
        if (weights[i] <= 0.0 || protected[i]) && p != before.get_pixel(x, y) {
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
