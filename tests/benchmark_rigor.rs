//! Benchmark Rigor Tests
//!
//! Addresses three specific credibility gaps identified in QA audit:
//!
//! 1. **Knowledge Update Evaluation** — Tests that the system ranks CURRENT
//!    facts above superseded facts (not just "both are retrieved").
//!
//! 2. **Genuine Multi-hop Queries** — Queries that require chaining information
//!    across 2+ memories to answer (not just topic aggregation).
//!
//! 3. **Distractor Corpus** — Mix 1000 irrelevant memories with 30 target
//!    memories and verify precision doesn't collapse under noise.
//!
//! Run: `cargo test --release --test benchmark_rigor -- --nocapture --ignored`

use clearmemory::entities::resolver::HeuristicResolver;
use clearmemory::migration;
use clearmemory::retrieval::rerank::PassthroughReranker;
use clearmemory::retrieval::{self, RecallConfig};
use clearmemory::storage::lance::LanceStorage;
use rusqlite::Connection;
use std::collections::HashMap;

struct Mem {
    id: &'static str,
    summary: &'static str,
    created_at: &'static str,
}

// =========================================================================
// 1. KNOWLEDGE UPDATE EVALUATION
// =========================================================================
// Tests that when a fact has been superseded, the system ranks the
// CURRENT fact higher than the old one. Not just "both are retrieved"
// but "current is ranked #1".

struct KnowledgeUpdateCase {
    query: &'static str,
    current_id: &'static str,     // should be ranked #1
    superseded_id: &'static str,  // should be ranked lower
    description: &'static str,
}

fn build_ku_corpus() -> Vec<Mem> {
    vec![
        // Auth provider: Auth0 → Clerk
        Mem { id: "ku-old-auth", summary: "Authentication: using Auth0 for all user authentication. Auth0 provides JWT tokens, social login, and MFA out of the box.", created_at: "2025-06-01" },
        Mem { id: "ku-new-auth", summary: "Authentication: migrated from Auth0 to Clerk. Clerk offers better React components, simpler pricing, and faster integration. All users now authenticate via Clerk.", created_at: "2025-12-15" },

        // Database: MySQL → PostgreSQL
        Mem { id: "ku-old-db", summary: "Database: running MySQL 5.7 as primary datastore. All services connect via connection pool with max 20 connections.", created_at: "2025-05-01" },
        Mem { id: "ku-new-db", summary: "Database: completed migration from MySQL to PostgreSQL 16. PostgreSQL selected for JSON support, window functions, and row-level security.", created_at: "2025-09-01" },

        // CI/CD: Jenkins → GitHub Actions
        Mem { id: "ku-old-ci", summary: "CI/CD: Jenkins runs all build and deploy pipelines. Managed by the platform team on a dedicated EC2 instance.", created_at: "2025-04-01" },
        Mem { id: "ku-new-ci", summary: "CI/CD: replaced Jenkins with GitHub Actions. YAML-based config, no server maintenance, native GitHub integration.", created_at: "2025-10-01" },

        // On-call: Marcus → Sarah/Kai/Priya
        Mem { id: "ku-old-oncall", summary: "On-call schedule: Marcus handles Monday through Thursday, Aisha covers Friday through Sunday.", created_at: "2025-08-01" },
        Mem { id: "ku-new-oncall", summary: "On-call schedule updated: Sarah covers Monday-Wednesday, Kai takes Thursday-Saturday, Priya handles Sunday.", created_at: "2025-11-01" },

        // Monitoring: Datadog → Prometheus
        Mem { id: "ku-old-mon", summary: "Monitoring: Datadog for all metrics, logs, and APM traces. Costs approximately $3,000 per month.", created_at: "2025-03-01" },
        Mem { id: "ku-new-mon", summary: "Monitoring: moved from Datadog to Prometheus plus Grafana plus Loki. Self-hosted, saving $3,000 per month.", created_at: "2025-09-15" },

        // Pool size: 20 → 50
        Mem { id: "ku-old-pool", summary: "Database connection pool configured with maximum 20 connections per service instance.", created_at: "2025-07-01" },
        Mem { id: "ku-new-pool", summary: "Increased database connection pool from 20 to 50 per instance after the connection exhaustion incident.", created_at: "2025-10-15" },

        // Distractors
        Mem { id: "ku-distractor-1", summary: "Frontend performance: reduced bundle size from 1.2MB to 480KB via code splitting.", created_at: "2025-09-05" },
        Mem { id: "ku-distractor-2", summary: "Security audit: PenTest Corp found 3 medium vulnerabilities including CORS misconfiguration.", created_at: "2025-11-20" },
        Mem { id: "ku-distractor-3", summary: "Sprint 14 retrospective: deployment went smoothly, zero downtime achieved.", created_at: "2025-10-08" },
    ]
}

