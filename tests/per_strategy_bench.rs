//! Per-Strategy Precision Benchmark
//!
//! Measures each retrieval strategy independently to identify strengths,
//! weaknesses, and regressions in individual components.
//!
//! Run with: `cargo test --test per_strategy_bench -- --nocapture`
//!
//! Semantic strategy tests require the embedding model and are marked #[ignore].

use clearmemory::entities;
use clearmemory::entities::resolver::HeuristicResolver;
use clearmemory::migration;
use clearmemory::retrieval::{graph, keyword, temporal};
use clearmemory::storage::lance::LanceStorage;
use rusqlite::Connection;

/// A test case for a single strategy.
struct StrategyTestCase {
    query: &'static str,
    expected_ids: Vec<&'static str>,
    description: &'static str,
}

/// Setup the shared test corpus in SQLite.
fn setup_corpus(conn: &Connection) {
    migration::runner::run_migrations(conn).unwrap();

    let memories = vec![
        ("kw-01", "We switched from Auth0 to Clerk for authentication because of better pricing and developer experience", "2026-01-15"),
        ("kw-02", "Database migration from MySQL to PostgreSQL completed successfully, all data integrity checks passed", "2026-01-20"),
        ("kw-03", "GraphQL API schema redesign with Apollo Federation for microservice architecture", "2026-01-25"),
        ("kw-04", "Fixed critical bug in payment processing: race condition causing double charges on concurrent requests", "2026-02-01"),
        ("kw-05", "Kubernetes cluster setup with auto-scaling policies: min 3 nodes, max 12, scale at 70% CPU", "2026-02-05"),
        ("kw-06", "Frontend performance: React memoization and code splitting reduced bundle from 2MB to 400KB", "2026-02-10"),
        ("kw-07", "Redis caching implementation for user sessions, TTL 24 hours, max memory 4GB with LRU eviction", "2026-02-15"),
        ("kw-08", "CI/CD pipeline: GitHub Actions with parallel test execution, deployment to Kubernetes via ArgoCD", "2026-02-20"),
        ("kw-09", "Security audit: implemented OWASP top 10 mitigations including SQL injection prevention and XSS filtering", "2026-02-25"),
        ("kw-10", "Team retrospective: need to improve code review turnaround time from 48h to 24h average", "2026-03-01"),
        ("kw-11", "Docker image optimization with multi-stage builds and distroless base, reduced from 1.2GB to 180MB", "2026-03-05"),
        ("kw-12", "Load testing results: system handles 10K concurrent users, bottleneck at database connection pool with 15K", "2026-03-10"),
        ("kw-13", "OAuth2 implementation with PKCE flow for mobile apps, refresh token rotation every 7 days", "2026-03-15"),
        ("kw-14", "Elasticsearch cluster deployment: 3 data nodes, 2 master nodes, index lifecycle management configured", "2026-03-20"),
        ("kw-15", "Sprint planning: Q2 priorities are API versioning, mobile app v2, and SOC2 certification", "2026-03-25"),
        // Temporal test cases with recent dates
        ("tmp-01", "Yesterday's deployment included hotfix for the login page timeout issue", "2026-04-12"),
        ("tmp-02", "Last week sprint review: completed 8 of 10 stories, carried over mobile push notifications", "2026-04-07"),
        ("tmp-03", "Meeting notes from early April: discussed Q2 roadmap and hiring plan", "2026-04-03"),
    ];

    for (id, summary, date) in &memories {
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) \
             VALUES (?1, ?2, ?3, 'clear', ?4)",
            rusqlite::params![id, format!("hash_{id}"), summary, date],
        )
        .unwrap();
    }
}

/// Compute precision@K for a set of results against expected IDs.
fn precision_at_k<T: AsRef<str>>(result_ids: &[T], expected_ids: &[&str], k: usize) -> f64 {
    let top_k: Vec<&str> = result_ids.iter().take(k).map(|s| s.as_ref()).collect();
    if top_k.is_empty() {
        return 0.0;
    }
    let hits = top_k.iter().filter(|r| expected_ids.contains(r)).count();
    hits as f64 / top_k.len() as f64
}

