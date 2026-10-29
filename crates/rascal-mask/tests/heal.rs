//! Task 2.8: `heal`'s typed graph and the shipped `seam` recipe, end to end offline through
//! every fake filler on the step panorama, with and without `max_side`; the graph executed
//! literally; the mis-sized filler refused; the re-encoding filler passing preservation;
//! continuity falling from the raw canvas to the composite; replay without the filler; the
//! run page with the filler call row; the key in no file and no span.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rascal_testkit::fake_filler::{Behaviour, spawn};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-heal-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rascal_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command.args(args).env_remove("RASCAL_LOG");
    for (k, v) in env {
        command.env(k, v);
    }
    command.output().unwrap()
}

fn rascal(args: &[&str]) -> Output {
    rascal_env(args, &[])
}

/// A configuration with the fake as a command filler.
fn command_config(
    dir: &Path,
    behaviour: Behaviour,
    max_side: Option<u32>,
    timeout_s: u64,
) -> PathBuf {
    let path = dir.join(format!(
        "{}-{}.toml",
        behaviour.name(),
        max_side.unwrap_or(0)
    ));
    let max_side = max_side
        .map(|m| format!("max_side = {m}\n"))
        .unwrap_or_default();
    std::fs::write(
        &path,
        format!(
            "config_version = 1\n[fillers.fake]\ncommand = [{:?}, \"fake-filler\", \"{}\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\npaste = \"feather\"\ntimeout_s = {timeout_s}\n{max_side}revision = \"fake\"\n",
            env!("CARGO_BIN_EXE_rascal"),
            behaviour.name()
        ),
    )
    .unwrap();
    path
}

