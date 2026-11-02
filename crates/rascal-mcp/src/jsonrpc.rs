//! The pieces of the wire the server and the pack client share: the image media type, the
//! invalid-params error, and a base64 decoder.

use rmcp::model::{ErrorCode, ErrorData};

/// The media type of every image block rascal sends.
pub const PNG: &str = "image/png";

/// A caller's mistake before any run (an unknown tool, malformed arguments): invalid params.
pub fn invalid(message: impl Into<String>) -> ErrorData {
    ErrorData::new(
        ErrorCode::INVALID_PARAMS,
        message.into(),
        Some(serde_json::json!({ "class": "usage" })),
    )
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
