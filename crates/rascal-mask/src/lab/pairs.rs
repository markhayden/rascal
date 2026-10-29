//! `rascal lab pairs <lab> --arms a,b` and `rascal lab decode <lab> <picks.json>` (SPEC.md
//! "The lab"). `pairs` takes the two arms' current cells for every protocol image, draws the
//! page order and the sides from the protocol's seed, writes `pairs/assignment.json` and the
//! static `pairs/index.html` with both results embedded and nothing that names an arm; the
//! page writes `picks.json`. `decode` checks the picks against the assignment's digest and
//! reports wins, losses and the outcome per group under the protocol's rules.

use std::path::{Path, PathBuf};

use rascal_core::Error;
use rascal_core::error::Result;
use rascal_core::lab::pairs::{self, Assignment, Candidate, Decode, Picks, Shown};
use rascal_core::lab::protocol::Protocol;
use rascal_core::panel;

use crate::lab::layout::LabDir;

const TEMPLATE: &str = include_str!("../../pages/pairs.html");

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

/// Writes the assignment and the page; returns the page's path.
pub fn pairs(root: &Path, arms: &str) -> Result<PathBuf> {
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

    let mut blocks = Vec::with_capacity(assignment.pair.len());
    for (n, pair) in assignment.pair.iter().enumerate() {
        let image_of = |shown: &Shown| -> Result<String> {
            let attempt = lab.attempt_dir(&shown.cell, shown.attempt);
            let record = lab.latest(&shown.cell)?.map(|l| l.record);
            let result = record
                .and_then(|r| r.result)
                .unwrap_or_else(|| "run/result.png".to_string());
            let path = attempt.join(result);
            let bytes = std::fs::read(&path).map_err(|source| Error::Io { path, source })?;
            Ok(panel::png_data_uri(&bytes))
        };
        let id = panel::escape(&pair.id);
        let buttons: String = pairs::PICKS
            .iter()
            .map(|pick| format!("<button data-pair=\"{id}\" data-pick=\"{pick}\">{pick}</button>"))
            .collect();
        blocks.push(format!(
            "<section class=\"pair\" id=\"pair-{id}\"><h2>Pair {} of {}: {id}</h2><div class=\"sides\"><figure><img src=\"{}\" alt=\"left\"><figcaption>left</figcaption></figure><figure><img src=\"{}\" alt=\"right\"><figcaption>right</figcaption></figure></div><div class=\"picks\">{buttons}</div></section>",
            n + 1,
            assignment.pair.len(),
            image_of(&pair.left)?,
            image_of(&pair.right)?
        ));
    }
    let ids: Vec<&str> = assignment.pair.iter().map(|p| p.id.as_str()).collect();
    let page = TEMPLATE
        .replace("{{LAB}}", &panel::escape(&record.name))
        .replace(
            "{{SUMMARY}}",
            &panel::escape(&format!(
                "{} pairs over {} groups; protocol {}.",
                assignment.pair.len(),
                protocol.group.len(),
                digest
            )),
        )
        .replace("{{COUNT}}", &assignment.pair.len().to_string())
        .replace("{{PAIRS}}", &blocks.join("\n"))
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
    std::fs::write(&index, page).map_err(|source| Error::Io {
        path: index.clone(),
        source,
    })?;
    Ok(index)
}

/// Decodes a picks file against the lab's assignment and protocol.
pub fn decode(root: &Path, picks: &Path) -> Result<Decode> {
    let lab = LabDir {
        root: root.to_path_buf(),
    };
    let (protocol, _) = protocol_of(&lab)?;
    let assignment_path = lab.root.join("pairs").join("assignment.json");
    let bytes = std::fs::read(&assignment_path).map_err(|source| Error::Io {
        path: assignment_path.clone(),
        source,
    })?;
    let assignment: Assignment = serde_json::from_slice(&bytes).map_err(|source| Error::Json {
        path: assignment_path.clone(),
        source,
    })?;
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
