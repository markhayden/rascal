//! M6 checkpoint: what moves a cell's identity and what does not. A prompt, a pack revision,
//! the bundle digest of an installed pack and the bytes of a bound input file each make a new
//! cell; a bound file's name, the spend ceiling and the order of keys do not; the committed
//! cell fixture's identity recomputes from its components. A cell whose latest attempt
//! failed runs again as a new attempt; a filler that fails leaves every cell failed and the
//! run at exit 3 after all of them; a filler's key appears in no file the lab writes, the
//! pairs page included.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rascal_core::lab::cell::{CellComponents, CellRecord};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("rascal-lab-identity-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `lab run` with the fixtures as the corpus root and `env` added.
fn run(matrix: &Path, config: &Path, out: &Path, extra: &[&str], env: &[(&str, &str)]) -> Output {
    let fixtures = repo_root().join("fixtures/images");
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command
        .args([
            "lab",
            "run",
            matrix.to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--corpus-root",
            fixtures.to_str().unwrap(),
            "--no-dash",
        ])
        .args(extra)
        .env_remove("RASCAL_LOG")
        .env_remove("RASCAL_CORPUS");
    for (k, v) in env {
        command.env(k, v);
    }
    command.output().unwrap()
}

fn cells(out: &Path) -> Vec<CellRecord> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(out.join("cells"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    dirs.sort();
    dirs.iter()
        .map(|d| {
            serde_json::from_str(&std::fs::read_to_string(d.join("cell.json")).unwrap()).unwrap()
        })
        .collect()
}

/// The lab's files: a one-image corpus (the chair blob), a protocol with one group, a
/// matrix with a `remove` arm bound to a location file and a `finder` arm through the fake
/// pack, and a configuration whose fake pack is launched through `<dir>/.venv/bin/rascal-tools`
/// (a symlink to this binary) beside a `bundle.toml`, with `pack_revision` and the filler's
/// `key_env` as given.
fn world(dir: &Path, pack_revision: &str, key_env: Option<&str>) -> (PathBuf, PathBuf) {
    std::fs::write(
        dir.join("corpus.toml"),
        "version = 1\n[[image]]\nid = \"chair\"\nfile = \"chair-blob.png\"\nsha256 = \"sha256:40e657ddd81ee448c90cdc287a376980c9e14f56583997049c6c0447d9c37242\"\ngroup = \"structured\"\nlicence = \"synthetic\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("protocol.toml"),
        "version = 1\nname = \"identity\"\nseed = 3\narms = [\"remove\", \"finder\"]\nmin_decided = 1\n[[group]]\nname = \"structured\"\nimages = [\"chair\"]\n",
    )
    .unwrap();
    std::fs::copy(
        repo_root().join("fixtures/locations/chairs.json"),
        dir.join("chairs.json"),
    )
    .unwrap();
    std::fs::write(
        dir.join("segment-test.toml"),
        rascal_testkit::fake_pack::RECIPE,
    )
    .unwrap();
    std::fs::write(
        dir.join("matrix.toml"),
        "version = 1\nname = \"identity\"\ncorpus = \"corpus.toml\"\nprotocol = \"protocol.toml\"\nseed = 3\n[[image]]\nid = \"chair\"\ninputs = { location = \"chairs.json\" }\n[[arm]]\nname = \"remove\"\nrecipe = \"remove\"\nfiller = \"fake\"\n[[arm]]\nname = \"finder\"\nrecipe = \"segment-test.toml\"\nfiller = \"fake\"\n",
    )
    .unwrap();
    // The pack "installed" under a bundle: a launcher symlink beside a bundle.toml.
    let install = dir.join("pack-install");
    let bin = install.join(".venv/bin");
    std::fs::create_dir_all(&bin).unwrap();
    let launcher = bin.join("rascal-tools");
    if !launcher.exists() {
        std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_rascal"), &launcher).unwrap();
    }
    if !install.join("bundle.toml").exists() {
        std::fs::write(install.join("bundle.toml"), "bundle_version = 1\n").unwrap();
    }
    // A key belongs to a hosted filler; one is configured beside the fake so the key is in
    // the environment and the configuration while the lab runs.
    let key = key_env
        .map(|name| format!("[fillers.hosted]\nurl = \"https://example.invalid/fill\"\nprofile = \"binary\"\nkey_env = {name:?}\nrevision = \"hosted\"\n"))
        .unwrap_or_default();
    let config = dir.join("rascal.toml");
    std::fs::write(
        &config,
        format!(
            "config_version = 1\n[fillers.fake]\ncommand = [{bin:?}, \"fake-filler\", \"exact\", \"{{image}}\", \"{{mask}}\", \"{{out}}\"]\nprofile = \"grey\"\nrevision = \"fake exact\"\n{key}[packs.fake]\ncommand = [{launcher:?}, \"fake-pack\", \"exact\"]\nrevision = {pack_revision:?}\n",
            bin = env!("CARGO_BIN_EXE_rascal"),
            launcher = launcher.display().to_string()
        ),
    )
    .unwrap();
    (dir.join("matrix.toml"), config)
}

fn file_digest(path: &Path) -> String {
    rascal_core::lab::file_digest(path).unwrap()
}

#[test]
fn a_prompt_a_pack_revision_the_bundle_digest_and_an_input_file_move_the_identity_and_a_name_does_not()
 {
    let dir = scratch("moves");
    let (matrix, config) = world(&dir, "fake 1", None);
    let out = dir.join("lab");
    let first = run(&matrix, &config, &out, &[], &[]);
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    assert!(
        text(&first.stdout).contains("2 ran, 0 reused"),
        "{}",
        text(&first.stdout)
    );
    let records = cells(&out);
    assert_eq!(records.len(), 2);
    let finder = records.iter().find(|c| c.arm == "finder").unwrap();
    let remove = records.iter().find(|c| c.arm == "remove").unwrap();
    // The input file's digest and the bundle digest are in the components.
    assert_eq!(
        remove.components.inputs["location"],
        file_digest(&dir.join("chairs.json"))
    );
    assert!(remove.components.finders.is_empty());
    let descriptor = &finder.components.finders[0];
    assert_eq!(
        descriptor.bundle_digest.as_deref(),
        Some(file_digest(&dir.join("pack-install/bundle.toml")).as_str())
    );
    assert_eq!(descriptor.revision, "fake 1");
    assert_eq!(descriptor.arguments["prompt"], "the chair");
    // The default constant set of a step that names none is in the components (remove's
    // steps name theirs; the finder recipe's feather step does).
    assert_eq!(finder.components.constants["feather"], "feather-v1");
    // A prompt change: the finder's cell is new, the remove cell is reused.
    let prompted = std::fs::read_to_string(&matrix).unwrap().replace(
        "inputs = { location = \"chairs.json\" }\n",
        "inputs = { location = \"chairs.json\" }\noverrides = { \"segment.prompt\" = \"the lamp\" }\n",
    );
    std::fs::write(dir.join("prompted.toml"), prompted).unwrap();
    // (The remove recipe has no segment step, so the override is checked per arm: the lab
    // refuses it, naming the step; a per-image override must fit every arm's recipe.)
    let out2 = run(&dir.join("prompted.toml"), &config, &out, &[], &[]);
    assert_eq!(out2.status.code(), Some(64), "{}", text(&out2.stderr));
    assert!(
        text(&out2.stderr).contains("no step segment"),
        "{}",
        text(&out2.stderr)
    );
    // So the prompt moves through the arm's overrides instead.
    let prompted = std::fs::read_to_string(&matrix).unwrap().replace(
        "recipe = \"segment-test.toml\"\n",
        "recipe = \"segment-test.toml\"\noverrides = { \"segment.prompt\" = \"the lamp\" }\n",
    );
    std::fs::write(&matrix, prompted).unwrap();
    let out2 = run(&matrix, &config, &out, &[], &[]);
    assert_eq!(out2.status.code(), Some(0), "{}", text(&out2.stderr));
    assert!(
        text(&out2.stdout).contains("1 ran, 1 reused"),
        "{}",
        text(&out2.stdout)
    );
    let lamp = cells(&out)
        .into_iter()
        .find(|c| c.arm == "finder" && c.components.finders[0].arguments["prompt"] == "the lamp")
        .unwrap();
    assert_ne!(lamp.identity, finder.identity);
    // A pack revision bump: the finder's cell is new again, remove reused.
    let (matrix, config) = world(&dir, "fake 2", None);
    let prompted = std::fs::read_to_string(&matrix).unwrap().replace(
        "recipe = \"segment-test.toml\"\n",
        "recipe = \"segment-test.toml\"\noverrides = { \"segment.prompt\" = \"the lamp\" }\n",
    );
    std::fs::write(&matrix, prompted).unwrap();
    let out3 = run(&matrix, &config, &out, &[], &[]);
    assert_eq!(out3.status.code(), Some(0), "{}", text(&out3.stderr));
    assert!(
        text(&out3.stdout).contains("1 ran, 1 reused"),
        "{}",
        text(&out3.stdout)
    );
    assert_eq!(cells(&out).len(), 4);
    // The bundle digest changes: the finder's cell is new.
    std::fs::write(
        dir.join("pack-install/bundle.toml"),
        "bundle_version = 1\n# rebuilt\n",
    )
    .unwrap();
    let out4 = run(&matrix, &config, &out, &[], &[]);
    assert_eq!(out4.status.code(), Some(0), "{}", text(&out4.stderr));
    assert!(
        text(&out4.stdout).contains("1 ran, 1 reused"),
        "{}",
        text(&out4.stdout)
    );
    assert_eq!(cells(&out).len(), 5);
    // The bound file renamed with the same bytes: the same cells. One byte changed: a new
    // remove cell (the finder recipe binds no location, so it is unmoved).
    std::fs::copy(dir.join("chairs.json"), dir.join("boxes.json")).unwrap();
    let renamed = std::fs::read_to_string(&matrix)
        .unwrap()
        .replace("chairs.json", "boxes.json");
    std::fs::write(&matrix, renamed).unwrap();
    let out5 = run(&matrix, &config, &out, &[], &[]);
    assert!(
        text(&out5.stdout).contains("0 ran, 2 reused"),
        "{}",
        text(&out5.stdout)
    );
    let mut location: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("boxes.json")).unwrap()).unwrap();
    let x1 = location["bbox"][2].as_f64().unwrap();
    location["bbox"][2] = serde_json::json!(x1 - 0.01);
    std::fs::write(dir.join("boxes.json"), location.to_string()).unwrap();
    let out6 = run(&matrix, &config, &out, &[], &[]);
    assert_eq!(out6.status.code(), Some(0), "{}", text(&out6.stderr));
    assert!(
        text(&out6.stdout).contains("1 ran, 1 reused"),
        "{}",
        text(&out6.stdout)
    );
    assert_eq!(cells(&out).len(), 6);
    // An input the image binds that no recipe takes from a file, and a required input an
    // image does not bind, are refused by name before anything runs.
    let bogus = std::fs::read_to_string(&matrix).unwrap().replace(
        "inputs = { location = \"boxes.json\" }",
        "inputs = { location = \"boxes.json\", bogus = \"x\" }",
    );
    std::fs::write(dir.join("bogus.toml"), bogus).unwrap();
    let out7 = run(
        &dir.join("bogus.toml"),
        &config,
        &dir.join("lab-b"),
        &[],
        &[],
    );
    assert_eq!(out7.status.code(), Some(64));
    assert!(
        text(&out7.stderr).contains("inputs.bogus is not one heal binds"),
        "{}",
        text(&out7.stderr)
    );
    let unbound = std::fs::read_to_string(&matrix)
        .unwrap()
        .replace("inputs = { location = \"boxes.json\" }\n", "");
    std::fs::write(dir.join("unbound.toml"), unbound).unwrap();
    let out8 = run(
        &dir.join("unbound.toml"),
        &config,
        &dir.join("lab-c"),
        &[],
        &[],
    );
    assert_eq!(out8.status.code(), Some(64));
    assert!(
        text(&out8.stderr).contains("arm remove needs inputs.location"),
        "{}",
        text(&out8.stderr)
    );
    assert!(
        !dir.join("lab-c/cells").exists()
            || std::fs::read_dir(dir.join("lab-c/cells"))
                .unwrap()
                .next()
                .is_none()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_identity_is_a_function_of_the_components_alone() {
    // The committed fixture recomputes.
    let fixture: CellRecord = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("fixtures/schemas/cell-v1/seam-thin.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(fixture.components.identity(), fixture.identity);
    // Key order in the serialized form does not matter; a revision does.
    let reordered: serde_json::Value = {
        let mut value = serde_json::to_value(&fixture.components).unwrap();
        let object = value.as_object_mut().unwrap();
        let mut entries: Vec<(String, serde_json::Value)> =
            object.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        entries.reverse();
        let mut map = serde_json::Map::new();
        for (k, v) in entries {
            map.insert(k, v);
        }
        serde_json::Value::Object(map)
    };
    let again: CellComponents = serde_json::from_value(reordered).unwrap();
    assert_eq!(again.identity(), fixture.identity);
    let mut bumped = fixture.components.clone();
    bumped.filler.revision.push_str(" 2");
    assert_ne!(bumped.identity(), fixture.identity);
    let mut constants: BTreeMap<String, String> = fixture.components.constants.clone();
    constants.insert("fan".into(), "fan-v1".into());
    let mut other_set = fixture.components.clone();
    other_set.constants = constants;
    assert_ne!(other_set.identity(), fixture.identity);
}

#[test]
fn a_failed_attempt_runs_again_a_failing_filler_fails_every_cell_and_the_key_is_in_no_file() {
    let dir = scratch("failed");
    let (matrix, config) = world(&dir, "fake 1", Some("FAKE_FILLER_KEY"));
    let out = dir.join("lab");
    let secret = "s3cret-value-never-written";
    let env = [("FAKE_FILLER_KEY", secret)];
    let first = run(&matrix, &config, &out, &[], &env);
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    // Mark one cell's attempt failed: the next run runs it again as attempt 2, keeps 1.
    let records = cells(&out);
    let remove = records.iter().find(|c| c.arm == "remove").unwrap();
    let hex = remove.identity.strip_prefix("sha256:").unwrap();
    let attempt = out.join("cells").join(hex).join("attempts/1/attempt.json");
    let edited = std::fs::read_to_string(&attempt)
        .unwrap()
        .replace("\"ok\"", "\"failed\"");
    std::fs::write(&attempt, edited).unwrap();
    let second = run(&matrix, &config, &out, &[], &env);
    assert_eq!(second.status.code(), Some(0), "{}", text(&second.stderr));
    let stdout = text(&second.stdout);
    assert!(
        stdout.contains("attempt 1 was failed; running again"),
        "{stdout}"
    );
    assert!(stdout.contains("1 ran, 1 reused"), "{stdout}");
    assert_eq!(
        std::fs::read_to_string(out.join("cells").join(hex).join("latest"))
            .unwrap()
            .trim(),
        "2"
    );
    assert!(
        out.join("cells")
            .join(hex)
            .join("attempts/1/run/result.png")
            .is_file()
    );
    assert!(
        out.join("cells")
            .join(hex)
            .join("attempts/2/run/result.png")
            .is_file()
    );
    // A failing filler: every cell runs, every attempt is failed, exit 3 after all of them.
    let failing = std::fs::read_to_string(&config)
        .unwrap()
        .replace("\"exact\", \"{image}\"", "\"failing\", \"{image}\"")
        .replace("revision = \"fake exact\"", "revision = \"fake failing\"");
    std::fs::write(dir.join("failing.toml"), failing).unwrap();
    let third = run(&matrix, &dir.join("failing.toml"), &out, &[], &env);
    assert_eq!(third.status.code(), Some(3), "{}", text(&third.stderr));
    assert!(
        text(&third.stderr).contains("2 cells failed"),
        "{}",
        text(&third.stderr)
    );
    assert!(
        text(&third.stdout).contains("0 ran, 0 reused, 0 stopped, 2 failed"),
        "{}",
        text(&third.stdout)
    );
    let failed: Vec<CellRecord> = cells(&out)
        .into_iter()
        .filter(|c| c.components.filler.revision == "fake failing")
        .collect();
    assert_eq!(failed.len(), 2);
    for cell in &failed {
        let hex = cell.identity.strip_prefix("sha256:").unwrap();
        let record: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(out.join("cells").join(hex).join("attempts/1/attempt.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(record["status"], "failed", "{record}");
        assert!(
            record["reason"].as_str().unwrap().contains("fail"),
            "{record}"
        );
    }
    // The current cells are the failing filler's, so pairs refuses them; a run under the
    // sound configuration reuses the sound cells and makes them current again.
    let pairs = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args([
            "lab",
            "pairs",
            out.to_str().unwrap(),
            "--arms",
            "remove,finder",
        ])
        .output()
        .unwrap();
    assert_eq!(pairs.status.code(), Some(64), "{}", text(&pairs.stderr));
    assert!(
        text(&pairs.stderr).contains("is failed, not ok"),
        "{}",
        text(&pairs.stderr)
    );
    let fourth = run(&matrix, &config, &out, &[], &env);
    assert!(
        text(&fourth.stdout).contains("0 ran, 2 reused"),
        "{}",
        text(&fourth.stdout)
    );
    // The pairs page over the sound cells, then: no file under the lab holds the key's
    // value or its name.
    let pairs = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args([
            "lab",
            "pairs",
            out.to_str().unwrap(),
            "--arms",
            "remove,finder",
        ])
        .output()
        .unwrap();
    assert_eq!(pairs.status.code(), Some(0), "{}", text(&pairs.stderr));
    let mut walked = 0;
    walk(&out, &mut |path| {
        let bytes = std::fs::read(path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(secret),
            "{}: holds the key's value",
            path.display()
        );
        assert!(
            !text.contains("FAKE_FILLER_KEY"),
            "{}: names the key",
            path.display()
        );
        walked += 1;
    });
    assert!(walked > 20, "{walked} files walked");
    let _ = std::fs::remove_dir_all(&dir);
}

fn walk(dir: &Path, visit: &mut impl FnMut(&Path)) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, visit);
        } else {
            visit(&path);
        }
    }
}
