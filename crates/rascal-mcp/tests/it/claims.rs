//! Task 12.7 (table-manners v0.3, 4.4 and 5): a correlation id's claim is released when its
//! request ends in a JSON-RPC error with no run, so the corrected request retries under the same
//! id; it is kept once a run is allocated, whatever the outcome, and kept with no run when the
//! request is cancelled before allocation. Task 12.6: the call's metadata reaches the action
//! (`CallMeta`), the named estimate included. The server runs in process over a pipe with an
//! actions double, driven by raw JSON-RPC lines written here (no SDK client).
#![allow(clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use rascal_core::workspace::Cancel;
use rascal_mcp::server::{
    ActionResult, Actions, CallError, CallMeta, McpResource, McpResourceTemplate, McpTool,
    Progress, ResourceBody, Server,
};
use rascal_mcp::transport::rmcp;
use rmcp::ServiceExt;
use serde_json::{Map, Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// The actions double: one exclusive action, `act`, whose arguments choose what it does:
/// `{"fail": "before"}` returns `Err` before allocating a run, `{"panic": "before"}` panics
/// there, `{"fail": "after"}` allocates and then returns `Err`, `{"hold": true}` allocates and
/// waits for the gate, anything else allocates and completes, its result echoing the estimate
/// the call named. `entered` counts the calls the server has begun handling: it asks
/// `exclusive` once per call, synchronously and just before it claims the call's correlation id,
/// so a test can wait until a request it sent holds its claim.
#[derive(Default)]
struct Double {
    allocated: AtomicUsize,
    gate: Arc<(Mutex<bool>, Condvar)>,
    entered: AtomicUsize,
    entering: tokio::sync::Notify,
}

impl Double {
    /// Waits until the server has begun handling `n` calls.
    async fn until_entered(&self, n: usize) {
        while self.entered.load(Ordering::SeqCst) < n {
            self.entering.notified().await;
        }
    }

    fn open_gate(&self) {
        let (open, wake) = &*self.gate;
        *open.lock().unwrap() = true;
        wake.notify_all();
    }
}

impl Actions for Double {
    fn tools(&self) -> Vec<McpTool> {
        let mut schema = Map::new();
        schema.insert("type".into(), json!("object"));
        vec![McpTool::new("act", "the double", Arc::new(schema))]
    }

    fn handles(&self, name: &str) -> bool {
        name == "act"
    }

    fn exclusive(&self, _name: &str) -> bool {
        self.entered.fetch_add(1, Ordering::SeqCst);
        // A stored permit: the waiter may not be waiting yet.
        self.entering.notify_one();
        true
    }

    fn call(
        &self,
        _name: &str,
        args: &Map<String, Value>,
        meta: &CallMeta,
        progress: &(dyn Fn(Progress) + Send + Sync),
        _cancel: Arc<Cancel>,
    ) -> Result<ActionResult, CallError> {
        match args
            .get("fail")
            .or(args.get("panic"))
            .and_then(|v| v.as_str())
        {
            Some("before") if args.contains_key("panic") => panic!("the double panics"),
            Some("before") => return Err("the double refuses before a run".into()),
            _ => {}
        }
        let n = self.allocated.fetch_add(1, Ordering::SeqCst) + 1;
        let reference = json!({
            "id": format!("run-{n}-1"),
            "resource": format!("double://run/run-{n}-1/run.json"),
            "session_id": null,
            "client_run_id": meta.client_run_id,
        });
        progress(Progress::Run(reference.clone()));
        progress(Progress::Line("allocated".to_string()));
        if args.get("fail").and_then(|v| v.as_str()) == Some("after") {
            return Err("the double fails after its run was allocated".into());
        }
        if args.get("hold") == Some(&json!(true)) {
            let (open, wake) = &*self.gate;
            let mut open = open.lock().unwrap();
            while !*open {
                open = wake.wait(open).unwrap();
            }
        }
        Ok(ActionResult {
            structured: json!({ "status": "completed", "run": reference,
                                "estimate": meta.estimate }),
            is_error: false,
            images: Vec::new(),
        })
    }

    fn resources(&self) -> Vec<McpResource> {
        Vec::new()
    }

    fn templates(&self) -> Vec<McpResourceTemplate> {
        Vec::new()
    }

    fn read(&self, _uri: &str) -> Result<Option<ResourceBody>, CallError> {
        Ok(None)
    }
}

/// The client end of the pipe: JSON-RPC, one message a line.
struct Wire {
    lines: tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
    writer: tokio::io::WriteHalf<tokio::io::DuplexStream>,
    next: u64,
}

impl Wire {
    async fn send(&mut self, message: Value) {
        let mut line = message.to_string();
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await.unwrap();
        self.writer.flush().await.unwrap();
    }

    async fn begin(&mut self, method: &str, params: Value) -> u64 {
        self.next += 1;
        let id = self.next;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await;
        id
    }

    /// Reads until the response to `id`, skipping notifications and other responses.
    async fn until(&mut self, id: u64) -> Value {
        loop {
            let line = self.lines.next_line().await.unwrap().unwrap();
            let message: Value = serde_json::from_str(&line).unwrap();
            if message.get("id").and_then(|v| v.as_u64()) == Some(id) {
                return message;
            }
        }
    }

    /// Reads until a progress notification for `token`.
    async fn progress(&mut self, token: &str) {
        loop {
            let line = self.lines.next_line().await.unwrap().unwrap();
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["method"] == "notifications/progress"
                && message["params"]["progressToken"] == token
            {
                return;
            }
        }
    }

    async fn act(&mut self, arguments: Value, client_run_id: Option<&str>, token: &str) -> u64 {
        let mut meta = json!({ "progressToken": token });
        if let Some(id) = client_run_id {
            meta["table-manners/client_run_id"] = json!(id);
        }
        self.begin(
            "tools/call",
            json!({ "name": "act", "arguments": arguments, "_meta": meta }),
        )
        .await
    }
}

/// A server over the double, and the client end of its pipe, initialized.
async fn serve(double: Arc<Double>) -> Wire {
    let server = Server::new(double, 1);
    let (client, server_end) = tokio::io::duplex(1 << 20);
    let (read, write) = tokio::io::split(server_end);
    tokio::spawn(async move {
        let transport = rmcp::transport::async_rw::AsyncRwTransport::new_server(read, write);
        if let Ok(service) = server.serve(transport).await {
            let _ = service.waiting().await;
        }
    });
    let (read, writer) = tokio::io::split(client);
    let mut wire = Wire {
        lines: BufReader::new(read).lines(),
        writer,
        next: 0,
    };
    let init = wire
        .begin(
            "initialize",
            json!({ "protocolVersion": "2025-11-25", "capabilities": {},
                    "clientInfo": { "name": "claims-test", "version": "0" } }),
        )
        .await;
    let answer = wire.until(init).await;
    assert!(answer.get("result").is_some(), "{answer:#}");
    wire.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .await;
    wire
}

/// The data of a `client_run_id_reused` refusal.
fn reused(message: &Value) -> Value {
    assert_eq!(message["error"]["code"], -32602, "{message:#}");
    assert_eq!(
        message["error"]["data"]["reason"], "client_run_id_reused",
        "{message:#}"
    );
    message["error"]["data"].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_refused_before_a_run_releases_its_id() {
    let double = Arc::new(Double::default());
    let mut wire = serve(double.clone()).await;
    let refused = wire.act(json!({ "fail": "before" }), Some("X"), "t1").await;
    let refused = wire.until(refused).await;
    // A plain invalid params error: no refusal data (no `reason`), since no contract refusal
    // was named; the data carries only rascal's error class.
    assert_eq!(refused["error"]["code"], -32602, "{refused:#}");
    assert_eq!(
        refused["error"]["data"],
        json!({ "class": "usage" }),
        "{refused:#}"
    );
    assert_eq!(double.allocated.load(Ordering::SeqCst), 0);
    // The corrected request is accepted under the same id, and its run carries it.
    let retry = wire.act(json!({}), Some("X"), "t2").await;
    let retry = wire.until(retry).await;
    assert_eq!(
        retry["result"]["structuredContent"]["run"]["client_run_id"], "X",
        "{retry:#}"
    );
    // Once a run carries it, the id is used: a reuse names that run.
    let again = wire.act(json!({}), Some("X"), "t3").await;
    let again = wire.until(again).await;
    assert_eq!(
        reused(&again)["run"],
        retry["result"]["structuredContent"]["run"]
    );
    assert_eq!(double.allocated.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_that_panics_before_a_run_releases_its_id() {
    let double = Arc::new(Double::default());
    let mut wire = serve(double.clone()).await;
    let panicked = wire
        .act(json!({ "panic": "before" }), Some("P"), "t1")
        .await;
    let panicked = wire.until(panicked).await;
    assert!(panicked.get("error").is_some(), "{panicked:#}");
    let retry = wire.act(json!({}), Some("P"), "t2").await;
    let retry = wire.until(retry).await;
    assert_eq!(
        retry["result"]["structuredContent"]["run"]["client_run_id"], "P",
        "{retry:#}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_that_fails_after_allocation_keeps_its_id_with_its_run() {
    let double = Arc::new(Double::default());
    let mut wire = serve(double.clone()).await;
    let failed = wire.act(json!({ "fail": "after" }), Some("A"), "t1").await;
    let failed = wire.until(failed).await;
    assert_eq!(failed["error"]["code"], -32602, "{failed:#}");
    // The run reference the double sent just before failing reached the claim.
    let retry = wire.act(json!({}), Some("A"), "t2").await;
    let retry = wire.until(retry).await;
    let data = reused(&retry);
    assert_eq!(data["run"]["id"], "run-1-1", "{retry:#}");
    assert_eq!(data["run"]["client_run_id"], "A");
    assert_eq!(double.allocated.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_cancelled_while_queued_keeps_its_id_with_no_run() {
    let double = Arc::new(Double::default());
    let mut wire = serve(double.clone()).await;
    let holder = wire.act(json!({ "hold": true }), None, "holder").await;
    wire.progress("holder").await;
    let queued = wire.act(json!({}), Some("Y"), "queued").await;
    // The server handles each request in its own task, so a later request can be handled first:
    // wait until the queued request holds its claim before cancelling it and retrying, or the
    // retry may claim the id first and wait behind the holder.
    double.until_entered(2).await;
    wire.send(
        json!({ "jsonrpc": "2.0", "method": "notifications/cancelled",
                      "params": { "requestId": queued, "reason": "test" } }),
    )
    .await;
    let retry = wire.act(json!({}), Some("Y"), "retry").await;
    let retry = wire.until(retry).await;
    assert_eq!(reused(&retry)["run"], Value::Null, "{retry:#}");
    double.open_gate();
    let done = wire.until(holder).await;
    assert_eq!(done["result"]["structuredContent"]["status"], "completed");
    // With the permit free, the cancelled request still never ran and its id stays claimed.
    let later = wire.act(json!({}), Some("Y"), "later").await;
    let later = wire.until(later).await;
    assert_eq!(reused(&later)["run"], Value::Null, "{later:#}");
    assert_eq!(double.allocated.load(Ordering::SeqCst), 1);
}

/// Task 12.6 (table-manners 4.5): `_meta["table-manners/estimate"]` reaches the action as sent,
/// whatever its form (the action judges it, since a run exists by then); without it, none.
#[tokio::test(flavor = "multi_thread")]
async fn the_named_estimate_reaches_the_action() {
    let double = Arc::new(Double::default());
    let mut wire = serve(double.clone()).await;
    for named in [json!("sha256:abc"), json!(7)] {
        let call = wire
            .begin(
                "tools/call",
                json!({ "name": "act", "arguments": {},
                        "_meta": { "table-manners/estimate": named.clone() } }),
            )
            .await;
        let answer = wire.until(call).await;
        assert_eq!(
            answer["result"]["structuredContent"]["estimate"], named,
            "{answer:#}"
        );
    }
    let plain = wire.act(json!({}), None, "plain").await;
    let plain = wire.until(plain).await;
    assert_eq!(
        plain["result"]["structuredContent"]["estimate"],
        Value::Null
    );
}
