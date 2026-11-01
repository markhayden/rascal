//! A fake segmenter pack install, shaped as `rascal tools setup` leaves one: the launcher
//! `<dir>/.venv/bin/rascal-tools` (a shell script that runs any program, such as the fake pack),
//! `<dir>/artifacts.toml` naming the model files, and with `models` the files themselves under
//! `<dir>/models/`. Readiness inspects exactly this shape (PLAN.md 12.2).

use std::path::{Path, PathBuf};

/// The model files the pack's `artifacts.toml` names (the detector, its tokenizer, the
/// segmenter's encoder and decoder).
pub const MODEL_FILES: [&str; 4] = [
    "owlv2-base-patch16.onnx",
    "owlv2-base-patch16.tokenizer.json",
    "sam2.1-hiera-tiny.encoder.onnx",
    "sam2.1-hiera-tiny.decoder.onnx",
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
    if models {
        write_models(&dir.join("models"))?;
    }
    Ok(FakeInstall {
        dir: dir.to_path_buf(),
        launcher,
    })
}

/// Writes every model file, non-empty, into `models`.
pub fn write_models(models: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(models)?;
    for name in MODEL_FILES {
        std::fs::write(models.join(name), b"fake weights")?;
    }
    Ok(())
}
