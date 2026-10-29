//! The fake tool pack (task 3.3): an MCP server over stdio with one tool, `segment`, built on
//! the SDK the real pack client speaks to. Its one behaviour is chosen by the first argument,
//! so a test's configuration selects it without touching the environment (which the client
//! allowlists):
//!
//! | behaviour | what `segment` does |
//! |---|---|
//! | `exact` | `max_instances` polygon regions labelled with the prompt, one plane PNG each, in the image's frame; sleeps `delay_ms` first |
//! | `bad-schema` | the envelope carries a field the schema does not know |
//! | `over-bounds` | more regions than the manifest's `max_regions` |
//! | `block-mismatch` | one image block more than `images` lists |
//! | `foreign-frame` | a region in a frame the call did not pass |
//! | `wrong-size` | a plane whose pixels are not the size `images` says |
//! | `non-png` | an image block whose media type is not PNG |
//! | `big-plane` | a first plane over the manifest's `max_image_bytes` (and over the SDK's default line limit) |
//! | `slow` | sleeps until cancelled, then answers an error (`exact` with `delay_ms` past the timeout is the late answer: the sleep ignores cancellation) |
//! | `die` | writes a line to stderr, then exits in the middle of the call |
//! | `ignore-signals` | answers as `exact`, but ignores stdin closing and SIGTERM, so only SIGKILL ends it |
//! | `mutate <marker file>` | as `die`; on the second start (the marker exists) lists `segment` with other bounds |
//! | `stderr` | writes a line with a token, a bearer value and the prompt to stderr, then answers as `exact` |
//! | `mute` | never answers `initialize` (reads nothing, writes nothing) |
//! | `env` | answers as `exact` with the names of its environment variables in the summary |
//! | `no-annotation` | lists the tool without rascal's annotation block |
//! | `old-version` | speaks revision 2025-06-18 only |
//!
//! `--concurrent` anywhere in the arguments marks the tool concurrent in its manifest;
//! `--name=<tool>` serves the tool under another name.

use std::borrow::Cow;
use std::sync::Arc;

use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorCode, ErrorData,
    Implementation, InitializeResult, ListToolsResult, PaginatedRequestParams, ProtocolVersion,
    ServerCapabilities, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ServiceExt, transport::stdio};

/// A test-only recipe through the fake: `segment` → `rasterize` of the first region →
/// `feather`. (The plan named `select`, which arrives in M4; `rasterize` takes one region.)
pub const RECIPE: &str = r#"version = 1
name = "segment-test"
use_case = "remove"

[inputs]
image = { type = "Image" }

[[step]]
id = "segment"
op = "segment"
inputs = { image = "inputs.image" }
prompt = "the chair"
max_instances = 2

[[step]]
id = "rasterize"
op = "rasterize"
inputs = { location = "segment.region_0", image = "inputs.image" }
use_case = "remove"

[[step]]
id = "feather"
op = "feather"
inputs = { mask = "rasterize.mask" }
sigma_px = 3.0

[outputs]
mask = "feather.mask"
image = "inputs.image"
"#;

/// The behaviours by name, as the first argument selects them.
pub const BEHAVIOURS: [&str; 17] = [
    "exact",
    "bad-schema",
    "over-bounds",
    "block-mismatch",
    "foreign-frame",
    "wrong-size",
    "non-png",
    "big-plane",
    "slow",
    "die",
    "ignore-signals",
    "mutate",
    "stderr",
    "mute",
    "env",
    "no-annotation",
    "old-version",
];

#[derive(Clone)]
struct Fake {
    behaviour: String,
    concurrent: bool,
    /// The tool's name (`segment` unless `--name=`).
    name: String,
    /// `mutate`'s marker: present on a restart, which then lists other bounds.
    mutated: bool,
}

