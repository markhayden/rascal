//! `rascal heal <recipe> <image> --filler <name> --config rascal.toml --out <dir>`: the fixed
//! typed graph of SPEC.md "Recipes" ("heal"), executed literally over the recipe's declared
//! outputs: apply, window, render, the filler call, paste, unwindow, put, the preservation
//! check and the measures. Every step is an op invocation in the journal; the filler call is
//! a journal note without its key. A checked `remove_prompt` (SPEC.md "Self-correcting remove",
//! part 3) asks the configured checker about each try's result.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::DynamicImage;
use rascal_core::Error;
use rascal_core::artifact::Artifact;
use rascal_core::config::{CheckerConfig, Config, FillerConfig};
use rascal_core::contract::table_manners::Verdict;
use rascal_core::error::Result;
use rascal_core::frame::{Frame, FrameKind, FrameTable, Plan};
use rascal_core::image::{Image, RenderingRole, Role};
use rascal_core::mask::Mask;
use rascal_core::op::{Inputs, Params, Status};
use rascal_core::recipe::Run;
use rascal_core::registry::OpRegistry;
use rascal_core::rendering::Profile;
use rascal_mask::checker::{self, Check};
use rascal_mask::filler::{self, FillRequest, Filler, FillerCall};

use crate::run::{self, RunArgs};
use crate::run_json;

/// What `heal` was asked to do: a run plus the filler.
pub struct HealArgs {
    pub run: RunArgs,
    pub filler: String,
    pub no_dash: bool,
    /// A previous run whose recorded filler output is reused when the request is the same,
    /// so a healed run replays without the filler (G4).
    pub replay: Option<PathBuf>,
    /// The fill cache's directory, over the configuration's `[cache] dir` (the lab gives its
    /// own, so arms with the same request share one fill).
    pub cache: Option<PathBuf>,
    /// Whether the result is checked (`checker::resolve`, settled before the run), and why not.
    pub check: Check,
    /// The most tries the limits allow (`estimate::Price::tries`); 1 when unchecked.
    pub tries: u32,
}

