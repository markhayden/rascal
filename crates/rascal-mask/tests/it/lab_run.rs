//! Task 6.2's scenario: `rascal lab run` over the fixtures with the fake filler and the fake
//! pack. Every cell runs once and its identity, components and finder descriptor are
//! recorded; a second invocation resumes and makes no external call; a filler `revision` bump
//! or a parameter change is a new cell while an unrelated change (the spend ceiling) is not;
//! `--redo` adds an attempt and keeps the old one; `--no-dash` skips the run page; a matrix
//! without `protocol`, a dependent-finder arm, a changed protocol, a foreign directory and a
//! changed corpus image are each refused naming the thing.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rascal_core::lab::cell::{CellRecord, LedgerLine};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-lab-run-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rascal(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .env_remove("RASCAL_LOG")
        .env_remove("RASCAL_CORPUS")
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

const PROTOCOL: &str = "version = 1\nname = \"lab-test\"\nseed = 7\narms = [\"fan-ramp\", \"thin\"]\nmin_decided = 1\n[[group]]\nname = \"sky\"\nimages = [\"sky-01\", \"sky-02\"]\n";

const MATRIX: &str = "version = 1\nname = \"lab-test\"\ncorpus = \"corpus.toml\"\nprotocol = \"protocol.toml\"\nseed = 7\n[[image]]\nid = \"sky-01\"\n[[image]]\nid = \"sky-02\"\n[[arm]]\nname = \"thin\"\nrecipe = \"seam\"\noverrides = { \"fan.constants\" = \"fan-thin-v1\" }\nfiller = \"fake\"\nspend_usd_per_call = 0.25\n[[arm]]\nname = \"fan-ramp\"\nrecipe = \"seam\"\nfiller = \"fake\"\n[[arm]]\nname = \"finder\"\nrecipe = \"segment-test.toml\"\nfiller = \"fake\"\n";

/// The lab's files in `dir`: the example corpus, a two-image protocol and a three-arm matrix
/// (the fake pack's `segment` recipe as the third arm), and the configuration with the fake
/// filler and the fake pack at `revision`.
fn world(dir: &Path, revision: &str) -> (PathBuf, PathBuf) {
    std::fs::copy(
        repo_root().join("examples/lab/corpus.toml"),
        dir.join("corpus.toml"),
    )
    .unwrap();
    std::fs::write(dir.join("protocol.toml"), PROTOCOL).unwrap();
    std::fs::write(dir.join("matrix.toml"), MATRIX).unwrap();
    std::fs::write(
        dir.join("segment-test.toml"),
        rascal_testkit::fake_pack::RECIPE,
    )
    .unwrap();
    let config = dir.join("rascal.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\nrevision = {revision:?}\n[packs.fake]\ncommand = [{bin:?}, \"fake-pack\", \"exact\"]\nrevision = \"fake 1\"\n",
            bin = env!("CARGO_BIN_EXE_rascal")
        ),
    )
    .unwrap();
    (dir.join("matrix.toml"), config)
}

