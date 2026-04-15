# server/ — MCP and HTTP API Interfaces

## Role in the Architecture

The `server` module provides the two external interfaces through which clients interact with Clear Memory: an MCP (Model Context Protocol) server over stdio using JSON-RPC 2.0, and an HTTP/JSON API server built with axum. Both interfaces expose the same 9 operations (recall, expand, reflect, status, retain, import, forget, streams, tags) defined in the CLAUDE.md spec.

In the overall architecture, the server layer sits at the top: it receives requests from external tools (Claude Code, Copilot CLI, ClearPathAI, curl, etc.), delegates to the `engine::Engine` for business logic, and returns formatted responses. The MCP server is the primary integration point for AI coding assistants, while the HTTP server supports programmatic access, health checks, and dashboard integrations.

## File-by-File Descriptions

### mod.rs

Module declaration only. Re-exports: `handlers`, `http`, `mcp`.

### handlers.rs

Shared request/response types used by both the MCP and HTTP interfaces. All types derive `Serialize` and/or `Deserialize` for JSON compatibility.

**Request types:**

- `RecallRequest` — `query: String`, `stream_id: Option<String>`, `tags: Option<Vec<String>>`, `include_archive: Option<bool>`
- `ExpandRequest` — `memory_id: String`
- `RetainRequest` — `content: String`, `tags: Option<Vec<String>>`, `classification: Option<String>`, `stream_id: Option<String>`
- `ForgetRequest` — `memory_id: String`, `reason: Option<String>`
- `ReflectRequest` — `query: Option<String>`, `stream_id: Option<String>`
- `StreamsRequest` — `action: String` (list/create/describe), `name: Option<String>`, `description: Option<String>`, `tags: Option<Vec<String>>`
- `TagsRequest` — `action: String` (list/add/remove), `tag_type: Option<String>`, `tag_value: Option<String>`, `memory_id: Option<String>`

**Response types:**

- `RecallResponse` — `results: Vec<RecallResult>`, `query: String`, `count: usize`
- `RecallResult` — `memory_id`, `summary: Option<String>`, `score: f64`, `created_at: String`
- `ExpandResponse` — `memory_id`, `content`, `source_format`, `created_at`
- `RetainResponse` — `memory_id`, `content_hash`
- `ForgetResponse` — `memory_id`, `status`
- `ReflectResponse` — `synthesis: String`, `source_count: usize`
- `StatusResponse` — `status`, `tier`, `memory_count: i64`, `corpus_size_bytes: u64`, `uptime_secs: u64`
- `StreamsResponse` — `streams: Vec<StreamInfo>`
- `StreamInfo` — `id`, `name`, `description: Option<String>`, `visibility`
- `TagsResponse` — `tags: Vec<TagInfo>`
- `TagInfo` — `tag_type`, `tag_value`

### mcp.rs

MCP server implementation using JSON-RPC 2.0 over stdio (line-delimited JSON).

**Key types:**

- `JsonRpcRequest` — Standard JSON-RPC 2.0 request: `jsonrpc`, `id`, `method`, `params`.
- `JsonRpcResponse` — Standard JSON-RPC 2.0 response: `jsonrpc`, `id`, `result`, `error`.
- `JsonRpcError` — Error with `code: i32` and `message: String`.
- `ToolDefinition` — MCP tool definition: `name`, `description`, `inputSchema` (JSON Schema).

**Key function:**

- `serve_stdio()` — Main entry point. Reads JSON-RPC requests line-by-line from stdin, dispatches to handler, writes responses to stdout. Runs synchronously (blocking stdin reads).

**Protocol methods handled:**

- `initialize` — Returns server info (name: "clearmemory", version from Cargo.toml) and capabilities.
- `tools/list` — Returns all 9 MCP tool definitions with JSON Schema input schemas.
- `tools/call` — Dispatches to the appropriate tool handler by name.

**Current tool call implementations:** The MCP tool call handlers are currently stubs that return static/placeholder responses. The `clearmemory_status` handler returns "healthy", `clearmemory_recall` returns a message with the query echoed, `clearmemory_retain` acknowledges storage with character count, and `clearmemory_reflect` returns "Reflect requires Tier 2 or higher". These stubs need to be wired to the `engine::Engine` instance.

