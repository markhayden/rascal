//! Task 13.11: `heal` split into the recipe run and one pass of its graph (`heal_once`) changes
//! nothing. Two heals recorded before the split are the golden: a `seam` heal through window,
//! unwindow and put, and a `remove-prompt` heal through the fake pack. Each is healed again and
//! compared: the result and the mask byte for byte, and the journal line for line after the
//! durations and the machine's paths are normalised away (every op, step, identity, parameter,
//! envelope and note equal). A measurement is compared to nine significant digits and a
//! `Series`'s identity not at all: `measure`'s statistics differ in the last digits between
//! platforms' maths libraries (macOS and Linux), and a Series hashes those values. `UPDATE_FIXTURES=1` rewrites the golden, which only a change that
//! means to alter heal's output should do.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("rascal-heal-golden-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// One recorded heal: the recipe, its image, the extra arguments and the configuration.
struct Case {
    name: &'static str,
    recipe: &'static str,
    image: &'static str,
    args: &'static [&'static str],
    config: fn(&Path) -> PathBuf,
}

/// The fake filler as a command, windowed (`max_side = 400` on the 112x400 seam canvas sends
/// the heal through window and unwindow).
fn seam_config(dir: &Path) -> PathBuf {
    let path = dir.join("rascal.toml");
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[fillers.fake]\ncommand = [{:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\npaste = \"feather\"\ntimeout_s = 10\nmax_side = 400\nrevision = \"fake\"\n",
            env!("CARGO_BIN_EXE_rascal")
        ),
    )
    .unwrap();
    path
}

/// The fake pack behind a fake segmenter install, and the fake filler.
fn remove_prompt_config(dir: &Path) -> PathBuf {
    let path = dir.join("rascal.toml");
    let bin = env!("CARGO_BIN_EXE_rascal");
    let install =
        rascal_testkit::fake_install::write(&dir.join("pack"), &[bin, "fake-pack", "exact"], true)
            .unwrap();
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[packs.fake]\ncommand = {}\nrevision = \"fake 1\"\n[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\nrevision = \"fake exact\"\n",
            install.command_toml()
        ),
    )
    .unwrap();
    path
}

const CASES: &[Case] = &[
    Case {
        name: "seam-windowed",
        recipe: "seam",
        image: "fixtures/images/step-pano.png",
        args: &[],
        config: seam_config,
    },
    Case {
        name: "remove-prompt",
        recipe: "remove-prompt",
        image: "fixtures/images/chair-blob.png",
        args: &["--prompt", "the chair"],
        config: remove_prompt_config,
    },
];

/// Replaces every machine path in `text` with a placeholder.
fn unpath(text: &str, dir: &Path) -> String {
    let mut text = text.to_string();
    for (path, name) in [
        (PathBuf::from(env!("CARGO_BIN_EXE_rascal")), "<rascal>"),
        (dir.to_path_buf(), "<dir>"),
        (repo_root(), "<repo>"),
    ] {
        text = text.replace(&path.display().to_string(), name);
    }
    text
}

/// A journal value with its durations dropped and its paths replaced, a note's JSON body
/// (`filler call: {...}`) included. A pack call's `request_bytes` goes too: the request names
/// the image by its path, so its size follows the machine's (a temporary directory's process
/// id one digit longer is two bytes more).
fn normalise(value: &mut serde_json::Value, dir: &Path) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("duration_ms");
            // A measurement's value, to nine significant digits.
            if map.contains_key("name")
                && map.contains_key("unit")
                && let Some(v) = map.get("value").and_then(serde_json::Value::as_f64)
            {
                map.insert("value".into(), serde_json::json!(format!("{v:.8e}")));
            }
            if map.contains_key("pack") {
                map.remove("request_bytes");
            }
            for v in map.values_mut() {
                normalise(v, dir);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                normalise(v, dir);
            }
        }
        serde_json::Value::String(s) => {
            let mut text = unpath(s, dir);
            if let Some(at) = text.find(": {")
                && let Ok(mut body) = serde_json::from_str::<serde_json::Value>(&text[at + 2..])
            {
                normalise(&mut body, dir);
                text = format!("{}{body}", &text[..at + 2]);
            }
            *s = text;
        }
        _ => {}
    }
}

fn normalised_journal(run: &Path, dir: &Path) -> String {
    let text = std::fs::read_to_string(run.join("journal.jsonl")).unwrap();
    text.lines()
        .map(|line| {
            let mut value: serde_json::Value = serde_json::from_str(line).unwrap();
            normalise(&mut value, dir);
            series_unidentified(&mut value);
            serde_json::to_string(&value).unwrap() + "\n"
        })
        .collect()
}

