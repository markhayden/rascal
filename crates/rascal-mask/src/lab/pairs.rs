//! `rascal lab pairs <lab> --arms a,b` and `rascal lab decode <lab> <picks.json>` (SPEC.md
//! "The lab"). `pairs` takes the two arms' current cells for every protocol image, draws the
//! page order and the sides from the protocol's seed, writes `pairs/assignment.json` and the
//! static `pairs/index.html` with both results embedded and nothing that names an arm; the
//! page shows each pair at its focus (the region both results changed, at full size), whole,
//! and for an equirectangular image from inside the sphere, with the unhealed source beside
//! the results, and writes `picks.json`. `decode` checks the picks against the assignment's digest and
//! reports wins, losses and the outcome per group under the protocol's rules.

use std::path::{Path, PathBuf};

use image::DynamicImage;
use rascal_core::Error;
use rascal_core::error::Result;
use rascal_core::frame::{FrameTable, Projection, Topology};
use rascal_core::lab::focus::{Changes, Focus};
use rascal_core::lab::pairs::{self, Assignment, Candidate, Decode, Picks, Shown};
use rascal_core::lab::protocol::Protocol;
use rascal_core::panel;

use crate::lab::layout::LabDir;

const TEMPLATE: &str = include_str!("../../pages/pairs.html");
/// The per-pair block of the template lies between these markers.
const PAIR_OPEN: &str = "<!-- pair -->";
const PAIR_CLOSE: &str = "<!-- /pair -->";

/// One side of a pair as loaded: the source the heal started from, its result, and the raw
/// frame they share.
struct Side {
    source: DynamicImage,
    result: DynamicImage,
    projection: Projection,
    topology: Topology,
}

/// Loads a side's images from its attempt; refuses an attempt that records no source.
fn load_side(lab: &LabDir, shown: &Shown) -> Result<Side> {
    let attempt = lab.attempt_dir(&shown.cell, shown.attempt);
    let record = lab.attempt(&shown.cell, shown.attempt)?;
    let file = |name: &str, path: Option<String>| -> Result<DynamicImage> {
        let path = path.ok_or_else(|| {
            Error::usage(format!(
                "cell {} attempt {} records no {name} image ({}); run the lab again",
                shown.cell, shown.attempt, record.status
            ))
        })?;
        rascal_core::image::decode_limited(&attempt.join(path))
    };
    let result = file("result", record.result.clone())?;
    let source = file("source", record.source.clone())?;
    let frames_path = attempt.join(&record.run).join("frames.json");
    let bytes = std::fs::read(&frames_path).map_err(|source| Error::Io {
        path: frames_path.clone(),
        source,
    })?;
    let frames: FrameTable = serde_json::from_slice(&bytes).map_err(|source| Error::Json {
        path: frames_path.clone(),
        source,
    })?;
    let raw = frames
        .frames
        .iter()
        .find(|f| f.is_raw())
        .ok_or_else(|| Error::op("lab", format!("{}: no raw frame", frames_path.display())))?;
    Ok(Side {
        source,
        result,
        projection: raw.projection,
        topology: raw.topology,
    })
}

/// The focus of a pair: the union of what the two results changed in their shared source.
fn focus_of(left: &Side, right: &Side) -> Result<Option<Focus>> {
    let mut changes = Changes::between(&left.source, &left.result)?;
    changes.join(&Changes::between(&right.source, &right.result)?)?;
    Ok(changes.focus(left.topology))
}

/// A whole image for the page: scaled to `max_side` on its longest side when it is larger.
fn whole(image: &DynamicImage, max_side: Option<u32>) -> Result<String> {
    match max_side {
        Some(side) if image.width().max(image.height()) > side => {
            data_uri(&image.resize(side, side, image::imageops::FilterType::Triangle))
        }
        _ => data_uri(image),
    }
}

fn data_uri(image: &DynamicImage) -> Result<String> {
    Ok(panel::png_data_uri(&rascal_core::canonical::encode_png(
        image,
    )?))
}

/// What the page says about a pair's focus.
fn focus_note(focus: Option<&Focus>, width: u32, height: u32) -> String {
    match focus {
        None => format!(
            "No pixel differs from the source in either result ({width}×{height}); the focus is the whole image."
        ),
        Some(f) if f.width == width && f.height == height => {
            format!("The changes reach across the whole image ({width}×{height}).")
        }
        Some(f) => format!(
            "Focus: {}×{} of {width}×{height} at column {}, row {}{}, with the changes of both sides inside.",
            f.width,
            f.height,
            f.x,
            f.y,
            if f.x + f.width > width {
                " (through the wrap edge)"
            } else {
                ""
            }
        ),
    }
}

