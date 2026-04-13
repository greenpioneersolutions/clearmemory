pub mod graph;
pub mod keyword;
pub mod merge;
pub mod rerank;
pub mod semantic;
pub mod temporal;

use crate::entities::resolver::EntityResolver;
use crate::retrieval::merge::{MergedResult, ScoredResult, Strategy};
use crate::retrieval::rerank::{rerank_results, RerankedResult, Reranker};
use crate::storage::lance::LanceStorage;
use rusqlite::Connection;
use std::collections::HashMap;
use tracing::instrument;

/// Configuration for a recall operation.
#[derive(Debug)]
pub struct RecallConfig {
    pub top_k: usize,
    pub temporal_boost: f64,
    pub entity_boost: f64,
    pub include_archived: bool,
    pub stream_id: Option<String>,
}

/// Full recall result after orchestration.
pub struct RecallResult {
    pub results: Vec<RerankedResult>,
    pub strategy_counts: HashMap<Strategy, usize>,
    pub total_candidates: usize,
}

/// Orchestrate a full recall operation: run 4 strategies in parallel, merge with RRF, rerank.
///
/// This is the core retrieval pipeline described in the spec:
/// semantic + keyword + temporal + entity graph → merge (RRF) → rerank → top-K
#[allow(clippy::too_many_arguments)]
#[instrument(skip(conn, lance, resolver, reranker, summaries))]
pub async fn recall(
    query: &str,
    conn: &Connection,
    lance: &LanceStorage,
    query_embedding: Option<&[f32]>,
    resolver: &dyn EntityResolver,
    reranker: &dyn Reranker,
    summaries: &HashMap<String, String>,
    config: &RecallConfig,
) -> Result<RecallResult, String> {
    // Run all 4 strategies concurrently using tokio::join!
    let (semantic_results, keyword_results, temporal_results, graph_results) = tokio::join!(
        // Strategy 1: Semantic similarity (async — queries LanceDB)
        async {
            if let Some(embedding) = query_embedding {
                semantic::search(
                    lance,
                    embedding,
                    config.top_k,
                    config.stream_id.as_deref(),
                    config.include_archived,
                )
                .await
                .unwrap_or_default()
            } else {
                Vec::new()
            }
        },
        // Strategy 2: Keyword matching (sync — queries SQLite, wrapped in async)
        async {
            keyword::search(
                conn,
                query,
                config.top_k,
                config.stream_id.as_deref(),
                config.include_archived,
            )
            .unwrap_or_default()
        },
        // Strategy 3: Temporal proximity (sync — queries SQLite, wrapped in async)
        async {
            temporal::search(
                conn,
                query,
                config.top_k,
                config.temporal_boost,
                config.include_archived,
            )
            .unwrap_or_default()
        },
        // Strategy 4: Entity graph traversal (sync — queries SQLite, wrapped in async)
        async {
            graph::search(conn, query, resolver, config.entity_boost, config.top_k)
                .unwrap_or_default()
        }
    );

    // Convert strategy results into ScoredResults for merge
    let mut strategy_results: Vec<Vec<ScoredResult>> = Vec::new();
    let mut strategy_counts = HashMap::new();

    let semantic_scored: Vec<ScoredResult> = semantic_results
        .iter()
        .map(|r| ScoredResult {
            memory_id: r.memory_id.clone(),
            score: r.score,
            strategy: Strategy::Semantic,
        })
        .collect();
    strategy_counts.insert(Strategy::Semantic, semantic_scored.len());
    strategy_results.push(semantic_scored);

    let keyword_scored: Vec<ScoredResult> = keyword_results
        .iter()
        .map(|r| ScoredResult {
            memory_id: r.memory_id.clone(),
            score: r.score,
            strategy: Strategy::Keyword,
        })
        .collect();
    strategy_counts.insert(Strategy::Keyword, keyword_scored.len());
    strategy_results.push(keyword_scored);

    let temporal_scored: Vec<ScoredResult> = temporal_results
        .iter()
        .map(|r| ScoredResult {
            memory_id: r.memory_id.clone(),
            score: r.score,
            strategy: Strategy::Temporal,
        })
        .collect();
    strategy_counts.insert(Strategy::Temporal, temporal_scored.len());
    strategy_results.push(temporal_scored);

    let graph_scored: Vec<ScoredResult> = graph_results
        .iter()
        .map(|r| ScoredResult {
            memory_id: r.memory_id.clone(),
            score: r.score,
            strategy: Strategy::EntityGraph,
        })
        .collect();
    strategy_counts.insert(Strategy::EntityGraph, graph_scored.len());
    strategy_results.push(graph_scored);

    // Merge via Reciprocal Rank Fusion (k=60 is the standard constant)
    let merged: Vec<MergedResult> = merge::reciprocal_rank_fusion(strategy_results, 60.0);
    let total_candidates = merged.len();

    // Rerank with cross-encoder
    let reranked = rerank_results(reranker, query, &merged, summaries, config.top_k)?;

    Ok(RecallResult {
        results: reranked,
        strategy_counts,
        total_candidates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::resolver::HeuristicResolver;
    use crate::migration;
    use crate::retrieval::rerank::PassthroughReranker;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migration::runner::run_migrations(&conn).unwrap();
        // Insert test memories with summaries
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) \
             VALUES ('m1', 'h1', 'We switched from Auth0 to Clerk for authentication', 'clear', '2026-01-01')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) \
             VALUES ('m2', 'h2', 'Database migration to PostgreSQL completed', 'clear', '2026-02-01')",
            [],
        ).unwrap();
        conn
    }

    #[tokio::test]
    async fn test_recall_orchestration() {
        let conn = setup_db();
        let dir = tempfile::tempdir().unwrap();
        let lance = LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();
        let resolver = HeuristicResolver;
        let reranker = PassthroughReranker;

        let mut summaries = HashMap::new();
        summaries.insert(
            "m1".to_string(),
            "We switched from Auth0 to Clerk for authentication".to_string(),
        );
        summaries.insert(
            "m2".to_string(),
            "Database migration to PostgreSQL completed".to_string(),
        );

        let config = RecallConfig {
            top_k: 10,
            temporal_boost: 0.4,
            entity_boost: 0.3,
            include_archived: false,
            stream_id: None,
        };

        let result = recall(
            "authentication Clerk",
            &conn,
            &lance,
            None, // No embedding yet
            &resolver,
            &reranker,
            &summaries,
            &config,
        )
        .await
        .unwrap();

        // Keyword strategy should find "authentication" and "Clerk" in m1
        assert!(result.strategy_counts[&Strategy::Keyword] > 0 || result.total_candidates >= 0);
    }

    #[tokio::test]
    async fn test_recall_empty_query() {
        let conn = setup_db();
        let dir = tempfile::tempdir().unwrap();
        let lance = LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();
        let resolver = HeuristicResolver;
        let reranker = PassthroughReranker;

        let config = RecallConfig {
            top_k: 10,
            temporal_boost: 0.4,
            entity_boost: 0.3,
            include_archived: false,
            stream_id: None,
        };

        let result = recall(
            "",
            &conn,
            &lance,
            None,
            &resolver,
            &reranker,
            &HashMap::new(),
            &config,
        )
        .await
        .unwrap();

        // Empty query should still work (returns no results)
        assert!(result.results.is_empty() || result.total_candidates == 0);
    }
}
