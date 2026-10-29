//! The fake MCP server the independent client is proven against (task 3.1): built on the SDK
//! `rascal serve` uses, over stdio, revision `2025-11-25` only. Two tools listed one per page
//! (so the cursor is exercised): `echo` answers with its arguments as structured content and a
//! 2x2 PNG image block; `fail` answers with a JSON-RPC error carrying a class; `invalid`
//! answers an `invalid_args` envelope with `isError`. A `slow` tool sleeps until cancelled.

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

#[derive(Clone)]
struct Fake;

const NAMES: [&str; 4] = ["echo", "fail", "invalid", "slow"];

fn tool(name: &'static str, description: &'static str) -> Tool {
    let schema: serde_json::Map<String, serde_json::Value> =
        serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": { "text": { "type": "string" } },
            "additionalProperties": false
        }))
        .expect("a schema object");
    Tool::new(name, description, Arc::new(schema))
}

/// A 2x2 grey PNG.
fn png() -> Vec<u8> {
    let image = image::DynamicImage::ImageLuma8(image::GrayImage::from_fn(2, 2, |x, y| {
        image::Luma([16 * (1 + x + 2 * y) as u8])
    }));
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encoding a 2x2 PNG in memory");
    bytes
}

fn base64(bytes: &[u8]) -> String {
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

impl ServerHandler for Fake {
    fn get_info(&self) -> InitializeResult {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
            .with_server_info(Implementation::new("rascal-fake-mcp-server", "0"))
            .with_instructions("a fake for the independent client")
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(&[ProtocolVersion::V_2025_11_25])
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        // One tool per page; the cursor is the index of the next one.
        let start: usize = request
            .and_then(|r| r.cursor)
            .map(|c| {
                c.parse()
                    .map_err(|_| ErrorData::invalid_params("bad cursor", None))
            })
            .transpose()?
            .unwrap_or(0);
        let descriptions = [
            "Echoes its arguments as structured content with one image block.",
            "Fails with a JSON-RPC error carrying a class.",
            "Answers an invalid_args envelope.",
            "Sleeps until cancelled.",
        ];
        let tools = match NAMES.get(start) {
            Some(name) => vec![tool(name, descriptions[start])],
            None => Vec::new(),
        };
        let next_cursor = (start + 1 < NAMES.len()).then(|| (start + 1).to_string());
        Ok(ListToolsResult {
            tools,
            next_cursor,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = serde_json::Value::Object(request.arguments.clone().unwrap_or_default());
        let result = match request.name.as_ref() {
            "echo" => {
                let mut result = CallToolResult::success(vec![
                    ContentBlock::text(format!("echo {args}")),
                    ContentBlock::image(base64(&png()), "image/png"),
                ]);
                result.structured_content = Some(serde_json::json!({
                    "status": "ok", "summary": "echoed", "arguments": args,
                    "images": [{ "frame": "raw:2x2:flat" }]
                }));
                result.is_error = Some(false);
                result
            }
            "invalid" => {
                let mut result = CallToolResult::success(vec![ContentBlock::text(
                    "invalid arguments: text is required",
                )]);
                result.structured_content = Some(serde_json::json!({
                    "status": "invalid_args", "summary": "invalid arguments: text is required"
                }));
                result.is_error = Some(true);
                result
            }
            "fail" => {
                return Err(ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "the fake failed as asked",
                    Some(serde_json::json!({ "class": "op" })),
                ));
            }
            "slow" => {
                context.ct.cancelled().await;
                return Err(ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "cancelled",
                    Some(serde_json::json!({ "class": "cancelled" })),
                ));
            }
            other => {
                return Err(ErrorData::invalid_params(
                    format!("no tool {other}"),
                    Some(serde_json::json!({ "class": "usage" })),
                ));
            }
        };
        Ok(result.into())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let service = match Fake.serve(stdio()).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("fake-mcp-server: {e}");
            std::process::exit(3);
        }
    };
    let _ = service.waiting().await;
}