fn run(matrix: &Path, config: &Path, out: &Path, extra: &[&str]) -> Output {
    let fixtures = repo_root().join("fixtures/images");
    let mut args = vec![
        "lab",
        "run",
        matrix.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--corpus-root",
        fixtures.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    rascal(&args)
}

fn ledger(out: &Path) -> Vec<LedgerLine> {
    std::fs::read_to_string(out.join("ledger.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn cells(out: &Path) -> Vec<CellRecord> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(out.join("cells"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    dirs.sort();
    dirs.iter()
        .map(|d| {
            serde_json::from_str(&std::fs::read_to_string(d.join("cell.json")).unwrap()).unwrap()
        })
        .collect()
}

#[test]
fn every_cell_runs_once_resumes_without_a_call_and_redo_adds_an_attempt() {
    let dir = scratch("cells");
    let (matrix, config) = world(&dir, "fake exact");
    let out = dir.join("lab");
    let first = run(&matrix, &config, &out, &[]);
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    let stdout = text(&first.stdout);
    assert!(
        stdout.contains("6 ran, 0 reused, 0 stopped, 0 failed"),
        "{stdout}"
    );
    assert!(
        stdout.contains("spend 0.50 USD"),
        "two thin cells at 0.25: {stdout}"
    );
    // Six filler calls and two pack calls in the ledger; the thin cells carry the spend.
    let lines = ledger(&out);
    assert_eq!(lines.iter().filter(|l| l.kind == "filler").count(), 6);
    assert_eq!(lines.iter().filter(|l| l.kind == "pack").count(), 2);
    assert_eq!(
        lines.iter().map(|l| l.spend_usd).sum::<f64>(),
        0.5,
        "the thin arm's two filler calls at 0.25"
    );
    assert!(
        lines.iter().all(|l| l.attempt == 1 && l.outcome == "ok"),
        "{lines:?}"
    );
    // The cells: six, each with attempt 1 ok, a result, the run page, and latest = 1.
    let records = cells(&out);
    assert_eq!(records.len(), 6);
    for cell in &records {
        let hex = cell.identity.strip_prefix("sha256:").unwrap();
        let cell_dir = out.join("cells").join(hex);
        assert_eq!(
            std::fs::read_to_string(cell_dir.join("latest"))
                .unwrap()
                .trim(),
            "1"
        );
        let attempt: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(cell_dir.join("attempts/1/attempt.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(attempt["status"], "ok", "{attempt}");
        assert_eq!(attempt["result"], "run/result.png");
        assert!(cell_dir.join("attempts/1/run/result.png").is_file());
        assert!(cell_dir.join("attempts/1/run/dash.html").is_file());
        assert!(cell_dir.join("attempts/1/recipe.toml").is_file());
        assert_eq!(cell.protocol_digest.len(), 71);
        assert_eq!(cell.components.filler.revision, "fake exact");
        assert!(cell.components.filler.target.contains("fake-filler"));
        assert!(cell.components.op_versions.contains_key("fan") || cell.arm == "finder");
        if cell.arm == "thin" {
            assert_eq!(cell.components.constants["fan"], "fan-thin-v1");
            assert_eq!(cell.components.overrides["fan.constants"], "fan-thin-v1");
        }
        if cell.arm == "finder" {
            let descriptor = &cell.components.finders[0];
            assert_eq!(
                (descriptor.pack.as_str(), descriptor.tool.as_str()),
                ("fake", "segment")
            );
            assert_eq!(descriptor.revision, "fake 1");
            assert_eq!(descriptor.arguments["prompt"], "the chair");
            assert_eq!(descriptor.arguments["max_instances"], 2);
            assert_eq!(descriptor.inputs["image"], cell.components.image);
            assert!(
                descriptor.bundle_digest.is_none(),
                "the fake pack is no bundle install"
            );
        } else {
            assert!(cell.components.finders.is_empty());
        }
    }
    // A second invocation resumes: nothing runs, no external call, the ledger unchanged.
    let second = run(&matrix, &config, &out, &[]);
    assert_eq!(second.status.code(), Some(0), "{}", text(&second.stderr));
    let stdout = text(&second.stdout);
    assert!(
        stdout.contains("resumed") && stdout.contains("0 ran, 6 reused"),
        "{stdout}"
    );
    assert!(stdout.contains("0 external calls"), "{stdout}");
    assert_eq!(ledger(&out).len(), lines.len());
    // An unrelated change (the spend ceiling) is the same cell.
    let cheaper = std::fs::read_to_string(&matrix)
        .unwrap()
        .replace("spend_usd_per_call = 0.25", "spend_usd_per_call = 0.0");
    std::fs::write(&matrix, cheaper).unwrap();
    let third = run(&matrix, &config, &out, &[]);
    assert!(
        text(&third.stdout).contains("0 ran, 6 reused"),
        "{}",
        text(&third.stdout)
    );
    // A parameter change is a new cell for the arm it touches.
    let wider = std::fs::read_to_string(&matrix).unwrap().replace(
        "overrides = { \"fan.constants\" = \"fan-thin-v1\" }",
        "overrides = { \"fan.constants\" = \"fan-thin-v1\", \"line.width_px\" = 3.0 }",
    );
    std::fs::write(&matrix, wider).unwrap();
    let fourth = run(&matrix, &config, &out, &[]);
    assert_eq!(fourth.status.code(), Some(0), "{}", text(&fourth.stderr));
    assert!(
        text(&fourth.stdout).contains("2 ran, 4 reused"),
        "{}",
        text(&fourth.stdout)
    );
    assert_eq!(cells(&out).len(), 8);
    // A filler revision bump is a new cell for every arm.
    let (matrix, config) = world(&dir, "fake exact 2");
    let fifth = run(&matrix, &config, &out, &[]);
    assert_eq!(fifth.status.code(), Some(0), "{}", text(&fifth.stderr));
    assert!(
        text(&fifth.stdout).contains("6 ran, 0 reused"),
        "{}",
        text(&fifth.stdout)
    );
    assert_eq!(cells(&out).len(), 14);
    // --redo adds attempt 2 to the arm's cells, keeps attempt 1, and --no-dash skips the page.
    let sixth = run(&matrix, &config, &out, &["--redo", "thin", "--no-dash"]);
    assert_eq!(sixth.status.code(), Some(0), "{}", text(&sixth.stderr));
    assert!(
        text(&sixth.stdout).contains("2 ran, 4 reused"),
        "{}",
        text(&sixth.stdout)
    );
    let thin: Vec<CellRecord> = cells(&out)
        .into_iter()
        .filter(|c| c.arm == "thin" && c.components.filler.revision == "fake exact 2")
        .collect();
    assert_eq!(thin.len(), 2);
    for cell in &thin {
        let cell_dir = out
            .join("cells")
            .join(cell.identity.strip_prefix("sha256:").unwrap());
        assert_eq!(
            std::fs::read_to_string(cell_dir.join("latest"))
                .unwrap()
                .trim(),
            "2"
        );
        assert!(
            cell_dir.join("attempts/1/run/result.png").is_file(),
            "attempt 1 is kept"
        );
        assert!(cell_dir.join("attempts/1/run/dash.html").is_file());
        assert!(cell_dir.join("attempts/2/run/result.png").is_file());
        assert!(
            !cell_dir.join("attempts/2/run/dash.html").exists(),
            "--no-dash"
        );
    }
    let out_redo = run(&matrix, &config, &out, &["--redo", "nowhere"]);
    assert_eq!(out_redo.status.code(), Some(64));
    assert!(text(&out_redo.stderr).contains("no such arm"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_refusals_name_the_field_the_binding_the_protocol_the_directory_and_the_image() {
    let dir = scratch("refusals");
    let (matrix, config) = world(&dir, "fake exact");
    // No protocol: refused naming the field, nothing written.
    let no_protocol = dir.join("no-protocol.toml");
    std::fs::write(
        &no_protocol,
        MATRIX.replace("protocol = \"protocol.toml\"\n", ""),
    )
    .unwrap();
    let out = run(&no_protocol, &config, &dir.join("lab-a"), &[]);
    assert_eq!(out.status.code(), Some(64), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("protocol is required"),
        "{}",
        text(&out.stderr)
    );
    assert!(!dir.join("lab-a").exists());
    // A dependent-finder arm: segment fed by segment's plane, refused naming the binding.
    std::fs::write(
        dir.join("chain.toml"),
        "version = 1\nname = \"chain\"\nuse_case = \"remove\"\n[inputs]\nimage = { type = \"Image\" }\n[[step]]\nid = \"a\"\nop = \"segment\"\ninputs = { image = \"inputs.image\" }\n[[step]]\nid = \"b\"\nop = \"segment\"\ninputs = { image = \"a.plane_0\" }\n[[step]]\nid = \"r\"\nop = \"rasterize\"\ninputs = { location = \"b.region_0\", image = \"inputs.image\" }\nuse_case = \"remove\"\n[outputs]\nmask = \"r.mask\"\n",
    )
    .unwrap();
    let chained = dir.join("chained.toml");
    std::fs::write(
        &chained,
        MATRIX.replace("recipe = \"segment-test.toml\"", "recipe = \"chain.toml\""),
    )
    .unwrap();
    let out = run(&chained, &config, &dir.join("lab-b"), &[]);
    assert_eq!(out.status.code(), Some(64), "{}", text(&out.stderr));
    let err = text(&out.stderr);
    assert!(
        err.contains("arm finder") && err.contains("finder b (segment) takes image = a.plane_0"),
        "{err}"
    );
    // A sound run, then the protocol changes: resumed under the old digest is refused.
    let lab = dir.join("lab-c");
    let out = run(&matrix, &config, &lab, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    std::fs::write(
        dir.join("protocol.toml"),
        PROTOCOL.replace("min_decided = 1", "min_decided = 2"),
    )
    .unwrap();
    let out = run(&matrix, &config, &lab, &[]);
    assert_eq!(out.status.code(), Some(64), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("new lab name"),
        "{}",
        text(&out.stderr)
    );
    std::fs::write(dir.join("protocol.toml"), PROTOCOL).unwrap();
    // Another lab's directory, and a foreign non-empty one.
    let other = dir.join("other.toml");
    std::fs::write(
        &other,
        MATRIX.replace("name = \"lab-test\"", "name = \"other\""),
    )
    .unwrap();
    std::fs::write(
        dir.join("protocol.toml"),
        PROTOCOL.replace("name = \"lab-test\"", "name = \"other\""),
    )
    .unwrap();
    let out = run(&other, &config, &lab, &[]);
    assert_eq!(out.status.code(), Some(64), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("holds lab lab-test, not other"),
        "{}",
        text(&out.stderr)
    );
    std::fs::write(dir.join("protocol.toml"), PROTOCOL).unwrap();
    let foreign = dir.join("foreign");
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(foreign.join("notes.txt"), "mine").unwrap();
    let out = run(&matrix, &config, &foreign, &[]);
    assert_eq!(out.status.code(), Some(64), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("holds no lab.json"),
        "{}",
        text(&out.stderr)
    );
    // A filler without a revision cannot be used in the lab.
    let no_revision = dir.join("no-revision.toml");
    std::fs::write(
        &no_revision,
        std::fs::read_to_string(&config)
            .unwrap()
            .replace("revision = \"fake exact\"\n", ""),
    )
    .unwrap();
    let out = run(&matrix, &no_revision, &dir.join("lab-d"), &[]);
    assert_eq!(out.status.code(), Some(64), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("needs a revision for lab use"),
        "{}",
        text(&out.stderr)
    );
    // A changed corpus image stops the run before any cell.
    let root = dir.join("root");
    std::fs::create_dir_all(&root).unwrap();
    for image in [
        "step-sky.png",
        "step-pano.png",
        "floor.png",
        "chair-blob.png",
    ] {
        std::fs::copy(
            repo_root().join("fixtures/images").join(image),
            root.join(image),
        )
        .unwrap();
    }
    std::fs::write(root.join("step-pano.png"), b"changed").unwrap();
    let out = rascal(&[
        "lab",
        "run",
        matrix.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
        "--out",
        dir.join("lab-e").to_str().unwrap(),
        "--corpus-root",
        root.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("corpus image sky-02 changed"),
        "{}",
        text(&out.stderr)
    );
    assert!(
        !dir.join("lab-e/cells").exists()
            || std::fs::read_dir(dir.join("lab-e/cells"))
                .unwrap()
                .next()
                .is_none()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Task 8.6: two arms that are different cells but send the filler identical requests (one
/// names `fan-v3`, the default, so its mask is the other's: the objects-01 case) share one
/// fill through the lab's cache: one external call per image, not two.
#[test]
fn arms_with_the_same_request_share_one_fill() {
    let dir = scratch("shared");
    let (matrix, config) = world(&dir, "fake exact");
    std::fs::write(
        dir.join("protocol.toml"),
        "version = 1\nname = \"lab-test\"\nseed = 7\narms = [\"a\", \"b\"]\nmin_decided = 1\n[[group]]\nname = \"sky\"\nimages = [\"sky-01\", \"sky-02\"]\n",
    )
    .unwrap();
    std::fs::write(
        &matrix,
        "version = 1\nname = \"lab-test\"\ncorpus = \"corpus.toml\"\nprotocol = \"protocol.toml\"\nseed = 7\n[[image]]\nid = \"sky-01\"\n[[image]]\nid = \"sky-02\"\n[[arm]]\nname = \"a\"\nrecipe = \"seam\"\nfiller = \"fake\"\nspend_usd_per_call = 0.04\n[[arm]]\nname = \"b\"\nrecipe = \"seam\"\noverrides = { \"fan.constants\" = \"fan-v3\" }\nfiller = \"fake\"\nspend_usd_per_call = 0.04\n",
    )
    .unwrap();
    let out = dir.join("lab");
    let first = run(&matrix, &config, &out, &[]);
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    let stdout = text(&first.stdout);
    assert!(stdout.contains("4 ran"), "{stdout}");
    assert!(stdout.contains("2 external calls"), "{stdout}");
    assert!(stdout.contains("spend 0.08 USD"), "{stdout}");
    assert_eq!(
        ledger(&out).iter().filter(|l| l.kind == "filler").count(),
        2
    );
    assert!(out.join("fills").is_dir());
    let _ = std::fs::remove_dir_all(&dir);
}
