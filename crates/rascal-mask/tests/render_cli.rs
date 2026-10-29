//! Task 1.5: `rascal render` with every profile from the command line.
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
    let dir = std::env::temp_dir().join(format!("rascal-rendercli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The rect mask of the floor fixture: columns 64..128, rows 40..100.
fn mask_of(dir: &Path) -> PathBuf {
    let floor = repo_root().join("fixtures/images/floor.png");
    let run = dir.join("run");
    let out = rascal(&[
        "run",
        "rect",
        floor.to_str().unwrap(),
        "--bbox",
        "0.2,0.2,0.4,0.5",
        "--out",
        run.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read_dir(run.join("artifacts"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "mask"))
        .unwrap()
}

#[test]
fn every_profile_renders_from_the_command_line() {
    let dir = scratch("profiles");
    let mask = mask_of(&dir);
    let mask = mask.to_str().unwrap();
    let floor = repo_root().join("fixtures/images/floor.png");
    let floor_px = image::open(&floor).unwrap().to_rgb8();
    let floor = floor.to_str().unwrap();

    let t = dir.join("tinted.png");
    let out = rascal(&[
        "render",
        mask,
        "--for",
        "tinted",
        "--image",
        floor,
        "--out",
        t.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let tinted = image::open(&t).unwrap().to_rgb8();
    let inside = tinted.get_pixel(100, 60);
    let src = floor_px.get_pixel(100, 60);
    assert!(
        inside[1] < src[1] && inside[2] > src[2],
        "tinted toward magenta: {inside:?} from {src:?}"
    );
    assert_eq!(
        tinted.get_pixel(10, 10).0,
        floor_px.get_pixel(10, 10).0,
        "outside untouched"
    );

    let t2 = dir.join("tinted-green.png");
    let out = rascal(&[
        "render",
        mask,
        "--for",
        "tinted",
        "--image",
        floor,
        "--colour",
        "#00ff00",
        "--strength",
        "1",
        "--out",
        t2.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        image::open(&t2).unwrap().to_rgb8().get_pixel(100, 60).0,
        [0, 255, 0]
    );

    let a = dir.join("alpha.png");
    let out = rascal(&[
        "render",
        mask,
        "--for",
        "alpha",
        "--image",
        floor,
        "--out",
        a.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let alpha = image::open(&a).unwrap().to_rgba8();
    assert_eq!(alpha.get_pixel(100, 60)[3], 0);
    assert_eq!(alpha.get_pixel(10, 10)[3], 255);

    let b = dir.join("binary.png");
    let out = rascal(&[
        "render",
        mask,
        "--for",
        "binary",
        "--threshold",
        "1",
        "--out",
        b.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        image::open(&b).unwrap().to_luma8().get_pixel(100, 60)[0],
        255
    );

    // The overlay as a PNG for pixel-exact checks: the core's edge is white, the interior is
    // tinted, the outside untouched.
    let o = dir.join("overlay.png");
    let out = rascal(&[
        "render",
        mask,
        "--for",
        "overlay",
        "--image",
        floor,
        "--out",
        o.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let overlay = image::open(&o).unwrap().to_rgb8();
    assert_eq!(
        overlay.get_pixel(64, 40).0,
        [255, 255, 255],
        "the core's edge is white"
    );
    assert_ne!(
        overlay.get_pixel(100, 60).0,
        floor_px.get_pixel(100, 60).0,
        "the interior is tinted"
    );
    assert_eq!(
        overlay.get_pixel(10, 10).0,
        floor_px.get_pixel(10, 10).0,
        "outside untouched"
    );
    // A .jpg destination gets a JPEG.
    let j = dir.join("overlay.jpg");
    let out = rascal(&[
        "render",
        mask,
        "--for",
        "overlay",
        "--image",
        floor,
        "--out",
        j.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bytes = std::fs::read(&j).unwrap();
    assert_eq!(&bytes[..2], &[0xff, 0xd8], "a JPEG for a .jpg destination");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_arguments_are_usage_errors() {
    let dir = scratch("bad");
    let mask = mask_of(&dir);
    let mask = mask.to_str().unwrap();
    let floor = repo_root().join("fixtures/images/floor.png");
    let floor = floor.to_str().unwrap();
    let out_file = dir.join("x.png");
    let out_file = out_file.to_str().unwrap();
    let out = rascal(&[
        "render",
        mask,
        "--for",
        "binary",
        "--threshold",
        "0",
        "--out",
        out_file,
    ]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("threshold must be in [1, 255]"));
    let out = rascal(&["render", mask, "--for", "alpha", "--out", out_file]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("give input image"));
    let out = rascal(&[
        "render", mask, "--for", "tinted", "--image", floor, "--colour", "red", "--out", out_file,
    ]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = rascal(&[
        "render",
        mask,
        "--for",
        "tinted",
        "--image",
        floor,
        "--strength",
        "2",
        "--out",
        out_file,
    ]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // An image in another frame.
    let small = dir.join("small.png");
    rascal_testkit::images::flat(100, 50, 90)
        .save(&small)
        .unwrap();
    let out = rascal(&[
        "render",
        mask,
        "--for",
        "tinted",
        "--image",
        small.to_str().unwrap(),
        "--out",
        out_file,
    ]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("frame"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_rendering_bound_where_a_mask_is_declared_is_refused_by_the_recipe() {
    let dir = scratch("role");
    let recipe = dir.join("r.toml");
    std::fs::write(
        &recipe,
        r#"
version = 1
name = "role"
use_case = "custom"
[inputs]
image = { type = "Image" }
location = { type = "Location" }
[[step]]
id = "mask"
op = "rasterize"
inputs = { location = "inputs.location", image = "inputs.image" }
[[step]]
id = "first"
op = "render"
inputs = { mask = "mask.mask" }
for = "grey"
[[step]]
id = "second"
op = "render"
inputs = { mask = "first.image" }
for = "grey"
[outputs]
mask = "mask.mask"
"#,
    )
    .unwrap();
    let floor = repo_root().join("fixtures/images/floor.png");
    let out = rascal(&[
        "run",
        recipe.to_str().unwrap(),
        floor.to_str().unwrap(),
        "--bbox",
        "0.2,0.2,0.4,0.5",
        "--out",
        dir.join("run").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("step second")
            && err.contains("input mask must be a Mask, but first.image is a Image"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
