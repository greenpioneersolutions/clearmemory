//! Retrieval Regression Test Suite
//!
//! This is the most critical quality gate for Clear Memory. It creates a realistic
//! corpus of 50+ memories and runs 20+ queries with known expected results.
//! The test measures MRR, Recall@5, Recall@10, and Precision@5.
//!
//! Run with: `cargo test --test retrieval_regression -- --nocapture`
//!
//! Tests that require the embedding model (semantic search) are marked #[ignore].
//! Without semantic search, only keyword + temporal + entity graph strategies are tested.

use clearmemory::entities::resolver::HeuristicResolver;
use clearmemory::migration;
use clearmemory::retrieval::merge::Strategy;
use clearmemory::retrieval::rerank::PassthroughReranker;
use clearmemory::retrieval::{self, RecallConfig};
use clearmemory::storage::lance::LanceStorage;
use rusqlite::Connection;
use std::collections::HashMap;

/// A single test case: a query and which memories should be retrieved.
struct TestCase {
    query: &'static str,
    /// Memory IDs that MUST appear in results for this query to pass.
    expected_memory_ids: Vec<&'static str>,
    description: &'static str,
}

/// A memory to insert into the test corpus.
struct TestMemory {
    id: &'static str,
    summary: &'static str,
    created_at: &'static str,
    stream_id: Option<&'static str>,
}