fn build_ku_cases() -> Vec<KnowledgeUpdateCase> {
    vec![
        KnowledgeUpdateCase {
            query: "What authentication provider do we use currently?",
            current_id: "ku-new-auth",
            superseded_id: "ku-old-auth",
            description: "Auth: Auth0 → Clerk",
        },
        KnowledgeUpdateCase {
            query: "What is our current database?",
            current_id: "ku-new-db",
            superseded_id: "ku-old-db",
            description: "DB: MySQL → PostgreSQL",
        },
        KnowledgeUpdateCase {
            query: "What CI/CD system are we using now?",
            current_id: "ku-new-ci",
            superseded_id: "ku-old-ci",
            description: "CI: Jenkins → GitHub Actions",
        },
        KnowledgeUpdateCase {
            query: "Who is currently on call?",
            current_id: "ku-new-oncall",
            superseded_id: "ku-old-oncall",
            description: "On-call: Marcus → Sarah/Kai/Priya",
        },
        KnowledgeUpdateCase {
            query: "What monitoring stack are we running?",
            current_id: "ku-new-mon",
            superseded_id: "ku-old-mon",
            description: "Monitoring: Datadog → Prometheus",
        },
        KnowledgeUpdateCase {
            query: "How many database connections can each instance use?",
            current_id: "ku-new-pool",
            superseded_id: "ku-old-pool",
            description: "Pool: 20 → 50",
        },
    ]
}

// =========================================================================
// 2. GENUINE MULTI-HOP QUERIES
// =========================================================================
// Queries that require connecting information across 2+ memories.
// The answer is not in any single memory — you must chain facts.

struct MultiHopCase {
    query: &'static str,
    required_memories: Vec<&'static str>, // ALL must be in top-10
    description: &'static str,
}

fn build_multihop_corpus() -> Vec<Mem> {
    vec![
        // Chain: Person → Bug → Outage → Project impact
        Mem { id: "mh-person", summary: "Kai Rivera is the senior backend engineer responsible for the payment service and API gateway.", created_at: "2025-07-01" },
        Mem { id: "mh-bug", summary: "Critical bug in the payment service: connection pool exhaustion causing OOM after 6 hours of sustained traffic.", created_at: "2025-10-05" },
        Mem { id: "mh-outage", summary: "Production outage on October 5th caused by the payment service OOM. Lasted 45 minutes, affected all checkout flows.", created_at: "2025-10-05" },
        Mem { id: "mh-impact", summary: "Q3 launch delayed by one week due to the October 5th outage. Management required full postmortem before proceeding.", created_at: "2025-10-08" },

        // Chain: Tool → Decision → Migration → Completion
        Mem { id: "mh-eval", summary: "Sarah evaluated Clerk, Okta, and WorkOS for authentication. Clerk won on developer experience and pricing.", created_at: "2025-08-15" },
        Mem { id: "mh-decision", summary: "Team approved Sarah's recommendation to adopt Clerk. Migration budget: 3 sprints.", created_at: "2025-08-20" },
        Mem { id: "mh-migration", summary: "Auth migration sprint 2 of 3: session management ported from custom JWT to Clerk sessions.", created_at: "2025-10-01" },
        Mem { id: "mh-complete", summary: "Auth migration completed ahead of schedule in sprint 2. All users now on Clerk. Legacy endpoints sunset in 30 days.", created_at: "2025-11-15" },

        // Chain: Vulnerability → Fix → Verification
        Mem { id: "mh-vuln", summary: "Security scan revealed CORS misconfiguration allowing requests from any origin on staging environment.", created_at: "2025-11-20" },
        Mem { id: "mh-fix", summary: "Priya restricted CORS origins to production and staging domains. Added CSP headers for additional protection.", created_at: "2025-11-22" },
        Mem { id: "mh-verify", summary: "Re-scan confirmed all 3 medium vulnerabilities from the PenTest Corp audit are now resolved.", created_at: "2025-12-01" },

        // Distractors
        Mem { id: "mh-dist-1", summary: "Sprint velocity has averaged 42 story points over the last 4 sprints.", created_at: "2025-10-15" },
        Mem { id: "mh-dist-2", summary: "New hire onboarding: Alex joined the frontend team on November 1st.", created_at: "2025-11-01" },
        Mem { id: "mh-dist-3", summary: "Coffee machine in the break room is broken again. Facilities has been notified.", created_at: "2025-11-10" },
    ]
}

