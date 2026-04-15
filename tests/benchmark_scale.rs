//! Corpus Scale Benchmark
//!
//! Measures retrieval quality at increasing corpus sizes: 500, 1000, 2000, 3000, 4000, 5000.
//! Uses a programmatic corpus generator that produces realistic engineering memories
//! with deterministic seeding so results are reproducible.
//!
//! Run: `cargo test --test benchmark_scale -- --nocapture --ignored`

use clearmemory::entities::resolver::HeuristicResolver;
use clearmemory::migration;
use clearmemory::retrieval::merge::Strategy;
use clearmemory::retrieval::rerank::PassthroughReranker;
use clearmemory::retrieval::{self, RecallConfig};
use clearmemory::storage::lance::LanceStorage;
use rusqlite::Connection;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Deterministic corpus generator
// ---------------------------------------------------------------------------

struct GeneratedMemory {
    id: String,
    summary: String,
    created_at: String,
}

const TECHS: &[&str] = &[
    "PostgreSQL", "MySQL", "MongoDB", "CockroachDB", "DynamoDB", "Redis", "Memcached",
    "Elasticsearch", "Solr", "Meilisearch", "Kafka", "RabbitMQ", "NATS", "Pulsar",
    "Kubernetes", "Docker Swarm", "Nomad", "ECS", "Lambda", "CloudRun",
    "React", "Vue", "Angular", "Svelte", "Next.js", "Remix", "Astro",
    "Go", "Rust", "Python", "Java", "TypeScript", "Ruby", "Elixir",
    "GraphQL", "REST", "gRPC", "WebSocket", "SSE", "MQTT",
    "Terraform", "Pulumi", "CloudFormation", "Ansible", "Chef",
    "Prometheus", "Grafana", "Datadog", "NewRelic", "Sentry", "PagerDuty",
    "Auth0", "Clerk", "Keycloak", "Okta", "Firebase Auth", "Supabase Auth",
    "Stripe", "PayPal", "Adyen", "Braintree", "Square",
    "S3", "GCS", "Azure Blob", "MinIO", "Cloudflare R2",
    "Nginx", "Envoy", "Traefik", "HAProxy", "Kong", "Istio",
    "ArgoCD", "FluxCD", "Jenkins", "GitHub Actions", "GitLab CI", "CircleCI",
    "Snowflake", "BigQuery", "Redshift", "ClickHouse", "TimescaleDB", "DuckDB",
    "Vault", "AWS KMS", "SOPS", "Doppler", "Infisical",
    "Sentry", "Bugsnag", "Rollbar", "Airbrake", "Honeybadger",
    "Twilio", "SendGrid", "Mailgun", "Postmark", "Amazon SES",
    "LaunchDarkly", "Unleash", "Flagsmith", "Split.io", "ConfigCat",
];

const PATTERNS: &[&str] = &[
    "circuit breaker", "saga pattern", "CQRS", "event sourcing", "outbox pattern",
    "bulkhead", "retry with backoff", "rate limiting", "token bucket", "sliding window",
    "blue-green deployment", "canary release", "feature flags", "A/B testing",
    "sharding", "read replicas", "connection pooling", "caching layer", "CDN",
    "microservices", "monolith", "modular monolith", "serverless", "edge computing",
    "zero-downtime migration", "dual-write", "change data capture", "event streaming",
    "distributed tracing", "structured logging", "health checks", "graceful shutdown",
];

const TEAMS: &[&str] = &[
    "platform", "frontend", "backend", "infrastructure", "security", "data",
    "mobile", "payments", "search", "notifications", "onboarding", "growth",
    "devex", "SRE", "compliance", "analytics",
];

const BUG_TYPES: &[&str] = &[
    "memory leak", "race condition", "deadlock", "N+1 query", "connection pool exhaustion",
    "timeout", "DNS resolution failure", "certificate expiry", "CORS misconfiguration",
    "null pointer", "off-by-one error", "timezone handling bug", "encoding issue",
    "file descriptor leak", "disk space exhaustion", "OOM kill", "CPU throttling",
    "network partition", "split brain", "data corruption", "index bloat",
    "rebalancing storm", "thundering herd", "cache stampede", "hot partition",
];