/// Compute recall@K for a set of results against expected IDs.
fn recall_at_k<T: AsRef<str>>(result_ids: &[T], expected_ids: &[&str], k: usize) -> f64 {
    if expected_ids.is_empty() {
        return 1.0;
    }
    let top_k: Vec<&str> = result_ids.iter().take(k).map(|s| s.as_ref()).collect();
    let hits = expected_ids.iter().filter(|e| top_k.contains(e)).count();
    hits as f64 / expected_ids.len() as f64
}

// ============================================================================
// KEYWORD STRATEGY TESTS
// ============================================================================

#[test]
fn test_keyword_strategy_precision() {
    let conn = Connection::open_in_memory().unwrap();
    setup_corpus(&conn);

    let cases = vec![
        StrategyTestCase {
            query: "authentication Clerk Auth0",
            expected_ids: vec!["kw-01"],
            description: "Auth provider switch",
        },
        StrategyTestCase {
            query: "GraphQL Apollo Federation",
            expected_ids: vec!["kw-03"],
            description: "GraphQL schema",
        },
        StrategyTestCase {
            query: "payment processing race condition",
            expected_ids: vec!["kw-04"],
            description: "Payment bug",
        },
        StrategyTestCase {
            query: "Kubernetes auto-scaling cluster",
            expected_ids: vec!["kw-05"],
            description: "K8s setup",
        },
        StrategyTestCase {
            query: "Redis caching user sessions",
            expected_ids: vec!["kw-07"],
            description: "Redis caching",
        },
        StrategyTestCase {
            query: "Docker multi-stage distroless image",
            expected_ids: vec!["kw-11"],
            description: "Docker optimization",
        },
        StrategyTestCase {
            query: "Elasticsearch cluster deployment",
            expected_ids: vec!["kw-14"],
            description: "Elasticsearch setup",
        },
        StrategyTestCase {
            query: "OAuth2 PKCE mobile refresh token",
            expected_ids: vec!["kw-13"],
            description: "OAuth flow",
        },
        StrategyTestCase {
            query: "security OWASP SQL injection XSS",
            expected_ids: vec!["kw-09"],
            description: "Security audit",
        },
        StrategyTestCase {
            query: "load testing concurrent users bottleneck",
            expected_ids: vec!["kw-12"],
            description: "Load testing",
        },
    ];

    let mut total_precision = 0.0;
    let mut total_recall = 0.0;
    let mut passes = 0;

    println!("\n========================================");
    println!("  KEYWORD STRATEGY BENCHMARK");
    println!("========================================");

    for tc in &cases {
        let results = keyword::search(&conn, tc.query, 5, None, false).unwrap();
        let result_ids: Vec<String> = results.iter().map(|r| r.memory_id.clone()).collect();

        let p5 = precision_at_k(&result_ids, &tc.expected_ids, 5);
        let r5 = recall_at_k(&result_ids, &tc.expected_ids, 5);

        total_precision += p5;
        total_recall += r5;

        let status = if r5 >= 1.0 { "HIT" } else { "MISS" };
        println!(
            "  [{status}] {}: P@5={p5:.2} R@5={r5:.2} (results: {:?})",
            tc.description, result_ids
        );

        if r5 >= 1.0 {
            passes += 1;
        }
    }

    let avg_precision = total_precision / cases.len() as f64;
    let avg_recall = total_recall / cases.len() as f64;

    println!("\n  Summary:");
    println!("  Avg Precision@5: {avg_precision:.4}");
    println!("  Avg Recall@5:    {avg_recall:.4}");
    println!(
        "  Pass rate:       {passes}/{} ({:.0}%)",
        cases.len(),
        passes as f64 / cases.len() as f64 * 100.0
    );
    println!("========================================\n");

    // Keyword search with exact terms should find most targets
    assert!(
        avg_recall > 0.6,
        "Keyword Recall@5 should be > 0.6, got {avg_recall:.4}"
    );
}

// ============================================================================
// TEMPORAL STRATEGY TESTS
// ============================================================================