fn build_multihop_cases() -> Vec<MultiHopCase> {
    vec![
        MultiHopCase {
            query: "Who was responsible for the service that caused the October outage?",
            required_memories: vec!["mh-person", "mh-bug", "mh-outage"],
            description: "Person → Bug → Outage (3-hop)",
        },
        MultiHopCase {
            query: "What was the business impact of the payment service memory leak?",
            required_memories: vec!["mh-bug", "mh-outage", "mh-impact"],
            description: "Bug → Outage → Impact (3-hop)",
        },
        MultiHopCase {
            query: "Who evaluated the options that led to our current auth system?",
            required_memories: vec!["mh-eval", "mh-decision"],
            description: "Evaluation → Decision (2-hop)",
        },
        MultiHopCase {
            query: "How long did the auth migration take from approval to completion?",
            required_memories: vec!["mh-decision", "mh-migration", "mh-complete"],
            description: "Decision → Migration → Complete (3-hop)",
        },
        MultiHopCase {
            query: "Who fixed the security vulnerability and was the fix verified?",
            required_memories: vec!["mh-vuln", "mh-fix", "mh-verify"],
            description: "Vuln → Fix → Verify (3-hop)",
        },
        MultiHopCase {
            query: "What happened between finding the CORS issue and confirming it was resolved?",
            required_memories: vec!["mh-vuln", "mh-fix", "mh-verify"],
            description: "Full vulnerability lifecycle (3-hop)",
        },
        MultiHopCase {
            query: "Trace the full chain from payment bug to project delay",
            required_memories: vec!["mh-bug", "mh-outage", "mh-impact"],
            description: "Bug → Outage → Delay (explicit chain request)",
        },
    ]
}

// =========================================================================
// 3. DISTRACTOR CORPUS
// =========================================================================
// Generate 1000 irrelevant memories and mix with 30 target memories.
// Tests precision under noise — can the system find needles in a haystack?

fn generate_distractors(count: usize) -> Vec<(String, String, String)> {
    let topics = [
        "Quarterly budget review for marketing department showed 12% over allocation",
        "Office renovation scheduled for building C wing 3 starting January",
        "Employee wellness program launching next month with yoga and meditation",
        "Parking lot resurfacing will close lot B for two weeks",
        "Annual company picnic rescheduled to September due to weather",
        "New vending machines installed on floors 2 and 4 with healthier options",
        "Fire drill scheduled for next Tuesday at 2pm all employees must participate",
        "Holiday party committee seeking volunteers for decoration and planning",
        "Updated travel policy requires pre-approval for trips over 500 dollars",
        "Building HVAC system upgrade will cause temperature fluctuations this week",
        "New recycling bins placed near all printers please separate paper and plastic",
        "Cafeteria menu changing to include more vegetarian options starting Monday",
        "Security badge readers being upgraded please keep old badge until notified",
        "Printer on floor 3 is out of toner IT has been notified expect delay",
        "Weekly standup moving from 9am to 10am starting next sprint",
        "Conference room B2 is being converted to a phone booth area",
        "Office plants need watering sign up sheet at reception desk",
        "Lost and found box is in the mail room check for missing items",
        "Building evacuation routes updated see posted maps near elevators",
        "New coffee blend available in the kitchen courtesy of the social committee",
    ];

    (0..count).map(|i| {
        let topic = topics[i % topics.len()];
        let id = format!("distractor-{:04}", i);
        let day = (i % 28) + 1;
        let month = (i % 12) + 1;
        let created = format!("2025-{:02}-{:02}", month, day);
        let text = format!("{} (memo #{})", topic, i);
        (id, text, created)
    }).collect()
}