/// The copied-shape manifest of `segment`.
fn compat_manifest(name: &str, concurrent: bool, mutated: bool) -> serde_json::Value {
    serde_json::json!({
        "manifest_version": 1,
        "name": name,
        "version": "0.1.0",
        "purpose": "Finds the instances a prompt names in an image and returns a polygon and a plane for each.",
        "when_to_use": "When the region to mask is an object named in words rather than drawn.",
        "when_not_to_use": "When the region is a known box or a drawn stroke; use rect or stroke.",
        "output_guidance": "One region per instance with its score, and one plane image per region in the image's frame; select or rasterize makes a mask.",
        "cost": { "tier": "expensive", "seconds": 2.0 },
        "returns": {
            "max_images": 8, "max_long_edge": 8192, "max_image_bytes": 16777216,
            "max_summary_bytes": 1024, "max_regions": if mutated { 4 } else { 8 }, "max_region_label_bytes": 256,
            "max_measurements": 8, "max_transforms": 0, "max_envelope_bytes": 1048576
        },
        "model_schema": { "type": "object", "properties": { "prompt": { "type": "string" } } },
        "input_schema": { "type": "object" },
        "output_schema": { "type": "object" },
        "concurrent": concurrent
    })
}

/// rascal's annotation block of `segment`.
fn annotation() -> serde_json::Value {
    serde_json::json!({
        "annotation_version": 1,
        "inputs": [
            { "name": "image", "artifact_type": "Image", "required": true, "purpose": "The image to segment." }
        ],
        "outputs": [
            { "name": "region", "artifact_type": "Location", "required": true, "many": true,
              "purpose": "One polygon per instance, with its score." },
            { "name": "plane", "artifact_type": "Image", "required": true, "many": true,
              "purpose": "One instance plane per region, in the image's frame." }
        ],
        "params": [
            { "name": "prompt", "purpose": "What to find, in words.", "kind": "string", "required": false, "default": "" },
            { "name": "max_instances", "purpose": "The most instances to return.", "kind": "integer", "required": false, "default": 2, "min": 1, "max": 8 },
            { "name": "delay_ms", "purpose": "A pause before answering, for tests of timing.", "kind": "integer", "required": false, "default": 0, "min": 0, "max": 60000 }
        ]
    })
}

fn tool(fake: &Fake) -> Tool {
    let schema: serde_json::Map<String, serde_json::Value> =
        serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "image": { "type": "string" },
                "prompt": { "type": "string" },
                "max_instances": { "type": "integer" },
                "delay_ms": { "type": "integer" }
            }
        }))
        .expect("a schema object");
    let mut tool = Tool::new(
        fake.name.clone(),
        "Finds the instances a prompt names in an image.",
        Arc::new(schema),
    );
    let mut meta = serde_json::Map::new();
    meta.insert(
        "manifest".to_string(),
        compat_manifest(&fake.name, fake.concurrent, fake.mutated),
    );
    if fake.behaviour != "no-annotation" {
        meta.insert("rascal".to_string(), annotation());
    }
    tool.meta = Some(rmcp::model::MetaObject(meta));
    tool
}

/// Standard base64, padded.
pub fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Standard base64, decoded; `None` on a bad character or length.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes().filter(|c| !c.is_ascii_whitespace()) {
        if c == b'=' {
            break;
        }
        let v = TABLE.iter().position(|&t| t == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The image's size and frame from `_harness`, checking the pixels really are reachable:
/// the file at `path` decodes, or the base64 decodes.
fn first_image(args: &serde_json::Value) -> Result<(u32, u32, String), ErrorData> {
    let harness = args
        .get("_harness")
        .ok_or_else(|| ErrorData::invalid_params("no _harness", None))?;
    let image = harness
        .get("images")
        .and_then(|i| i.as_array())
        .and_then(|i| i.first())
        .ok_or_else(|| ErrorData::invalid_params("no image in _harness", None))?;
    let width = image["width"].as_u64().unwrap_or(0) as u32;
    let height = image["height"].as_u64().unwrap_or(0) as u32;
    let frame = image["frame"].as_str().unwrap_or("").to_string();
    let decoded = if let Some(path) = image.get("path").and_then(|p| p.as_str()) {
        image::ImageReader::open(path)
            .and_then(|r| r.with_guessed_format())
            .map_err(|e| ErrorData::internal_error(format!("path {path}: {e}"), None))?
            .into_dimensions()
            .map_err(|e| ErrorData::internal_error(format!("path {path}: {e}"), None))?
    } else if let Some(b64) = image.get("base64").and_then(|p| p.as_str()) {
        let bytes = base64_decode(b64)
            .ok_or_else(|| ErrorData::internal_error("base64 does not decode", None))?;
        image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
            .into_dimensions()
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
    } else {
        return Err(ErrorData::invalid_params(
            "the image has neither path nor base64",
            None,
        ));
    };
    if decoded != (width, height) {
        return Err(ErrorData::internal_error(
            format!("_harness says {width}x{height} but the pixels are {decoded:?}"),
            None,
        ));
    }
    Ok((width, height, frame))
}

/// A plane with a box filled, as a PNG.
fn plane_png(width: u32, height: u32, bbox: [f64; 4]) -> Vec<u8> {
    let image = image::GrayImage::from_fn(width, height, |x, y| {
        let fx = (f64::from(x) + 0.5) / f64::from(width);
        let fy = (f64::from(y) + 0.5) / f64::from(height);
        let inside = fx >= bbox[0] && fx < bbox[2] && fy >= bbox[1] && fy < bbox[3];
        image::Luma([if inside { 255 } else { 0 }])
    });
    let mut bytes = Vec::new();
    image::DynamicImage::ImageLuma8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encoding a PNG in memory");
    bytes
}

/// A square of pseudo-random RGB noise as a PNG: it does not compress, so its size is about
/// three bytes per pixel.
fn noise_png(width: u32, height: u32) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let image = image::RgbImage::from_fn(width, height, |_, _| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        image::Rgb([state as u8, (state >> 8) as u8, (state >> 16) as u8])
    });
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgb8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encoding a PNG in memory");
    bytes
}

