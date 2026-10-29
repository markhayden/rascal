//! Task 6.3's scenario: after `lab run` on the example lab, `rascal lab pairs lab/ --arms
//! fan-ramp,thin` writes the assignment and a page that embeds every result, names the pair
//! ids and never an arm or a side; picks written the way the page writes them decode to the
//! per-group table and the outcome; stale, foreign, missing and duplicated picks are refused.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rascal_core::lab::pairs::{Assignment, Pick, Picks};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-lab-pairs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rascal(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .env_remove("RASCAL_LOG")
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn write_picks(
    path: &Path,
    assignment_bytes: &[u8],
    pick_of: impl Fn(usize, &rascal_core::lab::pairs::Pair) -> &'static str,
) -> Assignment {
    let assignment: Assignment = serde_json::from_slice(assignment_bytes).unwrap();
    let picks = Picks {
        picks_version: 1,
        assignment_digest: rascal_core::lab::bytes_digest(assignment_bytes),
        pick: assignment
            .pair
            .iter()
            .enumerate()
            .map(|(i, p)| Pick {
                pair: p.id.clone(),
                pick: pick_of(i, p).to_string(),
            })
            .collect(),
    };
    std::fs::write(path, serde_json::to_string_pretty(&picks).unwrap()).unwrap();
    assignment
}