**9 MCP tools defined:**

| Tool | Description |
|------|-------------|
| `clearmemory_recall` | Search memories with multi-strategy retrieval |
| `clearmemory_expand` | Get full verbatim content for a memory |
| `clearmemory_reflect` | Synthesize across memories (Tier 2+) |
| `clearmemory_status` | Corpus overview and health metrics |
| `clearmemory_retain` | Store a new memory |
| `clearmemory_import` | Bulk import from file or directory |
| `clearmemory_forget` | Invalidate a memory with temporal marking |
| `clearmemory_streams` | Manage streams |
| `clearmemory_tags` | Manage tags |

### http.rs

HTTP/JSON API server built with axum.

**Key types:**

- `AppState` — Shared state: `start_time: Instant`, `engine: Option<Arc<Engine>>`. Wrapped in `Arc<Mutex<AppState>>` for thread-safe access.

**Key functions:**

- `create_router(state)` — Builds the axum `Router` with all routes and shared state.
- `serve(bind_addr, port)` — Full server startup: loads config, initializes the engine via `Engine::init`, creates state, binds to address, and starts serving.

**Routes:**

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/health` | `health_handler` | Kubernetes-compatible health check. Returns service name and version. |
| GET | `/v1/status` | `status_handler` | Corpus status from engine (tier, memory count, uptime). Returns "degraded" if engine is None. |
| POST | `/v1/recall` | `recall_handler` | Delegates to `engine.recall()`. Returns 503 if engine unavailable. |
| GET | `/v1/expand/{memory_id}` | `expand_handler` | Delegates to `engine.expand()`. Returns 404 on not found. |
| POST | `/v1/retain` | `retain_handler` | Parses tags via `tags::taxonomy::parse_tag`, maps classification string to enum, delegates to `engine.retain()`. |
| POST | `/v1/forget` | `forget_handler` | Delegates to `engine.forget()`. |
| POST | `/v1/reflect` | `reflect_handler` | Stub: returns "Reflect requires Tier 2 or higher". |
| GET | `/v1/streams` | `streams_list_handler` | Stub: returns empty list. |
| POST | `/v1/streams` | `streams_create_handler` | Stub: returns `{"status": "created"}`. |
| GET | `/v1/tags` | `tags_list_handler` | Stub: returns empty list. |
| POST | `/v1/tags` | `tags_manage_handler` | Stub: returns `{"status": "ok"}`. |

**Engine integration:** The `recall_handler`, `expand_handler`, `retain_handler`, and `forget_handler` are fully wired to the `Engine`. The `reflect_handler`, `streams_*_handler`, and `tags_*_handler` are stubs.

## Key Public Types Other Modules Depend On

- `AppState` — used by the HTTP handler functions and test infrastructure
- The handler request/response types are the API contract for external consumers

## Relevant config.toml Keys

- `[server] mcp_enabled` — Whether MCP server is active (default: `true`)
- `[server] http_enabled` — Whether HTTP server is active (default: `true`)
- `[server] http_port` — HTTP server port (default: `8080`)
- `[server] mcp_port` — MCP server port (default: `9700`)
- `[security] bind_address` — Address to bind HTTP server (default: `"127.0.0.1"`)

## Deferred / Planned Functionality

- **MCP tool call wiring to Engine:** The MCP `handle_tool_call` function returns stub responses. It needs access to an `Engine` instance (similar to how `http.rs` uses `AppState.engine`) to perform real operations.
- **Authentication middleware:** Neither the MCP nor HTTP server currently validates API tokens. The `[auth]` config section defines token scopes and TTLs, and `security::auth` has token validation logic, but the middleware is not wired into the axum router or MCP handler.
- **Rate limiting middleware:** The `security::rate_limiter` module exists but is not integrated into the HTTP router or MCP handler loop.
- **TLS support:** The `[security]` config supports `tls_cert_path` and `tls_key_path` but TLS is not configured on the axum listener.
- **Reflect handler:** Currently returns a static message. Needs to invoke the reflect model (Qwen3-4B) when candle integration is complete.
- **Streams and tags handlers:** Currently return empty/stub responses. Need to be connected to the `streams::manager` and `tags::taxonomy` modules.
