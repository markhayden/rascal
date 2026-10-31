//! `rascal lab run <matrix> --config <rascal.toml> --out <lab dir>` (SPEC.md "The lab"): the
//! matrix validated, the corpus verified, then one cell per arm and image in two phases.
//! First every cell's identity is computed from its components and its record written (the
//! image's digest, every bound input's digest, the overrides, the recipe as it runs, the
//! constant sets, the op versions, the filler entry without its key, and every finder's
//! invocation descriptor), so nothing that could be refused is found after a filler has been
//! paid for. Then each cell runs: one whose `latest` attempt is complete is reused and makes
//! no external call; `--redo <arm>` adds an attempt to every cell of that arm and never
//! overwrites one. Each attempt runs `heal` in its own run directory; every filler or pack
//! call the run journaled is a ledger line with the arm's spend.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rascal_core::Error;
use rascal_core::config::Config;
use rascal_core::error::Result;
use rascal_core::journal::JournalLine;
use rascal_core::lab::cell::{
    AttemptRecord, CellComponents, CellRecord, FillerEntry, FinderDescriptor, LabRecord, LedgerLine,
};
use rascal_core::lab::corpus::{Corpus, Verification};
use rascal_core::lab::matrix::{Arm, Matrix, MatrixImage, apply_override};
use rascal_core::lab::protocol::Protocol;
use rascal_core::op::Status;
use rascal_core::recipe::Recipe;
use rascal_core::registry::OpRegistry;
use rascal_core::workspace::Workspace;
use rascal_mask::lab::layout::LabDir;
use rascal_mask::lab::resolve_matrix;
use rascal_mcp::client::Pack;
use rascal_mcp::provision::{PACK_NAME, install_dir_of};

use crate::heal::{self, HealArgs};
use crate::run::RunArgs;

/// What `lab run` was asked to do.
pub struct LabRunArgs {
    pub matrix: PathBuf,
    pub config: PathBuf,
    pub out: PathBuf,
    /// The corpus root; default `RASCAL_CORPUS`, else `~/rascal/corpus`.
    pub corpus_root: Option<PathBuf>,
    /// Add an attempt to every cell of this arm.
    pub redo: Option<String>,
    pub no_dash: bool,
}

/// What `lab run` did.
#[derive(Debug, Default)]
pub struct LabRunReport {
    pub ran: u32,
    pub reused: u32,
    pub stopped: u32,
    pub failed: u32,
    pub calls: u32,
    pub spend_usd: f64,
}

/// The journal note prefixes of external calls, with the ledger's kind for each.
const CALL_NOTES: [(&str, &str); 2] = [("filler call: ", "filler"), ("pack call: ", "pack")];

/// A cell ready to run: its record, the recipe as it runs and the files it binds.
struct Planned {
    cell: CellRecord,
    arm_index: usize,
    image_index: usize,
    recipe: Recipe,
    inputs: BTreeMap<String, PathBuf>,
}

