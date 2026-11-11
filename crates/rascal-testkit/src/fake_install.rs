//! A fake segmenter pack install, shaped as `rascal tools setup` leaves one: the launcher
//! `<dir>/.venv/bin/rascal-tools` (a shell script that runs any program, such as the fake pack),
//! `<dir>/artifacts.toml` naming the model files, `<dir>/segmenters.toml` ranking two models
//! (`sam3` first, `tiny` second, whose files these are), and with `models` tiny installed under
//! `<dir>/models/` as the pack lays an installation out (`<models>/tiny/current` naming
//! `<models>/tiny/<identity>/`), so it is the default there. Readiness inspects exactly this
//! shape (PLAN.md 12.2). [`write_catalogue`] and [`install_model`] lay out any other catalogue.

use std::path::{Path, PathBuf};

/// The model files the pack's `artifacts.toml` names (the detector, its tokenizer, the
/// segmenter's encoder and decoder).
pub const MODEL_FILES: [&str; 4] = [
    "owlv2-base-patch16.onnx",
    "owlv2-base-patch16.tokenizer.json",
    "sam2.1-hiera-tiny.encoder.onnx",
    "sam2.1-hiera-tiny.decoder3.onnx",
];

/// What [`write`] made.
#[derive(Debug, Clone)]
pub struct FakeInstall {
    /// The install directory.
    pub dir: PathBuf,
    /// `<dir>/.venv/bin/rascal-tools`.
    pub launcher: PathBuf,
}

impl FakeInstall {
    /// The launcher as a TOML string, for a `[packs.*] command`.
    pub fn command_toml(&self) -> String {
        format!("[{:?}, \"serve\"]", self.launcher.display().to_string())
    }
}

/// Writes the install under `dir`: a launcher that execs `program` with its arguments (and
/// whatever the launcher is given), `artifacts.toml`, and the model files when `models`.
pub fn write(dir: &Path, program: &[&str], models: bool) -> std::io::Result<FakeInstall> {
    let bin = dir.join(".venv/bin");
    std::fs::create_dir_all(&bin)?;
    let launcher = bin.join("rascal-tools");
    let quoted: Vec<String> = program
        .iter()
        .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
        .collect();
    std::fs::write(
        &launcher,
        format!("#!/bin/sh\nexec {} \"$@\"\n", quoted.join(" ")),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755))?;
    }
    let mut manifest = String::from("# a fake install's artifacts\n");
    for name in MODEL_FILES {
        manifest.push_str(&format!(
            "\n[[artifact]]\nname = \"{name}\"\nurl = \"https://example.invalid/{name}\"\nsha256 = \"{}\"\nlicence = \"Apache-2.0\"\n",
            "0".repeat(64)
        ));
    }
    std::fs::write(dir.join("artifacts.toml"), manifest)?;
    let files = MODEL_FILES
        .iter()
        .map(|n| format!("{n:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(
        dir.join("segmenters.toml"),
        format!(
            "[[segmenter]]\nid = \"tiny\"\ntitle = \"Fake Tiny\"\nlicence = \"Apache-2.0\"\nrank = 2\nparams = [\"threshold\", \"max_instances\", \"pad\"]\nfiles = [{files}]\nlicence_files = []\ndownload_mb = 1\ninstalled_mb = 1\nlab = {{ round = \"segment-01\", remove = 0.548 }}\n\n[[segmenter]]\nid = \"sam3\"\ntitle = \"Fake SAM 3\"\nlicence = \"SAM License\"\nrank = 1\nparams = [\"threshold\", \"max_instances\"]\nfiles = [\"sam3.onnx\"]\nlicence_files = []\ndownload_mb = 3400\ninstalled_mb = 3400\n"
        ),
    )?;
    if models {
        write_models(&dir.join("models"))?;
    }
    Ok(FakeInstall {
        dir: dir.to_path_buf(),
        launcher,
    })
}

/// The identity the fake installation is published under.
pub const INSTALLATION: &str = "f00dfeedf00dfeedf00dfeedf00dfeedf00dfeedf00dfeedf00dfeedf00dfeed";

/// Where the fake installation keeps a model file under `models`.
pub fn model_file(models: &Path, name: &str) -> PathBuf {
    models.join("tiny").join(INSTALLATION).join(name)
}

/// Installs every model file, non-empty, as the default model's installation under `models`.
pub fn write_models(models: &Path) -> std::io::Result<()> {
    let installation = models.join("tiny").join(INSTALLATION);
    std::fs::create_dir_all(&installation)?;
    for name in MODEL_FILES {
        std::fs::write(installation.join(name), b"fake weights")?;
    }
    std::fs::write(models.join("tiny/current"), format!("{INSTALLATION}\n"))
}