/// The `i`th instance's box: a column of boxes down the left half.
fn instance_bbox(i: usize) -> [f64; 4] {
    let top = 0.1 + 0.2 * (i % 4) as f64;
    let left = 0.1 + 0.4 * (i / 4) as f64;
    [left, top, left + 0.25, top + 0.15]
}

impl Fake {
    fn segment(
        &self,
        args: &serde_json::Value,
        envs: Vec<String>,
    ) -> Result<CallToolResult, ErrorData> {
        let (width, height, frame) = first_image(args)?;
        let prompt = args["prompt"].as_str().unwrap_or("").to_string();
        let max_instances = args["max_instances"].as_u64().unwrap_or(2) as usize;
        let count = match self.behaviour.as_str() {
            "over-bounds" => 9,
            _ => max_instances,
        };
        let mut regions = Vec::new();
        let mut images = Vec::new();
        let mut content = vec![ContentBlock::text(format!(
            "{count} instances of {prompt:?}"
        ))];
        for i in 0..count {
            let b = instance_bbox(i);
            // `big-plane`: the first plane is 2400x2400 of noise, over the manifest's 16 MiB.
            let big = self.behaviour == "big-plane" && i == 0;
            let polygon = [[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]];
            let region_frame = if self.behaviour == "foreign-frame" && i == 0 {
                "raw:1x1:flat".to_string()
            } else {
                frame.clone()
            };
            regions.push(serde_json::json!({
                "id": format!("instance-{i}"),
                "location": { "kind": "polygon", "frame": region_frame, "polygon": polygon },
                "label": if prompt.is_empty() { "instance".to_string() } else { prompt.clone() },
                "score": 0.9 - 0.1 * i as f64
            }));
            let (pw, ph) = if self.behaviour == "wrong-size" && i == 0 {
                (width / 2 + 1, height)
            } else {
                (width, height)
            };
            images.push(serde_json::json!({
                "frame": frame, "parent": frame, "width": width, "height": height,
                "media_type": "image/png", "identity": ""
            }));
            let png = if big {
                noise_png(2400, 2400)
            } else {
                plane_png(pw, ph, b)
            };
            content.push(ContentBlock::image(
                base64_encode(&png),
                if self.behaviour == "non-png" && i == 0 {
                    "image/jpeg"
                } else {
                    "image/png"
                },
            ));
        }
        if self.behaviour == "block-mismatch" {
            content.push(ContentBlock::image(
                base64_encode(&plane_png(width, height, instance_bbox(0))),
                "image/png",
            ));
        }
        let summary = match self.behaviour.as_str() {
            "env" => format!("env: {}", envs.join(",")),
            _ => format!("{count} instances of {prompt:?}"),
        };
        let mut envelope = serde_json::json!({
            "envelope_version": 1,
            "status": "ok",
            "summary": summary,
            "regions": regions,
            "images": images,
            "provenance": { "tool": "segment", "version": "0.1.0", "duration_ms": 1 }
        });
        if self.behaviour == "bad-schema" {
            envelope["verdict"] = serde_json::json!("pass");
        }
        let mut result = CallToolResult::success(content);
        result.structured_content = Some(envelope);
        result.is_error = Some(false);
        Ok(result)
    }
}

