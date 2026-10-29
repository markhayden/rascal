# ADR 0015: the MCP transport

Decided 2026-10-29 (M3 task 3.1). Status: accepted.

## Context

rascal speaks MCP on two sides: `rascal serve` is a server over stdio that any MCP client can
drive, and the pack client starts configured tool packs and calls them (SPEC.md "The MCP server
and client"). The spec fixes protocol revision `2025-11-25` and asks for pagination of
`tools/list`, cancellation, `structuredContent` on results and image content blocks. The plan
left the choice between the official Rust SDK and an in-house JSON-RPC implementation to this
ADR, on one test: the official crate is taken if it supports that revision with those four
features; otherwise the in-house path.

## Decision

- **The official SDK, `rmcp`, pinned to `=3.8.0`.** At that version `ProtocolVersion` carries
  `V_2025_11_25` (and `2026-07-28`, the newest); `tools/list` takes a cursor and returns
  `next_cursor`; every request handler receives a `CancellationToken` cancelled by the client's
  `notifications/cancelled`; `CallToolResult` has `structured_content`; content blocks include
  `image` with base64 data and a media type. The crate negotiates the version on connect and
  gates version-specific behaviour on the negotiated one.
- **rascal answers `2025-11-25` only.** `rascal_mcp::transport::supported_versions()` is the
  one-element list the server advertises and the client offers; a peer that cannot meet it is
  refused at `initialize`. Moving to a newer revision is a change to that list and to this ADR.
- **Features:** `server`, `client`, `transport-io` (stdio for `serve`), `transport-child-process`
  (packs) and `schemars`; `default-features = false`, so the derive macros and the HTTP
  transports are not compiled. The dependency lives in `rascal-mcp` and, for the fake server, in
  `rascal-testkit`; `rascal-core` knows nothing of it.
- **The shared shapes stay rascal's.** `rascal_mcp::jsonrpc` turns an envelope and its images
  into a `CallToolResult` (the envelope as `structuredContent`, one image block per entry of
  `images`, in order) and maps errors: validation stays in the envelope as `invalid_args`, an
  operational failure is a JSON-RPC error whose `data.class` names rascal's error class. Both
  sides use these functions, so a change to the wire shape is one place.
- **The independent client is written against the protocol document, not against `rmcp`.**
  `rascal_testkit::mcp_client` is newline-delimited JSON-RPC 2.0 over a child process's stdio,
  with its own base64 decoder and no import from `rascal-core`, `rascal-mcp` or `rmcp`; a test
  reads its own source to prove that. It is proven against a fake server built on `rmcp`, so the
  SDK's server side is shown to interoperate with a client that shares none of its code.

## Alternatives

- **An in-house JSON-RPC implementation.** Smaller dependency tree and full control of the
  wire, at the cost of writing and maintaining initialize, pagination, cancellation and the
  content model by hand, and of drifting from the revision's details. Rejected because the test
  the plan set is met by the official crate; the independent client is the small in-house piece
  that keeps the server honest.
- **A newer revision (`2026-07-28`).** It drops the `initialize` handshake. The spec names
  `2025-11-25`, the Python pack (M5) targets the same, and nothing rascal needs is missing from
  it. Revisit when the pack moves.

## Consequences

- `cargo deny` and the crate graph gain `rmcp` and its tree (`tokio`, `tokio-util`,
  `process-wrap`, `schemars 1`, `base64 0.23`); the licence check passes (Apache-2.0).
- `rascal serve` (3.2) is a `ServerHandler` with `list_tools` and `call_tool` written by hand
  over the op registry, not the derive macros, because the tools are the registry's manifests
  at run time.
- The pack client (3.3) uses `rmcp`'s child-process transport and client service; the
  environment allowlist, the `_harness` object and result validation are rascal's.
