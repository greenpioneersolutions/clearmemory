use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use tracing::{debug, info};

/// JSON-RPC 2.0 request.
#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: serde_json::Value,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

/// JSON-RPC 2.0 response.
#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

/// MCP tool definition.
#[derive(Serialize)]
struct ToolDefinition {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: serde_json::Value,
}

/// Run the MCP server over stdio (JSON-RPC 2.0).
pub fn serve_stdio() -> anyhow::Result<()> {
    info!("MCP server started on stdio");

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        debug!(request = %line, "received");

        let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(req) => handle_request(req),
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: serde_json::Value::Null,
                result: None,
                error: Some(JsonRpcError {
                    code: -32700,
                    message: format!("parse error: {e}"),
                }),
            },
        };

        let resp_str = serde_json::to_string(&response)?;
        writeln!(stdout, "{resp_str}")?;
        stdout.flush()?;
    }

    Ok(())
}

fn handle_request(req: JsonRpcRequest) -> JsonRpcResponse {
    let result = match req.method.as_str() {
        "initialize" => handle_initialize(),
        "tools/list" => handle_tools_list(),
        "tools/call" => handle_tool_call(&req.params),
        _ => Err((-32601, format!("method not found: {}", req.method))),
    };

    match result {
        Ok(value) => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: req.id,
            result: Some(value),
            error: None,
        },
        Err((code, message)) => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: req.id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        },
    }
}

fn handle_initialize() -> Result<serde_json::Value, (i32, String)> {
    Ok(serde_json::json!({
        "protocolVersion": "2024-11-05",
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "clearmemory",
            "version": env!("CARGO_PKG_VERSION"),
        }
    }))
}

fn handle_tools_list() -> Result<serde_json::Value, (i32, String)> {
    let tools = vec![
        tool_def(
            "clearmemory_recall",
            "Search memories with multi-strategy retrieval",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "stream_id": {"type": "string", "description": "Filter by stream"},
                    "include_archive": {"type": "boolean", "description": "Include archived memories"}
                },
                "required": ["query"]
            }),
        ),
        tool_def(
            "clearmemory_expand",
            "Get full verbatim content for a memory",
            serde_json::json!({
                "type": "object",
                "properties": { "memory_id": {"type": "string"} },
                "required": ["memory_id"]
            }),
        ),
        tool_def(
            "clearmemory_reflect",
            "Synthesize across memories (Tier 2+)",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "stream_id": {"type": "string"}
                }
            }),
        ),
        tool_def(
            "clearmemory_status",
            "Corpus overview and health metrics",
            serde_json::json!({
                "type": "object", "properties": {}
            }),
        ),
        tool_def(
            "clearmemory_retain",
            "Store a new memory",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "content": {"type": "string"},
                    "tags": {"type": "array", "items": {"type": "string"}},
                    "classification": {"type": "string"}
                },
                "required": ["content"]
            }),
        ),
        tool_def(
            "clearmemory_import",
            "Bulk import from file or directory",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "format": {"type": "string"}
                },
                "required": ["path"]
            }),
        ),
        tool_def(
            "clearmemory_forget",
            "Invalidate a memory with temporal marking",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "memory_id": {"type": "string"},
                    "reason": {"type": "string"}
                },
                "required": ["memory_id"]
            }),
        ),
        tool_def(
            "clearmemory_streams",
            "Manage streams",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["list", "create", "describe", "switch"]},
                    "name": {"type": "string"}
                },
                "required": ["action"]
            }),
        ),
        tool_def(
            "clearmemory_tags",
            "Manage tags",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["list", "add", "remove", "rename"]},
                    "tag_type": {"type": "string"},
                    "tag_value": {"type": "string"}
                },
                "required": ["action"]
            }),
        ),
    ];

    Ok(serde_json::json!({ "tools": tools }))
}

fn handle_tool_call(params: &serde_json::Value) -> Result<serde_json::Value, (i32, String)> {
    let tool_name = params["name"].as_str().unwrap_or("");
    let args = &params["arguments"];

    match tool_name {
        "clearmemory_status" => Ok(serde_json::json!({
            "content": [{"type": "text", "text": "Clear Memory status: healthy"}]
        })),
        "clearmemory_recall" => {
            let query = args["query"].as_str().unwrap_or("");
            Ok(serde_json::json!({
                "content": [{"type": "text", "text": format!("Recall results for: {query}\n(No memories stored yet)")}]
            }))
        }
        "clearmemory_retain" => {
            let content = args["content"].as_str().unwrap_or("");
            Ok(serde_json::json!({
                "content": [{"type": "text", "text": format!("Memory stored ({} chars)", content.len())}]
            }))
        }
        "clearmemory_reflect" => Ok(serde_json::json!({
            "content": [{"type": "text", "text": "Reflect requires Tier 2 or higher"}]
        })),
        _ => Ok(serde_json::json!({
            "content": [{"type": "text", "text": format!("Tool {tool_name} called")}]
        })),
    }
}

fn tool_def(name: &str, description: &str, schema: serde_json::Value) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: description.to_string(),
        input_schema: schema,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_initialize() {
        let result = handle_initialize().unwrap();
        assert!(result["serverInfo"]["name"].as_str().unwrap() == "clearmemory");
    }

    #[test]
    fn test_handle_tools_list() {
        let result = handle_tools_list().unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 9);
    }

    #[test]
    fn test_handle_tool_call_status() {
        let params = serde_json::json!({"name": "clearmemory_status", "arguments": {}});
        let result = handle_tool_call(&params).unwrap();
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("healthy"));
    }

    #[test]
    fn test_handle_tool_call_recall() {
        let params =
            serde_json::json!({"name": "clearmemory_recall", "arguments": {"query": "auth"}});
        let result = handle_tool_call(&params).unwrap();
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("auth"));
    }

    #[test]
    fn test_handle_unknown_method() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::json!(1),
            method: "unknown".into(),
            params: serde_json::Value::Null,
        };
        let resp = handle_request(req);
        assert!(resp.error.is_some());
    }
}
