use criterion::{criterion_group, criterion_main, Criterion};

use clearmemory::entities::resolver::HeuristicResolver;
use clearmemory::migration;
use clearmemory::retrieval::rerank::PassthroughReranker;
use clearmemory::retrieval::{self, RecallConfig};
use clearmemory::storage::lance::LanceStorage;
use rusqlite::Connection;
use std::collections::HashMap;

/// Benchmark the full recall pipeline (keyword + temporal + entity graph, no semantic).
///
/// This measures the end-to-end retrieval latency without embedding model overhead.
fn bench_recall_no_semantic(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let conn = Connection::open_in_memory().unwrap();
    migration::runner::run_migrations(&conn).unwrap();

    // Populate with a realistic corpus
    let summaries_data = vec![
        "Authentication middleware timeout fix by increasing JWT verification timeout",
        "Database connection pool exhaustion resolved by setting max connections to 20",
        "Memory leak in WebSocket handler fixed by cleaning up event listeners on disconnect",
        "Race condition in payment processing causing double charges on concurrent updates",
        "CSS rendering issue on Safari with flexbox gap property not supported",
        "Timezone handling correction in scheduling service for multi-timezone users",
        "Q1 migration milestones: schema migration, dual-write, cutover phases",
        "SOC2 audit preparation documenting data flows and access controls",
        "Frontend rewrite from Create React App to Next.js with App Router",
        "Mobile app v2 launch targeting 95% crash-free rate by April",
        "API versioning strategy maintaining v1 and v2 for 12 months",
        "Team agreed to deprecate old REST API v1 by June 2026",
        "Sprint retrospective identified slow deployment pipeline as bottleneck",
        "Code review guidelines requiring one approval, two for security changes",
        "On-call rotation switching from weekly to bi-weekly with secondary backup",
        "Redis caching layer for user profiles reducing p95 latency from 450ms to 35ms",
        "Database query optimization with composite index on orders table",
        "Docker image multi-stage build reducing size from 1.2GB to 180MB",
        "Kubernetes auto-scaling policies with min 3 max 12 nodes at 70% CPU",
        "GraphQL schema redesign with Apollo Federation for microservice architecture",
    ];

    let mut summaries: HashMap<String, String> = HashMap::new();
    for (i, s) in summaries_data.iter().enumerate() {
        let id = format!("mem-{i}");
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) \
             VALUES (?1, ?2, ?3, 'clear', '2026-02-01')",
            rusqlite::params![id, format!("hash-{i}"), s],
        )
        .unwrap();
        summaries.insert(id, s.to_string());
    }

    let dir = tempfile::tempdir().unwrap();
    let lance = rt.block_on(async {
        LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap()
    });

    let resolver = HeuristicResolver;
    let reranker = PassthroughReranker;
    let config = RecallConfig {
        top_k: 10,
        temporal_boost: 0.4,
        entity_boost: 0.3,
        include_archived: false,
        stream_id: None,
    };

    let queries = [
        "authentication JWT timeout",
        "database connection pool",
        "GraphQL schema federation",
        "payment processing race condition",
        "Docker image optimization",
    ];

    c.bench_function("recall_20_memories_no_semantic", |b| {
        let mut qi = 0;
        b.iter(|| {
            let query = queries[qi % queries.len()];
            qi += 1;
            let l = lance.clone();
            let s = summaries.clone();
            rt.block_on(async {
                retrieval::recall(query, &conn, &l, None, &resolver, &reranker, &s, &config)
                    .await
                    .unwrap()
            });
        });
    });
}

criterion_group!(benches, bench_recall_no_semantic);
criterion_main!(benches);
