//! Task 1.7a: the workspace from the command line. Two shells run `rascal rect --workspace`
//! concurrently fifty times each; the final generation lists one hundred locations and
//! `rascal workspace list` agrees.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn rascal(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .output()
        .unwrap()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-wscli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn import_then_rect_from_two_shells_fifty_times_each() {
    let dir = scratch("shells");
    let ws = dir.join("ws");
    let floor = repo_root().join("fixtures/images/floor.png");
    let out = rascal(&[
        "workspace",
        "import",
        ws.to_str().unwrap(),
        floor.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let printed = String::from_utf8_lossy(&out.stdout);
    assert!(printed.starts_with("img-1 raw:320x200:flat"), "{printed}");

    let shells: Vec<_> = (0..2)
        .map(|shell| {
            let ws = ws.clone();
            std::thread::spawn(move || {
                for i in 0..50 {
                    let x0 = 0.004 * (shell * 50 + i) as f64;
                    let bbox = format!("{x0},0.1,{},0.5", x0 + 0.1);
                    let out = rascal(&[
                        "rect",
                        "--workspace",
                        ws.to_str().unwrap(),
                        "--image",
                        "img-1",
                        "--bbox",
                        &bbox,
                    ]);
                    assert!(
                        out.status.success(),
                        "{}",
                        String::from_utf8_lossy(&out.stderr)
                    );
                    let text = String::from_utf8_lossy(&out.stdout);
                    assert!(
                        text.contains("location: loc-") && text.contains("committed: generation"),
                        "{text}"
                    );
                }
            })
        })
        .collect();
    for shell in shells {
        shell.join().unwrap();
    }

    let out = rascal(&[
        "workspace",
        "list",
        ws.to_str().unwrap(),
        "--type",
        "Location",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 100);
    let current: u64 = std::fs::read_to_string(ws.join("CURRENT"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(current, 101, "one import and one hundred rects");
    let generation: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(ws.join(format!("registry/{current}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(generation["locations"].as_array().unwrap().len(), 100);
    assert_eq!(generation["records"].as_array().unwrap().len(), 101);
    let journal = std::fs::read_to_string(ws.join("journal.jsonl")).unwrap();
    assert_eq!(journal.lines().count(), 101, "one line per invocation");
    // The plain listing names the image too.
    let out = rascal(&["workspace", "list", ws.to_str().unwrap()]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("img-1"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_op_subcommand_takes_ports_by_id_and_parameters_by_kind() {
    let dir = scratch("op");
    let ws = dir.join("ws");
    let floor = repo_root().join("fixtures/images/floor.png");
    assert!(
        rascal(&[
            "workspace",
            "import",
            ws.to_str().unwrap(),
            floor.to_str().unwrap()
        ])
        .status
        .success()
    );
    // rect then rasterize by ids, then render by id with a choice parameter.
    let out = rascal(&[
        "rect",
        "--workspace",
        ws.to_str().unwrap(),
        "--image",
        "img-1",
        "--bbox",
        "0.2,0.2,0.4,0.5",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = rascal(&[
        "rasterize",
        "--workspace",
        ws.to_str().unwrap(),
        "--image",
        "img-1",
        "--location",
        "loc-2",
        "--use_case",
        "repair",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("mask: mask-3"), "{text}");
    // A missing required port is a usage error from clap; an unknown id is a usage error from
    // the workspace; a bad parameter is invalid_args (64); not_applicable is 1 under --strict.
    let out = rascal(&[
        "rasterize",
        "--workspace",
        ws.to_str().unwrap(),
        "--image",
        "img-1",
    ]);
    assert_eq!(out.status.code(), Some(64));
    let out = rascal(&[
        "rect",
        "--workspace",
        ws.to_str().unwrap(),
        "--image",
        "img-9",
        "--bbox",
        "0.2,0.2,0.4,0.5",
    ]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("no such artifact"));
    let out = rascal(&[
        "rect",
        "--workspace",
        ws.to_str().unwrap(),
        "--image",
        "img-1",
        "--bbox",
        "0.4,0.2,0.2,0.5",
    ]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = rascal(&[
        "rect",
        "--workspace",
        ws.to_str().unwrap(),
        "--image",
        "img-1",
        "--bbox",
        "1.2,1.2,1.4,1.5",
        "--strict",
    ]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = rascal(&[
        "rect",
        "--workspace",
        ws.to_str().unwrap(),
        "--image",
        "img-1",
        "--bbox",
        "1.2,1.2,1.4,1.5",
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The help names the op's ports and parameters.
    let out = rascal(&["rasterize", "--help"]);
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(
        help.contains("--location") && help.contains("--use_case") && help.contains("--workspace"),
        "{help}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_run_with_a_shared_workspace_keeps_artifacts_there() {
    let dir = scratch("shared");
    let ws = dir.join("ws");
    let floor = repo_root().join("fixtures/images/floor.png");
    let out = rascal(&[
        "run",
        "rect",
        floor.to_str().unwrap(),
        "--bbox",
        "0.2,0.2,0.4,0.5",
        "--workspace",
        ws.to_str().unwrap(),
        "--out",
        dir.join("run").to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join("run/rect-grey.png").is_file());
    assert!(dir.join("run/frames.json").is_file());
    assert!(
        !dir.join("run/CURRENT").exists(),
        "the run directory is not a workspace of its own"
    );
    assert!(ws.join("CURRENT").exists());
    let masks = std::fs::read_dir(ws.join("artifacts"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|x| x == "mask")
        })
        .count();
    assert_eq!(masks, 1);
    let _ = std::fs::remove_dir_all(&dir);
}