#[test]
fn test_temporal_strategy_precision() {
    let conn = Connection::open_in_memory().unwrap();
    setup_corpus(&conn);

    // Temporal queries depend on current date. We use dates relative
    // to the corpus timestamps.
    let cases = vec![
        StrategyTestCase {
            query: "what happened in january",
            expected_ids: vec!["kw-01", "kw-02", "kw-03"],
            description: "January memories",
        },
        StrategyTestCase {
            query: "changes from february",
            expected_ids: vec![
                "kw-04", "kw-05", "kw-06", "kw-07", "kw-08", "kw-09", "kw-10",
            ],
            description: "February memories",
        },
        StrategyTestCase {
            query: "what did we do in march",
            expected_ids: vec!["kw-11", "kw-12", "kw-13", "kw-14", "kw-15"],
            description: "March memories",
        },
    ];

    let mut total_recall = 0.0;

    println!("\n========================================");
    println!("  TEMPORAL STRATEGY BENCHMARK");
    println!("========================================");

    for tc in &cases {
        let results = temporal::search(&conn, tc.query, 10, 0.4, false).unwrap();
        let result_ids: Vec<String> = results.iter().map(|r| r.memory_id.clone()).collect();

        let r10 = recall_at_k(&result_ids, &tc.expected_ids, 10);
        total_recall += r10;

        let found = tc
            .expected_ids
            .iter()
            .filter(|e| result_ids.iter().any(|r| r == **e))
            .count();

        println!(
            "  {}: R@10={r10:.2} (found {}/{} expected, {} total results)",
            tc.description,
            found,
            tc.expected_ids.len(),
            result_ids.len()
        );
    }

    let avg_recall = total_recall / cases.len() as f64;

    println!("\n  Avg Recall@10: {avg_recall:.4}");
    println!("========================================\n");

    // Temporal search should find memories in the right date range
    assert!(
        avg_recall > 0.2,
        "Temporal Recall@10 should be > 0.2, got {avg_recall:.4}"
    );
}

// ============================================================================
// ENTITY GRAPH STRATEGY TESTS
// ============================================================================

#[test]
fn test_entity_graph_strategy_precision() {
    let conn = Connection::open_in_memory().unwrap();
    setup_corpus(&conn);

    // Set up some entities and relationships for graph traversal
    let kai_id = entities::graph::create_entity(&conn, "Kai", Some("person")).unwrap();
    entities::graph::add_relationship(&conn, &kai_id, &kai_id, "works_on", Some("kw-01")).unwrap();
    entities::graph::add_relationship(&conn, &kai_id, &kai_id, "decided", Some("kw-04")).unwrap();

    let sarah_id = entities::graph::create_entity(&conn, "Sarah", Some("person")).unwrap();
    entities::graph::add_relationship(&conn, &sarah_id, &sarah_id, "works_on", Some("kw-03"))
        .unwrap();
    entities::graph::add_relationship(&conn, &sarah_id, &sarah_id, "works_on", Some("kw-06"))
        .unwrap();

    let auth_id = entities::graph::create_entity(&conn, "auth-service", Some("service")).unwrap();
    entities::graph::add_relationship(&conn, &auth_id, &auth_id, "related_to", Some("kw-01"))
        .unwrap();
    entities::graph::add_relationship(&conn, &auth_id, &auth_id, "related_to", Some("kw-13"))
        .unwrap();
    entities::graph::add_relationship(&conn, &auth_id, &auth_id, "related_to", Some("kw-09"))
        .unwrap();

    let resolver = HeuristicResolver;

    let cases = vec![
        StrategyTestCase {
            query: "What did Kai work on",
            expected_ids: vec!["kw-01", "kw-04"],
            description: "Entity: Kai's work",
        },
        StrategyTestCase {
            query: "Sarah contributions",
            expected_ids: vec!["kw-03", "kw-06"],
            description: "Entity: Sarah's work",
        },
        StrategyTestCase {
            query: "auth-service related changes",
            expected_ids: vec!["kw-01", "kw-13", "kw-09"],
            description: "Entity: auth-service",
        },
    ];

    let mut total_recall = 0.0;

    println!("\n========================================");
    println!("  ENTITY GRAPH STRATEGY BENCHMARK");
    println!("========================================");

    for tc in &cases {
        let results = graph::search(&conn, tc.query, &resolver, 0.3, 10).unwrap();
        let result_ids: Vec<String> = results.iter().map(|r| r.memory_id.clone()).collect();

        let r5 = recall_at_k(&result_ids, &tc.expected_ids, 5);
        total_recall += r5;

        let found = tc
            .expected_ids
            .iter()
            .filter(|e| result_ids.iter().any(|r| r == **e))
            .count();

        println!(
            "  {}: R@5={r5:.2} (found {}/{} expected, results: {:?})",
            tc.description,
            found,
            tc.expected_ids.len(),
            result_ids
        );
    }

    let avg_recall = total_recall / cases.len() as f64;

    println!("\n  Avg Recall@5: {avg_recall:.4}");
    println!("========================================\n");

    // Entity graph should find memories connected to queried entities
    assert!(
        avg_recall > 0.3,
        "Entity Graph Recall@5 should be > 0.3, got {avg_recall:.4}"
    );
}

