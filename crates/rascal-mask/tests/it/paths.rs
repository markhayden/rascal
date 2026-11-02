//! Task 10.1 (CONTRACT 5.2): `rascal serve` reads a caller's path only inside the directories
//! `--allow-dir` names, narrowed by `[serve] allow_dirs`. A file inside (in a subdirectory,
//! through a symlink that stays inside, through a link outside whose target is inside, through
//! an allowed directory that is itself a symlink) is read; a path outside, a `..` escape, a
//! symlink inside pointing out, a missing file inside or outside, a directory and a relative
//! path are each refused with one text that says nothing about the file. With no
//! `--allow-dir` every path is refused and inline images still work. A bad `--allow-dir` exits
//! 64 before anything is answered; the allowed set is printed on stderr. `serve` writes only
//! under its runs root, workspace and cache, and the person's own command line is not held to the rule.
//! Task 10.2 (CONTRACT 4.6): `rascal://session` lists the set as `allowed_dirs`, `[]` with none.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use rascal_testkit::mcp_client::{Client, Failure};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pano() -> PathBuf {
    repo_root().join("fixtures/images/step-pano.png")
}

/// The refusal every refused path gets, by slot.
fn refusal(slot: &str) -> String {
    format!(
        "{slot}: this path is not readable here (only files inside the directories rascal serve allows; CONTRACT 5.2)"
    )
}

/// A scratch directory, canonical, laid out as:
///
/// ```text
/// in/pano.png             the allowed directory's image
/// in/sub/pano.png         one in a subdirectory
/// in/inside-link.png  ->  in/pano.png        (stays inside)
/// in/out-link.png     ->  secret/pano.png    (points out)
/// secret/pano.png         outside
/// secret/in-link.png  ->  in/pano.png        (outside, pointing in)
/// secret/mask/            a stored mask (the seam golden), outside
/// link-dir            ->  in                 (a directory that is a symlink)
/// ```
fn layout(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-paths-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    let (inside, secret) = (dir.join("in"), dir.join("secret"));
    std::fs::create_dir_all(inside.join("sub")).unwrap();
    std::fs::create_dir_all(&secret).unwrap();
    for to in [
        inside.join("pano.png"),
        inside.join("sub/pano.png"),
        secret.join("pano.png"),
    ] {
        std::fs::copy(pano(), to).unwrap();
    }
    let link = |target: &Path, at: &Path| std::os::unix::fs::symlink(target, at).unwrap();
    link(&inside.join("pano.png"), &inside.join("inside-link.png"));
    link(&secret.join("pano.png"), &inside.join("out-link.png"));
    link(&inside.join("pano.png"), &secret.join("in-link.png"));
    link(&inside, &dir.join("link-dir"));
    copy_dir(
        &repo_root().join("fixtures/masks/seam-step-pano.mask"),
        &secret.join("mask"),
    );
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
    }
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// `rascal serve` over `dir` (its runs, workspace and data home inside it) with `args` after
/// the defaults; nothing but what `args` gives is allowed.
fn serve_command(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "serve",
            "--workspace",
            s(&dir.join("ws")),
            "--runs",
            s(&dir.join("runs")),
        ])
        .args(args)
        .env_remove("RASCAL_LOG")
        .env_remove("RASCAL_TOOLS_MODELS")
        .env("HOME", dir.join("home"))
        .env("XDG_DATA_HOME", dir.join("data"))
        .env("XDG_CACHE_HOME", dir.join("cache"));
    command
}

fn start(dir: &Path, args: &[&str]) -> Client {
    Client::start(&mut serve_command(dir, args)).unwrap()
}

/// `rascal serve` with `args` and stdin closed: it starts, prints its start lines and ends at
/// once with no client (or exits 64 on a bad flag, before any of that).
fn start_and_end(dir: &Path, args: &[&str]) -> Output {
    serve_command(dir, args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A `seam` call on `path`'s image: its result.
fn seam(client: &mut Client, path: &str) -> serde_json::Value {
    client
        .call("seam", serde_json::json!({ "image": { "path": path } }))
        .unwrap()
        .structured
        .unwrap()
}

/// Asserts `result` is `invalid`/`input` with exactly the refusal for `slot`.
fn refused(result: &serde_json::Value, slot: &str) {
    assert_eq!(result["status"], "invalid", "{result:#}");
    assert_eq!(result["error"]["code"], "input", "{result:#}");
    assert_eq!(result["error"]["reason"], refusal(slot), "{result:#}");
}

/// The estimate of `action` with `arguments`: `Ok` with the estimate, or the JSON-RPC error.
fn estimate(
    client: &mut Client,
    action: &str,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, (i64, Option<serde_json::Value>)> {
    match client.call(
        "estimate",
        serde_json::json!({ "action": action, "arguments": arguments }),
    ) {
        Ok(out) => Ok(out.structured.unwrap()),
        Err(Failure::Rpc { code, data, .. }) => Err((code, data)),
        Err(other) => panic!("{other:?}"),
    }
}

/// Every file under `dir`, relative to it.
fn listing(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                stack.push(path);
            } else {
                out.push(path.strip_prefix(dir).unwrap().to_path_buf());
            }
        }
    }
    out.sort();
    out
}

