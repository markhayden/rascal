//! Task 2.5's CLI scenario: `rascal paste base.png fill.png <mask> --mode hard --out c.png`;
//! a diff of `c.png` against `base.png` is nonzero only inside the core.
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
    let dir = std::env::temp_dir().join(format!("rascal-paste-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_hard_paste_changes_only_the_core_and_poisson_is_not_applicable() {
    let dir = scratch("hard");
    let floor = repo_root().join("fixtures/images/floor.png");
    let run = dir.join("run");
    let out = rascal(&[
        "run",
        "rect",
        floor.to_str().unwrap(),
        "--bbox",
        "0.25,0.25,0.75,0.75",
        "--out",
        run.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mask_dir = String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("mask: ").map(PathBuf::from))
        .unwrap();
    // A fill that differs from the base at every pixel.
    let base = image::open(&floor).unwrap().to_rgb8();
    let fill = image::RgbImage::from_fn(base.width(), base.height(), |x, y| {
        let p = base.get_pixel(x, y);
        image::Rgb([255 - p[0], 255 - p[1], 255 - p[2]])
    });
    let fill_path = dir.join("fill.png");
    fill.save(&fill_path).unwrap();
    let composite = dir.join("c.png");
    let out = rascal(&[
        "paste",
        floor.to_str().unwrap(),
        fill_path.to_str().unwrap(),
        mask_dir.to_str().unwrap(),
        "--mode",
        "hard",
        "--out",
        composite.to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let got = image::open(&composite).unwrap().to_rgb8();
    let core = image::open(mask_dir.join("core.png")).unwrap().to_luma8();
    let (w, h) = base.dimensions();
    let mut inside = 0;
    for y in 0..h {
        for x in 0..w {
            let changed = got.get_pixel(x, y) != base.get_pixel(x, y);
            let in_core = core.get_pixel(x, y)[0] != 0;
            assert_eq!(
                changed, in_core,
                "{x},{y}: the diff is nonzero exactly inside the core"
            );
            if in_core {
                assert_eq!(got.get_pixel(x, y), fill.get_pixel(x, y));
                inside += 1;
            }
        }
    }
    assert!(inside > 0);
    // feather on this binary mask with no blend is the same composite; the file is overwritten.
    let out = rascal(&[
        "paste",
        floor.to_str().unwrap(),
        fill_path.to_str().unwrap(),
        mask_dir.to_str().unwrap(),
        "--mode",
        "feather",
        "--out",
        composite.to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(image::open(&composite).unwrap().to_rgb8(), got);
    // poisson is not applicable until M7: exit 0 with the reason on stderr, 1 under --strict,
    // and nothing written.
    let other = dir.join("p.png");
    let out = rascal(&[
        "paste",
        floor.to_str().unwrap(),
        fill_path.to_str().unwrap(),
        mask_dir.to_str().unwrap(),
        "--mode",
        "poisson",
        "--out",
        other.to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("M7"));
    assert!(!other.exists());
    let out = rascal(&[
        "paste",
        floor.to_str().unwrap(),
        fill_path.to_str().unwrap(),
        mask_dir.to_str().unwrap(),
        "--mode",
        "poisson",
        "--out",
        other.to_str().unwrap(),
        "--strict",
    ]);
    assert_eq!(out.status.code(), Some(1));
    // A fill of another size is a usage error.
    let small = dir.join("small.png");
    image::RgbImage::new(10, 10).save(&small).unwrap();
    let out = rascal(&[
        "paste",
        floor.to_str().unwrap(),
        small.to_str().unwrap(),
        mask_dir.to_str().unwrap(),
        "--out",
        other.to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(64),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