/// Build the test corpus: 50+ realistic memories covering diverse topics.
fn build_corpus() -> Vec<TestMemory> {
    vec![
        // Architecture decisions
        TestMemory {
            id: "arch-001",
            summary: "We decided to switch from REST to GraphQL for the public API because it reduces over-fetching and lets mobile clients request exactly the fields they need",
            created_at: "2026-01-15",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "arch-002",
            summary: "Team agreed to use PostgreSQL instead of MongoDB for the user service because we need strong ACID guarantees for financial data",
            created_at: "2026-01-20",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "arch-003",
            summary: "Architecture decision: adopt event sourcing for the order service to enable full audit trails and temporal queries on order state",
            created_at: "2026-01-22",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "arch-004",
            summary: "Decided to use Clerk for authentication instead of Auth0 based on better pricing and developer experience with React integration",
            created_at: "2026-02-01",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "arch-005",
            summary: "We chose Redis for caching over Memcached because we need support for sorted sets and pub/sub for real-time features",
            created_at: "2026-02-05",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "arch-006",
            summary: "Selected Kubernetes over Docker Swarm for container orchestration due to better auto-scaling, service mesh support, and industry adoption",
            created_at: "2026-02-10",
            stream_id: Some("infrastructure"),
        },
        TestMemory {
            id: "arch-007",
            summary: "Decision to use gRPC for internal service-to-service communication while keeping GraphQL for external API clients",
            created_at: "2026-02-12",
            stream_id: Some("platform"),
        },

        // Bug fixes
        TestMemory {
            id: "bug-001",
            summary: "Fixed the authentication middleware timeout by increasing the JWT token verification timeout from 100ms to 500ms and adding retry logic",
            created_at: "2026-02-15",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "bug-002",
            summary: "Resolved the database connection pool exhaustion issue by setting max connections to 20, idle timeout to 30s, and adding connection health checks",
            created_at: "2026-02-18",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "bug-003",
            summary: "Fixed memory leak in the WebSocket handler caused by event listeners not being cleaned up on client disconnect",
            created_at: "2026-02-20",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "bug-004",
            summary: "Resolved race condition in the payment processing pipeline where concurrent updates to the same order could result in double charging",
            created_at: "2026-02-25",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "bug-005",
            summary: "Fixed CSS rendering issue on Safari where flexbox gap property was not supported in older versions",
            created_at: "2026-03-01",
            stream_id: Some("frontend"),
        },
        TestMemory {
            id: "bug-006",
            summary: "Corrected the timezone handling in the scheduling service that was causing appointments to show at the wrong time for users in different timezones",
            created_at: "2026-03-05",
            stream_id: Some("platform"),
        },

        // Project planning
        TestMemory {
            id: "proj-001",
            summary: "Q1 migration milestones: Phase 1 (Jan) - schema migration scripts ready, Phase 2 (Feb) - dual-write mode enabled, Phase 3 (Mar) - cutover to new database",
            created_at: "2026-01-05",
            stream_id: Some("q1-migration"),
        },
        TestMemory {
            id: "proj-002",
            summary: "SOC2 audit preparation: need to document all data flows, access controls, encryption at rest, incident response procedures by end of Q1",
            created_at: "2026-01-10",
            stream_id: Some("compliance"),
        },
        TestMemory {
            id: "proj-003",
            summary: "Frontend rewrite roadmap: migrate from Create React App to Next.js with App Router for better SEO and server-side rendering support",
            created_at: "2026-01-25",
            stream_id: Some("frontend"),
        },
        TestMemory {
            id: "proj-004",
            summary: "Mobile app v2 launch plan: release beta by March 15, full launch April 1, targeting 95% crash-free rate and 4.5 star rating",
            created_at: "2026-02-01",
            stream_id: Some("mobile"),
        },
        TestMemory {
            id: "proj-005",
            summary: "API versioning strategy: we will maintain v1 and v2 simultaneously for 12 months, then deprecate v1 with 6 months notice",
            created_at: "2026-02-20",
            stream_id: Some("platform"),
        },

        // Team discussions
        TestMemory {
            id: "team-001",
            summary: "Team agreed to deprecate the old REST API v1 endpoints by June 2026 and migrate all internal consumers to GraphQL",
            created_at: "2026-02-01",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "team-002",
            summary: "Sprint retrospective: deployment pipeline too slow, need to parallelize test suites and add better caching for CI/CD builds",
            created_at: "2026-02-14",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "team-003",
            summary: "Hiring plan discussion: need 2 senior backend engineers and 1 DevOps engineer by Q2 to support the platform scaling initiative",
            created_at: "2026-02-15",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "team-004",
            summary: "Code review guidelines updated: all PRs require at least one approval, security-sensitive changes require two, maximum 400 lines per PR",
            created_at: "2026-02-28",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "team-005",
            summary: "On-call rotation discussion: switching from weekly to bi-weekly rotations, adding secondary on-call for major incidents",
            created_at: "2026-03-01",
            stream_id: Some("platform"),
        },

        // Technical details
        TestMemory {
            id: "tech-001",
            summary: "Database connection pool configuration: max_connections=20, idle_timeout=30s, max_lifetime=300s, min_connections=5 for the user service",
            created_at: "2026-02-10",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "tech-002",
            summary: "Nginx reverse proxy setup: rate limiting at 100 req/s per IP, SSL termination with Let's Encrypt, WebSocket upgrade support on /ws path",
            created_at: "2026-02-15",
            stream_id: Some("infrastructure"),
        },
        TestMemory {
            id: "tech-003",
            summary: "Docker image optimization: multi-stage builds reduced image size from 1.2GB to 180MB, using distroless base images for security",
            created_at: "2026-02-20",
            stream_id: Some("infrastructure"),
        },
        TestMemory {
            id: "tech-004",
            summary: "Prometheus monitoring setup: scrape interval 15s, retention 30 days, alerting rules for p95 latency > 200ms and error rate > 1%",
            created_at: "2026-02-25",
            stream_id: Some("infrastructure"),
        },
        TestMemory {
            id: "tech-005",
            summary: "Kafka topic configuration for the event bus: 12 partitions, replication factor 3, retention 7 days, compression type lz4",
            created_at: "2026-03-01",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "tech-006",
            summary: "GraphQL schema design: using federation with Apollo Gateway, each service owns its own subgraph, stitching happens at the gateway level",
            created_at: "2026-03-05",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "tech-007",
            summary: "Elasticsearch cluster setup: 3 data nodes, 2 master-eligible nodes, index sharding strategy with 5 primary shards per index",
            created_at: "2026-03-10",
            stream_id: Some("infrastructure"),
        },

        // Security
        TestMemory {
            id: "sec-001",
            summary: "Security audit findings: need to implement rate limiting on login endpoint, add CSRF tokens to all forms, and upgrade TLS to 1.3",
            created_at: "2026-01-30",
            stream_id: Some("compliance"),
        },
        TestMemory {
            id: "sec-002",
            summary: "Implemented API key rotation: all service-to-service API keys now rotate every 90 days automatically via Vault",
            created_at: "2026-02-10",
            stream_id: Some("compliance"),
        },
        TestMemory {
            id: "sec-003",
            summary: "PII data handling policy: encrypt all PII at rest with AES-256, mask in logs, purge after 90 days unless user opts in to retention",
            created_at: "2026-02-20",
            stream_id: Some("compliance"),
        },
        TestMemory {
            id: "sec-004",
            summary: "Penetration test results: found XSS vulnerability in user profile page, SQL injection in search endpoint, both now patched",
            created_at: "2026-03-01",
            stream_id: Some("compliance"),
        },

        // Performance
        TestMemory {
            id: "perf-001",
            summary: "Performance optimization: added Redis caching layer for user profiles, reduced p95 latency from 450ms to 35ms for the /users endpoint",
            created_at: "2026-02-12",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "perf-002",
            summary: "Database query optimization: added composite index on (user_id, created_at) for the orders table, query time dropped from 2s to 50ms",
            created_at: "2026-02-18",
            stream_id: Some("platform"),
        },
        TestMemory {
            id: "perf-003",
            summary: "Frontend bundle size reduction: code splitting with dynamic imports reduced initial load from 2.5MB to 400KB, FCP improved from 3.2s to 0.8s",
            created_at: "2026-03-01",
            stream_id: Some("frontend"),
        },
        TestMemory {
            id: "perf-004",
            summary: "Load testing results: system handles 10,000 concurrent users with p99 latency under 500ms, bottleneck is database connection pool at 15K users",
            created_at: "2026-03-05",
            stream_id: Some("platform"),
        },

        // Operations
        TestMemory {
            id: "ops-001",
            summary: "Incident report: 45-minute outage on Feb 22 caused by certificate expiration on the API gateway, implementing automated cert renewal",
            created_at: "2026-02-22",
            stream_id: Some("infrastructure"),
        },
        TestMemory {
            id: "ops-002",
            summary: "Backup strategy: daily database snapshots to S3, point-in-time recovery enabled with 7-day retention, cross-region replication for disaster recovery",
            created_at: "2026-02-25",
            stream_id: Some("infrastructure"),
        },
        TestMemory {
            id: "ops-003",
            summary: "CI/CD pipeline improvements: parallel test execution cut build time from 20 minutes to 7 minutes, added automatic rollback on failed health checks",
            created_at: "2026-03-01",
            stream_id: Some("infrastructure"),
        },

        // Data & Analytics
        TestMemory {
            id: "data-001",
            summary: "Data pipeline architecture: Kafka -> Flink for real-time processing, Kafka -> S3 -> Spark for batch analytics, data warehouse on BigQuery",
            created_at: "2026-02-15",
            stream_id: Some("data"),
        },
        TestMemory {
            id: "data-002",
            summary: "A/B testing framework: using Statsig for feature flags and experimentation, minimum 2 weeks per experiment with 95% statistical significance",
            created_at: "2026-02-20",
            stream_id: Some("data"),
        },
        TestMemory {
            id: "data-003",
            summary: "User analytics dashboard shows 32% month-over-month growth in active users, retention rate at 68% after 30 days, NPS score improved to 72",
            created_at: "2026-03-01",
            stream_id: Some("data"),
        },

        // Design & UX
        TestMemory {
            id: "ux-001",
            summary: "Design system update: migrating from Material UI to Radix primitives with custom Tailwind styling for better accessibility and smaller bundle",
            created_at: "2026-02-10",
            stream_id: Some("frontend"),
        },
        TestMemory {
            id: "ux-002",
            summary: "User research findings: onboarding flow has 40% drop-off at step 3, need to simplify the workspace creation process",
            created_at: "2026-02-20",
            stream_id: Some("frontend"),
        },
        TestMemory {
            id: "ux-003",
            summary: "Dark mode implementation: using CSS custom properties with prefers-color-scheme media query, persisting preference in localStorage",
            created_at: "2026-03-05",
            stream_id: Some("frontend"),
        },

        // Testing
        TestMemory {
            id: "test-001",
            summary: "Testing strategy update: adopt Playwright for E2E tests replacing Cypress, add visual regression testing with Chromatic for Storybook components",
            created_at: "2026-02-15",
            stream_id: Some("frontend"),
        },
        TestMemory {
            id: "test-002",
            summary: "Integration test coverage now at 85%, added contract tests between API gateway and downstream services using Pact framework",
            created_at: "2026-03-01",
            stream_id: Some("platform"),
        },
    ]
}