/// What `heal` did.
pub struct HealReport {
    pub status: Status,
    pub stopped_at: Option<String>,
    pub reason: Option<String>,
    pub result: Option<PathBuf>,
    /// The source image's canonical PNG in the workspace, when the heal completed.
    pub source: Option<PathBuf>,
    pub dash: Option<PathBuf>,
    pub renders: Vec<(String, PathBuf)>,
    /// Whether the fill came from the cache: `Some(true)` a hit, `Some(false)` a miss (the
    /// filler was called and the fill stored), `None` no cache or no fill.
    pub cache: Option<bool>,
    /// The checker's verdict on the result (the kept try's); none when it was not checked or
    /// the check could not run.
    pub verdict: Option<Verdict>,
    /// A checked heal's tries, in order; empty when unchecked.
    pub tries: Vec<crate::run_json::TryOut>,
    /// The step a cancellation during the tries stopped before; `tries` are those completed
    /// before it.
    pub cancelled: Option<String>,
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
        recipe,
    } = run::run_recipe(&run::RunArgs {
        overrides: entry.overrides.clone(),
        overrides_from: Some(args.filler.clone()),
        ..args.run.clone()
    })?;
    let painter = Painter {
        name: &args.filler,
        entry,
        filler,
        profile,
        role,
        // A prompt the recipe bound (a finder's) is not the filler's.
        prompt: if run::declares_prompt(&recipe) {
            None
        } else {
            args.run.prompt.as_deref()
        },
        replay: args.replay.as_deref(),
        cache: args
            .cache
            .clone()
            .or_else(|| config.cache.as_ref().map(|c| c.dir.clone())),
    };
    // A configured checker that will not run says why (a missing key, `--no-check`).
    let checker = match (&args.check, &config.checker) {
        (Check::On, Some(checker)) => {
            run.note(format!(
                "Checked: up to {} {} within the spend limits.",
                args.tries,
                if args.tries == 1 { "try" } else { "tries" }
            ))?;
            Some(checker)
        }
        (Check::Off(off), _) => {
            if let Some(sentence) = off.sentence() {
                run.note(sentence)?;
            }
            None
        }
        (Check::On, None) => None,
    };
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
    let image = match outcome.outputs.get("image") {
        Some(Artifact::Image(i)) => i.clone(),
        _ => return Err(Error::usage("heal needs the recipe's outputs.image")),
    };
    let plan = match (
        run.frames()?.require(&mask.record.frame)?.kind,
        outcome.outputs.get("plan"),
    ) {
        (FrameKind::Cut, Some(Artifact::Plan(p))) => Some(p.clone()),
        (FrameKind::Cut, _) => {
            return Err(Error::usage(
                "heal needs the recipe's outputs.plan when the mask's frame is a cut:",
            ));
        }
        _ => None,
    };
    let input = match &input_image {
        Artifact::Image(i) => i.clone(),
        _ => unreachable!("bound by run"),
    };
    let subject = Subject {
        mask,
        image,
        plan,
        input,
    };

    // Where try 1's spend starts in the journal.
    let from = match checker {
        Some(_) => journal_notes(&run)?.len(),
        None => 0,
    };
    let (out_image, cache) = match heal_once(&run, &registry, &painter, &subject)? {
        Pass::Healed { image, cache } => (image, cache),
        Pass::Stopped {
            status,
            step,
            reason,
        } => return stopped(&run, args, status, Some(step), reason, outcome.renders),
    };
    let source = run.workspace.image_path(&subject.input);
    let (result_image, renders, verdict, tries) = match checker {
        None => (out_image, outcome.renders.clone(), None, Vec::new()),
        Some(checker) => {
            let prompt = args.run.prompt.as_deref().unwrap_or("");
            let checking = Checking {
                run: &run,
                registry: &registry,
                painter: &painter,
                recipe: &recipe,
                outcome: &outcome,
                checker,
                prompt,
                tries: args.tries,
                replay: args.replay.as_deref(),
            };
            let checked = checking.tries(subject, out_image, from)?;
            if let Some(step) = checked.cancelled {
                // The tries completed before the cancellation are the run's evidence.
                run.write_tables()?;
                return Ok(HealReport {
                    status: Status::Ok,
                    stopped_at: None,
                    reason: None,
                    result: None,
                    source: Some(source),
                    dash: None,
                    renders: Vec::new(),
                    cache,
                    verdict: None,
                    tries: try_outs(&run, &checked.tries, None),
                    cancelled: Some(step),
                });
            }
            let kept = &checked.tries[selected(&checked.tries)];
            run.record_output("mask", &kept.mask.record.identity)?;
            run.note(summary(kept, checked.tries.len(), prompt))?;
            let verdict = (!checked.failed).then(|| kept.verdict.clone()).flatten();
            let outs = try_outs(&run, &checked.tries, Some(kept.k));
            (kept.result.clone(), kept.renders.clone(), verdict, outs)
        }
    };
    let result = run.dir.join("result.png");
    rascal_core::canonical::write_png(&result, &result_image.pixels)?;
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
        source: Some(source),
        dash,
        renders,
        cache,
        verdict,
        tries,
        cancelled: None,
    })
}

/// The tries as `run.json` lists them, `kept` the one the run kept.
fn try_outs(run: &Run, tries: &[Try], kept: Option<u32>) -> Vec<run_json::TryOut> {
    tries
        .iter()
        .map(|t| run_json::TryOut {
            k: t.k,
            selected: Some(t.k) == kept,
            result: run
                .dir
                .join("tries")
                .join(t.k.to_string())
                .join("result.png"),
            mask: t.mask.record.identity.clone(),
            render: t
                .renders
                .iter()
                .find(|(profile, _)| profile == "grey")
                .map(|(_, path)| path.clone()),
            verdict: t.verdict.clone(),
            spend: t.spend,
        })
        .collect()
}

