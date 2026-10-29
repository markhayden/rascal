//! Task 3.3, the pack client's pure parts: the environment allowlist (removal wins over
//! declaration), stderr redaction, manifest validation from a tool listing, and result
//! validation of each kind of bad answer, on constructed values. The process-level suite
//! (timeouts, cancellation, death, replay) drives the fake pack from `rascal-mask`'s tests.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use rascal_core::frame::{Frame, Projection};
use rascal_core::pack::{CompatManifest, PackAnnotation};
use rascal_mcp::client::PackTool;
use rascal_mcp::client::env::{allowlist, is_credential, redact, secrets};
use rascal_mcp::client::validate::validate;
use rmcp::model::{CallToolResult, ContentBlock, Tool};

fn env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_allowlist_is_the_baseline_plus_declared_minus_every_credential() {
    let parent = env(&[
        ("PATH", "/bin"),
        ("HOME", "/h"),
        ("LANG", "C"),
        ("TMPDIR", "/t"),
        ("SHELL", "/bin/zsh"),
        ("RASCAL_TOOLS_MODELS", "/m"),
        ("HOSTED_FILLER_KEY", "key-one"),
        ("VENDOR_TOKEN", "key-two"),
        ("my_secret", "key-three"),
        ("OTHER", "x"),
    ]);
    let key_envs = strings(&["HOSTED_FILLER_KEY"]);
    // Declared or not, a credential is removed: removal wins over declaration.
    let declared = strings(&[
        "RASCAL_TOOLS_MODELS",
        "HOSTED_FILLER_KEY",
        "VENDOR_TOKEN",
        "my_secret",
    ]);
    let got = allowlist(&declared, &key_envs, parent.clone());
    let names: Vec<&str> = got.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        vec!["HOME", "LANG", "PATH", "RASCAL_TOOLS_MODELS", "TMPDIR"]
    );
    // Nothing declared: the baseline only; an absent baseline name is simply absent.
    let got = allowlist(&[], &key_envs, parent.clone());
    assert_eq!(got.len(), 4);
    assert!(!got.contains_key("LC_ALL"));
    assert!(is_credential("x_KEY", &[]) && is_credential("a_token", &[]));
    assert!(!is_credential("KEYBOARD", &[]) && !is_credential("TOKENIZER", &[]));
    // The secrets are the credential values, for redaction.
    let mut s = secrets(&key_envs, parent);
    s.sort();
    assert_eq!(
        s,
        vec!["key-one".to_string(), "key-three".into(), "key-two".into()]
    );
}

#[test]
fn stderr_redaction_covers_values_bearer_and_named_assignments() {
    let secrets = strings(&["sk-live-1234", "hunter2"]);
    let line = "GET https://x/?api_key=sk-live-1234 Authorization: Bearer abcdef token=ZZZ password: hunter2 KEYBOARD=qwerty";
    let out = redact(line, &secrets);
    assert!(
        !out.contains("sk-live-1234") && !out.contains("hunter2"),
        "{out}"
    );
    assert!(!out.contains("abcdef") && !out.contains("ZZZ"), "{out}");
    assert!(
        out.contains("Bearer [redacted]") && out.contains("token=[redacted]"),
        "{out}"
    );
    assert!(out.contains("password: [redacted]"), "{out}");
    assert!(
        out.contains("KEYBOARD=qwerty"),
        "a non-credential assignment stays: {out}"
    );
    assert_eq!(redact("plain text", &[]), "plain text");
    // Credentials inside a URL's query string and in a comma list, with no value known.
    let out = redact(
        "POST https://h/v1?key=AIzaSy123&token=def,api_token:xyz done",
        &[],
    );
    assert_eq!(
        out,
        "POST https://h/v1?key=[redacted]&token=[redacted],api_token:[redacted] done"
    );
}

fn compat() -> serde_json::Value {
    serde_json::json!({
        "name": "segment", "version": "0.1.0",
        "purpose": "p", "when_to_use": "w", "when_not_to_use": "n", "output_guidance": "o",
        "cost": { "tier": "expensive", "seconds": 2.0 },
        "returns": { "max_images": 8, "max_long_edge": 8192, "max_image_bytes": 16777216,
                     "max_summary_bytes": 64, "max_regions": 2, "max_region_label_bytes": 16,
                     "max_measurements": 8, "max_transforms": 0, "max_envelope_bytes": 1048576 },
        "model_schema": {}, "input_schema": {}, "output_schema": {}
    })
}

