//! Task 3.1: the independent client talks to the fake server: initialise at the pinned
//! revision, list through the cursor, call and read an image block, see an invalid_args
//! envelope and a JSON-RPC error with its class. And the client's source imports nothing from
//! rascal or the SDK.
#![allow(clippy::unwrap_used)]

use std::process::Command;

use rascal_testkit::mcp_client::{Client, Failure, PROTOCOL};

fn server() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fake-mcp-server"))
}

#[test]
fn the_client_initialises_lists_calls_and_reads_an_image_block() {
    let mut client = Client::start(&mut server()).unwrap();
    assert_eq!(client.server["protocolVersion"], PROTOCOL);
    assert_eq!(
        client.server["serverInfo"]["name"],
        "rascal-fake-mcp-server"
    );
    let tools = client.list_tools().unwrap();
    assert_eq!(
        tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["echo", "fail", "invalid", "slow"],
        "four pages of one tool each, followed through the cursor"
    );
    assert_eq!(tools[0].input_schema["type"], "object");
    let result = client
        .call("echo", serde_json::json!({ "text": "hi" }))
        .unwrap();
    assert!(!result.is_error);
    assert_eq!(
        result.structured.as_ref().unwrap()["arguments"]["text"],
        "hi"
    );
    assert_eq!(result.structured.as_ref().unwrap()["status"], "ok");
    assert_eq!(result.texts, vec!["echo {\"text\":\"hi\"}".to_string()]);
    assert_eq!(result.images.len(), 1);
    let (mime, bytes) = &result.images[0];
    assert_eq!(mime, "image/png");
    assert!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "the block decodes to a PNG"
    );
    let decoded = image::load_from_memory(bytes).unwrap();
    assert_eq!(
        (decoded.width(), decoded.height()),
        (2, 2),
        "the fake's 2x2 image"
    );
    // An invalid_args envelope is a result with isError, not a JSON-RPC error.
    let refused = client.call("invalid", serde_json::json!({})).unwrap();
    assert!(refused.is_error);
    assert_eq!(refused.structured.unwrap()["status"], "invalid_args");
    // An operational failure is a JSON-RPC error carrying its class.
    match client.call("fail", serde_json::json!({})) {
        Err(Failure::Rpc {
            code,
            message,
            data,
        }) => {
            assert_eq!(code, -32603);
            assert!(message.contains("failed as asked"));
            assert_eq!(data.unwrap()["class"], "op");
        }
        other => panic!("{other:?}"),
    }
    // An unknown tool is invalid params.
    match client.call("nope", serde_json::json!({})) {
        Err(Failure::Rpc { code, .. }) => assert_eq!(code, -32602),
        other => panic!("{other:?}"),
    }
    let status = client.close().unwrap();
    assert!(
        status.success(),
        "the server exits cleanly when stdin closes: {status}"
    );
}

#[test]
fn the_client_imports_nothing_from_rascal_or_the_sdk() {
    let source =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/mcp_client.rs")).unwrap();
    for forbidden in ["rascal_core", "rascal_mcp", "rmcp", "crate::"] {
        assert!(
            !source.contains(forbidden),
            "the independent client mentions {forbidden}"
        );
    }
    assert!(source.contains("2025-11-25"));
}
