//! Task 1.6: a shipped recipe by name and by path give semantically identical journals; the
//! binding flags bind what the recipe declares and refuse what it does not; the config file.
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
    let dir = std::env::temp_dir().join(format!("rascal-byname-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn journal(dir: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir.join("journal.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// Everything but the observations: durations go, the rest stays.
fn semantic(mut line: serde_json::Value) -> serde_json::Value {
    line.as_object_mut().unwrap().remove("duration_ms");
    if let Some(env) = line.get_mut("envelope").and_then(|e| e.as_object_mut())
        && let Some(prov) = env.get_mut("provenance").and_then(|p| p.as_object_mut())
    {
        prov.remove("duration_ms");
    }
    line
}

#[test]
fn by_name_and_by_path_agree_on_semantic_content() {
    let dir = scratch("agree");
    let floor = repo_root().join("fixtures/images/floor.png");
    let floor = floor.to_str().unwrap();
    let path = repo_root().join("crates/rascal-mask/recipes/rect.toml");
    let a = dir.join("a");
    let b = dir.join("b");
    assert!(
        rascal(&[
            "run",
            "rect",
            floor,
            "--bbox",
            "0.2,0.2,0.4,0.5",
            "--out",
            a.to_str().unwrap()
        ])
        .status
        .success()
    );
    let out = rascal(&[
        "run",
        path.to_str().unwrap(),
        floor,
        "--bbox",
        "0.2,0.2,0.4,0.5",
        "--out",
        b.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ja: Vec<_> = journal(&a).into_iter().map(semantic).collect();
    let jb: Vec<_> = journal(&b).into_iter().map(semantic).collect();
    assert_eq!(ja, jb);
    let rasterize = ja.iter().find(|l| l["step"] == "rasterize").unwrap();
    assert_eq!(
        rasterize["params"]["use_case"], "custom",
        "the resolved default is in the journal"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn location_files_bind_and_mismatched_flags_are_refused() {
    let dir = scratch("flags");
    let floor = repo_root().join("fixtures/images/floor.png");
    let floor = floor.to_str().unwrap();
    // --location with a bbox file, frame omitted: bound into the image's frame.
    let loc = dir.join("box.json");
    std::fs::write(&loc, r#"{"kind":"bbox","bbox":[0.2,0.2,0.4,0.5]}"#).unwrap();
    let out = rascal(&[
        "run",
        "rect",
        floor,
        "--location",
        loc.to_str().unwrap(),
        "--out",
        dir.join("r1").to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let grey = image::open(dir.join("r1/rect-grey.png"))
        .unwrap()
        .to_luma8();
    assert_eq!(grey.get_pixel(100, 60)[0], 255);
    // The same with the frame named wrongly.
    std::fs::write(
        &loc,
        r#"{"kind":"bbox","frame":"raw:1x1:flat","bbox":[0.2,0.2,0.4,0.5]}"#,
    )
    .unwrap();
    let out = rascal(&[
        "run",
        "rect",
        floor,
        "--location",
        loc.to_str().unwrap(),
        "--out",
        dir.join("r2").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).contains("names frame raw:1x1:flat"));
    // A malformed location file.
    std::fs::write(&loc, r#"{"kind":"blob"}"#).unwrap();
    let out = rascal(&[
        "run",
        "rect",
        floor,
        "--location",
        loc.to_str().unwrap(),
        "--out",
        dir.join("r3").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    // A flag the recipe does not declare, and a declared input without its flag.
    let out = rascal(&[
        "run",
        "rect",
        floor,
        "--bbox",
        "0.2,0.2,0.4,0.5",
        "--stroke",
        loc.to_str().unwrap(),
        "--out",
        dir.join("r4").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&out.stderr).contains("declares no inputs.stroke"));
    let out = rascal(&[
        "run",
        "rect",
        floor,
        "--out",
        dir.join("r5").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("needs inputs.location: give --bbox or --location")
    );
    // --bbox and --location together.
    let out = rascal(&[
        "run",
        "rect",
        floor,
        "--bbox",
        "0.2,0.2,0.4,0.5",
        "--location",
        loc.to_str().unwrap(),
        "--out",
        dir.join("r6").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    // --prompt binds a Text input; rect declares none.
    let out = rascal(&[
        "run",
        "rect",
        floor,
        "--bbox",
        "0.2,0.2,0.4,0.5",
        "--prompt",
        "chair",
        "--out",
        dir.join("r7").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("declares no inputs.prompt; --prompt does not apply")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mask_and_stroke_flags_bind_to_recipes_that_declare_them() {
    let dir = scratch("maskflags");
    let floor = repo_root().join("fixtures/images/floor.png");
    let floor = floor.to_str().unwrap();
    // A mask to bind: the rect run's.
    let first = dir.join("first");
    assert!(
        rascal(&[
            "run",
            "rect",
            floor,
            "--bbox",
            "0.2,0.2,0.4,0.5",
            "--out",
            first.to_str().unwrap()
        ])
        .status
        .success()
    );
    let mask = std::fs::read_dir(first.join("artifacts"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "mask"))
        .unwrap();
    // A recipe that takes inputs.mask and renders it.
    let recipe = dir.join("show.toml");
    std::fs::write(
        &recipe,
        r#"
version = 1
name = "show"
use_case = "custom"
[inputs]
image = { type = "Image" }
mask = { type = "Mask" }
[[step]]
id = "r"
op = "render"
inputs = { mask = "inputs.mask", image = "inputs.image" }
for = "tinted"
[outputs]
mask = "inputs.mask"
image = "inputs.image"
render = [{ for = "overlay", out = "o.png" }]
"#,
    )
    .unwrap();
    let out = rascal(&[
        "run",
        recipe.to_str().unwrap(),
        floor,
        "--mask",
        mask.to_str().unwrap(),
        "--out",
        dir.join("show").to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join("show/o.png").is_file());
    // A stroke file must be a stroke.
    let recipe2 = dir.join("stroke.toml");
    std::fs::write(
        &recipe2,
        r#"
version = 1
name = "stroke"
use_case = "repair"
[inputs]
image = { type = "Image" }
stroke = { type = "Location" }
[[step]]
id = "m"
op = "rasterize"
inputs = { location = "inputs.stroke", image = "inputs.image" }
[outputs]
mask = "m.mask"
"#,
    )
    .unwrap();
    let stroke = dir.join("stroke.json");
    std::fs::write(&stroke, r#"{"kind":"bbox","bbox":[0.2,0.2,0.4,0.5]}"#).unwrap();
    let out = rascal(&[
        "run",
        recipe2.to_str().unwrap(),
        floor,
        "--stroke",
        stroke.to_str().unwrap(),
        "--out",
        dir.join("s1").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("expected a stroke location, got a bbox")
    );
    std::fs::write(
        &stroke,
        r#"{"kind":"stroke","path":[[0.1,0.1],[0.3,0.3]],"radius_px":4}"#,
    )
    .unwrap();
    // The stroke binds and rasterizes to the dilated path.
    let out = rascal(&[
        "run",
        recipe2.to_str().unwrap(),
        floor,
        "--stroke",
        stroke.to_str().unwrap(),
        "--out",
        dir.join("s2").to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mask = std::fs::read_dir(dir.join("s2/artifacts"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "mask"))
        .unwrap();
    let core = image::open(mask.join("core.png")).unwrap().to_luma8();
    assert_eq!(
        core.get_pixel(64, 40)[0],
        255,
        "on the path: (0.2, 0.2) of 320x200"
    );
    assert_eq!(core.get_pixel(200, 40)[0], 0, "far from it");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_config_flag_reads_the_file() {
    let dir = scratch("config");
    let floor = repo_root().join("fixtures/images/floor.png");
    let floor = floor.to_str().unwrap();
    let config = dir.join("rascal.toml");
    std::fs::write(&config, "config_version = 1\n[workspace]\ndir = \"ws\"\n").unwrap();
    let out = rascal(&[
        "run",
        "rect",
        floor,
        "--bbox",
        "0.2,0.2,0.4,0.5",
        "--config",
        config.to_str().unwrap(),
        "--out",
        dir.join("ok").to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::write(&config, "config_version = 1\n[nope]\nx = 1\n").unwrap();
    let out = rascal(&[
        "run",
        "rect",
        floor,
        "--bbox",
        "0.2,0.2,0.4,0.5",
        "--config",
        config.to_str().unwrap(),
        "--out",
        dir.join("bad").to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