/// Build the test cases: queries with known expected results.
fn build_test_cases() -> Vec<TestCase> {
    vec![
        // Exact topic queries
        TestCase {
            query: "why did we switch to GraphQL",
            expected_memory_ids: vec!["arch-001"],
            description: "Direct architecture decision query",
        },
        TestCase {
            query: "authentication Clerk decision",
            expected_memory_ids: vec!["arch-004"],
            description: "Auth provider decision",
        },
        TestCase {
            query: "database connection pool configuration",
            expected_memory_ids: vec!["tech-001", "bug-002"],
            description: "Technical config query should find config and related bug fix",
        },
        TestCase {
            query: "JWT token timeout fix",
            expected_memory_ids: vec!["bug-001"],
            description: "Specific bug fix query",
        },
        TestCase {
            query: "Q1 migration plan milestones",
            expected_memory_ids: vec!["proj-001"],
            description: "Project planning query",
        },
        TestCase {
            query: "REST API deprecation",
            expected_memory_ids: vec!["team-001"],
            description: "Team decision about API deprecation",
        },
        TestCase {
            query: "Kubernetes container orchestration",
            expected_memory_ids: vec!["arch-006"],
            description: "Infrastructure decision query",
        },
        TestCase {
            query: "Redis caching performance improvement",
            expected_memory_ids: vec!["perf-001", "arch-005"],
            description: "Performance optimization with caching",
        },
        TestCase {
            query: "security audit findings vulnerabilities",
            expected_memory_ids: vec!["sec-001", "sec-004"],
            description: "Security-related query",
        },
        TestCase {
            query: "PII data handling encryption policy",
            expected_memory_ids: vec!["sec-003"],
            description: "Data privacy policy query",
        },
        TestCase {
            query: "frontend bundle size optimization",
            expected_memory_ids: vec!["perf-003"],
            description: "Frontend performance query",
        },
        TestCase {
            query: "Docker image optimization multi-stage build",
            expected_memory_ids: vec!["tech-003"],
            description: "Docker optimization query",
        },
        TestCase {
            query: "payment processing race condition double charging",
            expected_memory_ids: vec!["bug-004"],
            description: "Specific bug about payments",
        },
        TestCase {
            query: "CI/CD pipeline build time improvement",
            expected_memory_ids: vec!["ops-003", "team-002"],
            description: "CI/CD related query",
        },
        TestCase {
            query: "certificate expiration outage incident",
            expected_memory_ids: vec!["ops-001"],
            description: "Incident query",
        },
        TestCase {
            query: "Kafka topic configuration event bus",
            expected_memory_ids: vec!["tech-005"],
            description: "Kafka configuration query",
        },
        TestCase {
            query: "user onboarding drop-off research",
            expected_memory_ids: vec!["ux-002"],
            description: "UX research query",
        },
        TestCase {
            query: "GraphQL federation Apollo schema",
            expected_memory_ids: vec!["tech-006"],
            description: "GraphQL implementation details",
        },
        TestCase {
            query: "A/B testing experimentation framework",
            expected_memory_ids: vec!["data-002"],
            description: "Testing and experimentation query",
        },
        TestCase {
            query: "SOC2 audit compliance documentation",
            expected_memory_ids: vec!["proj-002"],
            description: "Compliance project query",
        },
        TestCase {
            query: "WebSocket memory leak event listener cleanup",
            expected_memory_ids: vec!["bug-003"],
            description: "Specific WebSocket bug",
        },
        TestCase {
            query: "code review guidelines PR approval",
            expected_memory_ids: vec!["team-004"],
            description: "Process and guidelines query",
        },
        TestCase {
            query: "Prometheus monitoring alerting rules",
            expected_memory_ids: vec!["tech-004"],
            description: "Monitoring setup query",
        },
        TestCase {
            query: "load testing concurrent users bottleneck",
            expected_memory_ids: vec!["perf-004"],
            description: "Load testing results query",
        },
        TestCase {
            query: "API key rotation Vault automatic",
            expected_memory_ids: vec!["sec-002"],
            description: "Secret rotation query",
        },
    ]
}

