//! Multi-Axis Benchmark
//!
//! Measures three dimensions simultaneously:
//! 1. **Quality** — Recall@10, MRR (standard retrieval metrics)
//! 2. **Latency** — p50, p95, p99 end-to-end recall latency
//! 3. **Efficiency** — tokens that would be injected vs naive full-context
//!
//! This follows the approach of Hindsight's Agent Memory Benchmark (AMB)
//! which weights quality, speed, and cost as independent axes.
//!
//! Run: `cargo test --release --test benchmark_multiaxis -- --nocapture --ignored`

use clearmemory::entities::resolver::HeuristicResolver;
use clearmemory::migration;
use clearmemory::retrieval::rerank::PassthroughReranker;
use clearmemory::retrieval::{self, RecallConfig};
use clearmemory::storage::lance::LanceStorage;
use rusqlite::Connection;
use std::collections::HashMap;
use std::time::Instant;

struct Mem {
    id: &'static str,
    summary: &'static str,
    created_at: &'static str,
}

struct TimedQuery {
    query: &'static str,
    expected_id: &'static str,
}

fn build_corpus() -> Vec<Mem> {
    vec![
        Mem { id: "ma-001", summary: "Selected PostgreSQL 16 as the primary database replacing MySQL for JSON support and row-level security", created_at: "2025-07-20" },
        Mem { id: "ma-002", summary: "Chose Kafka over RabbitMQ for event streaming because we need replay capability", created_at: "2025-07-25" },
        Mem { id: "ma-003", summary: "Authentication migrated from Auth0 to Clerk for better developer experience and pricing", created_at: "2025-08-10" },
        Mem { id: "ma-004", summary: "API gateway using Kong over Envoy for plugin ecosystem and lower operational complexity", created_at: "2025-08-01" },
        Mem { id: "ma-005", summary: "gRPC for inter-service communication with protobuf schemas REST only for public APIs", created_at: "2025-08-15" },
        Mem { id: "ma-006", summary: "Redis Cluster with read replicas for session storage and hot data caching layer", created_at: "2025-08-20" },
        Mem { id: "ma-007", summary: "Elasticsearch 8.x for full-text search replacing Solr with vector search capability", created_at: "2025-09-01" },
        Mem { id: "ma-008", summary: "Frontend state management adopting Zustand over Redux for simpler API and less boilerplate", created_at: "2025-09-05" },
        Mem { id: "ma-009", summary: "GitHub Actions replacing Jenkins for CI/CD with YAML config and reduced maintenance", created_at: "2025-09-10" },
        Mem { id: "ma-010", summary: "Grafana Prometheus Loki for observability replacing Datadog saving 70 percent on costs", created_at: "2025-09-15" },
        Mem { id: "ma-011", summary: "Fixed critical memory leak in payment service connection pool growing unbounded after 6 hours", created_at: "2025-08-05" },
        Mem { id: "ma-012", summary: "Production outage caused by expired TLS certificate on API gateway lasting 2 hours", created_at: "2025-09-03" },
        Mem { id: "ma-013", summary: "Resolved race condition in order processing causing duplicate orders on concurrent requests", created_at: "2025-08-12" },
        Mem { id: "ma-014", summary: "Security audit found 3 medium vulnerabilities CORS misconfiguration rate limiting password reset", created_at: "2025-11-20" },
        Mem { id: "ma-015", summary: "Sprint planning prioritized auth migration payment refactor and mobile API v2", created_at: "2025-10-08" },
    ]
}

fn build_queries() -> Vec<TimedQuery> {
    vec![
        TimedQuery { query: "what database did we choose", expected_id: "ma-001" },
        TimedQuery { query: "event streaming technology", expected_id: "ma-002" },
        TimedQuery { query: "authentication provider", expected_id: "ma-003" },
        TimedQuery { query: "API gateway decision", expected_id: "ma-004" },
        TimedQuery { query: "inter-service communication protocol", expected_id: "ma-005" },
        TimedQuery { query: "caching layer setup", expected_id: "ma-006" },
        TimedQuery { query: "full-text search infrastructure", expected_id: "ma-007" },
        TimedQuery { query: "frontend state management", expected_id: "ma-008" },
        TimedQuery { query: "CI/CD build system", expected_id: "ma-009" },
        TimedQuery { query: "monitoring and observability", expected_id: "ma-010" },
        TimedQuery { query: "payment service memory leak", expected_id: "ma-011" },
        TimedQuery { query: "TLS certificate outage", expected_id: "ma-012" },
        TimedQuery { query: "duplicate order bug", expected_id: "ma-013" },
        TimedQuery { query: "security vulnerabilities found", expected_id: "ma-014" },
        TimedQuery { query: "sprint priorities and planning", expected_id: "ma-015" },
    ]
}

fn estimate_tokens(text: &str) -> usize {
    // Rough estimate: ~4 chars per token for English text
    text.len() / 4
}