fn annotation() -> serde_json::Value {
    serde_json::json!({
        "annotation_version": 1,
        "inputs": [{ "name": "image", "artifact_type": "Image", "required": true, "purpose": "i" }],
        "outputs": [
            { "name": "region", "artifact_type": "Location", "required": true, "many": true, "purpose": "r" },
            { "name": "plane", "artifact_type": "Image", "required": true, "many": true, "purpose": "p" }
        ],
        "params": [{ "name": "prompt", "purpose": "q", "kind": "string", "required": false, "default": "" }]
    })
}

fn tool_with(meta: serde_json::Map<String, serde_json::Value>) -> Tool {
    let schema: serde_json::Map<String, serde_json::Value> =
        serde_json::from_value(serde_json::json!({ "type": "object" })).unwrap();
    let mut tool = Tool::new("segment", "d", Arc::new(schema));
    tool.meta = Some(rmcp::model::MetaObject(meta));
    tool
}

fn meta(
    compat: serde_json::Value,
    annotation: Option<serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    m.insert("manifest".into(), compat);
    if let Some(a) = annotation {
        m.insert("rascal".into(), a);
    }
    m
}

#[test]
fn a_listed_tool_becomes_a_native_manifest_or_is_refused_by_name() {
    let schemas = rascal_core::schemas::SchemaDir::discover();
    assert!(schemas.is_some(), "the tests run in a checkout");
    let ok = PackTool::from_tool(
        &tool_with(meta(compat(), Some(annotation()))),
        schemas.as_ref(),
    )
    .unwrap();
    assert_eq!(ok.manifest.name, "segment");
    assert_eq!(ok.manifest.outputs.len(), 2);
    assert!(ok.manifest.outputs.iter().all(|p| p.many));
    assert_eq!(ok.manifest.returns.max_regions, 2);
    // The derived output schema names the many ports as patterns, requiring the first of each.
    let schema = &ok.manifest.output_schema;
    assert!(
        schema["patternProperties"].get("^region_[0-9]+$").is_some(),
        "{schema}"
    );
    assert_eq!(schema["allOf"][0]["required"][0], "region_0");
    // Both typed parses are closed: an unknown field in either is refused.
    let mut bad = compat();
    bad["verdict"] = serde_json::json!("pass");
    let e = PackTool::from_tool(&tool_with(meta(bad, Some(annotation()))), schemas.as_ref())
        .unwrap_err();
    assert!(e.to_string().contains("manifest"), "{e}");
    let mut bad = annotation();
    bad["extra"] = serde_json::json!(1);
    let e =
        PackTool::from_tool(&tool_with(meta(compat(), Some(bad))), schemas.as_ref()).unwrap_err();
    assert!(e.to_string().contains("annotation"), "{e}");
    // No annotation block, no meta, a name that disagrees, a bound over rascal's ceiling.
    let e = PackTool::from_tool(&tool_with(meta(compat(), None)), None).unwrap_err();
    assert!(e.to_string().contains("no rascal"), "{e}");
    let mut tool = tool_with(meta(compat(), Some(annotation())));
    tool.meta = None;
    let e = PackTool::from_tool(&tool, None).unwrap_err();
    assert!(e.to_string().contains("no _meta"), "{e}");
    let mut other = compat();
    other["name"] = serde_json::json!("other");
    let e = PackTool::from_tool(&tool_with(meta(other, Some(annotation()))), None).unwrap_err();
    assert!(e.to_string().contains("names other"), "{e}");
    let mut huge = compat();
    huge["returns"]["max_images"] = serde_json::json!(1000);
    let e = PackTool::from_tool(&tool_with(meta(huge, Some(annotation()))), None).unwrap_err();
    assert!(e.to_string().contains("max_images 1000 is over"), "{e}");
    // A many input port is refused.
    let mut many_in = annotation();
    many_in["inputs"][0]["many"] = serde_json::json!(true);
    let e = PackTool::from_tool(&tool_with(meta(compat(), Some(many_in))), None).unwrap_err();
    assert!(e.to_string().contains("only an output may be"), "{e}");
    // Typed shapes agree with the compiled types.
    let _: CompatManifest = serde_json::from_value(compat()).unwrap();
    let _: PackAnnotation = serde_json::from_value(annotation()).unwrap();
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(
        width,
        height,
        image::Luma([7]),
    ));
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

fn b64(bytes: &[u8]) -> String {
    rascal_core::panel::base64(bytes)
}

fn answer(envelope: serde_json::Value, blocks: Vec<ContentBlock>) -> CallToolResult {
    let mut content = vec![ContentBlock::text("t")];
    content.extend(blocks);
    let mut result = CallToolResult::success(content);
    result.structured_content = Some(envelope);
    result.is_error = Some(false);
    result
}