// =========================================================================
// Test runners
// =========================================================================

fn setup_recall(
    corpus: &[Mem],
    extra: &[(String, String, String)],
    use_embeddings: bool,
) -> (
    Connection,
    LanceStorage,
    HashMap<String, String>,
    Option<clearmemory::storage::embeddings::EmbeddingManager>,
    tokio::runtime::Runtime,
    tempfile::TempDir,
) {
    let conn = Connection::open_in_memory().unwrap();
    migration::runner::run_migrations(&conn).unwrap();

    let mut summaries: HashMap<String, String> = HashMap::new();

    for mem in corpus {
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) VALUES (?1, ?2, ?3, 'clear', ?4)",
            rusqlite::params![mem.id, format!("hash-{}", mem.id), mem.summary, mem.created_at],
        ).unwrap();
        summaries.insert(mem.id.to_string(), mem.summary.to_string());
    }

    for (id, text, created) in extra {
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) VALUES (?1, ?2, ?3, 'clear', ?4)",
            rusqlite::params![id, format!("hash-{}", id), text, created],
        ).unwrap();
        summaries.insert(id.clone(), text.clone());
    }

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
        for mem in corpus {
            if let Ok(vec) = emb.embed_query(mem.summary) {
                rt.block_on(lance.insert(mem.id, &vec, None)).unwrap();
            }
        }
        for (id, text, _) in extra {
            if let Ok(vec) = emb.embed_query(text) {
                rt.block_on(lance.insert(id, &vec, None)).unwrap();
            }
        }
    }

    (conn, lance, summaries, embedder, rt, dir)
}

// =========================================================================
// Tests
// =========================================================================

#[test]
#[ignore]
fn test_knowledge_update_ranking() {
    println!();
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  Knowledge Update — Current Fact Must Rank Above Superseded   ║");
    println!("╠════════════════════════════════════════════════════════════════╣");

    let corpus = build_ku_corpus();
    let cases = build_ku_cases();
    let (conn, lance, summaries, embedder, rt, _dir) = setup_recall(&corpus, &[], true);

    let resolver = HeuristicResolver;
    let reranker = PassthroughReranker;
    let config = RecallConfig {
        top_k: 10,
        temporal_boost: 0.4,
        entity_boost: 0.3,
        include_archived: false,
        stream_id: None,
    };

    let mut current_ranked_first = 0;
    let mut both_retrieved = 0;

    for case in &cases {
        let query_vec = embedder.as_ref().and_then(|e| e.embed_query(case.query).ok());
        let result = rt.block_on(retrieval::recall(
            case.query, &conn, &lance, query_vec.as_deref(),
            &resolver, &reranker, &summaries, &config,
        )).unwrap();

        let ids: Vec<&str> = result.results.iter().map(|r| r.memory_id.as_str()).collect();

        let current_rank = ids.iter().position(|id| *id == case.current_id);
        let superseded_rank = ids.iter().position(|id| *id == case.superseded_id);

        let current_first = match (current_rank, superseded_rank) {
            (Some(c), Some(s)) => {
                both_retrieved += 1;
                c < s
            }
            (Some(_), None) => true,   // current found, old not — good
            _ => false,
        };

        if current_first {
            current_ranked_first += 1;
        }

        let status = if current_first { "PASS" } else { "FAIL" };
        println!("  {} | {} | current={:?} superseded={:?}",
            status, case.description,
            current_rank.map(|r| r + 1),
            superseded_rank.map(|r| r + 1));
    }

    let total = cases.len();
    println!();
    println!("  Current ranked above superseded: {}/{} ({:.1}%)",
        current_ranked_first, total, current_ranked_first as f64 / total as f64 * 100.0);
    println!("  Both retrieved in top 10: {}/{}", both_retrieved, total);

    println!("╚════════════════════════════════════════════════════════════════╝");
}

