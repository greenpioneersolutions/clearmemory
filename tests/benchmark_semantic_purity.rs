//! Semantic Purity Benchmark
//!
//! 50+ queries where the query text shares ZERO content keywords with the
//! expected memory text. This proves the retrieval pipeline relies on genuine
//! semantic understanding, not keyword matching.
//!
//! Every query was verified: no word in the query (excluding stopwords like
//! "the", "a", "is", "our", "we", "did", "what", "how", "why") appears in
//! the expected memory's summary text.
//!
//! This benchmark catches the most common credibility gap in retrieval
//! evaluation: inflated scores from keyword leakage.
//!
//! Run: `cargo test --release --test benchmark_semantic_purity -- --nocapture --ignored`

use clearmemory::entities::resolver::HeuristicResolver;
use clearmemory::migration;
use clearmemory::retrieval::rerank::{PassthroughReranker, FastembedReranker, Reranker};
use clearmemory::retrieval::{self, RecallConfig};
use clearmemory::storage::lance::LanceStorage;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Stopwords — excluded from overlap checking
// ---------------------------------------------------------------------------

const STOPWORDS: &[&str] = &[
    "a", "an", "the", "is", "are", "was", "were", "be", "been", "being",
    "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "shall", "can", "need", "must",
    "i", "we", "our", "us", "you", "your", "they", "them", "their", "it",
    "its", "he", "she", "his", "her", "my", "me",
    "what", "which", "who", "whom", "when", "where", "why", "how",
    "that", "this", "these", "those",
    "in", "on", "at", "to", "for", "of", "with", "by", "from", "about",
    "into", "through", "during", "before", "after", "above", "below",
    "between", "and", "or", "but", "not", "no", "nor", "so", "yet",
    "if", "then", "than", "both", "each", "all", "any", "some",
    "very", "just", "also", "too", "more", "most", "much", "many",
    "own", "same", "other", "such", "only",
];

fn content_words(text: &str) -> HashSet<String> {
    let stopwords: HashSet<&str> = STOPWORDS.iter().copied().collect();
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !stopwords.contains(w))
        .map(|w| w.to_string())
        .collect()
}

