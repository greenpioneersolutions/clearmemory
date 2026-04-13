use crate::server::handlers::*;
use axum::{
    extract::{Path as AxumPath, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// Shared application state for HTTP handlers.
pub struct AppState {
    pub start_time: std::time::Instant,
    pub engine: Option<Arc<crate::engine::Engine>>,
}

/// Create the axum router with all API routes.
pub fn create_router(state: Arc<Mutex<AppState>>) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/v1/status", get(status_handler))
        .route("/v1/recall", post(recall_handler))
        .route("/v1/expand/{memory_id}", get(expand_handler))
        .route("/v1/retain", post(retain_handler))
        .route("/v1/forget", post(forget_handler))
        .route("/v1/reflect", post(reflect_handler))
        .route("/v1/streams", get(streams_list_handler))
        .route("/v1/streams", post(streams_create_handler))
        .route("/v1/tags", get(tags_list_handler))
        .route("/v1/tags", post(tags_manage_handler))
        .with_state(state)
}

/// Start the HTTP server.
pub async fn serve(bind_addr: &str, port: u16) -> anyhow::Result<()> {
    let config = crate::config::Config::load()?;
    let engine = crate::engine::Engine::init(config).await?;

    let state = Arc::new(Mutex::new(AppState {
        start_time: std::time::Instant::now(),
        engine: Some(Arc::new(engine)),
    }));

    let app = create_router(state);
    let addr = format!("{bind_addr}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!(addr = %addr, "HTTP server started");
    axum::serve(listener, app).await?;

    Ok(())
}

// --- Handler implementations ---

async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "clearmemory",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn status_handler(State(state): State<Arc<Mutex<AppState>>>) -> Json<StatusResponse> {
    let (engine, uptime) = {
        let guard = state.lock().await;
        (guard.engine.clone(), guard.start_time.elapsed().as_secs())
    };

    if let Some(engine) = engine {
        let count = engine.sqlite.memory_count().await.unwrap_or(0);
        let _vectors = engine.lance.vector_count().await.unwrap_or(0);
        Json(StatusResponse {
            status: "healthy".into(),
            tier: engine.config.general.tier.to_string(),
            memory_count: count,
            corpus_size_bytes: 0,
            uptime_secs: uptime,
        })
    } else {
        Json(StatusResponse {
            status: "degraded".into(),
            tier: "unknown".into(),
            memory_count: 0,
            corpus_size_bytes: 0,
            uptime_secs: uptime,
        })
    }
}

async fn recall_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(req): Json<RecallRequest>,
) -> Result<Json<RecallResponse>, StatusCode> {
    let engine = {
        let guard = state.lock().await;
        guard
            .engine
            .clone()
            .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
    };

    let result = engine
        .recall(
            &req.query,
            req.stream_id,
            req.include_archive.unwrap_or(false),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(RecallResponse {
        count: result.results.len(),
        query: req.query,
        results: result
            .results
            .into_iter()
            .map(|h| RecallResult {
                memory_id: h.memory_id,
                summary: h.summary,
                score: h.score,
                created_at: h.created_at,
            })
            .collect(),
    }))
}

async fn expand_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    AxumPath(memory_id): AxumPath<String>,
) -> Result<Json<ExpandResponse>, StatusCode> {
    let engine = {
        let guard = state.lock().await;
        guard
            .engine
            .clone()
            .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
    };

    let result = engine
        .expand(&memory_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(Json(ExpandResponse {
        memory_id: result.memory_id,
        content: result.content,
        source_format: result.source_format,
        created_at: result.created_at,
    }))
}

async fn retain_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(req): Json<RetainRequest>,
) -> Result<Json<RetainResponse>, StatusCode> {
    let engine = {
        let guard = state.lock().await;
        guard
            .engine
            .clone()
            .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
    };

    let tags: Vec<(String, String)> = req
        .tags
        .unwrap_or_default()
        .iter()
        .filter_map(|t| crate::tags::taxonomy::parse_tag(t).ok())
        .collect();

    let classification = req.classification.map(|c| match c.as_str() {
        "public" => crate::Classification::Public,
        "confidential" => crate::Classification::Confidential,
        "pii" => crate::Classification::Pii,
        _ => crate::Classification::Internal,
    });

    let result = engine
        .retain(&req.content, tags, classification, req.stream_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(RetainResponse {
        memory_id: result.memory_id,
        content_hash: result.content_hash,
    }))
}

async fn forget_handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(req): Json<ForgetRequest>,
) -> Result<Json<ForgetResponse>, StatusCode> {
    let engine = {
        let guard = state.lock().await;
        guard
            .engine
            .clone()
            .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
    };

    engine
        .forget(&req.memory_id, req.reason.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ForgetResponse {
        memory_id: req.memory_id,
        status: "forgotten".into(),
    }))
}

async fn reflect_handler(Json(_req): Json<ReflectRequest>) -> Json<ReflectResponse> {
    Json(ReflectResponse {
        synthesis: "Reflect requires Tier 2 or higher".into(),
        source_count: 0,
    })
}

async fn streams_list_handler() -> Json<StreamsResponse> {
    Json(StreamsResponse {
        streams: Vec::new(),
    })
}

async fn streams_create_handler(Json(_req): Json<StreamsRequest>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "created"}))
}

async fn tags_list_handler() -> Json<TagsResponse> {
    Json(TagsResponse { tags: Vec::new() })
}

async fn tags_manage_handler(Json(_req): Json<TagsRequest>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn test_app() -> Router {
        let state = Arc::new(Mutex::new(AppState {
            start_time: std::time::Instant::now(),
            engine: None,
        }));
        create_router(state)
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = test_app();
        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_status_endpoint() {
        let app = test_app();
        let req = Request::builder()
            .uri("/v1/status")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_recall_endpoint() {
        let app = test_app();
        let body = serde_json::json!({"query": "test"});
        let req = Request::builder()
            .method("POST")
            .uri("/v1/recall")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Without engine, should return 503
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