/// The run's sentence about the try it kept, of `made`: which, and when it did not pass, what
/// the check still sees.
fn summary(kept: &Try, made: usize, prompt: &str) -> String {
    use rascal_core::contract::table_manners::VerdictValue;
    let kept_line = format!("Kept try {} of {made}.", kept.k);
    match &kept.verdict {
        Some(v) if v.value != VerdictValue::Pass => {
            let places = match v.places.len() {
                0 => "nowhere in particular".to_string(),
                1 => "in 1 place".to_string(),
                n => format!("in {n} places"),
            };
            let value = if v.value == VerdictValue::Fail {
                "fail"
            } else {
                "unsure"
            };
            format!("{kept_line} The check still sees {prompt} {places} ({value}).")
        }
        _ => kept_line,
    }
}

/// What a checked heal works with: the run and its registry, the filler, the recipe and the
/// outcome of its first run (whose planes and inputs every try's mask is rebuilt from), the
/// checker, the prompt and the most tries the limits allow.
struct Checking<'a> {
    run: &'a Run,
    registry: &'a OpRegistry,
    painter: &'a Painter<'a>,
    recipe: &'a rascal_core::recipe::Recipe,
    outcome: &'a rascal_core::recipe::Outcome,
    checker: &'a CheckerConfig,
    prompt: &'a str,
    tries: u32,
    /// A recorded run whose checks answer instead of the checker.
    replay: Option<&'a Path>,
}

/// One try of a checked heal: its number, its mask, its result, the renders its shaping wrote,
/// the checker's verdict on it (none when the check could not run) and what it spent.
struct Try {
    k: u32,
    mask: Arc<Mask>,
    result: Arc<Image>,
    renders: Vec<(String, PathBuf)>,
    verdict: Option<Verdict>,
    spend: filler::Spent,
}

/// The tries a checked heal made, and whether its checking ended in a check failure.
struct Checked {
    tries: Vec<Try>,
    failed: bool,
    /// The step a cancellation stopped before: the tries made are kept as evidence.
    cancelled: Option<String>,
}

