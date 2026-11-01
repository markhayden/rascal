//! A filler entry's `fill_max_side` (docs/fillers.md, owner decision 10): the request goes to
//! the filler downscaled to that longest side and the fill comes back scaled to the support;
//! the journal says so, the result is the panorama's size, and a replay reproduces the call.
//! Without it, or when the support already fits, nothing is scaled.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("rascal-heal-scaled-{name}-{}", std::process::id()));
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

fn config(dir: &Path, fill_max_side: Option<u32>) -> PathBuf {
    let path = dir.join("rascal.toml");
    let bin = env!("CARGO_BIN_EXE_rascal");
    let scale = fill_max_side
        .map(|s| format!("fill_max_side = {s}\n"))
        .unwrap_or_default();
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\nrevision = \"fake exact\"\n{scale}"
        ),
    )
    .unwrap();
    path
}

fn notes(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("journal.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|l| l["event"] == "note")
        .filter_map(|l| l["note"].as_str().map(String::from))
        .collect()
}

fn call_dimensions(notes: &[String]) -> (u64, u64) {
    let call = notes
        .iter()
        .find(|n| n.starts_with("filler call: "))
        .unwrap();
    let value: serde_json::Value =
        serde_json::from_str(call.trim_start_matches("filler call: ")).unwrap();
    (
        value["request_width"].as_u64().unwrap(),
        value["request_height"].as_u64().unwrap(),
    )
}

#[test]
fn a_scaled_window_sends_a_small_request_and_scales_the_fill_back() {
    let dir = scratch("scaled");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    // The seam's cut canvas on the 800x400 fixture is 112x400: a longest side of 100 scales
    // it to 28x100; a longest side of 400 leaves it alone.
    for (side, expect_scaled) in [(Some(100), true), (Some(400), false), (None, false)] {
        let cfg = config(&dir, side);
        let out = dir.join(format!("run-{}", side.unwrap_or(0)));
        let result = rascal(&[
            "heal",
            "seam",
            pano.to_str().unwrap(),
            "--filler",
            "fake",
            "--config",
            cfg.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--no-dash",
        ]);
        assert_eq!(
            result.status.code(),
            Some(0),
            "{side:?}: {}",
            text(&result.stderr)
        );
        let healed = image::open(out.join("result.png")).unwrap();
        assert_eq!((healed.width(), healed.height()), (800, 400), "{side:?}");
        let notes = notes(&out);
        let (w, h) = call_dimensions(&notes);
        let scale_note = notes
            .iter()
            .find(|n| n.starts_with("fill request scaled to "));
        if expect_scaled {
            assert!(w <= 100 && h <= 100, "{side:?}: the request was {w}x{h}");
            assert_eq!(h, 100, "{side:?}: the longest side is the limit");
            let note = scale_note.unwrap_or_else(|| panic!("{side:?}: no scale note in {notes:?}"));
            assert!(
                note.contains(&format!(
                    "scaled to {w}x{h} from 112x400 (fillers.fake fill_max_side = 100)"
                )),
                "{note}"
            );
        } else {
            assert_eq!((w, h), (112, 400), "{side:?}: the request is the support");
            assert!(scale_note.is_none(), "{side:?}: {notes:?}");
        }
    }
    // A replay of the scaled run reproduces the call from the recorded fill, at the scaled
    // size, and the result is the same.
    let cfg = config(&dir, Some(100));
    let replayed = dir.join("replayed");
    let result = rascal(&[
        "heal",
        "seam",
        pano.to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        cfg.to_str().unwrap(),
        "--out",
        replayed.to_str().unwrap(),
        "--replay",
        dir.join("run-100").to_str().unwrap(),
        "--no-dash",
    ]);
    assert_eq!(result.status.code(), Some(0), "{}", text(&result.stderr));
    let notes = notes(&replayed);
    assert!(
        notes
            .iter()
            .any(|n| n.contains("the filler was not called")),
        "{notes:?}"
    );
    assert_eq!(
        std::fs::read(replayed.join("result.png")).unwrap(),
        std::fs::read(dir.join("run-100/result.png")).unwrap(),
        "the replayed result is byte-identical"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