fn journal(dir: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir.join("journal.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn ops(lines: &[serde_json::Value]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| l["event"] == "op")
        .map(|l| l["op"].as_str().unwrap().to_string())
        .collect()
}

fn pano() -> PathBuf {
    repo_root().join("fixtures/images/step-pano.png")
}

fn measurement(envelope: &serde_json::Value, name: &str) -> f64 {
    envelope["measurements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == name)
        .unwrap_or_else(|| panic!("{name} in {envelope}"))["value"]
        .as_f64()
        .unwrap()
}

#[test]
fn the_seam_recipe_reproduces_the_golden_mask() {
    let dir = scratch("golden");
    let run = dir.join("run");
    let out = rascal(&[
        "run",
        "seam",
        pano().to_str().unwrap(),
        "--out",
        run.to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mask_dir = String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("mask: ").map(PathBuf::from))
        .unwrap();
    let golden = repo_root().join("fixtures/masks/seam-step-pano.mask");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        let _ = std::fs::remove_dir_all(&golden);
        copy_tree(&mask_dir, &golden);
    }
    for plane in ["core.png", "blend.png", "protect.png"] {
        let a = image::open(mask_dir.join(plane)).unwrap().to_luma8();
        let b = image::open(golden.join(plane)).unwrap().to_luma8();
        assert!(
            a == b,
            "{plane} differs from the golden; UPDATE_FIXTURES=1 regenerates it"
        );
    }
    let fresh = rascal_core::mask::Mask::read(&mask_dir).unwrap();
    let stored = rascal_core::mask::Mask::read(&golden).unwrap();
    assert_eq!(
        fresh.record.identity, stored.record.identity,
        "the identities agree"
    );
    assert!(fresh.profile.is_some() && fresh.prescriptions.len() == 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn heal_through_every_fake_with_and_without_max_side() {
    let dir = scratch("fakes");
    let pano_path = pano();
    let input = image::open(&pano_path).unwrap().to_rgb8();
    // max_side 400 admits the full-height support (90x400 on the 112x400 canvas) and still
    // sends the heal through window and unwindow; a smaller one is not applicable (scaled
    // windows are a later milestone).
    for max_side in [None, Some(400)] {
        for behaviour in [
            Behaviour::Exact,
            Behaviour::Reencoding,
            Behaviour::MisSized,
            Behaviour::Failing,
            Behaviour::Slow,
        ] {
            let timeout_s = if behaviour == Behaviour::Slow { 1 } else { 10 };
            let config = command_config(&dir, behaviour, max_side, timeout_s);
            let run = dir.join(format!("{}-{}", behaviour.name(), max_side.unwrap_or(0)));
            let out = rascal(&[
                "heal",
                "seam",
                pano_path.to_str().unwrap(),
                "--filler",
                "fake",
                "--config",
                config.to_str().unwrap(),
                "--out",
                run.to_str().unwrap(),
            ]);
            let err = String::from_utf8_lossy(&out.stderr).into_owned();
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            let lines = journal(&run);
            match behaviour {
                Behaviour::Exact | Behaviour::Reencoding => {
                    assert_eq!(
                        out.status.code(),
                        Some(0),
                        "{behaviour:?} {max_side:?}: {err}"
                    );
                    assert!(
                        run.join("result.png").exists(),
                        "{behaviour:?} {max_side:?}: no result.png; stdout {stdout} stderr {err}"
                    );
                    // The graph, literally, in the journal's op sequence.
                    let mut expected = vec![
                        "import", "cut", "line", "fan", "profile", "ramp", "render", "apply",
                    ];
                    if max_side.is_some() {
                        expected.push("window");
                    }
                    expected.extend(["render", "filler", "paste"]);
                    if max_side.is_some() {
                        expected.push("unwindow");
                    }
                    expected.extend(["put", "measure"]);
                    assert_eq!(ops(&lines), expected, "{behaviour:?} {max_side:?}");
                    // The filler call note and the stored fill.
                    let calls: Vec<&serde_json::Value> = lines
                        .iter()
                        .filter(|l| {
                            l["note"]
                                .as_str()
                                .is_some_and(|n| n.starts_with("filler call: "))
                        })
                        .collect();
                    assert_eq!(calls.len(), 1);
                    let call: serde_json::Value = serde_json::from_str(
                        calls[0]["note"]
                            .as_str()
                            .unwrap()
                            .strip_prefix("filler call: ")
                            .unwrap(),
                    )
                    .unwrap();
                    assert_eq!(call["outcome"], "ok");
                    assert_eq!(call["status"], 0);
                    let (w, h) = (
                        call["request_width"].as_u64().unwrap(),
                        call["request_height"].as_u64().unwrap(),
                    );
                    match max_side {
                        None => assert_eq!((w, h), (112, 400)),
                        Some(m) => assert!(
                            w <= u64::from(m) && h <= u64::from(m),
                            "windowed to {w}x{h}"
                        ),
                    }
                    // Preservation: every raw pixel the band mask never reaches (w = 0, mapped
                    // through the cut's two strips here, independently of heal's own mapping)
                    // equals the input, and the fill changed pixels inside the band.
                    let result = image::open(run.join("result.png")).unwrap().to_rgb8();
                    let reached = raw_reach_of_band(&run);
                    let mut changed = 0;
                    for (x, y, p) in result.enumerate_pixels() {
                        if p != input.get_pixel(x, y) {
                            changed += 1;
                            assert!(
                                reached[(y * 800 + x) as usize],
                                "{x},{y} changed where w = 0"
                            );
                        }
                    }
                    assert!(changed > 1000, "{behaviour:?}: {changed} pixels changed");
                    // The run page by default, with the filler call row.
                    let page = std::fs::read_to_string(run.join("dash.html")).unwrap();
                    assert!(
                        page.contains("data-calls=\"1\"") && page.contains("fake-filler"),
                        "the calls row"
                    );
                }
                Behaviour::MisSized => {
                    assert_eq!(out.status.code(), Some(3), "{err}");
                    assert!(err.contains("no resize"), "{err}");
                    assert!(!run.join("result.png").exists());
                    assert!(
                        lines.iter().any(|l| l["note"].as_str().is_some_and(|n| n
                            .starts_with("filler call: ")
                            && n.contains("no resize"))),
                        "the failed call is journaled"
                    );
                }
                Behaviour::Failing => {
                    assert_eq!(out.status.code(), Some(3), "{err}");
                    assert!(err.contains("failing as asked"), "{err}");
                }
                Behaviour::Slow => {
                    assert_eq!(out.status.code(), Some(3), "{err}");
                    assert!(err.contains("no answer within 1 s"), "{err}");
                }
                Behaviour::NonPng => unreachable!(),
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn continuity_falls_replay_needs_no_filler_and_no_dash_skips_the_page() {
    let dir = scratch("replay");
    let pano_path = pano();
    let config = command_config(&dir, Behaviour::Exact, None, 10);
    let first = dir.join("first");
    let out = rascal(&[
        "heal",
        "seam",
        pano_path.to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        config.to_str().unwrap(),
        "--out",
        first.to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Continuity at the line falls from the raw canvas to the composite: the journal's measure
    // line measured W' against I_F with the band mask; the raw canvas (img-2, the cut image)
    // is measured here with the same mask (mask-6, ramp's) through the workspace form.
    let lines = journal(&first);
    let measure = lines.iter().find(|l| l["op"] == "measure").unwrap();
    let after_mean = measurement(&measure["envelope"], "continuity.mean");
    let out = rascal(&[
        "measure",
        "--workspace",
        first.to_str().unwrap(),
        "--after",
        "img-2",
        "--mask",
        "mask-6",
        "--set",
        "continuity",
        "--json",
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let before_mean = measurement(&envelope, "continuity.mean");
    assert!(
        after_mean < before_mean,
        "continuity falls: {before_mean} -> {after_mean}"
    );
    // Replay: the same request reuses the recorded fill although the filler is gone.
    let gone = dir.join("gone.toml");
    std::fs::write(
        &gone,
        "config_version = 1\n[fillers.fake]\ncommand = [\"/nonexistent/filler\", \"{image}\", \"{mask}\", \"{out}\"]\nprofile = \"grey\"\nrevision = \"fake\"\n",
    )
    .unwrap();
    let second = dir.join("second");
    let out = rascal(&[
        "heal",
        "seam",
        pano_path.to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        gone.to_str().unwrap(),
        "--out",
        second.to_str().unwrap(),
        "--replay",
        first.to_str().unwrap(),
        "--no-dash",
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read(first.join("result.png")).unwrap(),
        std::fs::read(second.join("result.png")).unwrap(),
        "the replay reproduces the result"
    );
    assert!(!second.join("dash.html").exists(), "--no-dash");
    let replayed = journal(&second);
    assert!(replayed.iter().any(|l| {
        l["note"]
            .as_str()
            .is_some_and(|n| n.contains("replayed from the recorded fill"))
    }));
    assert!(
        !replayed.iter().any(|l| l["note"]
            .as_str()
            .is_some_and(|n| n.starts_with("filler call: "))),
        "no call was made"
    );
    // Without --replay the missing filler is an operational error.
    let third = dir.join("third");
    let out = rascal(&[
        "heal",
        "seam",
        pano_path.to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        gone.to_str().unwrap(),
        "--out",
        third.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(3));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_key_appears_in_no_file_under_out_and_in_no_span() {
    let dir = scratch("key");
    let key = "sk-live-9f8e7d6c5b4a-never-written";
    let (url, seen) = spawn(Behaviour::Exact, Some(key.to_string()));
    let config = dir.join("hosted.toml");
    std::fs::write(
        &config,
        format!("config_version = 1\n[fillers.hosted]\nurl = \"{url}\"\nprofile = \"grey\"\nkey_env = \"HEAL_TEST_KEY\"\ntimeout_s = 20\n"),
    )
    .unwrap();
    let run = dir.join("run");
    let out = rascal_env(
        &[
            "heal",
            "seam",
            pano().to_str().unwrap(),
            "--filler",
            "hosted",
            "--config",
            config.to_str().unwrap(),
            "--out",
            run.to_str().unwrap(),
        ],
        &[("HEAL_TEST_KEY", key), ("RASCAL_LOG", "trace")],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        seen.lock().unwrap()[0].authorization.as_deref(),
        Some(&*format!("Bearer {key}"))
    );
    let log = String::from_utf8_lossy(&out.stderr);
    assert!(
        log.contains("filler call finished"),
        "the filler span: {log}"
    );
    assert!(!log.contains(key), "the key is in a span");
    for entry in walk(&run) {
        let bytes = std::fs::read(&entry).unwrap();
        assert!(
            !bytes.windows(key.len()).any(|w| w == key.as_bytes()),
            "{} holds the key",
            entry.display()
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The raw pixels the run's band mask reaches, mapped by the test: the canvas's left half is
/// the panorama's rightmost strip and its right half the leftmost (strip 56 of 800).
fn raw_reach_of_band(run: &Path) -> Vec<bool> {
    let out = rascal(&[
        "workspace",
        "list",
        run.to_str().unwrap(),
        "--type",
        "Mask",
        "--json",
    ]);
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    // The ramp's mask is the last cut-frame mask with a prescription: the recipe's output.
    let band = rows
        .iter()
        .rfind(|r| r["frame"].as_str().unwrap().starts_with("cut:"))
        .unwrap();
    let hex = band["identity"]
        .as_str()
        .unwrap()
        .trim_start_matches("sha256:")
        .to_string();
    let mask =
        rascal_core::mask::Mask::read(&run.join("artifacts").join(format!("{hex}.mask"))).unwrap();
    assert_eq!(mask.dimensions(), (112, 400));
    let mut reached = vec![false; 800 * 400];
    for y in 0..400 {
        for x in 0..112 {
            if mask.weight(x, y) > 0.0 {
                let rx = if x < 56 { 800 - 56 + x } else { x - 56 };
                reached[(y * 800 + rx) as usize] = true;
            }
        }
    }
    reached
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn heal_over_http_with_an_image_input_profile_send_mask_and_a_prompt() {
    let dir = scratch("tinted");
    let (url, seen) = spawn(Behaviour::Exact, None);
    let config = dir.join("hosted.toml");
    std::fs::write(
        &config,
        format!("config_version = 1\n[fillers.hosted]\nurl = \"{url}\"\nprofile = \"tinted\"\nsend_mask = true\npaste = \"hard\"\ntimeout_s = 20\n"),
    )
    .unwrap();
    let run = dir.join("run");
    let out = rascal_env(
        &[
            "heal",
            "seam",
            pano().to_str().unwrap(),
            "--filler",
            "hosted",
            "--config",
            config.to_str().unwrap(),
            "--out",
            run.to_str().unwrap(),
            "--prompt",
            "the sky",
        ],
        &[("RASCAL_LOG", "trace")],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let request = &seen.lock().unwrap()[0];
    assert!(
        request.has_image && request.has_mask,
        "the rendering as image and the grey as mask"
    );
    assert_eq!(request.prompt.as_deref(), Some("the sky"));
    // The image sent is the tinted rendering (an RGB PNG), not the working image: the journal's
    // render steps say which renderings were made, and the grey one is there for send_mask.
    let lines = journal(&run);
    let renders: Vec<&str> = lines
        .iter()
        .filter(|l| l["op"] == "render")
        .map(|l| l["params"]["for"].as_str().unwrap())
        .collect();
    assert_eq!(
        renders,
        ["grey", "tinted", "grey"],
        "the recipe's grey, then heal's tinted and grey"
    );
    // The filler span carries the request's size and the event its status.
    let log = String::from_utf8_lossy(&out.stderr);
    let span_line = log
        .lines()
        .find(|l| l.contains("filler{"))
        .unwrap_or_else(|| panic!("a filler span: {log}"));
    assert!(
        span_line.contains("request_bytes=")
            && span_line.contains("width=112")
            && span_line.contains("height=400"),
        "{span_line}"
    );
    assert!(
        log.lines()
            .any(|l| l.contains("filler call finished") && l.contains("status=200")),
        "{log}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_max_side_under_the_support_stops_at_window_and_still_writes_the_page() {
    let dir = scratch("small");
    let config = command_config(&dir, Behaviour::Exact, Some(100), 10);
    let run = dir.join("run");
    let out = rascal(&[
        "heal",
        "seam",
        pano().to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        config.to_str().unwrap(),
        "--out",
        run.to_str().unwrap(),
        "--strict",
    ]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("step window reported not_applicable") && err.contains("max_side is 100"),
        "{err}"
    );
    assert!(!run.join("result.png").exists());
    assert!(
        run.join("dash.html").exists() && run.join("frames.json").exists(),
        "the page and the tables are written for a stopped heal"
    );
    assert!(!journal(&run).iter().any(|l| l["op"] == "paste"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_replay_with_a_changed_request_or_another_filler_calls_the_filler() {
    let dir = scratch("replay2");
    let config = command_config(&dir, Behaviour::Exact, None, 10);
    let first = dir.join("first");
    let out = rascal(&[
        "heal",
        "seam",
        pano().to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        config.to_str().unwrap(),
        "--out",
        first.to_str().unwrap(),
        "--no-dash",
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The same image but a prompt: another request, so the (absent) filler is called and fails.
    let gone = dir.join("gone.toml");
    std::fs::write(&gone, "config_version = 1\n[fillers.fake]\ncommand = [\"/nonexistent/filler\", \"{image}\", \"{mask}\", \"{out}\"]\nprofile = \"grey\"\nrevision = \"fake\"\n").unwrap();
    let second = dir.join("second");
    let out = rascal(&[
        "heal",
        "seam",
        pano().to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        gone.to_str().unwrap(),
        "--out",
        second.to_str().unwrap(),
        "--replay",
        first.to_str().unwrap(),
        "--prompt",
        "different",
        "--no-dash",
    ]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines = journal(&second);
    assert!(lines.iter().any(|l| {
        l["note"]
            .as_str()
            .is_some_and(|n| n.starts_with("filler call: ") && n.contains("cannot start"))
    }));
    assert!(
        !lines
            .iter()
            .any(|l| l["note"].as_str().is_some_and(|n| n.contains("replayed")))
    );
    // The same request but another filler name (or revision) does not reuse the fill either.
    let other = dir.join("other.toml");
    std::fs::write(&other, "config_version = 1\n[fillers.other]\ncommand = [\"/nonexistent/filler\", \"{image}\", \"{mask}\", \"{out}\"]\nprofile = \"grey\"\nrevision = \"fake\"\n").unwrap();
    let third = dir.join("third");
    let out = rascal(&[
        "heal",
        "seam",
        pano().to_str().unwrap(),
        "--filler",
        "other",
        "--config",
        other.to_str().unwrap(),
        "--out",
        third.to_str().unwrap(),
        "--replay",
        first.to_str().unwrap(),
        "--no-dash",
    ]);
    assert_eq!(out.status.code(), Some(3));
    // A replay directory without a journal is a usage error naming the rule.
    let fourth = dir.join("fourth");
    let out = rascal(&[
        "heal",
        "seam",
        pano().to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        gone.to_str().unwrap(),
        "--out",
        fourth.to_str().unwrap(),
        "--replay",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("no journal there"));
    let _ = std::fs::remove_dir_all(&dir);
}