impl ServerHandler for Fake {
    fn get_info(&self) -> InitializeResult {
        let version = if self.behaviour == "old-version" {
            ProtocolVersion::V_2025_06_18
        } else {
            ProtocolVersion::V_2025_11_25
        };
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(version)
            .with_server_info(Implementation::new("rascal-fake-pack", "0"))
            .with_instructions(format!("a fake pack behaving as {}", self.behaviour))
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        if self.behaviour == "old-version" {
            Cow::Borrowed(&[ProtocolVersion::V_2025_06_18])
        } else {
            Cow::Borrowed(&[ProtocolVersion::V_2025_11_25])
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: vec![tool(self)],
            next_cursor: None,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if request.name.as_ref() != self.name {
            return Err(ErrorData::invalid_params(
                format!("no tool {}", request.name),
                Some(serde_json::json!({ "class": "usage" })),
            ));
        }
        let args = serde_json::Value::Object(request.arguments.clone().unwrap_or_default());
        let delay = args["delay_ms"].as_u64().unwrap_or(0);
        if delay > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }
        match self.behaviour.as_str() {
            "slow" => {
                context.ct.cancelled().await;
                return Err(ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "cancelled",
                    Some(serde_json::json!({ "class": "cancelled" })),
                ));
            }
            "die" | "mutate" => {
                eprintln!("fake-pack: fatal: the model failed to load (token=SECRET-TOKEN-VALUE)");
                std::process::exit(7)
            }
            "stderr" => {
                eprintln!(
                    "fake-pack: calling upstream with token=SECRET-TOKEN-VALUE and Authorization: Bearer SECRET-BEARER-VALUE for prompt {}",
                    args["prompt"].as_str().unwrap_or("")
                );
            }
            _ => {}
        }
        let mut envs: Vec<String> = std::env::vars().map(|(k, _)| k).collect();
        envs.sort();
        Ok(self.segment(&args, envs)?.into())
    }
}

/// Serves the fake on stdio until the client goes away. `args` are the process arguments
/// after the program name: the behaviour, and `--concurrent`.
pub async fn serve(args: &[String]) -> i32 {
    let concurrent = args.iter().any(|a| a == "--concurrent");
    let name = args
        .iter()
        .find_map(|a| a.strip_prefix("--name="))
        .unwrap_or("segment")
        .to_string();
    let mut positional = args.iter().filter(|a| !a.starts_with("--"));
    let behaviour = positional
        .next()
        .cloned()
        .unwrap_or_else(|| "exact".to_string());
    if !BEHAVIOURS.contains(&behaviour.as_str()) {
        eprintln!(
            "fake-pack: unknown behaviour {behaviour}; one of {}",
            BEHAVIOURS.join(", ")
        );
        return 64;
    }
    // `mutate` keeps a marker across restarts: the second start lists other bounds.
    let mut mutated = false;
    if behaviour == "mutate" {
        let Some(marker) = positional.next() else {
            eprintln!("fake-pack: mutate takes a marker file path");
            return 64;
        };
        mutated = std::path::Path::new(marker).exists();
        if !mutated && std::fs::write(marker, "started once").is_err() {
            eprintln!("fake-pack: cannot write the marker {marker}");
            return 3;
        }
    }
    if behaviour == "mute" {
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
        return 0;
    }
    // `ignore-signals`: SIGTERM is handled and ignored; stdin closing is outlived.
    let ignoring = behaviour == "ignore-signals";
    let _sigterm = if ignoring {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok()
    } else {
        None
    };
    let fake = Fake {
        behaviour,
        concurrent,
        name,
        mutated,
    };
    match fake.serve(stdio()).await {
        Ok(service) => {
            let _ = service.waiting().await;
            if ignoring {
                tokio::time::sleep(std::time::Duration::from_secs(120)).await;
            }
            0
        }
        Err(e) => {
            eprintln!("fake-pack: {e}");
            3
        }
    }
}

/// The fake as a program: a current-thread runtime around [`serve`].
pub fn main(args: &[String]) -> i32 {
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(serve(args)),
        Err(e) => {
            eprintln!("fake-pack: runtime: {e}");
            3
        }
    }
}