impl Checking<'_> {
    /// The tries (SPEC.md "Self-correcting remove", part 3): try `k`'s result is checked; on a
    /// `fail` or `unsure` with tries left, the places it names (or, with none, a wider grow)
    /// make the planes of try `k + 1`, whose mask is rebuilt from every plane through the
    /// recipe's shaping (`select` onward) and healed again in `tries/<k + 1>/`. A pass, the last
    /// try, a check failure, or a try that cannot be made or filled ends them.
    ///
    /// `from` is the journal's note count before try 1's fill: each try's spend is what the
    /// notes since the try before it record.
    /// A cancellation during the tries keeps those completed.
    fn tries(&self, first: Subject, result: Arc<Image>, from: usize) -> Result<Checked> {
        let mut tries = Vec::new();
        match self.run_tries(first, result, from, &mut tries) {
            Ok(failed) => Ok(Checked {
                tries,
                failed,
                cancelled: None,
            }),
            Err(Error::Cancelled { step }) => Ok(Checked {
                tries,
                failed: false,
                cancelled: Some(step),
            }),
            Err(e) => Err(e),
        }
    }

    /// [`Checking::tries`], each completed try pushed onto `tries`; whether a check failure
    /// ended them.
    fn run_tries(
        &self,
        first: Subject,
        result: Arc<Image>,
        mut from: usize,
        tries: &mut Vec<Try>,
    ) -> Result<bool> {
        use checker::call::Transition;
        use rascal_core::contract::table_manners::VerdictValue;
        use rascal_core::recipe::{Rebind, Recipe};
        let raw = self
            .run
            .frames()?
            .require(&first.input.record.frame)?
            .clone();
        let mut planes = crate::retry::first_planes(&self.outcome.bindings);
        let mut shaping: Recipe = self.recipe.clone();
        let mut widened = false;
        let mut current = Try {
            k: 1,
            mask: first.mask.clone(),
            result,
            renders: self.outcome.renders.clone(),
            verdict: None,
            spend: filler::Spent::default(),
        };
        let mut subject = first;
        for k in 1..=self.tries {
            let dir = self.run.dir.join("tries").join(k.to_string());
            let Some((verdict, mut stored)) = check_try(
                self.run,
                self.checker,
                &subject.input,
                self.prompt,
                k,
                &dir,
                &current.result,
                self.replay,
            )?
            else {
                tries.push(self.settled(current, &mut from)?);
                return Ok(true);
            };
            current.verdict = Some(verdict.clone());
            if verdict.value == VerdictValue::Pass || k == self.tries {
                self.run.note(stored.note())?;
                tries.push(self.settled(current, &mut from)?);
                break;
            }
            let next = k + 1;
            let next_run = Run::open_in(
                &self.run.dir.join("tries").join(next.to_string()),
                self.run.workspace.clone(),
            )?
            .with_cancel(self.run.cancel.clone());
            let mut transition = Transition::default();
            if verdict.places.is_empty() {
                if !widened {
                    let (wider, px) = crate::retry::widened(&shaping, self.registry)?;
                    shaping = wider;
                    widened = true;
                    transition.grow_px = Some(px);
                }
            } else {
                let candidate = rascal_core::contract::reference(
                    &current.result,
                    &raw,
                    stored.candidate.clone(),
                );
                let retry = crate::retry::Retry {
                    run: &next_run,
                    registry: self.registry,
                    recipe: &shaping,
                    texts: &self.outcome.texts,
                    input: &subject.input,
                    raw: &raw,
                };
                match retry.locate(next, &candidate, &verdict.places, &mut transition)? {
                    Ok(added) => planes.extend(added),
                    Err(reason) => {
                        stored.transition = Some(transition);
                        self.run.note(stored.note())?;
                        self.run
                            .note(format!("The check could not run: {reason}."))?;
                        tries.push(self.settled(current, &mut from)?);
                        return Ok(true);
                    }
                }
            }
            stored.transition = Some(transition);
            self.run.note(stored.note())?;
            tries.push(self.settled(current, &mut from)?);
            let rebuilt = shaping.run_from(
                self.registry,
                self.outcome,
                &Rebind {
                    step: crate::retry::SELECT.to_string(),
                    port: "plane".to_string(),
                    artifacts: planes.clone(),
                },
                &next_run,
            )?;
            let (Status::Ok, Some(Artifact::Mask(mask)), Some(Artifact::Image(image))) = (
                rebuilt.status,
                rebuilt.outputs.get("mask"),
                rebuilt.outputs.get("image"),
            ) else {
                self.run.note(format!(
                    "Try {next} could not be made: {}",
                    rebuilt
                        .reason
                        .unwrap_or_else(|| "its mask was not produced".into())
                ))?;
                break;
            };
            if tries
                .last()
                .is_some_and(|t: &Try| t.mask.record.identity == mask.record.identity)
            {
                self.run.note(format!(
                    "Try {next} would repeat try {k}: its mask is unchanged, so the tries stop."
                ))?;
                break;
            }
            subject = Subject {
                mask: mask.clone(),
                image: image.clone(),
                plan: None,
                input: subject.input,
            };
            match heal_once(&next_run, self.registry, self.painter, &subject)? {
                Pass::Healed { image, .. } => {
                    current = Try {
                        k: next,
                        mask: subject.mask.clone(),
                        result: image,
                        renders: rebuilt.renders,
                        verdict: None,
                        spend: filler::Spent::default(),
                    };
                }
                Pass::Stopped { step, reason, .. } => {
                    self.run.note(format!(
                        "Try {next} could not be filled: {step} stopped{}",
                        reason.map(|r| format!(": {r}")).unwrap_or_default()
                    ))?;
                    break;
                }
            }
        }
        Ok(false)
    }
}

impl Checking<'_> {
    /// `kept` with its spend: what the journal's notes since `from` record; `from` moves on.
    fn settled(&self, mut kept: Try, from: &mut usize) -> Result<Try> {
        let notes = journal_notes(self.run)?;
        kept.spend = filler::spent(notes.get(*from..).unwrap_or_default());
        *from = notes.len();
        Ok(kept)
    }
}

/// The journal's notes so far, in order.
fn journal_notes(run: &Run) -> Result<Vec<String>> {
    Ok(
        rascal_core::journal::Journal::read(&run.workspace.journal_path())?
            .into_iter()
            .filter_map(|line| line.note)
            .collect(),
    )
}