const METRICS: &[&str] = &[
    "p95 latency dropped from 450ms to 120ms",
    "throughput increased from 1K to 5K rps",
    "error rate reduced from 2.3% to 0.01%",
    "memory usage decreased by 40%",
    "CPU utilization dropped from 85% to 45%",
    "build time reduced from 8 minutes to 90 seconds",
    "deployment time cut from 30 minutes to 5 minutes",
    "test suite runs 3x faster",
    "cold start time reduced from 12s to 800ms",
    "database query time improved from 3.2s to 15ms",
    "page load time decreased from 4.5s to 1.2s",
    "bundle size reduced by 60%",
    "container image size shrunk from 1.2GB to 180MB",
    "startup time improved from 45s to 3s",
    "cost reduced by 35% through right-sizing",
];

fn generate_corpus(count: usize) -> Vec<GeneratedMemory> {
    let mut memories = Vec::with_capacity(count);

    for i in 0..count {
        let category = i % 10;
        let tech_idx = i % TECHS.len();
        let tech2_idx = (i * 7 + 3) % TECHS.len();
        let pattern_idx = i % PATTERNS.len();
        let team_idx = i % TEAMS.len();
        let bug_idx = i % BUG_TYPES.len();
        let metric_idx = i % METRICS.len();

        // Spread dates across 18 months
        let day_offset = i % 540;
        let base_date = chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let date = base_date + chrono::Duration::days(day_offset as i64);
        let created_at = date.to_string();

        let summary = match category {
            0 => format!(
                "Architecture decision: chose {} over {} for the {} service because of better {} support and lower operational complexity",
                TECHS[tech_idx], TECHS[tech2_idx], TEAMS[team_idx], PATTERNS[pattern_idx]
            ),
            1 => format!(
                "Fixed {} in the {} service: root cause was {} in the {} layer. {}",
                BUG_TYPES[bug_idx], TEAMS[team_idx], BUG_TYPES[(bug_idx + 1) % BUG_TYPES.len()],
                TECHS[tech_idx], METRICS[metric_idx]
            ),
            2 => format!(
                "Sprint review: {} team completed migration to {}. Next sprint focuses on {} integration. {}",
                TEAMS[team_idx], TECHS[tech_idx], TECHS[tech2_idx], METRICS[metric_idx]
            ),
            3 => format!(
                "Code review feedback on {} integration: needs better error handling for {} scenarios. Suggested implementing {} pattern",
                TECHS[tech_idx], BUG_TYPES[bug_idx], PATTERNS[pattern_idx]
            ),
            4 => format!(
                "Debugging session: traced {} issue in {} to misconfigured {}. Applied {} as fix. {}",
                BUG_TYPES[bug_idx], TECHS[tech_idx], TECHS[tech2_idx], PATTERNS[pattern_idx], METRICS[metric_idx]
            ),
            5 => format!(
                "Team discussion: {} team evaluating {} vs {} for new {} feature. Key criteria: reliability, cost, and {} support",
                TEAMS[team_idx], TECHS[tech_idx], TECHS[tech2_idx], PATTERNS[pattern_idx], PATTERNS[(pattern_idx + 1) % PATTERNS.len()]
            ),
            6 => format!(
                "Technical spec: {} configuration for {} — implementing {} with {} backend. Target: {}",
                TECHS[tech_idx], TEAMS[team_idx], PATTERNS[pattern_idx], TECHS[tech2_idx], METRICS[metric_idx]
            ),
            7 => format!(
                "Incident postmortem: {}-hour outage in {} caused by {} in {}. Implemented {} to prevent recurrence",
                (i % 4) + 1, TEAMS[team_idx], BUG_TYPES[bug_idx], TECHS[tech_idx], PATTERNS[pattern_idx]
            ),
            8 => format!(
                "Security audit: found {} vulnerability in {} integration. Patched by implementing {} with {}. {} compliance verified",
                BUG_TYPES[bug_idx], TECHS[tech_idx], PATTERNS[pattern_idx], TECHS[tech2_idx],
                if i % 2 == 0 { "SOC 2" } else { "GDPR" }
            ),
            9 => format!(
                "Onboarding doc: {} setup guide for {} team — install {}, configure {}, verify with {}. Takes ~{} minutes",
                TECHS[tech_idx], TEAMS[team_idx], TECHS[tech2_idx], PATTERNS[pattern_idx],
                PATTERNS[(pattern_idx + 2) % PATTERNS.len()], (i % 30) + 10
            ),
            _ => unreachable!(),
        };

        memories.push(GeneratedMemory {
            id: format!("gen-{:05}", i),
            summary,
            created_at,
        });
    }

    memories
}

