//! The independent minimal MCP client (SPEC.md "The MCP server and client", ADR 0015): written
//! against the protocol document for revision `2025-11-25` and nothing else. Newline-delimited
//! JSON-RPC 2.0 over a child process's stdio: `initialize`, `notifications/initialized`,
//! `tools/list` to exhaustion through the cursor, `tools/call`, and image content blocks read
//! back as bytes with a base64 decoder of its own. It imports nothing from rascal or from the
//! SDK the server is built on, so what it proves about `rascal serve` is what any client sees.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

/// The protocol revision the client offers and requires.
pub const PROTOCOL: &str = "2025-11-25";

/// A connected server.
pub struct Client {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
    writer: std::process::ChildStdin,
    next_id: u64,
    /// The server's `initialize` result.
    pub server: serde_json::Value,
}

/// A tool as `tools/list` describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A `tools/call` result: the structured content, the text blocks, the image blocks decoded,
/// and the error flag.
#[derive(Debug, Clone, PartialEq)]
pub struct CallResult {
    pub structured: Option<serde_json::Value>,
    pub texts: Vec<String>,
    pub images: Vec<(String, Vec<u8>)>,
    pub is_error: bool,
}

/// A failure: the server's JSON-RPC error (code, message, data) or a transport problem.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    Rpc {
        code: i64,
        message: String,
        data: Option<serde_json::Value>,
    },
    Transport(String),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Rpc {
                code,
                message,
                data,
            } => write!(
                f,
                "JSON-RPC error {code}: {message} {}",
                data.clone().unwrap_or_default()
            ),
            Failure::Transport(t) => write!(f, "transport: {t}"),
        }
    }
}

impl Client {
    /// Starts the server command and completes the handshake; stderr is inherited so a
    /// server's tracing shows in the test output.
    pub fn start(command: &mut Command) -> Result<Client, Failure> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| Failure::Transport(format!("spawn: {e}")))?;
        let writer = child.stdin.take().expect("piped stdin");
        let reader = BufReader::new(child.stdout.take().expect("piped stdout"));
        let mut client = Client {
            child,
            reader,
            writer,
            next_id: 1,
            server: serde_json::Value::Null,
        };
        let server = client.request(
            "initialize",
            serde_json::json!({
                "protocolVersion": PROTOCOL,
                "capabilities": {},
                "clientInfo": { "name": "rascal-testkit-independent-client", "version": "0" }
            }),
        )?;
        let negotiated = server["protocolVersion"].as_str().unwrap_or("");
        if negotiated != PROTOCOL {
            return Err(Failure::Transport(format!(
                "the server answered protocol {negotiated}, not {PROTOCOL}"
            )));
        }
        if server["capabilities"]["tools"].is_null() {
            return Err(Failure::Transport(
                "the server advertises no tools".to_string(),
            ));
        }
        client.notify("notifications/initialized", serde_json::json!({}))?;
        client.server = server;
        Ok(client)
    }

    /// Every tool, following `nextCursor` until the server stops giving one.
    pub fn list_tools(&mut self) -> Result<Vec<Tool>, Failure> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        let mut pages = 0;
        loop {
            let params = match &cursor {
                Some(c) => serde_json::json!({ "cursor": c }),
                None => serde_json::json!({}),
            };
            let result = self.request("tools/list", params)?;
            for t in result["tools"].as_array().cloned().unwrap_or_default() {
                tools.push(Tool {
                    name: t["name"].as_str().unwrap_or("").to_string(),
                    description: t["description"].as_str().unwrap_or("").to_string(),
                    input_schema: t["inputSchema"].clone(),
                });
            }
            pages += 1;
            match result["nextCursor"].as_str() {
                Some(next) if pages < 10_000 => cursor = Some(next.to_string()),
                _ => return Ok(tools),
            }
        }
    }

    /// Calls a tool with JSON arguments.
    pub fn call(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallResult, Failure> {
        let result = self.request(
            "tools/call",
            serde_json::json!({ "name": name, "arguments": arguments }),
        )?;
        let mut texts = Vec::new();
        let mut images = Vec::new();
        for block in result["content"].as_array().cloned().unwrap_or_default() {
            match block["type"].as_str() {
                Some("text") => texts.push(block["text"].as_str().unwrap_or("").to_string()),
                Some("image") => {
                    let data = block["data"].as_str().unwrap_or("");
                    let bytes = base64_decode(data).ok_or_else(|| {
                        Failure::Transport("an image block's base64 does not decode".to_string())
                    })?;
                    images.push((block["mimeType"].as_str().unwrap_or("").to_string(), bytes));
                }
                _ => {}
            }
        }
        Ok(CallResult {
            structured: result.get("structuredContent").cloned(),
            texts,
            images,
            is_error: result["isError"].as_bool().unwrap_or(false),
        })
    }

    /// A request: one JSON line out, the matching response in (notifications and other
    /// traffic in between are skipped).
    pub fn request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, Failure> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(
            &serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
        )?;
        loop {
            let message = self.receive()?;
            if message.get("id").and_then(|v| v.as_u64()) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(Failure::Rpc {
                    code: error["code"].as_i64().unwrap_or(0),
                    message: error["message"].as_str().unwrap_or("").to_string(),
                    data: error.get("data").cloned(),
                });
            }
            return Ok(message["result"].clone());
        }
    }

    /// A notification: no id, no reply.
    pub fn notify(&mut self, method: &str, params: serde_json::Value) -> Result<(), Failure> {
        self.send(&serde_json::json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    fn send(&mut self, message: &serde_json::Value) -> Result<(), Failure> {
        let mut line = message.to_string();
        line.push('\n');
        self.writer
            .write_all(line.as_bytes())
            .and_then(|()| self.writer.flush())
            .map_err(|e| Failure::Transport(format!("write: {e}")))
    }

    fn receive(&mut self) -> Result<serde_json::Value, Failure> {
        let mut line = String::new();
        loop {
            line.clear();
            let n = self
                .reader
                .read_line(&mut line)
                .map_err(|e| Failure::Transport(format!("read: {e}")))?;
            if n == 0 {
                return Err(Failure::Transport(
                    "the server closed its stdout".to_string(),
                ));
            }
            if line.trim().is_empty() {
                continue;
            }
            return serde_json::from_str(&line)
                .map_err(|e| Failure::Transport(format!("not JSON: {e}: {}", line.trim())));
        }
    }

    /// Closes stdin and waits for the server to exit.
    pub fn close(mut self) -> std::io::Result<std::process::ExitStatus> {
        drop(self.writer);
        self.child.wait()
    }
}

/// Standard base64, padded, decoded; `None` on a bad character.
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