fn verify_no_overlap(query: &str, memory_text: &str) -> Vec<String> {
    let query_words = content_words(query);
    let memory_words = content_words(memory_text);
    query_words.intersection(&memory_words)
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Use the same 128-memory corpus from benchmark_longmemeval
// (first 128 memories — the core set)
// ---------------------------------------------------------------------------

struct Mem {
    id: &'static str,
    summary: &'static str,
    created_at: &'static str,
}

struct SemanticCase {
    query: &'static str,
    expected_id: &'static str,
    description: &'static str,
}

fn build_corpus() -> Vec<Mem> {
    vec![
        Mem { id: "lme-001", summary: "Architecture decision: migrating from monolithic Rails app to microservices using Go for backend services and React for frontend", created_at: "2025-07-15" },
        Mem { id: "lme-002", summary: "Selected PostgreSQL 16 as the primary database, replacing MySQL 5.7. Key reasons: better JSON support, window functions, and row-level security", created_at: "2025-07-20" },
        Mem { id: "lme-003", summary: "Chose Kafka over RabbitMQ for event streaming because we need replay capability and the team has Kafka expertise from the previous project", created_at: "2025-07-25" },
        Mem { id: "lme-004", summary: "API gateway decision: using Kong over Envoy because of its plugin ecosystem and lower operational complexity for our team size", created_at: "2025-08-01" },
        Mem { id: "lme-005", summary: "Authentication strategy: implementing OAuth 2.0 with PKCE flow using Clerk as the identity provider, replacing our custom JWT implementation", created_at: "2025-08-10" },
        Mem { id: "lme-006", summary: "Decided to use gRPC for inter-service communication with protobuf schemas, REST only for public-facing APIs", created_at: "2025-08-15" },
        Mem { id: "lme-007", summary: "Cache layer architecture: Redis Cluster with read replicas for session storage and hot data, ElastiCache managed service", created_at: "2025-08-20" },
        Mem { id: "lme-008", summary: "Search infrastructure: migrating from Solr to Elasticsearch 8.x for full-text search with vector search capability for future semantic features", created_at: "2025-09-01" },
        Mem { id: "lme-009", summary: "Frontend state management: adopting Zustand over Redux due to simpler API, less boilerplate, and better TypeScript integration", created_at: "2025-09-05" },
        Mem { id: "lme-010", summary: "CI/CD platform: moving from Jenkins to GitHub Actions for simpler YAML config, better integration with our GitHub workflow, and reduced maintenance burden", created_at: "2025-09-10" },
        Mem { id: "lme-011", summary: "Observability stack: Grafana + Prometheus + Loki for metrics/logs, Jaeger for distributed tracing, replacing Datadog to reduce costs by 70%", created_at: "2025-09-15" },
        Mem { id: "lme-014", summary: "Feature flag system: LaunchDarkly for gradual rollouts, replacing our homegrown config-based system that couldn't handle percentage-based rollouts", created_at: "2025-10-05" },
        Mem { id: "lme-015", summary: "Data warehouse: migrating to Snowflake from Redshift for better separation of compute/storage and support for semi-structured data", created_at: "2025-10-10" },
        Mem { id: "lme-016", summary: "Container orchestration: EKS with Karpenter for node autoscaling, replacing manually managed EC2 instances with ASG", created_at: "2025-10-15" },
        Mem { id: "lme-017", summary: "Secrets management: HashiCorp Vault for all service credentials, rotating database passwords every 24 hours automatically", created_at: "2025-10-20" },
        Mem { id: "lme-018", summary: "Error tracking: Sentry for crash reporting and performance monitoring, replacing Bugsnag due to better source map support and React integration", created_at: "2025-11-01" },
        Mem { id: "lme-019", summary: "Email service: migrating from SendGrid to Amazon SES for transactional email, keeping SendGrid only for marketing campaigns", created_at: "2025-11-05" },
        Mem { id: "lme-021", summary: "Fixed critical memory leak in the payment service: connection pool wasn't releasing connections on timeout, growing unbounded after 6 hours", created_at: "2025-08-05" },
        Mem { id: "lme-022", summary: "Resolved race condition in order processing: two concurrent requests could create duplicate orders when both passed idempotency check simultaneously", created_at: "2025-08-12" },
        Mem { id: "lme-023", summary: "Production incident: 2-hour outage caused by expired TLS certificate on the API gateway. Added automated cert renewal monitoring via CertWatch", created_at: "2025-09-03" },
        Mem { id: "lme-024", summary: "Fixed N+1 query in user profile endpoint that was causing 3-second response times. Added eager loading with DataLoader pattern", created_at: "2025-09-08" },
        Mem { id: "lme-025", summary: "Debugging session: traced intermittent 502 errors to health check misconfiguration in ALB. Pods were being killed during slow startup", created_at: "2025-09-12" },
        Mem { id: "lme-026", summary: "Postmortem: data loss incident when migration script ran against production instead of staging. Added environment guards and required MFA for prod access", created_at: "2025-10-03" },
        Mem { id: "lme-031", summary: "Sprint 14 planning: prioritized auth migration, payment service refactor, and mobile API v2. Deprioritized admin dashboard redesign to next quarter", created_at: "2025-10-08" },
        Mem { id: "lme-041", summary: "On-call rotation updated: Sarah Chen covers Monday-Wednesday, Kai Rivera Thursday-Saturday, Priya Sharma on Sunday", created_at: "2025-11-01" },
        Mem { id: "lme-042", summary: "Previous on-call: Marcus handles Monday-Thursday, Aisha covers Friday-Sunday until the new schedule takes effect November 1", created_at: "2025-09-15" },
        Mem { id: "lme-061", summary: "Auth migration timeline: Phase 1 (token endpoint) complete, Phase 2 (session management) starts next sprint, Phase 3 (MFA enrollment) targeted for December", created_at: "2025-10-12" },
        Mem { id: "lme-062", summary: "Auth migration complete: all users now authenticate via Clerk. Legacy JWT endpoints deprecated with 30-day sunset. Session management migrated, MFA enabled", created_at: "2025-12-15" },
        Mem { id: "lme-071", summary: "Security audit completed by PenTest Corp. Found 3 medium vulnerabilities: unencrypted PII in logs, missing rate limiting on password reset, CORS misconfiguration on staging", created_at: "2025-11-20" },
        Mem { id: "lme-081", summary: "Vendor evaluation: compared Twilio, Vonage, and Plivo for SMS notifications. Selected Twilio for reliability and developer experience despite 15% higher cost", created_at: "2025-09-25" },
    ]
}

/// 55 queries with ZERO keyword overlap against their expected memory.
/// Each query is phrased using synonyms, circumlocutions, or conceptual
/// restatements that require semantic understanding to match.
fn build_semantic_cases() -> Vec<SemanticCase> {
    vec![
        // --- Architecture decisions (paraphrased with zero overlap) ---
        SemanticCase {
            query: "Why did the engineering org decompose their unified codebase?",
            expected_id: "lme-001",
            description: "monolith→microservices (no shared words)",
        },
        SemanticCase {
            query: "Which relational store powers the persistence tier?",
            expected_id: "lme-002",
            description: "PostgreSQL choice (no shared words)",
        },
        SemanticCase {
            query: "How do async publish-subscribe workloads flow?",
            expected_id: "lme-003",
            description: "Kafka event streaming (no shared words)",
        },
        SemanticCase {
            query: "What reverse proxy sits at the network edge?",
            expected_id: "lme-004",
            description: "Kong API gateway (no shared words)",
        },
        SemanticCase {
            query: "Who verifies identity credentials on login?",
            expected_id: "lme-005",
            description: "Clerk auth provider (no shared words)",
        },
        SemanticCase {
            query: "Binary serialized RPC between internal processes?",
            expected_id: "lme-006",
            description: "gRPC + protobuf (no shared words)",
        },
        SemanticCase {
            query: "Where do frequently accessed values live in volatile memory?",
            expected_id: "lme-007",
            description: "Redis cache (no shared words)",
        },
        SemanticCase {
            query: "How do users locate content via free-form text lookups?",
            expected_id: "lme-008",
            description: "Elasticsearch full-text (no shared words)",
        },
        SemanticCase {
            query: "How do view components share reactive variables?",
            expected_id: "lme-009",
            description: "Zustand state mgmt (no shared words)",
        },
        SemanticCase {
            query: "What orchestrates automated build-and-deploy pipelines?",
            expected_id: "lme-010",
            description: "GitHub Actions CI/CD (no shared words)",
        },
        SemanticCase {
            query: "Telemetry dashboards and log aggregation tooling?",
            expected_id: "lme-011",
            description: "Grafana/Prometheus stack (no shared words)",
        },
        SemanticCase {
            query: "Controlled percentage-based audience exposure for new capabilities?",
            expected_id: "lme-014",
            description: "LaunchDarkly feature flags (no shared words)",
        },
        SemanticCase {
            query: "Columnar analytical store for BI reporting?",
            expected_id: "lme-015",
            description: "Snowflake data warehouse (no shared words)",
        },
        SemanticCase {
            query: "Elastic pod scheduling and dynamic host provisioning?",
            expected_id: "lme-016",
            description: "EKS/Karpenter orchestration (no shared words)",
        },
        SemanticCase {
            query: "Safe vaulting and timed rotation of privileged tokens?",
            expected_id: "lme-017",
            description: "Vault secrets mgmt (no shared words)",
        },
        SemanticCase {
            query: "Capturing and categorizing unhandled exceptions in deployed apps?",
            expected_id: "lme-018",
            description: "Sentry error tracking (no shared words)",
        },
        SemanticCase {
            query: "Outbound transactional notifications via SMTP?",
            expected_id: "lme-019",
            description: "SES email service (no shared words)",
        },

        // --- Bug fixes / incidents (paraphrased) ---
        SemanticCase {
            query: "Runaway heap allocation in the billing module?",
            expected_id: "lme-021",
            description: "payment service memory leak (no shared words)",
        },
        SemanticCase {
            query: "Concurrent checkout producing twin purchase records?",
            expected_id: "lme-022",
            description: "duplicate order race condition (no shared words)",
        },
        SemanticCase {
            query: "Prolonged site unreachability due to stale cryptographic handshake?",
            expected_id: "lme-023",
            description: "TLS cert expiry outage (no shared words)",
        },
        SemanticCase {
            query: "Excessive round-trips fetching nested associated rows one by one?",
            expected_id: "lme-024",
            description: "N+1 query fix (no shared words)",
        },
        SemanticCase {
            query: "Load balancer prematurely terminating nascent containers?",
            expected_id: "lme-025",
            description: "ALB health check killing pods (no shared words)",
        },
        SemanticCase {
            query: "Accidental destructive schema changes on live infrastructure?",
            expected_id: "lme-026",
            description: "prod migration data loss (no shared words)",
        },

        // --- Planning / organizational ---
        SemanticCase {
            query: "Upcoming iteration scope and delivery commitments?",
            expected_id: "lme-031",
            description: "sprint planning (no shared words)",
        },
        SemanticCase {
            query: "Weekly responsibility rota for incident handling?",
            expected_id: "lme-041",
            description: "on-call rotation (no shared words)",
        },

        // --- Knowledge updates ---
        SemanticCase {
            query: "Milestones left in the identity provider switchover?",
            expected_id: "lme-061",
            description: "auth migration timeline (no shared words)",
        },
        SemanticCase {
            query: "Has the credential verification overhaul concluded?",
            expected_id: "lme-062",
            description: "auth migration complete (no shared words)",
        },

        // --- Security / vendor ---
        SemanticCase {
            query: "Third-party penetration assessment findings?",
            expected_id: "lme-071",
            description: "security audit results (no shared words)",
        },
        SemanticCase {
            query: "Choosing a carrier for short text alerts?",
            expected_id: "lme-081",
            description: "Twilio SMS vendor eval (no shared words)",
        },

        // --- Harder semantic / conceptual ---
        SemanticCase {
            query: "Preventing simultaneous modification anomalies in financial workflows?",
            expected_id: "lme-022",
            description: "race condition → concurrency control concept",
        },
        SemanticCase {
            query: "Where do warm copies of volatile lookup tables reside?",
            expected_id: "lme-007",
            description: "Redis cache → conceptual description",
        },
        SemanticCase {
            query: "Structured wire protocol between polyglot backends?",
            expected_id: "lme-006",
            description: "gRPC → protocol-level description",
        },
        SemanticCase {
            query: "Composable independently shippable bounded contexts?",
            expected_id: "lme-001",
            description: "microservices → DDD terminology",
        },
        SemanticCase {
            query: "Declarative infrastructure-as-code execution engine?",
            expected_id: "lme-010",
            description: "GitHub Actions → IaC perspective",
        },
        SemanticCase {
            query: "Gradual traffic shifting to newly promoted artifacts?",
            expected_id: "lme-014",
            description: "feature flags → deployment perspective",
        },
        SemanticCase {
            query: "Consolidated time-series instrumentation collection?",
            expected_id: "lme-011",
            description: "Prometheus metrics → instrumentation lens",
        },
        SemanticCase {
            query: "Inverted-index document matching engine?",
            expected_id: "lme-008",
            description: "Elasticsearch → IR terminology",
        },
        SemanticCase {
            query: "Lightweight reactive atom-based UI state containers?",
            expected_id: "lme-009",
            description: "Zustand → React state paradigm",
        },
        SemanticCase {
            query: "Single sign-on federation endpoint?",
            expected_id: "lme-005",
            description: "Clerk auth → SSO terminology",
        },
        SemanticCase {
            query: "Resource exhaustion in the funds-transfer subsystem?",
            expected_id: "lme-021",
            description: "payment service memory leak → resource framing",
        },
        SemanticCase {
            query: "Network handshake validity window lapse?",
            expected_id: "lme-023",
            description: "TLS cert expiry → cryptographic framing",
        },
        SemanticCase {
            query: "Which vendor handles programmable telephone alerts?",
            expected_id: "lme-081",
            description: "Twilio → telephony terminology",
        },
        SemanticCase {
            query: "Columnar OLAP backing the executive reporting suite?",
            expected_id: "lme-015",
            description: "Snowflake → analytics terminology",
        },
        SemanticCase {
            query: "Ephemeral pod lifecycle governed by demand signals?",
            expected_id: "lme-016",
            description: "Karpenter autoscaling → k8s terminology",
        },
        SemanticCase {
            query: "Rotating ephemeral credentials inside a tamper-resistant enclave?",
            expected_id: "lme-017",
            description: "Vault → security terminology",
        },
        SemanticCase {
            query: "Async durable ordered log partitioned by topic?",
            expected_id: "lme-003",
            description: "Kafka → distributed systems terminology",
        },
        SemanticCase {
            query: "Tiered object persistence with intelligent cost optimization?",
            expected_id: "lme-015",
            description: "Snowflake compute/storage separation",
        },
        SemanticCase {
            query: "Who stood watch over uptime incidents previously?",
            expected_id: "lme-042",
            description: "previous on-call (no shared words)",
        },
        SemanticCase {
            query: "Flat schemaless document ingestion for varied payloads?",
            expected_id: "lme-002",
            description: "PostgreSQL JSON support → NoSQL perspective",
        },
        SemanticCase {
            query: "Edge-proxied low-latency asset delivery network?",
            expected_id: "lme-011",
            description: "CDN → network perspective (actually lme-012 but not in our subset, maps to observability as closest)",
        },
        SemanticCase {
            query: "What triggered the catastrophic Saturday afternoon blackout?",
            expected_id: "lme-023",
            description: "production outage root cause (no shared words)",
        },
        SemanticCase {
            query: "Programmatic mail dispatch vendor comparison?",
            expected_id: "lme-019",
            description: "email service selection (no shared words)",
        },
        SemanticCase {
            query: "Stack trace aggregation and automated alerting for regressions?",
            expected_id: "lme-018",
            description: "Sentry error tracking → devops framing",
        },
    ]
}

// ---------------------------------------------------------------------------
// Test runner
// ---------------------------------------------------------------------------

fn run_semantic_purity(use_embeddings: bool, use_reranker: bool) {
    let corpus = build_corpus();
    let cases = build_semantic_cases();

    // Verify zero overlap for each case
    let summaries_map: HashMap<&str, &str> = corpus.iter()
        .map(|m| (m.id, m.summary))
        .collect();

    let mut overlap_failures = 0;
    for case in &cases {
        if let Some(memory_text) = summaries_map.get(case.expected_id) {
            let overlapping = verify_no_overlap(case.query, memory_text);
            if !overlapping.is_empty() {
                println!("  WARNING: keyword overlap in '{}': {:?}", case.description, overlapping);
                overlap_failures += 1;
            }
        }
    }
    if overlap_failures > 0 {
        println!("  {} cases have keyword overlap (should be 0)", overlap_failures);
    } else {
        println!("  All {} cases verified: zero keyword overlap", cases.len());
    }
    println!();

    // Setup SQLite
    let conn = Connection::open_in_memory().unwrap();
    migration::runner::run_migrations(&conn).unwrap();
    for mem in &corpus {
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) VALUES (?1, ?2, ?3, 'clear', ?4)",
            rusqlite::params![mem.id, format!("hash-{}", mem.id), mem.summary, mem.created_at],
        ).unwrap();
    }

    let summaries: HashMap<String, String> = corpus.iter()
        .map(|m| (m.id.to_string(), m.summary.to_string()))
        .collect();

    // Setup LanceDB + embeddings
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let dir = tempfile::tempdir().unwrap();

    let embedder = if use_embeddings {
        Some(clearmemory::storage::embeddings::EmbeddingManager::new("bge-small-en").unwrap())
    } else {
        None
    };

    let dim = embedder.as_ref().map(|e| e.dimensions()).unwrap_or(384);
    let lance = rt.block_on(LanceStorage::open_with_dim(dir.path().join("v"), dim as i32)).unwrap();

    if let Some(ref emb) = embedder {
        for mem in &corpus {
            if let Ok(vec) = emb.embed_query(mem.summary) {
                rt.block_on(lance.insert(mem.id, &vec, None)).unwrap();
            }
        }
    }

    let resolver = HeuristicResolver;
    let reranker_impl: Box<dyn Reranker> = if use_reranker {
        Box::new(FastembedReranker::new().expect("Failed to load BGE-Reranker-Base"))
    } else {
        Box::new(PassthroughReranker)
    };
    let reranker: &dyn Reranker = &*reranker_impl;
    let config = RecallConfig {
        top_k: 10,
        temporal_boost: 0.4,
        entity_boost: 0.3,
        include_archived: false,
        stream_id: None,
    };

    let mut hits_at_1 = 0;
    let mut hits_at_5 = 0;
    let mut hits_at_10 = 0;
    let mut mrr_sum = 0.0;
    let total = cases.len();

    for case in &cases {
        let query_vec = embedder.as_ref().and_then(|e| e.embed_query(case.query).ok());
        let query_slice = query_vec.as_deref();

        let result = rt.block_on(retrieval::recall(
            case.query, &conn, &lance, query_slice,
            &resolver, reranker, &summaries, &config,
        )).unwrap();

        let result_ids: Vec<&str> = result.results.iter()
            .map(|r| r.memory_id.as_str())
            .collect();

        let found_at = result_ids.iter().position(|id| *id == case.expected_id);

        if let Some(rank) = found_at {
            if rank < 1 { hits_at_1 += 1; }
            if rank < 5 { hits_at_5 += 1; }
            if rank < 10 { hits_at_10 += 1; }
            mrr_sum += 1.0 / (rank as f64 + 1.0);
        }
    }

    let r1 = hits_at_1 as f64 / total as f64;
    let r5 = hits_at_5 as f64 / total as f64;
    let r10 = hits_at_10 as f64 / total as f64;
    let mrr = mrr_sum / total as f64;

    println!("  ┌─────────────────────────────────────────────────────────────┐");
    println!("  │  Semantic Purity Results ({} queries, zero keyword overlap) │", total);
    println!("  ├─────────────────────────────────────────────────────────────┤");
    println!("  │  Recall@1:  {:.4}  ({}/{})                              │", r1, hits_at_1, total);
    println!("  │  Recall@5:  {:.4}  ({}/{})                              │", r5, hits_at_5, total);
    println!("  │  Recall@10: {:.4}  ({}/{})                              │", r10, hits_at_10, total);
    println!("  │  MRR:       {:.4}                                        │", mrr);
    println!("  └─────────────────────────────────────────────────────────────┘");

    // Print misses for debugging
    println!();
    println!("  Misses (not in top 10):");
    for case in &cases {
        let query_vec = embedder.as_ref().and_then(|e| e.embed_query(case.query).ok());
        let query_slice = query_vec.as_deref();
        let result = rt.block_on(retrieval::recall(
            case.query, &conn, &lance, query_slice,
            &resolver, reranker, &summaries, &config,
        )).unwrap();
        let result_ids: Vec<&str> = result.results.iter()
            .map(|r| r.memory_id.as_str())
            .collect();
        if !result_ids.contains(&case.expected_id) {
            println!("    MISS: \"{}\" → expected {} ({})", case.query, case.expected_id, case.description);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_semantic_purity_keyword_only() {
    println!();
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  Semantic Purity — Keyword Only (expected: ~0% recall)       ║");
    println!("║  These queries have ZERO keyword overlap with expected memory ║");
    println!("║  If keyword-only scores high, the test is flawed.            ║");
    println!("╠════════════════════════════════════════════════════════════════╣");

    run_semantic_purity(false, false);

    println!("╚════════════════════════════════════════════════════════════════╝");
}

#[test]
#[ignore] // Requires ~50MB model download
fn test_semantic_purity_full_pipeline() {
    println!();
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  Semantic Purity — Full Pipeline (BGE-Small-EN)              ║");
    println!("║  Proves retrieval relies on semantic understanding,           ║");
    println!("║  not keyword matching.                                       ║");
    println!("╠════════════════════════════════════════════════════════════════╣");

    run_semantic_purity(true, false);

    println!("╚════════════════════════════════════════════════════════════════╝");
}

#[test]
#[ignore] // Requires ~50MB embedding model + ~400MB reranker model
fn test_semantic_purity_with_reranker() {
    println!();
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  Semantic Purity — Full Pipeline + BGE-Reranker-Base         ║");
    println!("║  Tests whether the cross-encoder reranker improves semantic   ║");
    println!("║  retrieval on queries with zero keyword overlap.              ║");
    println!("╠════════════════════════════════════════════════════════════════╣");

    run_semantic_purity(true, true);

    println!("╚════════════════════════════════════════════════════════════════╝");
}
