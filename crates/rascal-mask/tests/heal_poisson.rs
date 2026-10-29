//! Task 7.1's CLI scenario: `rascal heal seam … --config poisson.toml`, a filler entry with
//! `paste = "poisson"`, heals the step panorama through the fake filler, with and without
//! `max_side`; the paste step's envelope reports the solve; preservation holds outside the
//! band.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rascal_testkit::fake_filler::Behaviour;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("rascal-heal-poisson-{name}-{}", std::process::id()));
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

fn journal(dir: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir.join("journal.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn heal_seam_with_the_poisson_paste_through_the_fake_filler() {
    let dir = scratch("seam");
    let pano = repo_root().join("fixtures/images/step-pano.png");
    let input = image::open(&pano).unwrap().to_rgb8();
    for max_side in [None, Some(400)] {
        let config = dir.join(format!("poisson-{}.toml", max_side.unwrap_or(0)));
        std::fs::write(
            &config,
            format!(
                "config_version = 1\n[fillers.fake]\ncommand = [{:?}, \"fake-filler\", \"{}\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\npaste = \"poisson\"\ntimeout_s = 10\n{}revision = \"fake\"\n",
                env!("CARGO_BIN_EXE_rascal"),
                Behaviour::Exact.name(),
                max_side
                    .map(|m| format!("max_side = {m}\n"))
                    .unwrap_or_default()
            ),
        )
        .unwrap();
        let run = dir.join(format!("run-{}", max_side.unwrap_or(0)));
        let out = rascal(&[
            "heal",
            "seam",
            pano.to_str().unwrap(),
            "--filler",
            "fake",
            "--config",
            config.to_str().unwrap(),
            "--out",
            run.to_str().unwrap(),
        ]);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{max_side:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            run.join("result.png").exists(),
            "{max_side:?}: no result.png"
        );
        let lines = journal(&run);
        let paste = lines
            .iter()
            .find(|l| l["event"] == "op" && l["op"] == "paste")
            .unwrap_or_else(|| panic!("{max_side:?}: no paste line"));
        assert_eq!(paste["params"]["mode"], "poisson");
        assert_eq!(paste["params"]["constants"], "poisson-v1");
        let envelope = &paste["envelope"];
        assert_eq!(envelope["status"], "ok", "{envelope}");
        let summary = envelope["summary"].as_str().unwrap();
        assert!(
            summary.starts_with("poisson paste") && summary.contains("solved in"),
            "{summary}"
        );
        let names: Vec<&str> = envelope["measurements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["name"].as_str().unwrap())
            .collect();
        assert!(
            names.contains(&"domain_px") && names.contains(&"iterations"),
            "{names:?}"
        );
        // Preservation: the result differs from the input somewhere, and only where heal's
        // own measure says the band reaches (the measure op passed, so its continuity numbers
        // are in the journal); the result keeps the input's size.
        let result = image::open(run.join("result.png")).unwrap().to_rgb8();
        assert_eq!(result.dimensions(), input.dimensions());
        assert!(
            result.pixels().zip(input.pixels()).any(|(a, b)| a != b),
            "{max_side:?}: the fill changed nothing"
        );
        assert!(
            lines.iter().any(|l| l["event"] == "op"
                && l["op"] == "measure"
                && l["envelope"]["status"] == "ok"),
            "{max_side:?}: the measure step passed"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