/// A journal line's `Series` artifacts with their identities replaced, in the envelope and in
/// `outputs`: a Series hashes `measure`'s values, which differ in the last digits by platform.
fn series_unidentified(line: &mut serde_json::Value) {
    let Some(artifacts) = line
        .pointer_mut("/envelope/artifacts")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    let mut series = Vec::new();
    for (id, artifact) in artifacts.iter_mut() {
        if artifact["artifact_type"] == "Series" {
            artifact["identity"] = serde_json::json!("<series>");
            series.push(id.clone());
        }
    }
    if let Some(outputs) = line
        .get_mut("outputs")
        .and_then(serde_json::Value::as_object_mut)
    {
        for id in series {
            if outputs.contains_key(&id) {
                outputs.insert(id, serde_json::json!("<series>"));
            }
        }
    }
}

/// The files of the Mask the run recorded as its output, by name.
fn mask_files(run: &Path) -> Vec<(String, Vec<u8>)> {
    let outputs: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(run.join("outputs.json")).unwrap()).unwrap();
    let identity = outputs["mask"].as_str().unwrap();
    let hex = identity.strip_prefix("sha256:").unwrap();
    let mask = run.join("artifacts").join(format!("{hex}.mask"));
    let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(&mask)
        .unwrap()
        .map(|e| {
            let path = e.unwrap().path();
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read(&path).unwrap(),
            )
        })
        .collect();
    files.sort();
    files
}

#[test]
fn a_heal_reproduces_the_golden_recorded_before_the_split() {
    for case in CASES {
        let dir = scratch(case.name);
        let config = (case.config)(&dir);
        let run = dir.join("run");
        let image = repo_root().join(case.image);
        let mut args = vec![
            "heal",
            case.recipe,
            image.to_str().unwrap(),
            "--filler",
            "fake",
            "--config",
            config.to_str().unwrap(),
            "--out",
            run.to_str().unwrap(),
            "--no-dash",
        ];
        args.extend(case.args);
        let out = Command::new(env!("CARGO_BIN_EXE_rascal"))
            .args(&args)
            .env_remove("RASCAL_LOG")
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}: {}",
            case.name,
            String::from_utf8_lossy(&out.stderr)
        );
        let result = std::fs::read(run.join("result.png")).unwrap();
        let mask = mask_files(&run);
        let journal = normalised_journal(&run, &dir);

        let golden = repo_root().join("fixtures/heal").join(case.name);
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            let _ = std::fs::remove_dir_all(&golden);
            std::fs::create_dir_all(golden.join("mask")).unwrap();
            std::fs::write(golden.join("result.png"), &result).unwrap();
            for (name, bytes) in &mask {
                std::fs::write(golden.join("mask").join(name), bytes).unwrap();
            }
            std::fs::write(golden.join("journal.jsonl"), &journal).unwrap();
        }
        assert!(
            result == std::fs::read(golden.join("result.png")).unwrap(),
            "{}: result.png differs from the golden",
            case.name
        );
        let mut stored: Vec<(String, Vec<u8>)> = std::fs::read_dir(golden.join("mask"))
            .unwrap()
            .map(|e| {
                let path = e.unwrap().path();
                (
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    std::fs::read(&path).unwrap(),
                )
            })
            .collect();
        stored.sort();
        assert_eq!(
            mask.iter().map(|(n, _)| n).collect::<Vec<_>>(),
            stored.iter().map(|(n, _)| n).collect::<Vec<_>>(),
            "{}: the mask's files",
            case.name
        );
        for ((name, a), (_, b)) in mask.iter().zip(&stored) {
            assert!(a == b, "{}: mask {name} differs from the golden", case.name);
        }
        // The golden as recorded, under the same rules as the run's journal (its paths already
        // stand as placeholders).
        let expected: String = std::fs::read_to_string(golden.join("journal.jsonl"))
            .unwrap()
            .lines()
            .map(|line| {
                let mut value: serde_json::Value = serde_json::from_str(line).unwrap();
                normalise(&mut value, &dir);
                series_unidentified(&mut value);
                serde_json::to_string(&value).unwrap() + "\n"
            })
            .collect();
        for (n, (a, b)) in journal.lines().zip(expected.lines()).enumerate() {
            assert_eq!(a, b, "{}: journal line {}", case.name, n + 1);
        }
        assert_eq!(
            journal.lines().count(),
            expected.lines().count(),
            "{}: journal length",
            case.name
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