// ---------------------------------------------------------------------------
// Fixed query set (same queries tested at every corpus size)
// ---------------------------------------------------------------------------

struct ScaleTestCase {
    query: &'static str,
    description: &'static str,
    // We check if ANY of these substrings appear in top-10 result summaries
    expected_keywords: Vec<&'static str>,
}

fn build_scale_queries() -> Vec<ScaleTestCase> {
    vec![
        // Direct factual queries (should always work)
        ScaleTestCase { query: "PostgreSQL database decision", description: "direct keyword", expected_keywords: vec!["PostgreSQL"] },
        ScaleTestCase { query: "Kafka event streaming", description: "direct keyword", expected_keywords: vec!["Kafka"] },
        ScaleTestCase { query: "Kubernetes container orchestration", description: "direct keyword", expected_keywords: vec!["Kubernetes"] },
        ScaleTestCase { query: "React frontend framework", description: "direct keyword", expected_keywords: vec!["React"] },
        ScaleTestCase { query: "Redis caching solution", description: "direct keyword", expected_keywords: vec!["Redis"] },
        ScaleTestCase { query: "GraphQL API design", description: "direct keyword", expected_keywords: vec!["GraphQL"] },
        ScaleTestCase { query: "Terraform infrastructure as code", description: "direct keyword", expected_keywords: vec!["Terraform"] },
        ScaleTestCase { query: "Prometheus monitoring setup", description: "direct keyword", expected_keywords: vec!["Prometheus"] },
        ScaleTestCase { query: "Stripe payment integration", description: "direct keyword", expected_keywords: vec!["Stripe"] },
        ScaleTestCase { query: "ArgoCD deployment pipeline", description: "direct keyword", expected_keywords: vec!["ArgoCD"] },

        // Bug/incident queries
        ScaleTestCase { query: "memory leak investigation", description: "bug type", expected_keywords: vec!["memory leak"] },
        ScaleTestCase { query: "race condition fix", description: "bug type", expected_keywords: vec!["race condition"] },
        ScaleTestCase { query: "connection pool exhaustion", description: "bug type", expected_keywords: vec!["connection pool"] },
        ScaleTestCase { query: "timeout debugging", description: "bug type", expected_keywords: vec!["timeout"] },
        ScaleTestCase { query: "certificate expiry incident", description: "bug type", expected_keywords: vec!["certificate"] },

        // Pattern/architecture queries
        ScaleTestCase { query: "circuit breaker implementation", description: "pattern", expected_keywords: vec!["circuit breaker"] },
        ScaleTestCase { query: "event sourcing architecture", description: "pattern", expected_keywords: vec!["event sourcing"] },
        ScaleTestCase { query: "blue-green deployment strategy", description: "pattern", expected_keywords: vec!["blue-green"] },
        ScaleTestCase { query: "CQRS pattern decision", description: "pattern", expected_keywords: vec!["CQRS"] },
        ScaleTestCase { query: "zero-downtime migration approach", description: "pattern", expected_keywords: vec!["zero-downtime"] },

        // Team queries
        ScaleTestCase { query: "platform team decisions", description: "team", expected_keywords: vec!["platform"] },
        ScaleTestCase { query: "security team audit findings", description: "team", expected_keywords: vec!["security", "Security"] },
        ScaleTestCase { query: "frontend team work", description: "team", expected_keywords: vec!["frontend"] },
        ScaleTestCase { query: "infrastructure team changes", description: "team", expected_keywords: vec!["infrastructure"] },
        ScaleTestCase { query: "data team pipeline", description: "team", expected_keywords: vec!["data"] },

        // Conceptual/paraphrase queries (hardest)
        ScaleTestCase { query: "improving system performance", description: "conceptual", expected_keywords: vec!["latency", "throughput", "faster", "reduced", "improved"] },
        ScaleTestCase { query: "reducing costs", description: "conceptual", expected_keywords: vec!["cost", "reduced", "cheaper", "right-sizing"] },
        ScaleTestCase { query: "production reliability", description: "conceptual", expected_keywords: vec!["outage", "incident", "reliability", "circuit breaker", "health"] },
        ScaleTestCase { query: "developer onboarding", description: "conceptual", expected_keywords: vec!["onboarding", "setup guide", "install"] },
        ScaleTestCase { query: "compliance requirements", description: "conceptual", expected_keywords: vec!["SOC 2", "GDPR", "compliance", "audit"] },
    ]
}