/// The try a checked heal keeps (SPEC.md "Self-correcting remove", part 3): the first `pass`;
/// failing that `unsure` over `fail` (and both over a try whose check could not run), then
/// fewer places, then the earlier try. An index into `tries`.
fn selected(tries: &[Try]) -> usize {
    use rascal_core::contract::table_manners::VerdictValue;
    let rank = |t: &Try| match &t.verdict {
        Some(v) => (
            match v.value {
                VerdictValue::Pass => 0,
                VerdictValue::Unsure => 1,
                VerdictValue::Fail => 2,
            },
            v.places.len(),
            t.k,
        ),
        None => (3, 0, t.k),
    };
    tries
        .iter()
        .enumerate()
        .min_by_key(|(_, t)| rank(t))
        .map_or(0, |(i, _)| i)
}

/// Asks the checker about try `k`'s result: the result is written as `result.png` in the try's
/// directory `dir`, the check staged in `dir/check/` and called, and the call journaled. The
/// verdict with its record (journaled by the caller once the try's transition is known), or
/// none when the check could not run, which the journal says ("The check could not run: …").
#[allow(clippy::too_many_arguments)]
fn check_try(
    run: &Run,
    config: &CheckerConfig,
    input: &Image,
    prompt: &str,
    k: u32,
    dir: &Path,
    result: &Image,
    replay: Option<&Path>,
) -> Result<Option<(Verdict, checker::call::StoredCheck)>> {
    use checker::call::{self, CallStarted, Request, StoredCheck};
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let candidate = dir.join("result.png");
    rascal_core::canonical::write_png(&candidate, &result.pixels)?;
    let staged = call::stage(
        config,
        &Request {
            source: &run.workspace.image_path(input),
            candidate: &candidate,
            prompt,
        },
        &dir.join("check"),
    )?;
    if let Some(recording) = replay {
        let stored = call::recorded(recording, &staged.identity, k)?.ok_or_else(|| {
            Error::usage(format!(
                "--replay {}: no recorded check of request {} (try {k}); under --replay the checker is never called",
                recording.display(),
                staged.identity
            ))
        })?;
        run.note(format!(
            "checker call replayed from the recorded check {} of try {}; the checker was not called",
            stored.request, stored.attempt
        ))?;
        let verdict = stored.verdict.clone();
        return Ok(Some((
            verdict,
            StoredCheck {
                attempt: k,
                arguments: staged.arguments,
                transition: None,
                ..stored
            },
        )));
    }
    if run.cancelled() {
        return Err(Error::Cancelled {
            step: "checker".to_string(),
        });
    }
    run.note(
        CallStarted {
            spend_usd: config.spend_usd_per_call,
        }
        .note(),
    )?;
    let called = tracing::info_span!("checker", attempt = k)
        .in_scope(|| call::call(config, &staged, run.cancel.as_deref()));
    run.note(called.call.note())?;
    if called.cancelled {
        return Err(Error::Cancelled {
            step: "checker".to_string(),
        });
    }
    match called.answer {
        Ok(verdict) => {
            let stored = StoredCheck {
                attempt: k,
                request: staged.identity,
                revision: config.revision.clone(),
                source: staged.source,
                candidate: staged.candidate,
                arguments: staged.arguments,
                verdict: verdict.clone(),
                transition: None,
            };
            Ok(Some((verdict, stored)))
        }
        Err(reason) => {
            run.note(format!("The check could not run: {reason}."))?;
            Ok(None)
        }
    }
}

/// The filler a heal paints with, read from the configuration: what every pass of the graph
/// renders, requests and pastes with.
struct Painter<'a> {
    /// The entry's name, `fillers.<name>`.
    name: &'a str,
    entry: &'a FillerConfig,
    filler: Box<dyn Filler>,
    profile: Profile,
    role: RenderingRole,
    /// The prompt the filler is sent: `--prompt`, unless the recipe bound it.
    prompt: Option<&'a str>,
    /// A previous run whose recorded fill answers the same request.
    replay: Option<&'a Path>,
    /// The fill cache's directory, when there is one.
    cache: Option<PathBuf>,
}

