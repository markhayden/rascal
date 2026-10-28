//! Task 1.1: the walking slice. `rascal run rect` on the floor fixture writes a mask record, a
//! grey rendering and a one-line-per-invocation journal; the four exit codes; identities
//! stable across two runs; the three kinds of `--out`.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn rascal(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .output()
        .unwrap()
}

fn floor() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/images/floor.png")
}

struct Scratch(PathBuf);
impl Scratch {
    fn new(name: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rascal-slice-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
    fn join(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }
    fn s(&self, rel: &str) -> String {
        self.join(rel).to_string_lossy().into_owned()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn journal(dir: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir.join("journal.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn run_rect(scratch: &Scratch, out: &str, extra: &[&str]) -> Output {
    let floor = floor();
    let out_dir = scratch.s(out);
    let mut args = vec![
        "run",
        "rect",
        floor.to_str().unwrap(),
        "--bbox",
        "0.2,0.2,0.4,0.5",
        "--out",
        out_dir.as_str(),
    ];
    args.extend_from_slice(extra);
    rascal(&args)
}

#[test]
fn the_scenario_writes_a_mask_a_rendering_and_a_journal() {
    let scratch = Scratch::new("scenario");
    let out = run_rect(&scratch, "run", &[]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = scratch.join("run");

    // The mask record and its planes, named by identity under artifacts/.
    let masks: Vec<PathBuf> = std::fs::read_dir(run.join("artifacts"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "mask"))
        .collect();
    assert_eq!(masks.len(), 1, "{masks:?}");
    for name in ["mask.toml", "core.png", "blend.png", "protect.png"] {
        assert!(masks[0].join(name).is_file(), "{name}");
    }
    let record: toml::Table =
        toml::from_str(&std::fs::read_to_string(masks[0].join("mask.toml")).unwrap()).unwrap();
    assert_eq!(record["version"].as_integer(), Some(1));
    assert_eq!(record["frame"].as_str(), Some("raw:320x200:flat"));
    assert_eq!(record["use_case"].as_str(), Some("custom"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&format!("mask: {}", masks[0].display())),
        "{stdout}"
    );

    // The grey rendering: white inside the box, black outside. The box 0.2..0.4 × 0.2..0.5 on
    // 320×200 is columns 64..128 and rows 40..100.
    let grey = image::open(run.join("rect-grey.png")).unwrap().to_luma8();
    assert_eq!(grey.dimensions(), (320, 200));
    assert_eq!(grey.get_pixel(64, 40)[0], 255);
    assert_eq!(grey.get_pixel(127, 99)[0], 255);
    assert_eq!(grey.get_pixel(63, 40)[0], 0);
    assert_eq!(grey.get_pixel(128, 40)[0], 0);
    assert_eq!(grey.get_pixel(64, 39)[0], 0);
    assert_eq!(grey.get_pixel(64, 100)[0], 0);
    assert_eq!(grey.get_pixel(0, 0)[0], 0);
    let white = grey.pixels().filter(|p| p[0] == 255).count();
    assert_eq!(white, 64 * 60);
    assert_eq!(
        grey.pixels().filter(|p| p[0] != 0 && p[0] != 255).count(),
        0
    );

    // The inputs, the tables and the journal: one line per invocation, nothing else.
    assert!(run.join("frames.json").is_file() && run.join("locations.json").is_file());
    assert_eq!(
        std::fs::read_dir(run.join("inputs")).unwrap().count(),
        2,
        "the canonical PNG and its record"
    );
    let lines = journal(&run);
    let steps: Vec<&str> = lines
        .iter()
        .map(|l| l["step"].as_str().unwrap_or("-"))
        .collect();
    assert_eq!(steps, ["rasterize", "outputs.render.grey"]);
    assert_eq!(lines[0]["seq"], 1);
    assert_eq!(lines[1]["seq"], 2);
    assert_eq!(lines[0]["envelope"]["status"], "ok");
    assert!(
        lines[0]["inputs"]["image"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert!(
        lines[0]["inputs"]["location"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert!(
        lines[0]["outputs"]["mask"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert!(lines[0]["duration_ms"].is_u64());
    assert_eq!(lines[1]["inputs"]["mask"], lines[0]["outputs"]["mask"]);
    let location_identity = lines[0]["inputs"]["location"].as_str().unwrap();
    let mask_identity = lines[0]["outputs"]["mask"].as_str().unwrap();
    assert!(
        masks[0]
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with(&mask_identity[7..])
    );
    let locations: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(run.join("locations.json")).unwrap())
            .unwrap();
    assert_eq!(locations[0]["identity"], location_identity);
    assert_eq!(locations[0]["location"]["kind"], "bbox");
}

#[test]
fn identities_are_stable_across_two_runs() {
    let scratch = Scratch::new("stable");
    assert!(run_rect(&scratch, "one", &[]).status.success());
    assert!(run_rect(&scratch, "two", &[]).status.success());
    let one = journal(&scratch.join("one"));
    let two = journal(&scratch.join("two"));
    assert_eq!(one.len(), two.len());
    for (a, b) in one.iter().zip(&two) {
        assert_eq!(a["inputs"], b["inputs"]);
        assert_eq!(a["outputs"], b["outputs"]);
        assert_eq!(a["envelope"]["artifacts"], b["envelope"]["artifacts"]);
    }
}

#[test]
fn render_overwrites_a_file_destination() {
    let scratch = Scratch::new("render");
    assert!(run_rect(&scratch, "run", &[]).status.success());
    let mask = std::fs::read_dir(scratch.join("run/artifacts"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "mask"))
        .unwrap();
    let target = scratch.s("nested/t.png");
    for _ in 0..2 {
        let out = rascal(&[
            "render",
            mask.to_str().unwrap(),
            "--for",
            "grey",
            "--out",
            &target,
        ]);
        assert!(
            out.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let grey = image::open(&target).unwrap().to_luma8();
    assert_eq!(grey.get_pixel(100, 50)[0], 255);
    // A profile a later task brings is an operational error, and names the task.
    let out = rascal(&[
        "render",
        mask.to_str().unwrap(),
        "--for",
        "binary",
        "--out",
        &target,
    ]);
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("1.5"));
}

#[test]
fn the_four_exit_codes() {
    let scratch = Scratch::new("exits");
    let floor = floor();
    let floor = floor.to_str().unwrap();

    // 3: a missing image.
    let out = rascal(&[
        "run",
        "rect",
        "no-such.png",
        "--bbox",
        "0.2,0.2,0.4,0.5",
        "--out",
        &scratch.s("a"),
    ]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // 64: a bad flag, a malformed bbox, a bbox with x0 >= x1, a missing --bbox, an unknown recipe.
    let out = rascal(&[
        "run",
        "rect",
        floor,
        "--no-such-flag",
        "--out",
        &scratch.s("b"),
    ]);
    assert_eq!(out.status.code(), Some(64));
    let out = rascal(&[
        "run",
        "rect",
        floor,
        "--bbox",
        "0.2,0.2,0.4",
        "--out",
        &scratch.s("c"),
    ]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = rascal(&[
        "run",
        "rect",
        floor,
        "--bbox",
        "0.4,0.2,0.2,0.5",
        "--out",
        &scratch.s("d"),
    ]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = rascal(&["run", "rect", floor, "--out", &scratch.s("e")]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = rascal(&[
        "run",
        "no-such-recipe",
        floor,
        "--bbox",
        "0.2,0.2,0.4,0.5",
        "--out",
        &scratch.s("f"),
    ]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // 1 under --strict when the box is wholly outside the image; 0 without --strict, and in
    // both cases the journal has the not_applicable line and no mask was published.
    let out = rascal(&[
        "run",
        "rect",
        floor,
        "--bbox",
        "1.2,1.2,1.4,1.5",
        "--out",
        &scratch.s("g"),
        "--strict",
    ]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = rascal(&[
        "run",
        "rect",
        floor,
        "--bbox",
        "1.2,1.2,1.4,1.5",
        "--out",
        &scratch.s("h"),
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for name in ["g", "h"] {
        let lines = journal(&scratch.join(name));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["envelope"]["status"], "not_applicable");
        assert!(lines[0]["outputs"].as_object().unwrap().is_empty());
        let published = std::fs::read_dir(scratch.join(name).join("artifacts"))
            .map(|d| d.count())
            .unwrap_or(0);
        assert_eq!(published, 0);
        assert!(!scratch.join(name).join("rect-grey.png").exists());
    }
}

#[test]
fn a_run_directory_must_be_new_or_empty() {
    let scratch = Scratch::new("outdir");
    std::fs::create_dir_all(scratch.join("taken")).unwrap();
    std::fs::write(scratch.join("taken/x"), "x").unwrap();
    let out = run_rect(&scratch, "taken", &[]);
    assert_eq!(out.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).contains("new or empty"));
    // Empty is fine, and so is absent.
    std::fs::create_dir_all(scratch.join("empty")).unwrap();
    assert!(run_rect(&scratch, "empty", &[]).status.success());
    assert!(run_rect(&scratch, "absent/deeper", &[]).status.success());
}

#[test]
fn a_two_to_one_image_assumes_equirectangular_and_journals_it() {
    let scratch = Scratch::new("pano");
    let pano = scratch.join("pano.png");
    rascal_testkit::images::flat(400, 200, 120)
        .save(&pano)
        .unwrap();
    let out = rascal(&[
        "run",
        "rect",
        pano.to_str().unwrap(),
        "--bbox",
        "0.1,0.1,0.2,0.2",
        "--out",
        &scratch.s("run"),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines = journal(&scratch.join("run"));
    assert_eq!(lines[0]["event"], "note");
    assert!(
        lines[0]["note"]
            .as_str()
            .unwrap()
            .contains("assumed equirectangular")
    );
    let frames: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(scratch.join("run/frames.json")).unwrap())
            .unwrap();
    assert_eq!(frames["frames"][0]["id"], "raw:400x200:equirectangular");
    assert_eq!(frames["frames"][0]["topology"], "wrap");
    let out = rascal(&[
        "run",
        "rect",
        pano.to_str().unwrap(),
        "--bbox",
        "0.1,0.1,0.2,0.2",
        "--out",
        &scratch.s("flat"),
        "--projection",
        "flat",
    ]);
    assert!(out.status.success());
    assert_eq!(journal(&scratch.join("flat"))[0]["event"], "op");
}
