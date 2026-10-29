//! The HTTP filler: a multipart POST of `image`, `mask` (when the profile has one) and
//! `prompt` (when given), a bearer key from `key_env` read at call time, a timeout, and a
//! PNG body back.

use std::path::Path;
use std::time::{Duration, Instant};

use image::DynamicImage;
use rascal_core::Error;
use rascal_core::error::Result;

use super::{FillRequest, Filler, FillerCall, decode_answer, excerpt, runtime};

/// Where a key comes from: the process environment, or in a test a closure.
type KeySource = std::sync::Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

pub struct HttpFiller {
    name: String,
    url: String,
    key_env: Option<String>,
    timeout: Duration,
    key_source: KeySource,
}

impl HttpFiller {
    pub fn new(
        name: &str,
        url: &str,
        key_env: Option<String>,
        timeout: Duration,
    ) -> Result<HttpFiller> {
        // The TLS provider is process-wide; installing it twice is not an error here.
        let _ = rustls::crypto::ring::default_provider().install_default();
        Ok(HttpFiller {
            name: name.to_string(),
            url: url.to_string(),
            key_env,
            timeout,
            key_source: std::sync::Arc::new(|var| std::env::var(var).ok()),
        })
    }

    /// Reads keys through `source` instead of the environment (tests).
    pub fn with_key_source(
        mut self,
        source: impl Fn(&str) -> Option<String> + Send + Sync + 'static,
    ) -> HttpFiller {
        self.key_source = std::sync::Arc::new(source);
        self
    }

    /// The bearer key, read now and handed to the header only.
    fn key(&self) -> Result<Option<String>> {
        match &self.key_env {
            None => Ok(None),
            Some(var) => (self.key_source)(var).map(Some).ok_or_else(|| {
                Error::usage(format!(
                    "fillers.{}: key_env names {var}, which is not set",
                    self.name
                ))
            }),
        }
    }
}

impl Filler for HttpFiller {
    fn name(&self) -> &str {
        &self.name
    }

    fn call(&self, request: &FillRequest, _scratch: &Path) -> Result<(DynamicImage, FillerCall)> {
        let key = self.key()?;
        let mut call = FillerCall {
            filler: self.name.clone(),
            target: self.url.clone(),
            request_width: request.width,
            request_height: request.height,
            request_bytes: request.bytes(),
            response_bytes: 0,
            duration_ms: 0,
            status: -1,
            outcome: String::new(),
        };
        let started = Instant::now();
        let rt = runtime()?;
        let result: Result<(u16, Vec<u8>, String)> = rt.block_on(async {
            let client = reqwest::Client::builder()
                .timeout(self.timeout)
                .build()
                .map_err(|e| Error::op("filler", format!("fillers.{}: client: {e}", self.name)))?;
            let mut form = reqwest::multipart::Form::new().part(
                "image",
                reqwest::multipart::Part::bytes(request.image_png.clone())
                    .file_name("image.png")
                    .mime_str("image/png")
                    .map_err(|e| Error::op("filler", e.to_string()))?,
            );
            if let Some(mask) = &request.mask_png {
                form = form.part(
                    "mask",
                    reqwest::multipart::Part::bytes(mask.clone())
                        .file_name("mask.png")
                        .mime_str("image/png")
                        .map_err(|e| Error::op("filler", e.to_string()))?,
                );
            }
            if let Some(prompt) = &request.prompt {
                form = form.text("prompt", prompt.clone());
            }
            let mut builder = client.post(&self.url).multipart(form);
            if let Some(key) = &key {
                builder = builder.bearer_auth(key);
            }
            let response = builder.send().await.map_err(|e| {
                let kind = if e.is_timeout() {
                    format!("no answer within {} s", self.timeout.as_secs())
                } else {
                    // reqwest's message may contain the URL; never the key.
                    e.without_url().to_string()
                };
                Error::op("filler", format!("fillers.{}: {kind}", self.name))
            })?;
            let status = response.status().as_u16();
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            let body = response.bytes().await.map_err(|e| {
                Error::op(
                    "filler",
                    format!("fillers.{}: reading the body: {e}", self.name),
                )
            })?;
            Ok((status, body.to_vec(), content_type))
        });
        call.duration_ms = started.elapsed().as_millis() as u64;
        let (status, body, content_type) = match result {
            Ok(v) => v,
            Err(e) => {
                call.outcome = e.to_string();
                return Err(e);
            }
        };
        call.status = i64::from(status);
        call.response_bytes = body.len() as u64;
        if !(200..300).contains(&status) {
            let e = Error::op(
                "filler",
                format!(
                    "fillers.{}: HTTP {status}: {}",
                    self.name,
                    excerpt(&body, 200).trim()
                ),
            );
            call.outcome = e.to_string();
            return Err(e);
        }
        let who = format!("fillers.{} ({content_type})", self.name);
        match decode_answer(&body, request, &who) {
            Ok(image) => {
                call.outcome = "ok".to_string();
                Ok((image, call))
            }
            Err(e) => {
                call.outcome = e.to_string();
                Err(e)
            }
        }
    }
}
