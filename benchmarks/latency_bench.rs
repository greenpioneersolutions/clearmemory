use criterion::{criterion_group, criterion_main, Criterion};

use arrow_array::{ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use std::sync::Arc;

/// Benchmark LanceDB insert latency.
fn bench_lance_insert(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let dir = tempfile::tempdir().unwrap();
    let lance = rt.block_on(async {
        clearmemory::storage::lance::LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap()
    });

    // Pre-generate a vector
    let vector: Vec<f32> = (0..384).map(|i| (i as f32 / 384.0).sin()).collect();
    let mut counter = 0u64;

    c.bench_function("lance_insert_single", |b| {
        b.iter(|| {
            counter += 1;
            let id = format!("bench-mem-{counter}");
            let v = vector.clone();
            let l = lance.clone();
            rt.block_on(async move {
                l.insert(&id, &v, None).await.unwrap();
            });
        });
    });
}

/// Benchmark LanceDB search latency.
fn bench_lance_search(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let dir = tempfile::tempdir().unwrap();
    let lance = rt.block_on(async {
        let l = clearmemory::storage::lance::LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();

        // Pre-populate with 100 vectors for realistic search
        for i in 0..100 {
            let vector: Vec<f32> = (0..384)
                .map(|j| ((i * 384 + j) as f32 / 1000.0).sin())
                .collect();
            l.insert(&format!("mem-{i}"), &vector, None).await.unwrap();
        }
        l
    });

    let query_vector: Vec<f32> = (0..384).map(|i| (i as f32 / 384.0).cos()).collect();

    c.bench_function("lance_search_100_vectors", |b| {
        b.iter(|| {
            let q = query_vector.clone();
            let l = lance.clone();
            rt.block_on(async move {
                l.search(&q, 10, None, false).await.unwrap();
            });
        });
    });
}

/// Benchmark keyword search latency.
fn bench_keyword_search(c: &mut Criterion) {
    use clearmemory::migration;
    use clearmemory::retrieval::keyword;
    use rusqlite::Connection;

    let conn = Connection::open_in_memory().unwrap();
    migration::runner::run_migrations(&conn).unwrap();

    // Populate with 200 memories
    for i in 0..200 {
        let summaries = [
            "Authentication middleware with JWT token verification and refresh logic",
            "Database migration from MySQL to PostgreSQL for ACID compliance",
            "GraphQL API redesign with Apollo Federation microservices",
            "Redis caching layer for user session management with TTL",
            "Kubernetes cluster auto-scaling configuration and deployment",
            "CI/CD pipeline optimization with parallel test execution",
            "Security audit findings and OWASP vulnerability remediation",
            "Frontend performance optimization with React memoization",
            "Docker image optimization using multi-stage builds",
            "Load testing results showing system capacity and bottlenecks",
        ];
        let summary = summaries[i % summaries.len()];
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) \
             VALUES (?1, ?2, ?3, 'clear', '2026-01-01')",
            rusqlite::params![format!("mem-{i}"), format!("hash-{i}"), summary],
        )
        .unwrap();
    }

    c.bench_function("keyword_search_200_memories", |b| {
        b.iter(|| {
            keyword::search(&conn, "authentication JWT token", 10, None, false).unwrap();
        });
    });
}

/// Benchmark RRF merge latency.
fn bench_rrf_merge(c: &mut Criterion) {
    use clearmemory::retrieval::merge::{reciprocal_rank_fusion, ScoredResult, Strategy};

    // Create realistic strategy results
    let make_results = |strategy: Strategy, count: usize| -> Vec<ScoredResult> {
        (0..count)
            .map(|i| ScoredResult {
                memory_id: format!("mem-{}", i * 3 + strategy as usize),
                score: 1.0 - i as f64 * 0.05,
                strategy,
            })
            .collect()
    };

    c.bench_function("rrf_merge_4_strategies_10_each", |b| {
        b.iter(|| {
            let strategies = vec![
                make_results(Strategy::Semantic, 10),
                make_results(Strategy::Keyword, 10),
                make_results(Strategy::Temporal, 10),
                make_results(Strategy::EntityGraph, 10),
            ];
            reciprocal_rank_fusion(strategies, 60.0);
        });
    });
}

criterion_group!(
    benches,
    bench_lance_insert,
    bench_lance_search,
    bench_keyword_search,
    bench_rrf_merge,
);
criterion_main!(benches);