fn good_envelope(frame: &str) -> serde_json::Value {
    serde_json::json!({
        "envelope_version": 1, "status": "ok", "summary": "one",
        "regions": [{ "id": "a", "location": { "kind": "polygon", "frame": frame,
                      "polygon": [[0.1, 0.1], [0.4, 0.1], [0.4, 0.3]] }, "label": "chair", "score": 0.9 }],
        "images": [{ "frame": frame, "parent": frame, "width": 32, "height": 20,
                     "media_type": "image/png", "identity": "" }],
        "provenance": { "tool": "segment", "version": "0.1.0", "duration_ms": 1 }
    })
}

#[test]
fn result_validation_rejects_each_bad_answer_by_name() {
    let tool = PackTool::from_tool(&tool_with(meta(compat(), Some(annotation()))), None).unwrap();
    let frame = Frame::raw(32, 20, Projection::Flat);
    let frames: BTreeMap<String, &Frame> = [(frame.id.clone(), &frame)].into_iter().collect();
    let id = frame.id.clone();
    let block = || ContentBlock::image(b64(&png(32, 20)), "image/png");

    // The good answer passes and decodes.
    let ok = validate(
        &tool.manifest,
        &answer(good_envelope(&id), vec![block()]),
        &frames,
    )
    .unwrap();
    assert_eq!(ok.images.len(), 1);
    assert_eq!(ok.envelope.regions[0].label, "chair");

    let cases: Vec<(&str, CallToolResult)> = vec![
        ("no structuredContent", {
            let mut r = answer(good_envelope(&id), vec![block()]);
            r.structured_content = None;
            r
        }),
        ("does not parse", {
            let mut e = good_envelope(&id);
            e["verdict"] = serde_json::json!("pass");
            answer(e, vec![block()])
        }),
        ("more than 2 regions", {
            let mut e = good_envelope(&id);
            let r = e["regions"][0].clone();
            e["regions"] = serde_json::json!([r.clone(), r.clone(), r]);
            answer(e, vec![block()])
        }),
        ("summary longer than 64 bytes", {
            let mut e = good_envelope(&id);
            e["summary"] = serde_json::json!("x".repeat(65));
            answer(e, vec![block()])
        }),
        (
            "2 image blocks for 1 images",
            answer(good_envelope(&id), vec![block(), block()]),
        ),
        (
            "0 image blocks for 1 images",
            answer(good_envelope(&id), vec![]),
        ),
        ("which the call did not pass", {
            let mut e = good_envelope(&id);
            e["regions"][0]["location"]["frame"] = serde_json::json!("raw:9x9:flat");
            answer(e, vec![block()])
        }),
        ("image 0 is in frame raw:9x9:flat", {
            let mut e = good_envelope(&id);
            e["images"][0]["frame"] = serde_json::json!("raw:9x9:flat");
            answer(e, vec![block()])
        }),
        ("is image/jpeg, not image/png", {
            answer(
                good_envelope(&id),
                vec![ContentBlock::image(b64(&png(32, 20)), "image/jpeg")],
            )
        }),
        ("is 16x20 but images[0] says 32x20", {
            answer(
                good_envelope(&id),
                vec![ContentBlock::image(b64(&png(16, 20)), "image/png")],
            )
        }),
        ("did not decode", {
            answer(
                good_envelope(&id),
                vec![ContentBlock::image(b64(b"not a png at all"), "image/png")],
            )
        }),
        ("is 33x20 but frame", {
            let mut e = good_envelope(&id);
            e["images"][0]["width"] = serde_json::json!(33);
            answer(e, vec![block()])
        }),
        ("isError is true but the status is Ok", {
            let mut r = answer(good_envelope(&id), vec![block()]);
            r.is_error = Some(true);
            r
        }),
        ("a pack names no artifacts", {
            let mut e = good_envelope(&id);
            e["artifacts"] = serde_json::json!({ "x": { "artifact_type": "Image", "id": "x", "identity": "sha256:0000000000000000000000000000000000000000000000000000000000000000" } });
            answer(e, vec![block()])
        }),
        ("region a: ", {
            let mut e = good_envelope(&id);
            e["regions"][0]["location"]["polygon"] = serde_json::json!([[0.1, 0.1], [0.4, 0.1]]);
            answer(e, vec![block()])
        }),
        // (A non-finite score cannot arrive: serde_json refuses NaN and out-of-range numbers
        // before the validator sees them; the check stays as a guard.)
        ("image 0 is image/jpeg, not image/png", {
            let mut e = good_envelope(&id);
            e["images"][0]["media_type"] = serde_json::json!("image/jpeg");
            answer(e, vec![block()])
        }),
    ];
    for (expect, result) in cases {
        let e = validate(&tool.manifest, &result, &frames)
            .unwrap_err()
            .to_string();
        assert!(e.contains(expect), "expected {expect:?} in {e}");
        assert!(e.contains("answer rejected"), "{e}");
    }
}
