//! The command filler: `command = ["prog", "{image}", "{mask}", "{out}"]` with the
//! placeholders substituted, run with a timeout; exit 0 and a PNG at `{out}` is the contract.

use std::path::Path;
use std::time::{Duration, Instant};

use image::DynamicImage;
use rascal_core::Error;
use rascal_core::error::Result;

use super::{CallOutcome, FillRequest, Filler, FillerCall, decode_answer, excerpt, runtime};

pub struct CommandFiller {
    name: String,
    argv: Vec<String>,
    timeout: Duration,
}

impl CommandFiller {
    pub fn new(name: &str, argv: Vec<String>, timeout: Duration) -> CommandFiller {
        CommandFiller {
            name: name.to_string(),
            argv,
            timeout,
        }
    }

    /// The argument vector with the placeholders substituted.
    fn substituted(
        &self,
        image: &Path,
        mask: Option<&Path>,
        out: &Path,
        prompt: Option<&str>,
    ) -> Result<Vec<String>> {
        let mut argv = Vec::with_capacity(self.argv.len());
        for arg in &self.argv {
            let mut value = arg.replace("{image}", &image.display().to_string());
            value = value.replace("{out}", &out.display().to_string());
            if value.contains("{mask}") {
                let Some(mask) = mask else {
                    return Err(Error::usage(format!(
                        "fillers.{}: the command names {{mask}} but the profile sends no mask (set send_mask = true or use a mask-input profile)",
                        self.name
                    )));
                };
                value = value.replace("{mask}", &mask.display().to_string());
            }
            if value.contains("{prompt}") {
                value = value.replace("{prompt}", prompt.unwrap_or(""));
            }
            argv.push(value);
        }
        Ok(argv)
    }
}

impl Filler for CommandFiller {
    fn name(&self) -> &str {
        &self.name
    }

    fn call(&self, request: &FillRequest, scratch: &Path) -> CallOutcome {
        let mut call = FillerCall {
            filler: self.name.clone(),
            target: self.argv.join(" "),
            request_width: request.width,
            request_height: request.height,
            request_bytes: request.bytes(),
            response_bytes: 0,
            duration_ms: 0,
            status: -1,
            outcome: String::new(),
        };
        let fill = self.run(request, scratch, &mut call);
        call.outcome = match &fill {
            Ok(_) => "ok".to_string(),
            Err(e) => e.to_string(),
        };
        CallOutcome { fill, call }
    }
}

impl CommandFiller {
    fn run(
        &self,
        request: &FillRequest,
        scratch: &Path,
        call: &mut FillerCall,
    ) -> Result<DynamicImage> {
        let image_path = scratch.join("request-image.png");
        let mask_path = scratch.join("request-mask.png");
        let out_path = scratch.join("response.png");
        std::fs::write(&image_path, &request.image_png).map_err(|e| Error::io(&image_path, e))?;
        if let Some(mask) = &request.mask_png {
            std::fs::write(&mask_path, mask).map_err(|e| Error::io(&mask_path, e))?;
        }
        let _ = std::fs::remove_file(&out_path);
        let argv = self.substituted(
            &image_path,
            request.mask_png.as_ref().map(|_| mask_path.as_path()),
            &out_path,
            request.prompt.as_deref(),
        )?;
        let started = Instant::now();
        let rt = runtime()?;
        let outcome = rt.block_on(async {
            let mut command = tokio::process::Command::new(&argv[0]);
            command
                .args(&argv[1..])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true);
            let child = command.spawn().map_err(|e| {
                Error::op(
                    "filler",
                    format!("fillers.{}: cannot start {}: {e}", self.name, argv[0]),
                )
            })?;
            match tokio::time::timeout(self.timeout, child.wait_with_output()).await {
                Ok(Ok(output)) => Ok(output),
                Ok(Err(e)) => Err(Error::op("filler", format!("fillers.{}: {e}", self.name))),
                Err(_) => Err(Error::op(
                    "filler",
                    format!(
                        "fillers.{}: no answer within {} s; the process was killed",
                        self.name,
                        self.timeout.as_secs()
                    ),
                )),
            }
        });
        call.duration_ms = started.elapsed().as_millis() as u64;
        let output = outcome?;
        call.status = i64::from(output.status.code().unwrap_or(-1));
        if !output.status.success() {
            return Err(Error::op(
                "filler",
                format!(
                    "fillers.{}: exited with {}: {}",
                    self.name,
                    output
                        .status
                        .code()
                        .map_or("a signal".to_string(), |c| c.to_string()),
                    excerpt(&output.stderr, 4096).trim()
                ),
            ));
        }
        let bytes = std::fs::read(&out_path).map_err(|e| {
            Error::op(
                "filler",
                format!(
                    "fillers.{}: exited 0 but wrote no {}: {e}",
                    self.name,
                    out_path.display()
                ),
            )
        })?;
        call.response_bytes = bytes.len() as u64;
        decode_answer(&bytes, request, &format!("fillers.{}", self.name))
    }
}