/// Insert the corpus into SQLite.
fn setup_corpus(conn: &Connection, corpus: &[TestMemory]) {
    migration::runner::run_migrations(conn).unwrap();

    for mem in corpus {
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at, stream_id) \
             VALUES (?1, ?2, ?3, 'clear', ?4, ?5)",
            rusqlite::params![
                mem.id,
                format!("hash_{}", mem.id),
                mem.summary,
                mem.created_at,
                mem.stream_id,
            ],
        )
        .unwrap();
    }
}

/// Compute Mean Reciprocal Rank across all test cases.
fn compute_mrr(results: &[(usize, Vec<String>)], test_cases: &[TestCase]) -> f64 {
    let mut rr_sum = 0.0;
    let mut count = 0;

    for (i, (_, result_ids)) in results.iter().enumerate() {
        let expected = &test_cases[i].expected_memory_ids;
        // Find the rank of the first expected memory in the results
        let mut best_rank = None;
        for exp_id in expected {
            if let Some(pos) = result_ids.iter().position(|r| r == exp_id) {
                best_rank = Some(match best_rank {
                    Some(prev) => std::cmp::min(prev, pos),
                    None => pos,
                });
            }
        }
        if let Some(rank) = best_rank {
            rr_sum += 1.0 / (rank as f64 + 1.0);
        }
        count += 1;
    }

    if count == 0 {
        return 0.0;
    }
    rr_sum / count as f64
}