#[test]
fn a_file_inside_runs_and_every_escape_reads_the_same() {
    let dir = layout("escapes");
    let inside = dir.join("in");
    let mut client = start(&dir, &["--allow-dir", s(&inside)]);
    // Inside, in a subdirectory, through a link that stays inside, and through a link outside
    // whose target is inside (the rule is on the canonical form).
    for path in [
        inside.join("pano.png"),
        inside.join("sub/pano.png"),
        inside.join("inside-link.png"),
        dir.join("secret/in-link.png"),
        dir.join("link-dir/pano.png"),
    ] {
        let result = seam(&mut client, s(&path));
        assert_eq!(
            result["status"],
            "completed",
            "{}: {result:#}",
            path.display()
        );
    }
    // Outside, a `..` escape, a link inside pointing out, missing inside, missing outside, a
    // directory, a relative path, a file that is not there at all: one reason, byte for byte.
    let refused_paths = [
        s(&dir.join("secret/pano.png")).to_string(),
        format!("{}/../secret/pano.png", s(&inside)),
        s(&inside.join("out-link.png")).to_string(),
        s(&inside.join("missing.png")).to_string(),
        s(&dir.join("secret/missing.png")).to_string(),
        s(&inside.join("sub")).to_string(),
        "in/pano.png".to_string(),
        "/etc/hosts".to_string(),
    ];
    let ws_before = listing(&dir.join("ws"));
    let mut reasons = Vec::new();
    for path in &refused_paths {
        let result = seam(&mut client, path);
        refused(&result, "image");
        // Nothing of the refused image entered the run.
        assert_eq!(result["images"]["in"], serde_json::json!([]), "{result:#}");
        reasons.push(result["error"]["reason"].clone());
    }
    assert!(reasons.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(listing(&dir.join("ws")), ws_before);
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn estimate_and_import_are_held_to_the_rule() {
    let dir = layout("slots");
    let inside = dir.join("in");
    let mut client = start(&dir, &["--allow-dir", s(&inside)]);
    let image = serde_json::json!({ "path": s(&inside.join("pano.png")) });

    // estimate: inside is an estimate; outside and missing are the same -32602 refusal.
    estimate(&mut client, "seam", serde_json::json!({ "image": image })).unwrap();
    let mut seen = Vec::new();
    for path in [dir.join("secret/pano.png"), inside.join("missing.png")] {
        let (code, data) = estimate(
            &mut client,
            "seam",
            serde_json::json!({ "image": { "path": s(&path) } }),
        )
        .unwrap_err();
        assert_eq!(code, -32602);
        let data = data.unwrap();
        assert_eq!(data["reason"], "invalid_arguments", "{data:#}");
        assert_eq!(data["error"]["code"], "input", "{data:#}");
        assert_eq!(data["error"]["reason"], refusal("image"));
        seen.push(data);
    }
    assert_eq!(seen[0], seen[1]);

    // A Mask argument is a reference, never a path (task 10.3): a path there, even to a
    // stored mask, is a wrong request, refused before anything is read or run.
    let protect = serde_json::json!({ "image": image, "mask": s(&dir.join("secret/mask")) });
    let (code, data) = estimate(&mut client, "protect", protect.clone()).unwrap_err();
    assert_eq!(code, -32602);
    let data = data.unwrap();
    assert_eq!(data["error"]["code"], "usage", "{data:#}");
    assert!(
        data["error"]["reason"]
            .as_str()
            .unwrap()
            .starts_with("mask: not an artifact reference"),
        "{data:#}"
    );
    let runs = listing(&dir.join("runs"));
    match client.call("protect", protect) {
        Err(Failure::Rpc { code, data, .. }) => {
            assert_eq!(code, -32602);
            assert_ne!(
                data.as_ref().map(|d| d["reason"].clone()),
                Some("binding".into())
            );
        }
        other => panic!("a path for a reference: {other:?}"),
    }
    assert_eq!(listing(&dir.join("runs")), runs, "no run");

    // An operation's image port by path (task 10.6): inside runs; outside and missing are each
    // an `invalid`/`input` run with the one refusal, and nothing outside enters the store.
    let line = |client: &mut Client, path: &Path| {
        client
            .call(
                "line",
                serde_json::json!({ "image": { "path": s(path) }, "column": 0.5, "width_px": 8 }),
            )
            .unwrap()
            .structured
            .unwrap()
    };
    let ran = line(&mut client, &inside.join("pano.png"));
    assert_eq!(ran["status"], "completed", "{ran:#}");
    for path in [dir.join("secret/pano.png"), dir.join("secret/missing.png")] {
        let result = line(&mut client, &path);
        refused(&result, "image");
        assert_eq!(
            result["artifacts"]["in"],
            serde_json::json!([]),
            "{result:#}"
        );
    }
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn with_no_allowed_directory_every_path_is_refused_and_inline_runs() {
    let dir = layout("closed");
    let mut client = start(&dir, &[]);
    assert_eq!(allowed_dirs(&mut client), serde_json::json!([]));
    for path in [dir.join("in/pano.png"), dir.join("in/missing.png")] {
        refused(&seam(&mut client, s(&path)), "image");
    }
    let data = rascal_core::panel::base64(&std::fs::read(pano()).unwrap());
    let result = client
        .call(
            "seam",
            serde_json::json!({ "image": { "inline": { "media_type": "image/png", "data": data } } }),
        )
        .unwrap()
        .structured
        .unwrap();
    assert_eq!(result["status"], "completed", "{result:#}");
    let line = client
        .call(
            "line",
            serde_json::json!({ "image": { "path": s(&dir.join("in/pano.png")) }, "column": 0.5, "width_px": 8 }),
        )
        .unwrap()
        .structured
        .unwrap();
    refused(&line, "image");
    client.close().unwrap();
    let out = start_and_end(&dir, &[]);
    assert_ne!(out.status.code(), Some(64), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("no directory is allowed, so every caller path is refused"),
        "{}",
        stderr(&out)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `rascal://session`'s `allowed_dirs` (4.6).
fn allowed_dirs(client: &mut Client) -> serde_json::Value {
    let result = client
        .request(
            "resources/read",
            serde_json::json!({ "uri": "rascal://session" }),
        )
        .unwrap();
    let session: serde_json::Value =
        serde_json::from_str(result["contents"][0]["text"].as_str().unwrap()).unwrap();
    session["allowed_dirs"].clone()
}

/// The line `serve` prints with the allowed set, as a list.
fn allowed_line(out: &Output) -> Vec<String> {
    let text = stderr(out);
    if text.contains("no directory is allowed") {
        return Vec::new();
    }
    text.lines()
        .find_map(|l| l.strip_prefix("rascal serve: callers' paths are read only inside: "))
        .unwrap_or_else(|| panic!("no allowed line in {text}"))
        .split(", ")
        .map(str::to_string)
        .collect()
}

#[test]
fn the_configuration_narrows_to_the_overlap_and_never_widens() {
    let dir = layout("overlap");
    let (inside, sub) = (dir.join("in"), dir.join("in/sub"));
    let config = |name: &str, dirs: &[&Path]| {
        let path = dir.join(format!("{name}.toml"));
        let list: Vec<String> = dirs.iter().map(|d| format!("{:?}", s(d))).collect();
        std::fs::write(
            &path,
            format!(
                "config_version = 1\n[serve]\nallow_dirs = [{}]\n",
                list.join(", ")
            ),
        )
        .unwrap();
        path
    };
    let set = |args: &[&str]| {
        let out = start_and_end(&dir, args);
        assert_ne!(out.status.code(), Some(64), "{}", stderr(&out));
        allowed_line(&out)
    };
    let with = |config: &Path| set(&["--allow-dir", s(&inside), "--config", s(config)]);
    // No [serve]: the flags as given (resolved: the link to `in` is `in`).
    assert_eq!(
        set(&["--allow-dir", s(&dir.join("link-dir"))]),
        vec![s(&inside)]
    );
    // A configured subdirectory of the flag directory replaces it.
    assert_eq!(with(&config("sub", &[&sub])), vec![s(&sub)]);
    // A configured directory over the flag directory leaves the flag directory.
    assert_eq!(with(&config("over", &[&dir])), vec![s(&inside)]);
    // A configured directory beside it, and `[]`, allow nothing.
    assert!(with(&config("beside", &[&dir.join("secret")])).is_empty());
    assert!(with(&config("empty", &[])).is_empty());
    // A configured directory with no flag allows nothing.
    assert!(set(&["--config", s(&config("alone", &[&inside]))]).is_empty());
    // A configuration that does not load allows nothing (fail closed), and says why.
    let broken = dir.join("broken.toml");
    std::fs::write(
        &broken,
        "config_version = 1\n[serve]\nallow_dirs = [\"sub\"]\n",
    )
    .unwrap();
    assert!(with(&broken).is_empty());
    let out = start_and_end(&dir, &["--allow-dir", s(&inside), "--config", s(&broken)]);
    assert!(
        stderr(&out).contains(
            "the configuration does not load, so what its [serve] allow_dirs would allow is unknown and no caller path is allowed"
        ),
        "{}",
        stderr(&out)
    );

    // And the calls follow the set: with `in/sub` configured, `in/pano.png` is refused.
    let narrowed = config("calls", &[&sub]);
    let mut client = start(&dir, &["--allow-dir", s(&inside), "--config", s(&narrowed)]);
    assert_eq!(allowed_dirs(&mut client), serde_json::json!([s(&sub)]));
    refused(&seam(&mut client, s(&inside.join("pano.png"))), "image");
    estimate(
        &mut client,
        "seam",
        serde_json::json!({ "image": { "path": s(&sub.join("pano.png")) } }),
    )
    .unwrap();
    client.close().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bad_allow_dir_exits_64_before_answering() {
    let dir = layout("bad-flag");
    for (flag, why) in [
        ("in".to_string(), "not an absolute path"),
        (s(&dir.join("absent")).to_string(), "no such directory"),
        (s(&dir.join("in/pano.png")).to_string(), "not a directory"),
    ] {
        let out = start_and_end(&dir, &["--allow-dir", &flag]);
        assert_eq!(out.status.code(), Some(64), "{flag}: {}", stderr(&out));
        assert!(out.stdout.is_empty(), "{flag}: answered something");
        assert!(
            stderr(&out).contains(&format!("--allow-dir {flag}: {why}")),
            "{}",
            stderr(&out)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn serve_writes_only_under_its_own_directories() {
    let dir = layout("writes");
    let inside = dir.join("in");
    let before = listing(&dir);
    let mut client = start(&dir, &["--allow-dir", s(&inside)]);
    let path = s(&inside.join("pano.png")).to_string();
    let by_path = client
        .call(
            "seam",
            serde_json::json!({ "image": { "path": path }, "return_images": "path" }),
        )
        .unwrap();
    assert_eq!(by_path.structured.unwrap()["status"], "completed");
    let data = rascal_core::panel::base64(&std::fs::read(pano()).unwrap());
    let inline = client
        .call(
            "seam",
            serde_json::json!({ "image": { "inline": { "media_type": "image/png", "data": data } } }),
        )
        .unwrap();
    assert_eq!(inline.structured.unwrap()["status"], "completed");
    // An operation by path and inline: its outputs commit to the store, its previews go to its
    // run directory.
    for image in [
        serde_json::json!({ "path": path }),
        serde_json::json!({ "inline": { "media_type": "image/png", "data": data } }),
    ] {
        let line = client
            .call(
                "line",
                serde_json::json!({ "image": image, "column": 0.5, "width_px": 8 }),
            )
            .unwrap();
        assert_eq!(line.structured.unwrap()["status"], "completed");
    }
    client.close().unwrap();
    let created: Vec<PathBuf> = listing(&dir)
        .into_iter()
        .filter(|p| !before.contains(p))
        .collect();
    assert!(!created.is_empty());
    for path in &created {
        assert!(
            ["runs", "ws", "cache"]
                .iter()
                .any(|root| path.starts_with(root)),
            "serve wrote {} outside its own directories",
            path.display()
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_persons_own_command_line_takes_any_path() {
    let dir = layout("own");
    let outside = dir.join("secret/pano.png");
    let run = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(["run", "seam", s(&outside), "--out", s(&dir.join("run"))])
        .env_remove("RASCAL_LOG")
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let mut estimate = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(["estimate", "--json", "-"])
        .env_remove("RASCAL_LOG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let request = serde_json::json!({ "action": "seam",
            "arguments": { "image": { "path": s(&outside) } } });
        estimate
            .stdin
            .take()
            .unwrap()
            .write_all(request.to_string().as_bytes())
            .unwrap();
    }
    let out = estimate.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let _ = std::fs::remove_dir_all(&dir);
}