#[test]
fn pairs_writes_a_blind_page_and_decode_reports_the_outcome_per_group() {
    let dir = scratch("flow");
    let examples = repo_root().join("examples/lab");
    let config = dir.join("rascal.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\nrevision = \"fake exact\"\n",
            bin = env!("CARGO_BIN_EXE_rascal")
        ),
    )
    .unwrap();
    let lab = dir.join("lab");
    let fixtures = repo_root().join("fixtures/images");
    let out = rascal(&[
        "lab",
        "run",
        examples.join("matrix.toml").to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
        "--out",
        lab.to_str().unwrap(),
        "--corpus-root",
        fixtures.to_str().unwrap(),
        "--no-dash",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    // Pairs before any run of an arm: refused naming the cell.
    let out = rascal(&[
        "lab",
        "pairs",
        lab.to_str().unwrap(),
        "--arms",
        "fan-ramp,wide",
    ]);
    assert_eq!(out.status.code(), Some(64), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("no cell for arm wide"),
        "{}",
        text(&out.stderr)
    );
    let out = rascal(&[
        "lab",
        "pairs",
        lab.to_str().unwrap(),
        "--arms",
        "fan-ramp,thin",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let page_path = lab.join("pairs/index.html");
    assert!(text(&out.stdout).contains(&page_path.display().to_string()));
    let page = std::fs::read_to_string(&page_path).unwrap();
    let assignment_bytes = std::fs::read(lab.join("pairs/assignment.json")).unwrap();
    let assignment: Assignment = serde_json::from_slice(&assignment_bytes).unwrap();
    assert_eq!(assignment.pair.len(), 4);
    assert_eq!(
        assignment.arms,
        ["fan-ramp".to_string(), "thin".to_string()]
    );
    assert_eq!(assignment.lab, "seam-demo");
    // The page: eight embedded images, every pair id, the digest, and nothing that names an
    // arm, a side's cell or the filler's command.
    assert_eq!(page.matches("data:image/png;base64,").count(), 8);
    for pair in &assignment.pair {
        assert!(
            page.contains(&format!("data-pair=\"{}\"", pair.id)),
            "{}",
            pair.id
        );
        assert!(
            !page.contains(&pair.left.cell) && !page.contains(&pair.right.cell),
            "cells are not named"
        );
    }
    assert!(
        !page.contains("fan-ramp") && !page.contains("thin"),
        "arms are not named"
    );
    assert!(!page.contains("a_side") && !page.contains("fake-filler"));
    assert!(page.contains(&rascal_core::lab::bytes_digest(&assignment_bytes)));
    // The order on the page is the assignment's.
    let positions: Vec<usize> = assignment
        .pair
        .iter()
        .map(|p| page.find(&format!("id=\"pair-{}\"", p.id)).unwrap())
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]));
    // Picks: fan-ramp wins every sky pair; on structured one loss and one same.
    let picks = dir.join("picks.json");
    write_picks(&picks, &assignment_bytes, |_, pair| {
        if pair.group == "sky" {
            if pair.a_side == "left" {
                "left"
            } else {
                "right"
            }
        } else if pair.id == "structured-01" {
            if pair.a_side == "left" {
                "right"
            } else {
                "left"
            }
        } else {
            "same"
        }
    });
    let out = rascal(&[
        "lab",
        "decode",
        lab.to_str().unwrap(),
        picks.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    let sky = stdout.lines().find(|l| l.starts_with("sky")).unwrap();
    let structured = stdout
        .lines()
        .find(|l| l.starts_with("structured"))
        .unwrap();
    let cells = |line: &str| -> Vec<String> { line.split_whitespace().map(String::from).collect() };
    assert_eq!(
        cells(sky),
        ["sky", "2", "2", "2", "0", "0", "0", "preferred"]
    );
    assert_eq!(
        cells(structured),
        ["structured", "2", "1", "0", "1", "1", "0", "inconclusive"]
    );
    assert!(stdout.contains("wins are fan-ramp's over thin"), "{stdout}");
    // Stale: the assignment is drawn again after the protocol's seed changes? The lab's
    // protocol is fixed; a picks file from another assignment carries another digest.
    let stale = dir.join("stale.json");
    let mut other = std::fs::read_to_string(&picks).unwrap();
    other = other.replace(
        &rascal_core::lab::bytes_digest(&assignment_bytes),
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    );
    std::fs::write(&stale, other).unwrap();
    let out = rascal(&[
        "lab",
        "decode",
        lab.to_str().unwrap(),
        stale.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("stale or foreign"),
        "{}",
        text(&out.stderr)
    );
    // Foreign, missing and duplicated pairs.
    let foreign = dir.join("foreign.json");
    write_picks(&foreign, &assignment_bytes, |_, _| "left");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&foreign).unwrap()).unwrap();
    doc["pick"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"pair": "sky-99", "pick": "left"}));
    std::fs::write(&foreign, doc.to_string()).unwrap();
    let out = rascal(&[
        "lab",
        "decode",
        lab.to_str().unwrap(),
        foreign.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(text(&out.stderr).contains("sky-99 is not in the assignment"));
    let missing = dir.join("missing.json");
    write_picks(&missing, &assignment_bytes, |_, _| "left");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&missing).unwrap()).unwrap();
    doc["pick"].as_array_mut().unwrap().pop();
    std::fs::write(&missing, doc.to_string()).unwrap();
    let out = rascal(&[
        "lab",
        "decode",
        lab.to_str().unwrap(),
        missing.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(text(&out.stderr).contains("has no pick"));
    let duplicated = dir.join("duplicated.json");
    write_picks(&duplicated, &assignment_bytes, |_, _| "left");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&duplicated).unwrap()).unwrap();
    let first = doc["pick"][0].clone();
    doc["pick"].as_array_mut().unwrap().push(first);
    std::fs::write(&duplicated, doc.to_string()).unwrap();
    let out = rascal(&[
        "lab",
        "decode",
        lab.to_str().unwrap(),
        duplicated.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(text(&out.stderr).contains("is picked twice"));
    // A protocol edited after the run: pairs and decode refuse the lab.
    let protocol = lab.join("protocol.toml");
    let original = std::fs::read_to_string(&protocol).unwrap();
    std::fs::write(
        &protocol,
        original.replace("min_decided = 2", "min_decided = 3"),
    )
    .unwrap();
    let out = rascal(&[
        "lab",
        "decode",
        lab.to_str().unwrap(),
        picks.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("is not the protocol this lab ran under"));
    std::fs::write(&protocol, original).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
