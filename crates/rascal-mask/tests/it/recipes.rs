//! Task 4.4: the four M4 recipes reproduce their golden masks on the fixtures; `remove` with
//! and without `--protect`; the two-pass test, `heal seam` then `heal protect` in one
//! workspace through the exact fake: the first edit is preserved and a middle pixel may change.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn rascal(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(args)
        .env_remove("RASCAL_LOG")
        .output()
        .unwrap()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-recipes-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn mask_dir(out: &Output) -> PathBuf {
    text(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("mask: ").map(PathBuf::from))
        .unwrap_or_else(|| panic!("no mask line in {}", text(&out.stdout)))
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The mask under `fixtures/masks/<name>.mask` equals the one just made, plane for plane;
/// `UPDATE_FIXTURES=1` rewrites it.
fn assert_golden(mask_dir: &Path, name: &str) {
    let golden = repo_root().join(format!("fixtures/masks/{name}.mask"));
    assert!(
        std::env::var_os("UPDATE_FIXTURES").is_none() || std::env::var_os("CI").is_none(),
        "UPDATE_FIXTURES must not be set in CI: a rewrite would pass unseen"
    );
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        let _ = std::fs::remove_dir_all(&golden);
        copy_tree(mask_dir, &golden);
    }
    for plane in ["core.png", "blend.png", "protect.png"] {
        let a = image::open(mask_dir.join(plane)).unwrap().to_luma8();
        let b = image::open(golden.join(plane)).unwrap().to_luma8();
        assert!(
            a == b,
            "{name}: {plane} differs from the golden; UPDATE_FIXTURES=1 regenerates it"
        );
    }
    let fresh = rascal_core::mask::Mask::read(mask_dir).unwrap();
    let stored = rascal_core::mask::Mask::read(&golden).unwrap();
    assert_eq!(
        fresh.record.identity, stored.record.identity,
        "{name}: identity"
    );
}

fn count(plane: &image::GrayImage) -> usize {
    plane.pixels().filter(|p| p[0] != 0).count()
}

