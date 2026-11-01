//! Task 3.1: the pinned revision, and the shared wire shapes round-tripping through the SDK's
//! serialisation: an envelope as structured content with its images as blocks, the decode
//! back, and the two error mappings.
#![allow(clippy::unwrap_used)]

use rascal_mcp::jsonrpc::{
    base64_decode, class_of, image_blocks, invalid, operational, tool_result,
};
use rascal_mcp::transport::{PROTOCOL, accepts, rmcp, supported_versions};
use rmcp::model::{CallToolResult, ContentBlock, ErrorCode, ProtocolVersion};

#[test]
fn the_revision_is_pinned_and_known_to_the_sdk() {
    assert_eq!(PROTOCOL, "2025-11-25");
    let versions = supported_versions();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0], ProtocolVersion::V_2025_11_25);
    assert!(ProtocolVersion::KNOWN_VERSIONS.contains(&ProtocolVersion::V_2025_11_25));
    assert!(accepts(&ProtocolVersion::V_2025_11_25));
    assert!(!accepts(&ProtocolVersion::V_2025_06_18));
    assert!(
        !accepts(&ProtocolVersion::LATEST),
        "the newest revision is not the pinned one"
    );
}

#[test]
fn an_envelope_and_its_images_become_a_result_and_come_back() {
    let envelope = serde_json::json!({
        "envelope_version": 1, "status": "ok", "summary": "two images",
        "images": [{"frame": "raw:4x3:flat"}, {"frame": "raw:4x3:flat"}],
    });
    let a = png(4, 3, 10);
    let b = png(4, 3, 200);
    let result = tool_result(envelope.clone(), &[a.clone(), b.clone()]);
    assert_eq!(result.structured_content, Some(envelope));
    assert_eq!(result.is_error, Some(false));
    assert!(matches!(&result.content[0], ContentBlock::Text(t) if t.text == "two images"));
    assert_eq!(result.content.len(), 3);
    // Through the wire: serde round trip as the SDK would send it.
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"structuredContent\"") && json.contains("\"mimeType\":\"image/png\""));
    let back: CallToolResult = serde_json::from_str(&json).unwrap();
    let images = image_blocks(&back).unwrap();
    assert_eq!(
        images,
        vec![a, b],
        "the blocks decode to the bytes, in order"
    );
    // invalid_args stays in the envelope and marks the result as an error.
    let refused = tool_result(
        serde_json::json!({"status": "invalid_args", "summary": "no"}),
        &[],
    );
    assert_eq!(refused.is_error, Some(true));
    assert_eq!(image_blocks(&refused).unwrap(), Vec::<Vec<u8>>::new());
    // A non-PNG block is refused by position.
    let mut odd = result.clone();
    odd.content.push(ContentBlock::image("AAAA", "image/jpeg"));
    assert!(image_blocks(&odd).unwrap_err().contains("content block 3"));
}

#[test]
fn errors_map_to_json_rpc_with_their_class() {
    let usage = rascal_core::Error::usage("bad flag");
    let e = operational(&usage);
    assert_eq!(e.code, ErrorCode::INTERNAL_ERROR);
    assert_eq!(e.data.unwrap()["class"], "usage");
    assert_eq!(class_of(&rascal_core::Error::op("fan", "x")), "op");
    let io = rascal_core::Error::io(std::path::Path::new("/x"), std::io::Error::other("gone"));
    assert_eq!(class_of(&io), "io");
    let i = invalid("unknown tool");
    assert_eq!(i.code, ErrorCode::INVALID_PARAMS);
    assert_eq!(i.message, "unknown tool");
}

#[test]
fn base64_decodes_what_the_encoder_wrote() {
    for len in 0..40 {
        let bytes: Vec<u8> = (0..len).map(|i| (i * 37 % 251) as u8).collect();
        let text = rascal_core::panel::base64(&bytes);
        assert_eq!(base64_decode(&text).unwrap(), bytes, "len {len}");
    }
    assert!(base64_decode("@@@@").is_none());
}

fn png(w: u32, h: u32, v: u8) -> Vec<u8> {
    let image =
        image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(w, h, image::Luma([v])));
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}