fn evaluate_at_scale(
    corpus_size: usize,
    embedder: Option<&clearmemory::storage::embeddings::EmbeddingManager>,
) {
    let corpus = generate_corpus(corpus_size);
    let queries = build_scale_queries();

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
        .map(|m| (m.id.clone(), m.summary.clone()))
        .collect();

    // Setup LanceDB
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let dir = tempfile::tempdir().unwrap();

    let dim = embedder.map(|e| e.dimensions()).unwrap_or(384);
    let lance = rt.block_on(LanceStorage::open_with_dim(dir.path().join("v"), dim as i32)).unwrap();

    // Index with embeddings if available
    if let Some(emb) = embedder {
        for mem in &corpus {
            if let Ok(vec) = emb.embed_query(&mem.summary) {
                rt.block_on(lance.insert(&mem.id, &vec, None)).unwrap();
            }
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

    let mut hits = 0;
    let mut total = 0;
    let mut mrr_sum = 0.0;

    for case in &queries {
        let query_vec = embedder.and_then(|e| e.embed_query(case.query).ok());
        let query_slice = query_vec.as_deref();

        let result = rt.block_on(retrieval::recall(
            case.query, &conn, &lance, query_slice,
            &resolver, &reranker, &summaries, &config,
        )).unwrap();

        let result_ids: Vec<&str> = result.results.iter().map(|r| r.memory_id.as_str()).collect();

        // Check if any top-10 result contains expected keywords
        let found = result_ids.iter().any(|id| {
            if let Some(summary) = summaries.get(*id) {
                let lower = summary.to_lowercase();
                case.expected_keywords.iter().any(|kw| lower.contains(&kw.to_lowercase()))
            } else {
                false
            }
        });

        if found {
            hits += 1;
            // Find rank of first relevant result for MRR
            let rank = result_ids.iter().position(|id| {
                if let Some(summary) = summaries.get(*id) {
                    let lower = summary.to_lowercase();
                    case.expected_keywords.iter().any(|kw| lower.contains(&kw.to_lowercase()))
                } else {
                    false
                }
            });
            if let Some(r) = rank {
                mrr_sum += 1.0 / (r as f64 + 1.0);
            }
        }
        total += 1;
    }

    let recall = hits as f64 / total as f64;
    let mrr = mrr_sum / total as f64;

    println!(
        "  {:>5} memories | Recall@10: {:.4} ({:>2}/{}) | MRR: {:.4}",
        corpus_size, recall, hits, total, mrr
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_scale_keyword_only() {
    println!();
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║  CORPUS SCALE BENCHMARK — Keyword Only (no model)        ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║  Same 30 queries tested at each corpus size.             ║");
    println!("║  Measures how retrieval degrades as corpus grows.        ║");
    println!("╠════════════════════════════════════════════════════════════╣");

    for &size in &[500, 1000, 2000, 3000, 4000, 5000, 10000] {
        evaluate_at_scale(size, None);
    }

    println!("╚════════════════════════════════════════════════════════════╝");
    println!();
}

#[test]
#[ignore] // Requires ~50MB model download
fn test_scale_full_pipeline() {
    let embedder = clearmemory::storage::embeddings::EmbeddingManager::new("bge-small-en").unwrap();

    println!();
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║  CORPUS SCALE BENCHMARK — Full Pipeline (BGE-Small-EN)   ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║  Same 30 queries tested at each corpus size.             ║");
    println!("║  Semantic + Keyword + Temporal + Entity Graph.           ║");
    println!("╠════════════════════════════════════════════════════════════╣");

    for &size in &[500, 1000, 2000, 3000, 4000, 5000, 10000] {
        evaluate_at_scale(size, Some(&embedder));
    }

    println!("╚════════════════════════════════════════════════════════════╝");
    println!();
}

/// Run only the 10,000 memory benchmark (separate test for long-running isolation).
#[test]
#[ignore]
fn test_scale_10k_full_pipeline() {
    let embedder = clearmemory::storage::embeddings::EmbeddingManager::new("bge-small-en").unwrap();

    println!();
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║  10K CORPUS BENCHMARK — Full Pipeline (BGE-Small-EN)     ║");
    println!("╠════════════════════════════════════════════════════════════╣");

    evaluate_at_scale(10000, Some(&embedder));

    println!("╚════════════════════════════════════════════════════════════╝");
    println!();
}