#[test]
fn remove_reproduces_its_golden_with_and_without_protect() {
    let dir = scratch("remove");
    let images = repo_root().join("fixtures/images");
    let chairs = repo_root().join("fixtures/locations/chairs.json");
    let out = rascal(&[
        "run",
        "remove",
        images.join("chair-blob.png").to_str().unwrap(),
        "--location",
        chairs.to_str().unwrap(),
        "--out",
        dir.join("run").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let plain = mask_dir(&out);
    assert_golden(&plain, "remove-chair-blob");
    let mask = rascal_core::mask::Mask::read(&plain).unwrap();
    assert_eq!(mask.record.use_case, rascal_core::mask::UseCase::Remove);
    // The shadow: the core reaches 12 px below the grown box (the box's bottom is 0.79 of
    // 320 = 252.8 → pixel 253 exclusive, grown 4, then 12 down).
    assert!(mask.core.get_pixel(190, 266)[0] != 0 && mask.core.get_pixel(190, 270)[0] == 0);
    assert!(mask.blend.pixels().any(|p| p[0] != 0), "feathered");
    assert!(dir.join("run/remove-grey.png").is_file());
    // Without --protect the optional input is simply absent: the journal shows subtract
    // passing the mask through.
    let journal = std::fs::read_to_string(dir.join("run/journal.jsonl")).unwrap();
    assert!(journal.contains("passes through"), "{journal}");
    // With --protect: a mask over the chair's left leg keeps those pixels out.
    let protect = rascal(&[
        "run",
        "rect",
        images.join("chair-blob.png").to_str().unwrap(),
        "--bbox",
        "0.28,0.55,0.36,0.80",
        "--out",
        dir.join("protect-run").to_str().unwrap(),
    ]);
    assert!(protect.status.success(), "{}", text(&protect.stderr));
    let protect_mask = mask_dir(&protect);
    let out = rascal(&[
        "run",
        "remove",
        images.join("chair-blob.png").to_str().unwrap(),
        "--location",
        chairs.to_str().unwrap(),
        "--protect",
        protect_mask.to_str().unwrap(),
        "--out",
        dir.join("run2").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let protected = rascal_core::mask::Mask::read(&mask_dir(&out)).unwrap();
    assert!(count(&protected.protect) > 0 && count(&protected.core) < count(&mask.core));
    assert_eq!(
        protected.core.get_pixel(150, 220)[0],
        0,
        "inside the protected leg"
    );
    assert_ne!(protected.protect.get_pixel(150, 220)[0], 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn repair_and_repair_area_reproduce_their_goldens() {
    let dir = scratch("repair");
    let images = repo_root().join("fixtures/images");
    let stroke = repo_root().join("fixtures/locations/scratch-stroke.json");
    let out = rascal(&[
        "run",
        "repair",
        images.join("scratch-smooth.png").to_str().unwrap(),
        "--stroke",
        stroke.to_str().unwrap(),
        "--out",
        dir.join("repair").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let repaired = mask_dir(&out);
    assert_golden(&repaired, "repair-scratch-smooth");
    let mask = rascal_core::mask::Mask::read(&repaired).unwrap();
    assert_eq!(mask.record.use_case, rascal_core::mask::UseCase::Repair);
    // Tightened: far narrower than the 8 px stroke, and the journal names the tighten step.
    let journal = std::fs::read_to_string(dir.join("repair/journal.jsonl")).unwrap();
    assert!(journal.contains("\"op\":\"tighten\""));
    let drawn =
        rascal_ops::rasterize::dilate_path(&rascal_testkit::images::scratch_path(), 8.0, 400, 300);
    assert!(count(&mask.core) * 2 < count(&drawn));
    let out = rascal(&[
        "run",
        "repair-area",
        images.join("chair-blob.png").to_str().unwrap(),
        "--bbox",
        "0.6,0.7,0.8,0.9",
        "--out",
        dir.join("area").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let area = mask_dir(&out);
    assert_golden(&area, "repair-area-chair-blob");
    let mask = rascal_core::mask::Mask::read(&area).unwrap();
    assert_eq!(
        count(&mask.core),
        (0.2 * 480.0) as usize * (0.2 * 320.0) as usize
    );
    assert!(mask.blend.pixels().any(|p| p[0] != 0));
    // The wrong flag for a recipe is refused by name.
    let out = rascal(&[
        "run",
        "repair",
        images.join("scratch-smooth.png").to_str().unwrap(),
        "--bbox",
        "0.1,0.1,0.2,0.2",
        "--out",
        dir.join("wrong").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(text(&out.stderr).contains("declares no inputs.location"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn protect_inverts_the_seam_mask_into_the_raw_frame_and_needs_the_workspace() {
    let dir = scratch("protect");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let ws = dir.join("ws");
    let out = rascal(&[
        "run",
        "seam",
        pano.to_str().unwrap(),
        "--out",
        dir.join("seam").to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let seam_mask = mask_dir(&out);
    let out = rascal(&[
        "run",
        "protect",
        pano.to_str().unwrap(),
        "--mask",
        seam_mask.to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
        "--out",
        dir.join("protect").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let inverted = mask_dir(&out);
    assert_golden(&inverted, "protect-step-pano");
    let mask = rascal_core::mask::Mask::read(&inverted).unwrap();
    assert_eq!(mask.record.frame, "raw:800x400:equirectangular");
    assert_eq!(mask.record.use_case, rascal_core::mask::UseCase::Protect);
    // Protect sits on the two edge spans, the middle is core.
    assert!(mask.protect.get_pixel(2, 200)[0] != 0 && mask.protect.get_pixel(797, 200)[0] != 0);
    assert!(mask.core.get_pixel(400, 200)[0] != 0 && mask.protect.get_pixel(400, 200)[0] == 0);
    // Without the workspace that knows the cut frame, the mask is refused with the hint.
    let out = rascal(&[
        "run",
        "protect",
        pano.to_str().unwrap(),
        "--mask",
        seam_mask.to_str().unwrap(),
        "--out",
        dir.join("protect2").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(64));
    assert!(
        text(&out.stderr).contains("does not know the mask's frame"),
        "{}",
        text(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn heal_seam_then_heal_protect_preserves_the_first_edit_and_may_change_the_middle() {
    let dir = scratch("two-pass");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let ws = dir.join("ws");
    let cfg = dir.join("rascal.toml");
    std::fs::write(
        &cfg,
        format!(
            "config_version = 1\n[fillers.fake]\ncommand = [{:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\npaste = \"feather\"\nrevision = \"fake\"\n",
            env!("CARGO_BIN_EXE_rascal")
        ),
    )
    .unwrap();
    let first = rascal(&[
        "heal",
        "seam",
        pano.to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        cfg.to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
        "--out",
        dir.join("pass1").to_str().unwrap(),
        "--no-dash",
    ]);
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    let first_mask = text(&first.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("mask: "))
        .map(str::to_string);
    let composite1 = dir.join("pass1/result.png");
    assert!(composite1.is_file());
    // The seam mask of pass one is the newest cut-frame mask in the shared workspace.
    let out = rascal(&[
        "workspace",
        "list",
        ws.to_str().unwrap(),
        "--type",
        "Mask",
        "--json",
    ]);
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    let seam_mask = rows
        .iter()
        .rfind(|r| r["frame"].as_str().unwrap().starts_with("cut:"))
        .map(|r| {
            ws.join("artifacts").join(format!(
                "{}.mask",
                r["identity"]
                    .as_str()
                    .unwrap()
                    .trim_start_matches("sha256:")
            ))
        })
        .unwrap();
    let _ = first_mask;
    let second = rascal(&[
        "heal",
        "protect",
        composite1.to_str().unwrap(),
        "--mask",
        seam_mask.to_str().unwrap(),
        "--filler",
        "fake",
        "--config",
        cfg.to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
        "--out",
        dir.join("pass2").to_str().unwrap(),
        "--no-dash",
    ]);
    assert_eq!(second.status.code(), Some(0), "{}", text(&second.stderr));
    let a = image::open(&composite1).unwrap().to_rgb8();
    let b = image::open(dir.join("pass2/result.png")).unwrap().to_rgb8();
    let seam = rascal_core::mask::Mask::read(&seam_mask).unwrap();
    // Every raw pixel the first mask reached (its support, mapped onto the edge spans) is
    // identical between the two composites; the preservation check enforced it, and here it
    // is read back. The canvas's left half is the panorama's rightmost strip columns.
    let s = seam.record.width / 2;
    let w = a.width();
    let mut preserved = 0usize;
    for y in 0..a.height() {
        for cx in 0..seam.record.width {
            let on = seam.core.get_pixel(cx, y)[0] != 0 || seam.blend.get_pixel(cx, y)[0] != 0;
            if !on {
                continue;
            }
            let x = if cx < s { w - s + cx } else { cx - s };
            assert_eq!(
                a.get_pixel(x, y),
                b.get_pixel(x, y),
                "first edit changed at {x},{y}"
            );
            preserved += 1;
        }
    }
    assert!(preserved > 1000, "{preserved} pixels were protected");
    // And the middle, which the first pass left alone, was repainted by the second.
    let changed = (0..a.height())
        .flat_map(|y| (s + 10..w - s - 10).map(move |x| (x, y)))
        .filter(|&(x, y)| a.get_pixel(x, y) != b.get_pixel(x, y))
        .count();
    assert!(changed > 1000, "{changed} middle pixels changed");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn heal_remove_with_a_location_through_the_exact_fake_and_a_jpeg_input() {
    let dir = scratch("heal-remove");
    let images = repo_root().join("fixtures/images");
    let cfg = dir.join("rascal.toml");
    std::fs::write(
        &cfg,
        format!(
            "config_version = 1\n[fillers.fake]\ncommand = [{:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\npaste = \"feather\"\nrevision = \"fake\"\n",
            env!("CARGO_BIN_EXE_rascal")
        ),
    )
    .unwrap();
    // The chair blob as a JPEG: the first half of the plan's scenario and the JPEG path at once.
    let jpeg = dir.join("chair-blob.jpg");
    image::open(images.join("chair-blob.png"))
        .unwrap()
        .to_rgb8()
        .save_with_format(&jpeg, image::ImageFormat::Jpeg)
        .unwrap();
    let out = rascal(&[
        "heal",
        "remove",
        jpeg.to_str().unwrap(),
        "--location",
        repo_root()
            .join("fixtures/locations/chairs.json")
            .to_str()
            .unwrap(),
        "--filler",
        "fake",
        "--config",
        cfg.to_str().unwrap(),
        "--out",
        dir.join("run").to_str().unwrap(),
        "--no-dash",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let result = image::open(dir.join("run/result.png")).unwrap().to_rgb8();
    let input = image::open(&jpeg).unwrap().to_rgb8();
    // heal prints the result and the renders; the recipe's mask is the run's one raw-frame
    // mask (the fake filler adds none).
    let rows: Vec<serde_json::Value> = serde_json::from_slice(
        &rascal(&[
            "workspace",
            "list",
            dir.join("run").to_str().unwrap(),
            "--type",
            "Mask",
            "--json",
        ])
        .stdout,
    )
    .unwrap();
    let identity = rows.last().unwrap()["identity"]
        .as_str()
        .unwrap()
        .to_string();
    let mask = rascal_core::mask::Mask::read(
        &dir.join("run/artifacts")
            .join(format!("{}.mask", identity.trim_start_matches("sha256:"))),
    )
    .unwrap();
    // Outside the mask's support every pixel is the input's; under the shadow rows the fake
    // repainted.
    let mut outside = 0usize;
    for y in 0..input.height() {
        for x in 0..input.width() {
            if mask.weight(x, y) == 0.0 {
                assert_eq!(result.get_pixel(x, y), input.get_pixel(x, y), "{x},{y}");
                outside += 1;
            }
        }
    }
    assert!(outside > 100_000);
    let changed = (255..266)
        .filter(|&y| result.get_pixel(190, y) != input.get_pixel(190, y))
        .count();
    assert!(
        changed > 5,
        "the shadow rows below the box were repainted ({changed})"
    );
    assert!(
        std::fs::read_to_string(dir.join("run/journal.jsonl"))
            .unwrap()
            .contains("filler call: ")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