pub fn lab_run(args: &LabRunArgs) -> Result<LabRunReport> {
    let config = Config::read(&args.config)?;
    let matrix_dir = args
        .matrix
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();

    // The lab directory first, from the matrix's name and the protocol's digest: a new one, or
    // an earlier run of this lab to resume. `--redo` must name an arm before anything exists.
    let matrix = Matrix::read(&args.matrix)?;
    if let Some(redo) = &args.redo
        && matrix.arm(redo).is_none()
    {
        return Err(Error::usage(format!("--redo {redo}: no such arm")));
    }
    let (_, protocol_digest) = Protocol::read(&matrix_dir.join(&matrix.protocol))?;
    let record = LabRecord {
        lab_version: 1,
        name: matrix.name.clone(),
        protocol_digest,
        matrix: args.matrix.display().to_string(),
        current: BTreeMap::new(),
    };
    let (lab, resumed) = LabDir::open_or_create(&args.out, &record)?;
    if !resumed {
        // The protocol beside the record, so pairs and decode need nothing outside the lab.
        std::fs::copy(
            matrix_dir.join(&matrix.protocol),
            lab.root.join("protocol.toml"),
        )
        .map_err(|source| Error::Io {
            path: lab.root.join("protocol.toml"),
            source,
        })?;
    }

    // The registry the arms validate against: the shipped ops and the configured packs'
    // tools, the latter being the finders. The packs journal into the lab's own workspace;
    // starting them is not an external call.
    let workspace_dir = args.out.join("workspace");
    std::fs::create_dir_all(&workspace_dir).map_err(|source| Error::Io {
        path: workspace_dir.clone(),
        source,
    })?;
    let workspace = Arc::new(Workspace::open(&workspace_dir)?);
    let (registry, packs) = crate::packs::registry_for(Some(&config), &workspace, None)?;
    let mut finder_names: Vec<String> = Vec::new();
    for pack in &packs {
        for tool in pack.tools()? {
            finder_names.push(tool.manifest.name.clone());
        }
    }
    let finders: Vec<&str> = finder_names.iter().map(String::as_str).collect();
    let validated = resolve_matrix(&args.matrix, &registry, &finders)?;

    // The corpus: every image the matrix uses, verified before anything runs.
    let root = Corpus::root(args.corpus_root.as_deref());
    let mut image_files: BTreeMap<&str, PathBuf> = BTreeMap::new();
    for (image, found) in validated.corpus.verify(&root)? {
        if !validated.matrix.image.iter().any(|i| i.id == image.id) {
            continue;
        }
        match found {
            Verification::Ok => {
                image_files.insert(image.id.as_str(), root.join(&image.file));
            }
            Verification::Missing => {
                return Err(Error::op(
                    "lab",
                    format!(
                        "corpus image {} is missing ({}); rascal corpus verify",
                        image.id,
                        root.join(&image.file).display()
                    ),
                ));
            }
            Verification::Mismatch { actual } => {
                return Err(Error::op(
                    "lab",
                    format!(
                        "corpus image {} changed ({}: {actual}, manifest {}); rascal corpus verify",
                        image.id,
                        root.join(&image.file).display(),
                        image.sha256
                    ),
                ));
            }
        }
    }

    // Every arm's filler, with the revision the identity needs.
    let mut filler_entries: BTreeMap<&str, FillerEntry> = BTreeMap::new();
    for arm in &validated.matrix.arm {
        let entry = config.fillers.get(&arm.filler).ok_or_else(|| {
            Error::usage(format!(
                "arm {}: no [fillers.{}] in {}",
                arm.name,
                arm.filler,
                args.config.display()
            ))
        })?;
        let revision = entry.revision.clone().ok_or_else(|| {
            Error::usage(format!(
                "[fillers.{}] needs a revision for lab use (free text the owner bumps when the filler changes)",
                arm.filler
            ))
        })?;
        filler_entries.insert(
            arm.name.as_str(),
            FillerEntry {
                target: entry
                    .command
                    .as_ref()
                    .map(|c| c.join(" "))
                    .or_else(|| entry.url.clone())
                    .unwrap_or_default(),
                profile: entry.profile.clone(),
                paste: entry.paste.clone(),
                max_side: entry.max_side,
                fill_max_side: entry.fill_max_side,
                send_mask: entry.send_mask,
                revision,
            },
        );
    }

    println!(
        "lab {} ({}): protocol {}, {} images × {} arms{}",
        record.name,
        lab.root.display(),
        record.protocol_digest,
        validated.matrix.image.len(),
        validated.matrix.arm.len(),
        if resumed { ", resumed" } else { "" }
    );

    // Phase one: every cell's identity, before any cell runs.
    let mut planned = Vec::new();
    let mut current = BTreeMap::new();
    for (image_index, image) in validated.matrix.image.iter().enumerate() {
        let inputs = input_files(image, &matrix_dir)?;
        for (arm_index, arm) in validated.matrix.arm.iter().enumerate() {
            let mut recipe = validated.recipes[arm_index].clone();
            for (key, value) in &image.overrides {
                apply_override(&mut recipe, &registry, key, value).map_err(|m| {
                    Error::usage(format!("image {}: override {key}: {m}", image.id))
                })?;
            }
            // Only the inputs the arm's recipe declares are bound; the matrix validation
            // checked that every required one is.
            let bound: BTreeMap<String, (PathBuf, String)> = inputs
                .iter()
                .filter(|(name, _)| recipe.inputs.contains_key(*name))
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect();
            let components = components(
                &validated
                    .corpus
                    .image(&image.id)
                    .map(|i| i.sha256.clone())
                    .unwrap_or_default(),
                &bound,
                arm,
                image,
                &recipe,
                &registry,
                &packs,
                &finders,
                filler_entries[arm.name.as_str()].clone(),
            )?;
            let cell = CellRecord {
                cell_version: 1,
                identity: components.identity(),
                arm: arm.name.clone(),
                image: image.id.clone(),
                protocol_digest: validated.protocol_digest.clone(),
                components,
            };
            lab.write_cell(&cell)?;
            current.insert(format!("{}/{}", arm.name, image.id), cell.identity.clone());
            planned.push(Planned {
                cell,
                arm_index,
                image_index,
                recipe,
                inputs: bound.into_iter().map(|(n, (p, _))| (n, p)).collect(),
            });
        }
    }
    lab.set_current(current)?;

    // Phase two: the runs.
    let mut report = LabRunReport::default();
    for plan in &planned {
        let arm = &validated.matrix.arm[plan.arm_index];
        let image = &validated.matrix.image[plan.image_index];
        let cell = &plan.cell;
        let short = &cell.identity[7..19];
        let redo = args.redo.as_deref() == Some(arm.name.as_str());
        if !redo && let Some(latest) = lab.latest(&cell.identity)? {
            if latest.record.status == "ok" {
                println!(
                    "cell {} × {} {short}: reused attempt {} (no external call)",
                    arm.name, image.id, latest.attempt
                );
                report.reused += 1;
                continue;
            }
            println!(
                "cell {} × {} {short}: attempt {} was {}; running again",
                arm.name, image.id, latest.attempt, latest.record.status
            );
        }
        let attempt = lab.next_attempt(&cell.identity)?;
        let dir = lab.attempt_dir(&cell.identity, attempt);
        std::fs::create_dir_all(&dir).map_err(|source| Error::Io {
            path: dir.clone(),
            source,
        })?;
        let recipe_path = dir.join("recipe.toml");
        let text = toml::to_string(&plan.recipe).map_err(|e| Error::op("lab", e.to_string()))?;
        std::fs::write(&recipe_path, text).map_err(|source| Error::Io {
            path: recipe_path.clone(),
            source,
        })?;
        let run_dir = dir.join("run");
        let heal_args = HealArgs {
            run: RunArgs {
                recipe: recipe_path.display().to_string(),
                image: image_files[image.id.as_str()].clone(),
                out: run_dir.clone(),
                bbox: None,
                location: plan.inputs.get("location").cloned(),
                stroke: plan.inputs.get("stroke").cloned(),
                protect: plan.inputs.get("protect").cloned(),
                mask: plan.inputs.get("mask").cloned(),
                prompt: None,
                config: Some(args.config.clone()),
                workspace: None,
                projection: None,
                strict: false,
                replay: None,
                overrides: BTreeMap::new(),
                overrides_from: None,
                sets: Vec::new(),
                cancel: None,
            },
            filler: arm.filler.clone(),
            no_dash: args.no_dash,
            replay: None,
            // One cache for the lab: arms whose fill requests are identical (the same mask
            // under the same filler and revision) share one call (objects-01, objects-02).
            cache: Some(args.out.join("fills")),
        };
        let outcome = heal::heal(&heal_args);
        let calls = calls_in(&run_dir.join("journal.jsonl"));
        for (kind, target, outcome_text, duration_ms) in &calls {
            lab.ledger_append(&LedgerLine {
                ledger_version: 1,
                cell: cell.identity.clone(),
                attempt,
                arm: arm.name.clone(),
                image: image.id.clone(),
                kind: kind.to_string(),
                target: target.clone(),
                outcome: outcome_text.clone(),
                duration_ms: *duration_ms,
                spend_usd: if *kind == "filler" {
                    arm.spend_usd_per_call
                } else {
                    0.0
                },
            })?;
            report.calls += 1;
            if *kind == "filler" {
                report.spend_usd += arm.spend_usd_per_call;
            }
        }
        let relative = |path: &PathBuf| {
            path.strip_prefix(&dir)
                .unwrap_or(path)
                .display()
                .to_string()
        };
        let (status, result, source, stopped_at, reason) = match &outcome {
            Ok(heal) if heal.status == Status::Ok => (
                "ok",
                heal.result.as_ref().map(relative),
                heal.source.as_ref().map(relative),
                None,
                None,
            ),
            Ok(heal) => (
                "stopped",
                None,
                None,
                heal.stopped_at.clone(),
                heal.reason.clone(),
            ),
            Err(e) => ("failed", None, None, None, Some(e.to_string())),
        };
        lab.finish_attempt(
            &cell.identity,
            &AttemptRecord {
                attempt_version: 1,
                attempt,
                status: status.to_string(),
                run: "run".to_string(),
                result,
                source,
                stopped_at,
                reason: reason.clone(),
                calls: calls.len() as u32,
            },
        )?;
        match status {
            "ok" => report.ran += 1,
            "stopped" => report.stopped += 1,
            _ => report.failed += 1,
        }
        println!(
            "cell {} × {} {short}: attempt {attempt} {status}{} ({} call{})",
            arm.name,
            image.id,
            reason.map(|r| format!(": {r}")).unwrap_or_default(),
            calls.len(),
            if calls.len() == 1 { "" } else { "s" }
        );
    }
    println!(
        "lab {}: {} ran, {} reused, {} stopped, {} failed; {} external call{}, spend {:.2} USD",
        record.name,
        report.ran,
        report.reused,
        report.stopped,
        report.failed,
        report.calls,
        if report.calls == 1 { "" } else { "s" },
        report.spend_usd
    );
    if report.failed > 0 {
        return Err(Error::op(
            "lab",
            format!(
                "{} cell{} failed; their attempts record why",
                report.failed,
                if report.failed == 1 { "" } else { "s" }
            ),
        ));
    }
    Ok(report)
}