/// Compute Recall@K: fraction of expected memories found in top-K results.
fn compute_recall_at_k(results: &[(usize, Vec<String>)], test_cases: &[TestCase], k: usize) -> f64 {
    let mut total_expected = 0;
    let mut total_found = 0;

    for (i, (_, result_ids)) in results.iter().enumerate() {
        let expected = &test_cases[i].expected_memory_ids;
        let top_k: Vec<&String> = result_ids.iter().take(k).collect();
        for exp_id in expected {
            total_expected += 1;
            if top_k.iter().any(|r| r.as_str() == *exp_id) {
                total_found += 1;
            }
        }
    }

    if total_expected == 0 {
        return 0.0;
    }
    total_found as f64 / total_expected as f64
}

/// Compute Precision@K: fraction of top-K results that are expected.
fn compute_precision_at_k(
    results: &[(usize, Vec<String>)],
    test_cases: &[TestCase],
    k: usize,
) -> f64 {
    let mut precision_sum = 0.0;
    let mut count = 0;

    for (i, (_, result_ids)) in results.iter().enumerate() {
        let expected = &test_cases[i].expected_memory_ids;
        let top_k: Vec<&String> = result_ids.iter().take(k).collect();
        if top_k.is_empty() {
            continue;
        }
        let hits = top_k
            .iter()
            .filter(|r| expected.iter().any(|e| e == &r.as_str()))
            .count();
        precision_sum += hits as f64 / top_k.len() as f64;
        count += 1;
    }

    if count == 0 {
        return 0.0;
    }
    precision_sum / count as f64
}