/// What one pass of the graph heals: the mask `M` and the image `I_F` in the mask's frame `F`,
/// the plan when `F` is a cut, and this invocation's input image `B` (raw).
struct Subject {
    mask: Arc<Mask>,
    image: Arc<Image>,
    plan: Option<Arc<Plan>>,
    input: Arc<Image>,
}

/// How one pass of the graph ended.
enum Pass {
    /// The result in raw, and whether its fill came from the cache (as [`HealReport::cache`]).
    Healed {
        image: Arc<Image>,
        cache: Option<bool>,
    },
    /// A step that was not ok, and why: nothing was healed.
    Stopped {
        status: Status,
        step: String,
        reason: Option<String>,
    },
}

/// One pass of `heal`'s graph (SPEC.md "Recipes"), from a mask to a result: apply, window,
/// render, the filler call, paste, unwindow, put, the preservation check and the measures, each
/// an op invocation (the filler call a journal note) in the run's workspace.
fn heal_once(
    run: &Run,
    registry: &OpRegistry,
    painter: &Painter,
    subject: &Subject,
) -> Result<Pass> {
    let Subject {
        mask,
        image: image_f,
        plan,
        input: input_b,
    } = subject;
    let entry = painter.entry;
    let frames = run.frames()?;
    let frame_f = frames.require(&mask.record.frame)?.clone();

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
            if out.envelope.status != Status::Ok {
                return Ok(Pass::Stopped {
                    status: out.envelope.status,
                    step: "window".into(),
                    reason: out.envelope.reason,
                });
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
    let profile = painter.profile;
    let rendering = render(registry, run, &mask_c, &working_c, profile)?;
    let grey = if entry.send_mask && profile != Profile::Grey {
        Some(render(registry, run, &mask_c, &working_c, Profile::Grey)?)
    } else {
        None
    };

    // Fill = filler(...): the request as the matrix says; a recorded fill replays it.
    let request = FillRequest::build(
        painter.role,
        entry.send_mask,
        &working_c,
        &rendering,
        grey.as_deref(),
        painter.prompt,
    )?;
    // A filler with fill_max_side is sent a downscaled request and its fill is scaled back.
    let (request, scaled_from) = match entry
        .fill_max_side
        .map(|side| request.scaled(side))
        .transpose()?
        .flatten()
    {
        Some((scaled, from)) => {
            run.note(format!(
                "fill request scaled to {}x{} from {}x{} (fillers.{} fill_max_side = {}); the fill is scaled back to the support",
                scaled.width,
                scaled.height,
                from.0,
                from.1,
                painter.name,
                entry.fill_max_side.unwrap_or_default()
            ))?;
            (scaled, Some(from))
        }
        None => (request, None),
    };
    let request_identity = request_identity(&request);
    let frame_c = crop_frame.clone().unwrap_or_else(|| frame_f.clone());
    let wanted = StoredFill {
        request: request_identity.clone(),
        fill: String::new(),
        filler: painter.name.to_string(),
        revision: entry.revision.clone(),
    };
    let mut cache_use: Option<bool> = None;
    let mut origin = serde_json::json!({ "filler": painter.name, "profile": entry.profile, "request": request_identity, "scaled_from": scaled_from.map(|(w, h)| format!("{w}x{h}")) });
    let fill = match painter
        .replay
        .map(|dir| recorded_fill(dir, &wanted))
        .transpose()?
        .flatten()
    {
        Some((pixels, stored)) => {
            // A recorded fill is the request's size, or the support's when the request was
            // scaled (the stored fill is the scaled-back one).
            let size = (pixels.width(), pixels.height());
            if size != (request.width, request.height) && scaled_from != Some(size) {
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
                serde_json::json!(painter.replay.map(|d| d.display().to_string()));
            origin["recorded_fill"] = serde_json::json!(stored.fill);
            run.note(format!(
                "filler call replayed from the recorded fill {} of fillers.{} for request {request_identity}; the filler was not called",
                stored.fill, stored.filler
            ))?;
            pixels
        }
        None => match painter
            .cache
            .as_deref()
            .map(|dir| cached_fill(dir, &wanted))
            .transpose()?
            .flatten()
        {
            Some((pixels, key)) => {
                // A cached fill is the request's size: anything else is refused, not resized
                // to the support under a scaled window or handed to paste to refuse.
                if (pixels.width(), pixels.height()) != (request.width, request.height) {
                    return Err(Error::op(
                        "heal",
                        format!(
                            "the cached fill is {}x{} but the request is {}x{} ({key} in the fill cache)",
                            pixels.width(),
                            pixels.height(),
                            request.width,
                            request.height
                        ),
                    ));
                }
                cache_use = Some(true);
                origin["cached"] = serde_json::json!(key);
                run.note(format!(
                    "fill from the cache ({key}) for request {request_identity} by fillers.{}; the filler was not called",
                    painter.name
                ))?;
                pixels
            }
            None => {
                if run.cancelled() {
                    return Err(Error::Cancelled {
                        step: "filler".to_string(),
                    });
                }
                let scratch = run.dir.join("filler");
                std::fs::create_dir_all(&scratch).map_err(|e| Error::io(&scratch, e))?;
                let span = tracing::info_span!(
                    "filler",
                    filler = %painter.name,
                    request_bytes = request.bytes(),
                    width = request.width,
                    height = request.height
                );
                run.note(
                    filler::CallStarted {
                        filler: painter.name.to_string(),
                        spend_usd: entry.spend_usd_per_call.unwrap_or(0.0),
                    }
                    .note(),
                )?;
                let outcome = span.in_scope(|| painter.filler.call(&request, &scratch));
                let _ = std::fs::remove_dir_all(&scratch);
                note_call(run, &outcome.call)?;
                let fill = outcome.fill?;
                if let Some(dir) = &painter.cache {
                    store_fill(dir, &wanted, &fill)?;
                    cache_use = Some(false);
                }
                fill
            }
        },
    };
    let fill = match scaled_from {
        Some((width, height)) if (fill.width(), fill.height()) != (width, height) => {
            fill.resize_exact(width, height, image::imageops::FilterType::Lanczos3)
        }
        _ => fill,
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
                return Ok(Pass::Stopped {
                    status,
                    step: "paste".into(),
                    reason: out.envelope.reason,
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
    preservation_check(input_b, &out_image, mask, &frames, plan.as_deref())?;

    // The measures in F: the band set and drift with before = I_F and after = W'.
    {
        let mut inputs: Inputs = BTreeMap::new();
        inputs.insert("before".into(), Artifact::Image(image_f.clone()));
        inputs.insert("after".into(), Artifact::Image(composite_f.clone()));
        inputs.insert("mask".into(), Artifact::Mask(mask.clone()));
        let _ = run.invoke("measure", "measure", registry, &inputs, &Params::new())?;
    }

    Ok(Pass::Healed {
        image: out_image,
        cache: cache_use,
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
        source: None,
        dash,
        renders,
        cache: None,
        verdict: None,
        tries: Vec::new(),
        cancelled: None,
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

/// A fill's key in the cache: the request's identity with the filler's name and revision, so
/// one filler's pixels are never passed off as another's (table-manners 4.4).
fn cache_key(wanted: &StoredFill) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(wanted.request.as_bytes());
    hasher.update([0u8]);
    hasher.update(wanted.filler.as_bytes());
    hasher.update([0u8]);
    hasher.update(wanted.revision.as_deref().unwrap_or("").as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The cached fill for this request, filler and revision, when there is one.
fn cached_fill(dir: &Path, wanted: &StoredFill) -> Result<Option<(DynamicImage, String)>> {
    let key = cache_key(wanted);
    let path = dir.join(format!("{key}.png"));
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some((rascal_core::image::decode_limited(&path)?, key)))
}

/// Keeps a fill in the cache, atomically (a temporary name, then a rename), so two runs that
/// store the same fill at once leave one whole file.
fn store_fill(dir: &Path, wanted: &StoredFill, fill: &DynamicImage) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let key = cache_key(wanted);
    let tmp = dir.join(format!(".{key}.{}.png", std::process::id()));
    rascal_core::canonical::write_png(&tmp, fill)?;
    let path = dir.join(format!("{key}.png"));
    std::fs::rename(&tmp, &path).map_err(|e| Error::io(&path, e))
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