// ============================================================================
// SEMANTIC STRATEGY TESTS (requires embedding model)
// ============================================================================

#[test]
#[ignore]
fn test_semantic_strategy_precision() {
    use clearmemory::retrieval::semantic;
    use clearmemory::storage::embeddings::EmbeddingManager;

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let emb = EmbeddingManager::new("bge-small-en").expect("failed to load embedding model");

        let conn = Connection::open_in_memory().unwrap();
        setup_corpus(&conn);

        let dir = tempfile::tempdir().unwrap();
        let lance = LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();

        // Embed and insert all memories
        let mut stmt = conn
            .prepare("SELECT id, summary FROM memories WHERE summary IS NOT NULL")
            .unwrap();
        let memories: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .flatten()
            .collect();

        for (id, summary) in &memories {
            let vector = emb.embed_query(summary).unwrap();
            lance.insert(id, &vector, None).await.unwrap();
        }

        let cases = vec![
            StrategyTestCase {
                query: "switching authentication providers",
                expected_ids: vec!["kw-01"],
                description: "Semantic: auth switch (paraphrase)",
            },
            StrategyTestCase {
                query: "migrating to a new database system",
                expected_ids: vec!["kw-02"],
                description: "Semantic: DB migration (paraphrase)",
            },
            StrategyTestCase {
                query: "making containers smaller and more secure",
                expected_ids: vec!["kw-11"],
                description: "Semantic: Docker optimization (conceptual)",
            },
            StrategyTestCase {
                query: "how does the system perform under high traffic",
                expected_ids: vec!["kw-12"],
                description: "Semantic: load testing (conceptual)",
            },
            StrategyTestCase {
                query: "user interface rendering speed improvements",
                expected_ids: vec!["kw-06"],
                description: "Semantic: frontend perf (conceptual)",
            },
            StrategyTestCase {
                query: "handling money transactions safely",
                expected_ids: vec!["kw-04"],
                description: "Semantic: payment safety (conceptual)",
            },
            StrategyTestCase {
                query: "protecting the application from attacks",
                expected_ids: vec!["kw-09"],
                description: "Semantic: security (conceptual)",
            },
            StrategyTestCase {
                query: "storing temporary data for quick access",
                expected_ids: vec!["kw-07"],
                description: "Semantic: caching (conceptual)",
            },
        ];

        let mut total_recall = 0.0;

        println!("\n========================================");
        println!("  SEMANTIC STRATEGY BENCHMARK");
        println!("========================================");

        for tc in &cases {
            let query_vec = emb.embed_query(tc.query).unwrap();
            let results = semantic::search(&lance, &query_vec, 5, None, false)
                .await
                .unwrap();

            let result_ids: Vec<String> = results.iter().map(|r| r.memory_id.clone()).collect();
            let r5 = recall_at_k(&result_ids, &tc.expected_ids, 5);
            total_recall += r5;

            let status = if r5 >= 1.0 { "HIT" } else { "MISS" };
            println!(
                "  [{status}] {}: R@5={r5:.2} (results: {:?})",
                tc.description, result_ids
            );
        }

        let avg_recall = total_recall / cases.len() as f64;

        println!("\n  Avg Recall@5: {avg_recall:.4}");
        println!("========================================\n");

        // Semantic search should handle paraphrases and conceptual queries
        assert!(
            avg_recall > 0.5,
            "Semantic Recall@5 should be > 0.5, got {avg_recall:.4}"
        );
    });
}
