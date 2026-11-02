//! Task 3.1: the pinned revision, the invalid-params error and the shared base64 decoder.
#![allow(clippy::unwrap_used)]

use rascal_mcp::jsonrpc::{base64_decode, invalid};
use rascal_mcp::transport::{PROTOCOL, accepts, rmcp, supported_versions};
use rmcp::model::{ErrorCode, ProtocolVersion};

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
fn a_callers_mistake_is_invalid_params_with_the_usage_class() {
    let i = invalid("unknown tool");
    assert_eq!(i.data.unwrap()["class"], "usage");
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

#[test]
fn the_server_knows_no_op() {
    // Task 10.6: the server serves the binary's actions and nothing else; no op, registry,
    // pack, workspace or envelope of rascal's own is named in it.
    let source = include_str!("../../src/server.rs");
    for word in [
        "rascal_core::op",
        "OpRegistry",
        "Workspace",
        "registry",
        "workspace_",
        "Pack",
        "envelope",
    ] {
        assert!(!source.contains(word), "server.rs names {word}");
    }
}