#[test]
#[ignore]
fn test_genuine_multihop() {
    println!();
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  Genuine Multi-hop — Queries Requiring 2-3 Memory Chains     ║");
    println!("╠════════════════════════════════════════════════════════════════╣");

    let corpus = build_multihop_corpus();
    let cases = build_multihop_cases();
    let (conn, lance, summaries, embedder, rt, _dir) = setup_recall(&corpus, &[], true);

    let resolver = HeuristicResolver;
    let reranker = PassthroughReranker;
    let config = RecallConfig {
        top_k: 10,
        temporal_boost: 0.4,
        entity_boost: 0.3,
        include_archived: false,
        stream_id: None,
    };

    let mut full_chain_found = 0;
    let mut partial_found = 0;

    for case in &cases {
        let query_vec = embedder.as_ref().and_then(|e| e.embed_query(case.query).ok());
        let result = rt.block_on(retrieval::recall(
            case.query, &conn, &lance, query_vec.as_deref(),
            &resolver, &reranker, &summaries, &config,
        )).unwrap();

        let ids: Vec<&str> = result.results.iter().map(|r| r.memory_id.as_str()).collect();
        let found: Vec<&&str> = case.required_memories.iter()
            .filter(|req| ids.contains(req))
            .collect();

        let all_found = found.len() == case.required_memories.len();
        if all_found {
            full_chain_found += 1;
        }
        if !found.is_empty() {
            partial_found += 1;
        }

        let status = if all_found { "FULL" } else if !found.is_empty() { "PART" } else { "MISS" };
        println!("  {} | {} | found {}/{} required memories",
            status, case.description, found.len(), case.required_memories.len());
    }

    let total = cases.len();
    println!();
    println!("  Full chain retrieved: {}/{} ({:.1}%)",
        full_chain_found, total, full_chain_found as f64 / total as f64 * 100.0);
    println!("  Partial chain: {}/{}", partial_found, total);

    println!("╚════════════════════════════════════════════════════════════════╝");
}

