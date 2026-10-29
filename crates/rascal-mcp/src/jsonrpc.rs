//! The shapes rascal puts on the wire, shared by the server and the pack client: an envelope
//! and its images as a tool result, and the two error mappings. A change to the wire is a
//! change here and nowhere else.

use rmcp::model::{CallToolResult, ContentBlock, ErrorCode, ErrorData};
use serde_json::Value;

/// The media type of every image block rascal sends.
pub const PNG: &str = "image/png";

/// A tool result from an envelope (as JSON) and the PNG bytes of its `images`, in order: the
/// envelope is the `structuredContent`, each image one `image` block, and the text block is
/// the envelope's summary so a client that shows text only still sees something.
pub fn tool_result(envelope: Value, images: &[Vec<u8>]) -> CallToolResult {
    let summary = envelope
        .get("summary")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let is_error = envelope
        .get("status")
        .and_then(|s| s.as_str())
        .is_some_and(|s| s == "invalid_args");
    let mut content = vec![ContentBlock::text(summary)];
    for bytes in images {
        content.push(ContentBlock::image(rascal_core::panel::base64(bytes), PNG));
    }
    let mut result = CallToolResult::success(content);
    result.structured_content = Some(envelope);
    result.is_error = Some(is_error);
    result
}

/// The image blocks of a result, decoded from base64, in order; a block that is not a PNG
/// image, or whose data does not decode, is an error naming its position.
pub fn image_blocks(result: &CallToolResult) -> Result<Vec<Vec<u8>>, String> {
    let mut out = Vec::new();
    for (i, block) in result.content.iter().enumerate() {
        if let ContentBlock::Image(image) = block {
            if image.mime_type != PNG {
                return Err(format!(
                    "content block {i}: {} is not {PNG}",
                    image.mime_type
                ));
            }
            out.push(
                base64_decode(&image.data)
                    .ok_or_else(|| format!("content block {i}: base64 does not decode"))?,
            );
        }
    }
    Ok(out)
}

/// An operational failure as a JSON-RPC error: the message, and the error class in `data`.
pub fn operational(error: &rascal_core::Error) -> ErrorData {
    ErrorData::new(
        ErrorCode::INTERNAL_ERROR,
        error.to_string(),
        Some(serde_json::json!({ "class": class_of(error) })),
    )
}

/// A caller's mistake outside any envelope (an unknown tool, malformed arguments): invalid
/// params.
pub fn invalid(message: impl Into<String>) -> ErrorData {
    ErrorData::new(
        ErrorCode::INVALID_PARAMS,
        message.into(),
        Some(serde_json::json!({ "class": "usage" })),
    )
}

/// The error class a JSON-RPC error carries.
pub fn class_of(error: &rascal_core::Error) -> &'static str {
    use rascal_core::Error;
    match error {
        Error::Usage { .. } | Error::Recipe { .. } => "usage",
        Error::Io { .. } => "io",
        Error::Image { .. } => "image",
        Error::Json { .. } | Error::Toml { .. } => "format",
        Error::MaskInvariant { .. } => "mask_invariant",
        Error::Op { .. } => "op",
        _ => "operational",
    }
}

/// Standard base64, padded, decoded; `None` on a bad character or length.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes().filter(|c| !c.is_ascii_whitespace()) {
        if c == b'=' {
            break;
        }
        let v = TABLE.iter().position(|t| *t == c)? as u32;
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