/// One entry of a catalogue [`write_catalogue`] writes.
#[derive(Debug, Clone)]
pub struct CatalogueEntry<'a> {
    pub id: &'a str,
    /// The preference order, 1 best.
    pub rank: u32,
    /// The platforms it runs on; empty for every platform.
    pub platforms: &'a [&'a str],
}

/// The files [`install_model`] lays out for `id` and [`write_catalogue`] names for it.
pub fn files_of(id: &str) -> [String; 2] {
    [format!("{id}.onnx"), format!("LICENSE.{id}")]
}

/// Replaces `<dir>/segmenters.toml` with `entries`, in the order given, each naming
/// [`files_of`] its id.
pub fn write_catalogue(dir: &Path, entries: &[CatalogueEntry<'_>]) -> std::io::Result<()> {
    let mut text = String::new();
    for e in entries {
        let [file, licence] = files_of(e.id);
        text.push_str(&format!(
            "[[segmenter]]\nid = {:?}\ntitle = {:?}\nlicence = \"Apache-2.0\"\nrank = {}\nparams = [\"threshold\", \"max_instances\"]\nfiles = [{file:?}]\nlicence_files = [{licence:?}]\ndownload_mb = 1\ninstalled_mb = 1\n",
            e.id,
            e.id.to_uppercase(),
            e.rank
        ));
        if !e.platforms.is_empty() {
            text.push_str(&format!("platforms = {:?}\n", e.platforms));
        }
        text.push('\n');
    }
    std::fs::write(dir.join("segmenters.toml"), text)
}

/// Installs `id` under `models` as the pack lays an installation out, with [`files_of`] it
/// non-empty; without `complete`, its licence file is left out, so it is no installation.
pub fn install_model(models: &Path, id: &str, complete: bool) -> std::io::Result<()> {
    let installation = models.join(id).join(INSTALLATION);
    std::fs::create_dir_all(&installation)?;
    let [file, licence] = files_of(id);
    std::fs::write(installation.join(file), b"fake weights")?;
    if complete {
        std::fs::write(installation.join(licence), b"licence")?;
    }
    std::fs::write(models.join(id).join("current"), format!("{INSTALLATION}\n"))
}

/// Makes the install's launcher also a fake installer (SPEC.md "Installing a model"), standing in
/// for `rascal-tools get` and `rascal-tools remove` and otherwise exec'ing `program` as before.
/// `get <id>` touches `<models>/.got-<id>`, writes the status file `installing` with its pid and
/// progress 0.5, waits (until SIGTERM when `<models>/.slow` exists, else a second), then publishes
/// `<models>/<id>/abc/<id>.onnx` and `current` and records the outcome; SIGTERM records
/// `cancelled` and exits 130. `remove <id>` deletes the model's directory and records the outcome.
/// `<models>` is `RASCAL_TOOLS_MODELS`, which the installer is always given (`serve` needs
/// none).
pub fn write_installer(install: &FakeInstall, program: &[&str]) -> std::io::Result<()> {
    let quoted: Vec<String> = program
        .iter()
        .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
        .collect();
    let script = format!(
        r#"#!/bin/sh
done='{{"last_outcome":{{"action":"ACTION","state":"STATE","error":null,"at":"2026-11-09T00:00:00Z"}}}}'
case "$1" in
  get)
    M="${{RASCAL_TOOLS_MODELS:?}}"; id="${{2:?}}"; mkdir -p "$M/.status"; touch "$M/.got-$id"
    trap 'echo "$done" | sed "s/ACTION/install/;s/STATE/cancelled/" > "$M/.status/$id.json"; exit 130' TERM
    echo "{{\"state\":\"installing\",\"progress\":0.5,\"pid\":$$}}" > "$M/.status/$id.json"
    if [ -e "$M/.slow" ]; then sleep 60 & wait $!; else sleep 1 & wait $!; fi
    mkdir -p "$M/$id/abc"; echo w > "$M/$id/abc/$id.onnx"; echo abc > "$M/$id/current"
    echo "$done" | sed "s/ACTION/install/;s/STATE/succeeded/" > "$M/.status/$id.json"
    echo "$id: installed abc";;
  remove)
    M="${{RASCAL_TOOLS_MODELS:?}}"; id="${{2:?}}"; rm -rf "${{M:?}}/${{id:?}}"; mkdir -p "$M/.status"
    echo "$done" | sed "s/ACTION/uninstall/;s/STATE/succeeded/" > "$M/.status/$id.json"
    echo "$id: removed";;
  *) exec {} "$@";;
esac
"#,
        quoted.join(" ")
    );
    std::fs::write(&install.launcher, script)
}