/// The lab's protocol, kept beside `lab.json` by `lab run`, with its digest checked against
/// the record.
fn protocol_of(lab: &LabDir) -> Result<(Protocol, String)> {
    let record = lab.record()?;
    let (protocol, digest) = Protocol::read(&lab.root.join("protocol.toml"))?;
    if digest != record.protocol_digest {
        return Err(Error::op(
            "lab",
            format!(
                "{}: protocol.toml ({digest}) is not the protocol this lab ran under ({})",
                lab.root.display(),
                record.protocol_digest
            ),
        ));
    }
    Ok((protocol, digest))
}

/// The template around and between the pair markers: head, the per-pair block, tail.
fn split_template() -> Result<(&'static str, &'static str, &'static str)> {
    let (head, rest) = TEMPLATE
        .split_once(PAIR_OPEN)
        .ok_or_else(|| Error::op("lab", format!("pairs.html: no {PAIR_OPEN} marker")))?;
    let (block, tail) = rest
        .split_once(PAIR_CLOSE)
        .ok_or_else(|| Error::op("lab", format!("pairs.html: no {PAIR_CLOSE} marker")))?;
    Ok((head, block, tail))
}

/// Writes the assignment and the page; returns the page's path. `whole_max_side` scales the
/// whole images (the whole and look-around views) to that longest side, said on the page;
/// the focus crops are always at full size.
pub fn pairs(root: &Path, arms: &str, whole_max_side: Option<u32>) -> Result<PathBuf> {
    let lab = LabDir {
        root: root.to_path_buf(),
    };
    let record = lab.record()?;
    let (protocol, digest) = protocol_of(&lab)?;
    let arms: Vec<&str> = arms.split(',').map(str::trim).collect();
    let [a, b] = arms.as_slice() else {
        return Err(Error::usage(format!(
            "--arms takes two arm names, a,b; got {arms:?}"
        )));
    };
    if a == b {
        return Err(Error::usage("--arms takes two different arms"));
    }
    let mut candidates = Vec::new();
    for group in &protocol.group {
        for image in &group.images {
            let shown = |arm: &str| -> Result<Shown> {
                let key = format!("{arm}/{image}");
                let identity = record.current.get(&key).ok_or_else(|| {
                    Error::usage(format!(
                        "no cell for arm {arm} on image {image} in the last lab run (the current cells are {})",
                        if record.current.is_empty() { "none".to_string() } else { record.current.keys().cloned().collect::<Vec<_>>().join(", ") }
                    ))
                })?;
                let latest = lab.latest(identity)?.ok_or_else(|| {
                    Error::usage(format!("cell {identity} ({key}) has no attempt"))
                })?;
                if latest.record.status != "ok" || latest.record.result.is_none() {
                    return Err(Error::usage(format!(
                        "cell {identity} ({key}): attempt {} is {}, not ok; run the lab again",
                        latest.attempt, latest.record.status
                    )));
                }
                Ok(Shown {
                    cell: identity.clone(),
                    attempt: latest.attempt,
                })
            };
            candidates.push(Candidate {
                image: image.clone(),
                group: group.name.clone(),
                a: shown(a)?,
                b: shown(b)?,
            });
        }
    }
    let assignment = Assignment::draw(
        &protocol,
        &digest,
        [a.to_string(), b.to_string()],
        candidates,
    );
    let dir = lab.root.join("pairs");
    std::fs::create_dir_all(&dir).map_err(|source| Error::Io {
        path: dir.clone(),
        source,
    })?;
    let assignment_path = dir.join("assignment.json");
    let text = serde_json::to_string_pretty(&assignment)
        .map_err(|e| Error::op("lab", e.to_string()))?
        + "\n";
    std::fs::write(&assignment_path, &text).map_err(|source| Error::Io {
        path: assignment_path.clone(),
        source,
    })?;
    let assignment_digest = rascal_core::lab::bytes_digest(text.as_bytes());

    let (head, block, tail) = split_template()?;
    let mut blocks = Vec::with_capacity(assignment.pair.len());
    for (n, pair) in assignment.pair.iter().enumerate() {
        let left = load_side(&lab, &pair.left)?;
        let right = load_side(&lab, &pair.right)?;
        let (width, height) = (left.result.width(), left.result.height());
        if (right.result.width(), right.result.height()) != (width, height)
            || right.projection != left.projection
        {
            return Err(Error::usage(format!(
                "pair {}: the sides are {width}×{height} {} and {}×{} {}; not one image",
                pair.id,
                left.projection.name(),
                right.result.width(),
                right.result.height(),
                right.projection.name()
            )));
        }
        let focus = focus_of(&left, &right)?;
        let crop = |side: &Side| -> Result<String> {
            match &focus {
                Some(f) => data_uri(&DynamicImage::ImageRgb8(f.crop(&side.result))),
                None => Ok(String::new()),
            }
        };
        let id = panel::escape(&pair.id);
        let buttons: String = pairs::PICKS
            .iter()
            .map(|pick| format!("<button data-pair=\"{id}\" data-pick=\"{pick}\">{pick}</button>"))
            .collect();
        blocks.push(
            block
                .replace("{{ID}}", &id)
                .replace("{{N}}", &(n + 1).to_string())
                .replace("{{COUNT}}", &assignment.pair.len().to_string())
                .replace("{{WIDTH}}", &width.to_string())
                .replace("{{HEIGHT}}", &height.to_string())
                .replace(
                    "{{FOCUS}}",
                    &focus
                        .map(|f| format!("{},{},{},{}", f.x, f.y, f.width, f.height))
                        .unwrap_or_default(),
                )
                .replace("{{PROJECTION}}", left.projection.name())
                .replace(
                    "{{NOTE}}",
                    &panel::escape(&focus_note(focus.as_ref(), width, height)),
                )
                .replace("{{LEFT_CROP}}", &crop(&left)?)
                .replace("{{RIGHT_CROP}}", &crop(&right)?)
                .replace(
                    "{{SOURCE_CROP}}",
                    &match &focus {
                        Some(f) => data_uri(&DynamicImage::ImageRgb8(f.crop(&left.source)))?,
                        None => String::new(),
                    },
                )
                .replace("{{LEFT}}", &whole(&left.result, whole_max_side)?)
                .replace("{{RIGHT}}", &whole(&right.result, whole_max_side)?)
                .replace("{{SOURCE}}", &whole(&left.source, whole_max_side)?)
                .replace("{{BUTTONS}}", &buttons),
        );
    }
    let ids: Vec<&str> = assignment.pair.iter().map(|p| p.id.as_str()).collect();
    let page = format!("{head}{}{tail}", blocks.join("\n"))
        .replace("{{LAB}}", &panel::escape(&record.name))
        .replace(
            "{{SUMMARY}}",
            &panel::escape(&format!(
                "{} pairs over {} groups; protocol {}.{}",
                assignment.pair.len(),
                protocol.group.len(),
                digest,
                whole_max_side
                    .map(|side| format!(
                        " The whole images are scaled to a longest side of {side} px for the whole and look-around views; the focus is at full size."
                    ))
                    .unwrap_or_default()
            )),
        )
        .replace("{{COUNT}}", &assignment.pair.len().to_string())
        .replace(
            "{{DIGEST}}",
            &serde_json::to_string(&assignment_digest)
                .map_err(|e| Error::op("lab", e.to_string()))?,
        )
        .replace(
            "{{IDS}}",
            &serde_json::to_string(&ids).map_err(|e| Error::op("lab", e.to_string()))?,
        );
    let index = dir.join("index.html");
    std::fs::write(&index, &page).map_err(|source| Error::Io {
        path: index.clone(),
        source,
    })?;
    println!(
        "pairs page: {} pairs, {:.1} MB with every result embedded",
        assignment.pair.len(),
        page.len() as f64 / 1e6
    );
    Ok(index)
}

