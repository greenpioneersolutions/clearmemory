use serde::{Deserialize, Serialize};

/// Request/response types for all 9 MCP/HTTP tool handlers.

#[derive(Deserialize)]
pub struct RecallRequest {
    pub query: String,
    pub stream_id: Option<String>,
    pub tags: Option<Vec<String>>,
    pub include_archive: Option<bool>,
}

#[derive(Serialize)]
pub struct RecallResponse {
    pub results: Vec<RecallResult>,
    pub query: String,
    pub count: usize,
}

#[derive(Serialize)]
pub struct RecallResult {
    pub memory_id: String,
    pub summary: Option<String>,
    pub score: f64,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct ExpandRequest {
    pub memory_id: String,
}

#[derive(Serialize)]
pub struct ExpandResponse {
    pub memory_id: String,
    pub content: String,
    pub source_format: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct RetainRequest {
    pub content: String,
    pub tags: Option<Vec<String>>,
    pub classification: Option<String>,
    pub stream_id: Option<String>,
}

#[derive(Serialize)]
pub struct RetainResponse {
    pub memory_id: String,
    pub content_hash: String,
}

#[derive(Deserialize)]
pub struct ForgetRequest {
    pub memory_id: String,
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct ForgetResponse {
    pub memory_id: String,
    pub status: String,
}

#[derive(Deserialize)]
pub struct ReflectRequest {
    pub query: Option<String>,
    pub stream_id: Option<String>,
}

#[derive(Serialize)]
pub struct ReflectResponse {
    pub synthesis: String,
    pub source_count: usize,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub tier: String,
    pub memory_count: i64,
    pub corpus_size_bytes: u64,
    pub uptime_secs: u64,
}

#[derive(Deserialize)]
pub struct StreamsRequest {
    pub action: String, // list, create, describe
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct StreamsResponse {
    pub streams: Vec<StreamInfo>,
}

#[derive(Serialize)]
pub struct StreamInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub visibility: String,
}

#[derive(Deserialize)]
pub struct TagsRequest {
    pub action: String, // list, add, remove
    pub tag_type: Option<String>,
    pub tag_value: Option<String>,
    pub memory_id: Option<String>,
}

#[derive(Serialize)]
pub struct TagsResponse {
    pub tags: Vec<TagInfo>,
}

#[derive(Serialize)]
pub struct TagInfo {
    pub tag_type: String,
    pub tag_value: String,
}