/// Run the keyword-only retrieval regression test.
/// This test does NOT require the embedding model and validates that keyword
/// search can find memories based on term overlap.
#[test]
fn test_keyword_retrieval_regression() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let conn = Connection::open_in_memory().unwrap();
        let corpus = build_corpus();
        setup_corpus(&conn, &corpus);

        let test_cases = build_test_cases();
        let dir = tempfile::tempdir().unwrap();
        let lance = LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();
        let resolver = HeuristicResolver;
        let reranker = PassthroughReranker;

        let summaries: HashMap<String, String> = corpus
            .iter()
            .map(|m| (m.id.to_string(), m.summary.to_string()))
            .collect();

        let config = RecallConfig {
            top_k: 10,
            temporal_boost: 0.4,
            entity_boost: 0.3,
            include_archived: false,
            stream_id: None,
        };

        let mut results: Vec<(usize, Vec<String>)> = Vec::new();
        let mut strategy_contributions: HashMap<Strategy, usize> = HashMap::new();

        for (i, tc) in test_cases.iter().enumerate() {
            let result = retrieval::recall(
                tc.query, &conn, &lance, None, // No embedding -- keyword only
                &resolver, &reranker, &summaries, &config,
            )
            .await
            .unwrap();

            let result_ids: Vec<String> =
                result.results.iter().map(|r| r.memory_id.clone()).collect();

            // Track strategy contributions
            for (strategy, count) in &result.strategy_counts {
                *strategy_contributions.entry(*strategy).or_insert(0) += count;
            }

            // Check if expected memories are found
            let missing: Vec<&str> = tc
                .expected_memory_ids
                .iter()
                .filter(|exp| !result_ids.iter().any(|r| r == **exp))
                .copied()
                .collect();

            if !missing.is_empty() {
                println!(
                    "  [MISS] Case {}: \"{}\" -- missing: {:?}",
                    i, tc.description, missing
                );
            }

            results.push((i, result_ids));
        }

        // Compute metrics
        let mrr = compute_mrr(&results, &test_cases);
        let recall_5 = compute_recall_at_k(&results, &test_cases, 5);
        let recall_10 = compute_recall_at_k(&results, &test_cases, 10);
        let precision_5 = compute_precision_at_k(&results, &test_cases, 5);

        println!("\n========================================");
        println!("  RETRIEVAL REGRESSION RESULTS (Keyword Only)");
        println!("========================================");
        println!("  Corpus size:    {} memories", corpus.len());
        println!("  Test cases:     {}", test_cases.len());
        println!("  MRR:            {mrr:.4}");
        println!("  Recall@5:       {recall_5:.4}");
        println!("  Recall@10:      {recall_10:.4}");
        println!("  Precision@5:    {precision_5:.4}");
        println!("\n  Strategy contributions:");
        for (strategy, count) in &strategy_contributions {
            println!("    {strategy:?}: {count} results");
        }
        println!("========================================\n");

        // Quality assertions -- keyword search alone should find a reasonable
        // fraction of results since our test queries include key terms
        assert!(
            recall_10 > 0.3,
            "Recall@10 should be > 0.3 for keyword search, got {recall_10:.4}"
        );
        assert!(
            mrr > 0.2,
            "MRR should be > 0.2 for keyword search, got {mrr:.4}"
        );
    });
}