/// A matrix image's bound input files by recipe input name: the path (relative to the
/// matrix) and its digest. The names were checked by the matrix validation; the files must
/// exist.
fn input_files(
    image: &MatrixImage,
    matrix_dir: &Path,
) -> Result<BTreeMap<String, (PathBuf, String)>> {
    let mut out = BTreeMap::new();
    for (name, file) in &image.inputs {
        let path = matrix_dir.join(file);
        let digest = if path.is_dir() {
            // A stored mask: its record names its identity.
            rascal_core::lab::file_digest(&path.join("mask.json"))?
        } else {
            rascal_core::lab::file_digest(&path)?
        };
        out.insert(name.clone(), (path, digest));
    }
    Ok(out)
}

/// The components a cell's identity is computed from.
#[allow(clippy::too_many_arguments)]
fn components(
    image_digest: &str,
    inputs: &BTreeMap<String, (PathBuf, String)>,
    arm: &Arm,
    image: &MatrixImage,
    recipe: &Recipe,
    registry: &OpRegistry,
    packs: &[Arc<Pack>],
    finders: &[&str],
    filler: FillerEntry,
) -> Result<CellComponents> {
    let mut overrides = BTreeMap::new();
    for (key, value) in arm.overrides.iter().chain(&image.overrides) {
        overrides.insert(
            key.clone(),
            serde_json::to_value(toml::Value::from(value))
                .map_err(|e| Error::op("lab", e.to_string()))?,
        );
    }
    let mut constants = BTreeMap::new();
    let mut op_versions = BTreeMap::new();
    let mut descriptors = Vec::new();
    for step in &recipe.step {
        let manifest = registry.require(&step.op)?.manifest();
        // The set the step selects, or the op's default (the first it lists).
        if let Some(set) = step
            .constants
            .clone()
            .or_else(|| manifest.constants.first().cloned())
        {
            constants.insert(step.id.clone(), set);
        }
        op_versions.insert(step.op.clone(), manifest.version.clone());
        if !finders.contains(&step.op.as_str()) {
            continue;
        }
        let pack = packs
            .iter()
            .find(|p| {
                p.tools()
                    .is_ok_and(|t| t.iter().any(|t| t.manifest.name == step.op))
            })
            .ok_or_else(|| Error::op("lab", format!("no pack serves {}", step.op)))?;
        let revision = pack.config().revision.clone().ok_or_else(|| {
            Error::usage(format!(
                "[packs.{}] needs a revision for lab use (free text the owner bumps when a model or its configuration changes)",
                pack.name()
            ))
        })?;
        let bundle_digest = install_dir_of(&pack.config().command, PACK_NAME)
            .map(|dir| dir.join("bundle.toml"))
            .filter(|p| p.is_file())
            .map(|p| rascal_core::lab::file_digest(&p))
            .transpose()?;
        let arguments = manifest
            .resolve_params(&step.params)
            .map_err(|m| Error::usage(format!("step {}: {m}", step.id)))?;
        let mut descriptor_inputs = BTreeMap::new();
        for (port, binding) in &step.inputs {
            let identity = match binding.split_once('.') {
                Some(("inputs", "image")) => image_digest.to_string(),
                Some(("inputs", name)) => inputs
                    .get(name)
                    .map(|(_, digest)| digest.clone())
                    .unwrap_or_else(|| binding.clone()),
                _ => binding.clone(),
            };
            descriptor_inputs.insert(port.clone(), identity);
        }
        descriptors.push(FinderDescriptor {
            step: step.id.clone(),
            pack: pack.name().to_string(),
            bundle_digest,
            revision,
            env: pack.config().env.clone(),
            tool: step.op.clone(),
            arguments: serde_json::to_value(arguments)
                .map_err(|e| Error::op("lab", e.to_string()))?,
            inputs: descriptor_inputs,
        });
    }
    Ok(CellComponents {
        image: image_digest.to_string(),
        inputs: inputs
            .iter()
            .map(|(name, (_, digest))| (name.clone(), digest.clone()))
            .collect(),
        overrides,
        recipe: serde_json::to_value(recipe).map_err(|e| Error::op("lab", e.to_string()))?,
        constants,
        op_versions,
        filler,
        finders: descriptors,
    })
}

/// The external calls a run's journal records: kind, target, outcome, duration.
fn calls_in(journal: &Path) -> Vec<(&'static str, String, String, u64)> {
    let Ok(text) = std::fs::read_to_string(journal) else {
        return Vec::new();
    };
    let mut calls = Vec::new();
    for line in text.lines() {
        let Ok(line) = serde_json::from_str::<JournalLine>(line) else {
            continue;
        };
        let Some(note) = line.note.as_deref() else {
            continue;
        };
        for (prefix, kind) in CALL_NOTES {
            if let Some(json) = note.strip_prefix(prefix) {
                let call: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
                let target = ["filler", "pack"]
                    .iter()
                    .find_map(|k| call.get(k).and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string();
                let outcome = call
                    .get("outcome")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let duration_ms = call
                    .get("duration_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                calls.push((kind, target, outcome, duration_ms));
            }
        }
    }
    calls
}