#[test]
#[ignore]
fn test_distractor_corpus() {
    println!();
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  Distractor Corpus — 1000 irrelevant + 15 target memories    ║");
    println!("║  Tests precision under noise: can we find needles?            ║");
    println!("╠════════════════════════════════════════════════════════════════╣");

    let target_corpus = vec![
        Mem { id: "target-001", summary: "Selected PostgreSQL 16 as the primary database replacing MySQL for JSON support and row-level security", created_at: "2025-07-20" },
        Mem { id: "target-002", summary: "Chose Kafka over RabbitMQ for event streaming replay capability", created_at: "2025-07-25" },
        Mem { id: "target-003", summary: "Authentication migrated from Auth0 to Clerk for better developer experience", created_at: "2025-08-10" },
        Mem { id: "target-004", summary: "Implemented token bucket rate limiter for API gateway endpoints", created_at: "2025-09-01" },
        Mem { id: "target-005", summary: "Fixed critical memory leak in payment service connection pool", created_at: "2025-10-05" },
        Mem { id: "target-006", summary: "Production outage lasted 45 minutes caused by expired TLS certificate on API gateway", created_at: "2025-09-03" },
        Mem { id: "target-007", summary: "Frontend bundle size reduced from 1.2MB to 480KB through code splitting and lazy loading", created_at: "2025-09-05" },
        Mem { id: "target-008", summary: "Observability stack migrated from Datadog to Grafana Prometheus Loki saving $3000 per month", created_at: "2025-09-15" },
        Mem { id: "target-009", summary: "Container orchestration running on EKS with Karpenter for node autoscaling", created_at: "2025-10-15" },
        Mem { id: "target-010", summary: "Secrets management using HashiCorp Vault with 24 hour automatic password rotation", created_at: "2025-10-20" },
        Mem { id: "target-011", summary: "Security audit by PenTest Corp found three medium vulnerabilities including CORS misconfiguration", created_at: "2025-11-20" },
        Mem { id: "target-012", summary: "GraphQL federation with Apollo Router each team owns their subgraph schema", created_at: "2025-10-01" },
        Mem { id: "target-013", summary: "Data warehouse migrated to Snowflake from Redshift for compute storage separation", created_at: "2025-10-10" },
        Mem { id: "target-014", summary: "Feature flags via LaunchDarkly for gradual percentage based rollouts", created_at: "2025-10-05" },
        Mem { id: "target-015", summary: "Error tracking moved from Bugsnag to Sentry for better source map and React support", created_at: "2025-11-01" },
    ];

    let distractors = generate_distractors(1000);

    println!("  Corpus: {} target + {} distractor = {} total memories",
        target_corpus.len(), distractors.len(), target_corpus.len() + distractors.len());

    let (conn, lance, summaries, embedder, rt, _dir) = setup_recall(&target_corpus, &distractors, true);

    let resolver = HeuristicResolver;
    let reranker = PassthroughReranker;
    let config = RecallConfig {
        top_k: 10,
        temporal_boost: 0.4,
        entity_boost: 0.3,
        include_archived: false,
        stream_id: None,
    };

    struct DistractorQuery {
        query: &'static str,
        expected_id: &'static str,
    }

    let queries = vec![
        DistractorQuery { query: "what database did we choose", expected_id: "target-001" },
        DistractorQuery { query: "event streaming technology", expected_id: "target-002" },
        DistractorQuery { query: "authentication provider", expected_id: "target-003" },
        DistractorQuery { query: "API rate limiting", expected_id: "target-004" },
        DistractorQuery { query: "payment service bug", expected_id: "target-005" },
        DistractorQuery { query: "production outage TLS", expected_id: "target-006" },
        DistractorQuery { query: "frontend performance optimization", expected_id: "target-007" },
        DistractorQuery { query: "monitoring and observability stack", expected_id: "target-008" },
        DistractorQuery { query: "kubernetes container management", expected_id: "target-009" },
        DistractorQuery { query: "credential and secret storage", expected_id: "target-010" },
        DistractorQuery { query: "security vulnerabilities found", expected_id: "target-011" },
        DistractorQuery { query: "GraphQL API architecture", expected_id: "target-012" },
        DistractorQuery { query: "analytics data warehouse", expected_id: "target-013" },
        DistractorQuery { query: "feature flag rollout system", expected_id: "target-014" },
        DistractorQuery { query: "crash and exception tracking", expected_id: "target-015" },
    ];

    let mut hits = 0;
    let mut distractor_in_top10 = 0;
    let total_slots = queries.len() * 10; // 15 queries * 10 results each

    for q in &queries {
        let query_vec = embedder.as_ref().and_then(|e| e.embed_query(q.query).ok());
        let result = rt.block_on(retrieval::recall(
            q.query, &conn, &lance, query_vec.as_deref(),
            &resolver, &reranker, &summaries, &config,
        )).unwrap();

        let ids: Vec<&str> = result.results.iter().map(|r| r.memory_id.as_str()).collect();
        let found = ids.contains(&q.expected_id);
        if found { hits += 1; }

        let distractors_found = ids.iter().filter(|id| id.starts_with("distractor-")).count();
        distractor_in_top10 += distractors_found;

        let status = if found { "HIT " } else { "MISS" };
        println!("  {} | \"{}\" | distractors in top 10: {}",
            status, q.query, distractors_found);
    }

    let recall = hits as f64 / queries.len() as f64;
    let precision = (total_slots - distractor_in_top10) as f64 / total_slots as f64;

    println!();
    println!("  Recall@10: {:.1}% ({}/{})", recall * 100.0, hits, queries.len());
    println!("  Precision (non-distractor rate in top 10): {:.1}%", precision * 100.0);
    println!("  Distractor contamination: {}/{} slots ({:.1}%)",
        distractor_in_top10, total_slots, distractor_in_top10 as f64 / total_slots as f64 * 100.0);

    println!("╚════════════════════════════════════════════════════════════════╝");
}