/// Full retrieval regression test with semantic search (requires embedding model).
/// Run with: `cargo test --test retrieval_regression test_full_retrieval_regression -- --nocapture --ignored`
#[test]
#[ignore]
fn test_full_retrieval_regression() {
    use clearmemory::storage::embeddings::EmbeddingManager;

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // Load embedding model
        let emb = EmbeddingManager::new("bge-small-en").expect("failed to load embedding model");

        let conn = Connection::open_in_memory().unwrap();
        let corpus = build_corpus();
        setup_corpus(&conn, &corpus);

        let test_cases = build_test_cases();
        let dir = tempfile::tempdir().unwrap();
        let lance = LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();

        // Insert embeddings for all memories
        println!("Embedding {} memories...", corpus.len());
        for mem in &corpus {
            let vector = emb.embed_query(mem.summary).unwrap();
            lance.insert(mem.id, &vector, mem.stream_id).await.unwrap();
        }
        println!(
            "Embedded {} memories, {} vectors stored",
            corpus.len(),
            lance.vector_count().await.unwrap()
        );

        let resolver = HeuristicResolver;
        let reranker = PassthroughReranker;

        let summaries: HashMap<String, String> = corpus
            .iter()
            .map(|m| (m.id.to_string(), m.summary.to_string()))
            .collect();

        let config = RecallConfig {
            top_k: 10,
            temporal_boost: 0.4,
            entity_boost: 0.3,
            include_archived: false,
            stream_id: None,
        };

        let mut results: Vec<(usize, Vec<String>)> = Vec::new();
        let mut strategy_contributions: HashMap<Strategy, usize> = HashMap::new();

        for (i, tc) in test_cases.iter().enumerate() {
            // Generate query embedding
            let query_vec = emb.embed_query(tc.query).unwrap();

            let result = retrieval::recall(
                tc.query,
                &conn,
                &lance,
                Some(&query_vec),
                &resolver,
                &reranker,
                &summaries,
                &config,
            )
            .await
            .unwrap();

            let result_ids: Vec<String> =
                result.results.iter().map(|r| r.memory_id.clone()).collect();

            for (strategy, count) in &result.strategy_counts {
                *strategy_contributions.entry(*strategy).or_insert(0) += count;
            }

            let found: Vec<&str> = tc
                .expected_memory_ids
                .iter()
                .filter(|exp| result_ids.iter().any(|r| r == **exp))
                .copied()
                .collect();

            let missing: Vec<&str> = tc
                .expected_memory_ids
                .iter()
                .filter(|exp| !result_ids.iter().any(|r| r == **exp))
                .copied()
                .collect();

            if !missing.is_empty() {
                println!(
                    "  [MISS] Case {}: \"{}\" -- missing: {:?} (found: {:?})",
                    i, tc.description, missing, found
                );
            } else {
                println!(
                    "  [HIT]  Case {}: \"{}\" -- all found in top {}",
                    i,
                    tc.description,
                    result_ids.len()
                );
            }

            results.push((i, result_ids));
        }

        let mrr = compute_mrr(&results, &test_cases);
        let recall_5 = compute_recall_at_k(&results, &test_cases, 5);
        let recall_10 = compute_recall_at_k(&results, &test_cases, 10);
        let precision_5 = compute_precision_at_k(&results, &test_cases, 5);

        println!("\n========================================");
        println!("  RETRIEVAL REGRESSION RESULTS (Full Pipeline)");
        println!("========================================");
        println!("  Corpus size:    {} memories", corpus.len());
        println!("  Test cases:     {}", test_cases.len());
        println!("  MRR:            {mrr:.4}");
        println!("  Recall@5:       {recall_5:.4}");
        println!("  Recall@10:      {recall_10:.4}");
        println!("  Precision@5:    {precision_5:.4}");
        println!("\n  Strategy contributions:");
        for (strategy, count) in &strategy_contributions {
            println!("    {strategy:?}: {count} results");
        }
        println!("========================================\n");

        // With semantic search, quality should be significantly higher
        assert!(
            recall_10 > 0.6,
            "Recall@10 with semantic search should be > 0.6, got {recall_10:.4}"
        );
        assert!(
            mrr > 0.4,
            "MRR with semantic search should be > 0.4, got {mrr:.4}"
        );
        assert!(
            recall_5 > 0.5,
            "Recall@5 with semantic search should be > 0.5, got {recall_5:.4}"
        );
    });
}
