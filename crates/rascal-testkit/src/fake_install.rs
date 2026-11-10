//! A fake segmenter pack install, shaped as `rascal tools setup` leaves one: the launcher
//! `<dir>/.venv/bin/rascal-tools` (a shell script that runs any program, such as the fake pack),
//! `<dir>/artifacts.toml` naming the model files, `<dir>/segmenters.toml` naming the default
//! model's files, and with `models` the default model installed under `<dir>/models/` as the pack
//! lays an installation out (`<models>/tiny/current` naming `<models>/tiny/<identity>/`). Readiness inspects exactly this shape (PLAN.md 12.2).

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
            "[[segmenter]]\nid = \"tiny\"\ntitle = \"Fake Tiny\"\nlicence = \"Apache-2.0\"\ndefault = true\nparams = [\"threshold\", \"max_instances\", \"pad\"]\nfiles = [{files}]\nlicence_files = []\ndownload_mb = 1\ninstalled_mb = 1\nlab = {{ round = \"segment-01\", remove = 0.548 }}\n\n[[segmenter]]\nid = \"sam3\"\ntitle = \"Fake SAM 3\"\nlicence = \"SAM License\"\nparams = [\"threshold\", \"max_instances\"]\nfiles = [\"sam3.onnx\"]\nlicence_files = []\ndownload_mb = 3400\ninstalled_mb = 3400\n"
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