/// Decodes a picks file against the lab's assignment and protocol.
pub fn decode(root: &Path, picks: &Path) -> Result<Decode> {
    let lab = LabDir {
        root: root.to_path_buf(),
    };
    let (protocol, _) = protocol_of(&lab)?;
    let record = lab.record()?;
    let assignment_path = lab.root.join("pairs").join("assignment.json");
    let bytes = std::fs::read(&assignment_path).map_err(|source| Error::Io {
        path: assignment_path.clone(),
        source,
    })?;
    let assignment: Assignment = serde_json::from_slice(&bytes).map_err(|source| Error::Json {
        path: assignment_path.clone(),
        source,
    })?;
    if assignment.lab != record.name || assignment.protocol_digest != record.protocol_digest {
        return Err(Error::usage(format!(
            "{}: an assignment of lab {} under protocol {}, not this lab ({} under {})",
            assignment_path.display(),
            assignment.lab,
            assignment.protocol_digest,
            record.name,
            record.protocol_digest
        )));
    }
    let digest = rascal_core::lab::bytes_digest(&bytes);
    let text = std::fs::read(picks).map_err(|source| Error::Io {
        path: picks.to_path_buf(),
        source,
    })?;
    let picks_file: Picks = serde_json::from_slice(&text).map_err(|source| Error::Json {
        path: picks.to_path_buf(),
        source,
    })?;
    pairs::decode(&protocol, &assignment, &digest, &picks_file)
        .map_err(|m| Error::usage(format!("{}: {m}", picks.display())))
}

/// The decode as a table and the outcome per group.
pub fn print_decode(decode: &Decode) {
    println!(
        "{:<14} {:>5} {:>7} {:>5} {:>6} {:>5} {:>7}  outcome",
        "group", "pairs", "decided", "wins", "losses", "same", "neither"
    );
    for g in &decode.groups {
        println!(
            "{:<14} {:>5} {:>7} {:>5} {:>6} {:>5} {:>7}  {}",
            g.group, g.pairs, g.decided, g.wins, g.losses, g.same, g.neither, g.outcome
        );
    }
    println!(
        "wins are {}'s over {}; preferred: wins / decided at least the protocol's fraction; not_worse: losses / decided below it; inconclusive: fewer decided pairs than min_decided",
        decode.arms[0], decode.arms[1]
    );
}