#[test]
#[ignore]
fn test_multiaxis_benchmark() {
    println!();
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  Multi-Axis Benchmark — Quality + Latency + Efficiency       ║");
    println!("╠════════════════════════════════════════════════════════════════╣");

    let corpus = build_corpus();
    let queries = build_queries();

    // Setup
    let conn = Connection::open_in_memory().unwrap();
    migration::runner::run_migrations(&conn).unwrap();

    let mut summaries: HashMap<String, String> = HashMap::new();
    let mut total_corpus_tokens = 0;

    for mem in &corpus {
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) VALUES (?1, ?2, ?3, 'clear', ?4)",
            rusqlite::params![mem.id, format!("hash-{}", mem.id), mem.summary, mem.created_at],
        ).unwrap();
        summaries.insert(mem.id.to_string(), mem.summary.to_string());
        total_corpus_tokens += estimate_tokens(mem.summary);
    }

    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let embedder = clearmemory::storage::embeddings::EmbeddingManager::new("bge-small-en").unwrap();
    let dim = embedder.dimensions();
    let lance = rt.block_on(LanceStorage::open_with_dim(dir.path().join("v"), dim as i32)).unwrap();

    for mem in &corpus {
        if let Ok(vec) = embedder.embed_query(mem.summary) {
            rt.block_on(lance.insert(mem.id, &vec, None)).unwrap();
        }
    }

    let resolver = HeuristicResolver;
    let reranker = PassthroughReranker;
    let config = RecallConfig {
        top_k: 10,
        temporal_boost: 0.4,
        entity_boost: 0.3,
        include_archived: false,
        stream_id: None,
    };

    // Run queries and collect metrics
    let mut latencies_ms: Vec<f64> = Vec::new();
    let mut hits = 0;
    let mut mrr_sum = 0.0;
    let mut tokens_injected_sum = 0;

    for q in &queries {
        let start = Instant::now();

        let query_vec = embedder.embed_query(q.query).ok();
        let result = rt.block_on(retrieval::recall(
            q.query, &conn, &lance, query_vec.as_deref(),
            &resolver, &reranker, &summaries, &config,
        )).unwrap();

        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        latencies_ms.push(elapsed);

        let ids: Vec<&str> = result.results.iter().map(|r| r.memory_id.as_str()).collect();
        let found = ids.iter().position(|id| *id == q.expected_id);

        if let Some(rank) = found {
            hits += 1;
            mrr_sum += 1.0 / (rank as f64 + 1.0);
        }

        // Tokens that would be injected (top-10 summaries)
        let injected: usize = result.results.iter()
            .filter_map(|r| summaries.get(&r.memory_id))
            .map(|s| estimate_tokens(s))
            .sum();
        tokens_injected_sum += injected;
    }

    let n = queries.len() as f64;

    // Quality metrics
    let recall_10 = hits as f64 / n;
    let mrr = mrr_sum / n;

    // Latency metrics (sort for percentiles)
    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = latencies_ms[latencies_ms.len() / 2];
    let p95 = latencies_ms[(latencies_ms.len() as f64 * 0.95) as usize];
    let p99 = latencies_ms[(latencies_ms.len() as f64 * 0.99).min(latencies_ms.len() as f64 - 1.0) as usize];

    // Efficiency metrics
    let avg_tokens_injected = tokens_injected_sum as f64 / n;
    let naive_tokens = total_corpus_tokens as f64; // naive: inject entire corpus
    let token_reduction = 1.0 - (avg_tokens_injected / naive_tokens);

    println!();
    println!("  ┌─────────────────────────────────────────────────────────────┐");
    println!("  │  QUALITY                                                    │");
    println!("  │  Recall@10: {:.1}%  ({}/{})                              │", recall_10 * 100.0, hits, queries.len());
    println!("  │  MRR:       {:.4}                                        │", mrr);
    println!("  ├─────────────────────────────────────────────────────────────┤");
    println!("  │  LATENCY (end-to-end recall, including embedding)          │");
    println!("  │  p50:  {:>7.1}ms                                          │", p50);
    println!("  │  p95:  {:>7.1}ms                                          │", p95);
    println!("  │  p99:  {:>7.1}ms                                          │", p99);
    println!("  ├─────────────────────────────────────────────────────────────┤");
    println!("  │  EFFICIENCY (token usage)                                  │");
    println!("  │  Avg tokens injected per query: {:.0}                    │", avg_tokens_injected);
    println!("  │  Full corpus tokens: {}                                  │", total_corpus_tokens);
    println!("  │  Token reduction vs naive: {:.1}%                        │", token_reduction * 100.0);
    println!("  └─────────────────────────────────────────────────────────────┘");

    // Export results
    let results = serde_json::json!({
        "quality": {
            "recall_at_10": recall_10,
            "mrr": mrr,
        },
        "latency_ms": {
            "p50": p50,
            "p95": p95,
            "p99": p99,
        },
        "efficiency": {
            "avg_tokens_injected": avg_tokens_injected,
            "corpus_tokens": total_corpus_tokens,
            "token_reduction_pct": token_reduction * 100.0,
        },
        "config": {
            "corpus_size": corpus.len(),
            "query_count": queries.len(),
            "top_k": config.top_k,
            "embedding_model": "bge-small-en",
        }
    });

    let output_path = std::path::Path::new("tests/results/multiaxis_results.json");
    if let Some(parent) = output_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(&results) {
        let _ = std::fs::write(output_path, json);
        println!();
        println!("  Results exported to: {}", output_path.display());
    }

    println!("╚════════════════════════════════════════════════════════════════╝");
}
