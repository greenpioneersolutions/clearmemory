//! LongMemEval-Style Benchmark Runner
//!
//! Implements the evaluation methodology from the LongMemEval paper
//! (Liu et al., 2024) adapted for Clear Memory's retrieval pipeline.
//!
//! LongMemEval defines 5 task types for evaluating long-term memory:
//! 1. Information Extraction (IE) — retrieve specific facts
//! 2. Temporal Reasoning (TR) — time-based queries
//! 3. Multi-hop Reasoning (MH) — connect information across memories
//! 4. Knowledge Update (KU) — handle superseded information
//! 5. Abstraction (AB) — synthesize across multiple memories
//!
//! This benchmark measures: MRR, Recall@K, NDCG@10, per-task-type breakdown.
//!
//! Run: `cargo test --test benchmark_longmemeval -- --nocapture --ignored`

use clearmemory::entities::resolver::HeuristicResolver;
use clearmemory::migration;
use clearmemory::retrieval::merge::Strategy;
use clearmemory::retrieval::rerank::PassthroughReranker;
use clearmemory::retrieval::{self, RecallConfig};
use clearmemory::storage::lance::LanceStorage;
use rusqlite::Connection;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Evaluation framework
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TaskType {
    InformationExtraction,
    TemporalReasoning,
    MultiHopReasoning,
    KnowledgeUpdate,
    Abstraction,
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskType::InformationExtraction => write!(f, "Info Extraction"),
            TaskType::TemporalReasoning => write!(f, "Temporal Reasoning"),
            TaskType::MultiHopReasoning => write!(f, "Multi-hop Reasoning"),
            TaskType::KnowledgeUpdate => write!(f, "Knowledge Update"),
            TaskType::Abstraction => write!(f, "Abstraction"),
        }
    }
}

struct EvalCase {
    query: &'static str,
    expected: Vec<&'static str>,
    task_type: TaskType,
    difficulty: &'static str, // "easy", "medium", "hard"
}

struct EvalResult {
    case_idx: usize,
    task_type: TaskType,
    difficulty: &'static str,
    mrr: f64,
    recall_at_1: f64,
    recall_at_3: f64,
    recall_at_5: f64,
    recall_at_10: f64,
    ndcg_at_10: f64,
    found_by: Vec<Strategy>,
}

fn compute_ndcg(expected: &[&str], results: &[String], k: usize) -> f64 {
    let results_k: Vec<&str> = results.iter().take(k).map(|s| s.as_str()).collect();

    // DCG
    let mut dcg = 0.0;
    for (i, result) in results_k.iter().enumerate() {
        if expected.contains(result) {
            dcg += 1.0 / (i as f64 + 2.0).log2(); // log2(i+2) because rank is 1-indexed
        }
    }

    // Ideal DCG (all expected items at the top)
    let ideal_count = expected.len().min(k);
    let mut idcg = 0.0;
    for i in 0..ideal_count {
        idcg += 1.0 / (i as f64 + 2.0).log2();
    }

    if idcg == 0.0 {
        0.0
    } else {
        dcg / idcg
    }
}

// ---------------------------------------------------------------------------
// Corpus: 200 memories for LongMemEval-style evaluation
// ---------------------------------------------------------------------------

struct Mem {
    id: &'static str,
    summary: &'static str,
    created_at: &'static str,
}

fn build_longmemeval_corpus() -> Vec<Mem> {
    vec![
        // --- Architecture decisions (20) ---
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
        Mem { id: "lme-012", summary: "CDN strategy: CloudFront with Lambda@Edge for dynamic content optimization, S3 for static assets with 1-year cache headers", created_at: "2025-09-20" },
        Mem { id: "lme-013", summary: "Mobile API: implementing GraphQL federation with Apollo Router, each team owns their subgraph schema", created_at: "2025-10-01" },
        Mem { id: "lme-014", summary: "Feature flag system: LaunchDarkly for gradual rollouts, replacing our homegrown config-based system that couldn't handle percentage-based rollouts", created_at: "2025-10-05" },
        Mem { id: "lme-015", summary: "Data warehouse: migrating to Snowflake from Redshift for better separation of compute/storage and support for semi-structured data", created_at: "2025-10-10" },
        Mem { id: "lme-016", summary: "Container orchestration: EKS with Karpenter for node autoscaling, replacing manually managed EC2 instances with ASG", created_at: "2025-10-15" },
        Mem { id: "lme-017", summary: "Secrets management: HashiCorp Vault for all service credentials, rotating database passwords every 24 hours automatically", created_at: "2025-10-20" },
        Mem { id: "lme-018", summary: "Error tracking: Sentry for crash reporting and performance monitoring, replacing Bugsnag due to better source map support and React integration", created_at: "2025-11-01" },
        Mem { id: "lme-019", summary: "Email service: migrating from SendGrid to Amazon SES for transactional email, keeping SendGrid only for marketing campaigns", created_at: "2025-11-05" },
        Mem { id: "lme-020", summary: "Storage architecture: S3 with intelligent tiering for user uploads, presigned URLs for direct upload from client, CloudFront for delivery", created_at: "2025-11-10" },

        // --- Bug fixes and incidents (20) ---
        Mem { id: "lme-021", summary: "Fixed critical memory leak in the payment service: connection pool wasn't releasing connections on timeout, growing unbounded after 6 hours", created_at: "2025-08-05" },
        Mem { id: "lme-022", summary: "Resolved race condition in order processing: two concurrent requests could create duplicate orders when both passed idempotency check simultaneously", created_at: "2025-08-12" },
        Mem { id: "lme-023", summary: "Production incident: 2-hour outage caused by expired TLS certificate on the API gateway. Added automated cert renewal monitoring via CertWatch", created_at: "2025-09-03" },
        Mem { id: "lme-024", summary: "Fixed N+1 query in user profile endpoint that was causing 3-second response times. Added eager loading with DataLoader pattern", created_at: "2025-09-08" },
        Mem { id: "lme-025", summary: "Debugging session: traced intermittent 502 errors to health check misconfiguration in ALB. Pods were being killed during slow startup", created_at: "2025-09-12" },
        Mem { id: "lme-026", summary: "Root cause analysis: notification service dropping messages because SQS visibility timeout was shorter than processing time. Increased to 5 minutes", created_at: "2025-09-18" },
        Mem { id: "lme-027", summary: "Security fix: patched SQL injection vulnerability in the search endpoint. Parameterized all user input to prepared statements", created_at: "2025-10-02" },
        Mem { id: "lme-028", summary: "Performance fix: reduced dashboard load time from 8s to 1.2s by implementing cursor-based pagination and removing unnecessary aggregation queries", created_at: "2025-10-08" },
        Mem { id: "lme-029", summary: "Fixed timezone handling bug: events were displaying in UTC instead of user's local time because moment.js was removed but replacement didn't handle DST", created_at: "2025-10-12" },
        Mem { id: "lme-030", summary: "Incident postmortem: database failover took 15 minutes instead of expected 30 seconds due to stale DNS cache in the application layer. Added TTL=60s", created_at: "2025-10-18" },
        Mem { id: "lme-031", summary: "Fixed file upload timeout: large files (>100MB) were timing out because nginx proxy_read_timeout was set to 60s default. Increased to 300s for upload endpoints", created_at: "2025-11-02" },
        Mem { id: "lme-032", summary: "Resolved Kafka consumer lag: consumer group rebalancing was taking too long because session.timeout.ms was too low. Increased from 10s to 45s", created_at: "2025-11-08" },
        Mem { id: "lme-033", summary: "Fixed CORS issue affecting mobile app: wildcard origin wasn't matching the React Native WebView user agent. Added explicit origin whitelist", created_at: "2025-11-15" },
        Mem { id: "lme-034", summary: "Memory issue in batch processing: CSV import of 1M rows was loading everything into memory. Switched to streaming parser with 10K row chunks", created_at: "2025-11-20" },
        Mem { id: "lme-035", summary: "Hotfix: password reset tokens were not being invalidated after use, allowing replay attacks. Added single-use flag and 15-minute TTL", created_at: "2025-12-01" },
        Mem { id: "lme-036", summary: "Fixed webhook delivery: retry logic was using fixed delay instead of exponential backoff, causing thundering herd on recovery from downstream outages", created_at: "2025-12-05" },
        Mem { id: "lme-037", summary: "Resolved deadlock in transaction service: two services acquiring locks in opposite order on the same account records during concurrent transfers", created_at: "2025-12-10" },
        Mem { id: "lme-038", summary: "Fixed image processing pipeline: thumbnails were being generated at wrong DPI for retina displays. Changed from 72dpi to 144dpi for @2x assets", created_at: "2025-12-15" },
        Mem { id: "lme-039", summary: "Security patch: upgraded Log4j to 2.17.1 across all Java services to address CVE-2021-44228. No evidence of exploitation found in audit logs", created_at: "2025-12-18" },
        Mem { id: "lme-040", summary: "Hotfix: search index corruption after schema migration. Rebuilt Elasticsearch indices from primary data source with zero-downtime reindex", created_at: "2025-12-22" },

        // --- Project planning and status (20) ---
        Mem { id: "lme-041", summary: "Q3 2025 roadmap: (1) complete microservices migration for user and payment services, (2) launch new mobile app, (3) SOC 2 Type I certification", created_at: "2025-07-01" },
        Mem { id: "lme-042", summary: "Sprint 23 retrospective: completed 8 of 10 stories, blocked on payment gateway integration waiting for sandbox access from Stripe", created_at: "2025-08-08" },
        Mem { id: "lme-043", summary: "Project milestone: user service migration complete, 100% traffic now routed to new Go service. Old Rails endpoint deprecated", created_at: "2025-09-01" },
        Mem { id: "lme-044", summary: "Q4 planning: priorities are (1) payment service migration, (2) search overhaul with Elasticsearch, (3) launch analytics dashboard v2", created_at: "2025-10-01" },
        Mem { id: "lme-045", summary: "Sprint 28 review: payment service beta launched to 10% of traffic, latency improved from 450ms to 120ms compared to Rails version", created_at: "2025-11-01" },
        Mem { id: "lme-046", summary: "Technical debt review: identified 47 deprecated API endpoints still receiving traffic, created sunset plan with 90-day migration windows", created_at: "2025-11-15" },
        Mem { id: "lme-047", summary: "Mobile app launch: v1.0 released to App Store and Play Store, 95% crash-free rate in first week, 4.2 star rating", created_at: "2025-12-01" },
        Mem { id: "lme-048", summary: "SOC 2 Type I audit complete: no critical findings, 3 observations around access review cadence and incident response documentation", created_at: "2025-12-10" },
        Mem { id: "lme-049", summary: "Q1 2026 roadmap: (1) SOC 2 Type II prep, (2) international expansion (EU data residency), (3) AI-powered search features", created_at: "2026-01-05" },
        Mem { id: "lme-050", summary: "Budget review: infrastructure costs reduced 35% through reserved instances, right-sizing, and Datadog-to-Grafana migration", created_at: "2026-01-10" },
        Mem { id: "lme-051", summary: "Team restructure: splitting platform team into core-platform (databases, infra) and developer-experience (CI/CD, tooling, onboarding)", created_at: "2026-01-15" },
        Mem { id: "lme-052", summary: "Sprint 33 retro: velocity improved 20% after switching to trunk-based development, average PR merge time down from 2 days to 4 hours", created_at: "2026-01-20" },

        // --- Knowledge updates (facts that change over time) (20) ---
        Mem { id: "lme-053", summary: "Database connection pool configuration: max_pool_size=20, min_idle=5, connection_timeout=30s, idle_timeout=600s", created_at: "2025-07-10" },
        Mem { id: "lme-054", summary: "Updated database pool config: increased max_pool_size from 20 to 50 after traffic growth, reduced idle_timeout to 300s to free connections faster", created_at: "2025-11-10" },
        Mem { id: "lme-055", summary: "API rate limits: 100 requests per minute for free tier, 1000/min for pro tier, 10000/min for enterprise tier", created_at: "2025-08-01" },
        Mem { id: "lme-056", summary: "Rate limits updated: free tier increased to 200/min after customer feedback, enterprise tier now 50000/min to support batch processing use case", created_at: "2026-02-01" },
        Mem { id: "lme-057", summary: "Primary on-call rotation: Alice (Mon-Wed), Bob (Thu-Fri), Charlie (weekends). Escalation to Sarah (engineering manager) after 30 minutes", created_at: "2025-08-15" },
        Mem { id: "lme-058", summary: "On-call rotation changed: Alice moved to platform team, replaced by David. New rotation: David (Mon-Wed), Bob (Thu-Fri), Eva (weekends)", created_at: "2026-01-15" },
        Mem { id: "lme-059", summary: "Deployment cadence: deploying to production twice per week (Tuesday and Thursday) after staging validation", created_at: "2025-07-01" },
        Mem { id: "lme-060", summary: "Deployment cadence updated: moving to continuous deployment with feature flags, any merged PR deploys to production within 30 minutes", created_at: "2026-01-01" },
        Mem { id: "lme-061", summary: "Auth provider is Auth0 with custom rules for MFA enforcement and role-based access control", created_at: "2025-07-15" },
        Mem { id: "lme-062", summary: "Completed migration from Auth0 to Clerk. All Auth0 tenants decommissioned. Clerk handles MFA, RBAC, and session management", created_at: "2026-02-15" },
        Mem { id: "lme-063", summary: "Frontend build tool: webpack 5 with babel for transpilation, taking 4 minutes for production build", created_at: "2025-08-01" },
        Mem { id: "lme-064", summary: "Migrated frontend build from webpack to Vite. Production build time reduced from 4 minutes to 45 seconds. HMR now under 100ms", created_at: "2025-12-15" },

        // --- Technical deep dives (20) ---
        Mem { id: "lme-065", summary: "Database indexing strategy: composite index on (tenant_id, created_at DESC) for the orders table reduced query time from 2.3s to 12ms for tenant-scoped queries", created_at: "2025-08-20" },
        Mem { id: "lme-066", summary: "Implemented circuit breaker pattern for external API calls using Hystrix-go with 5s timeout, 10 error threshold, 30s recovery window", created_at: "2025-09-05" },
        Mem { id: "lme-067", summary: "WebSocket implementation for real-time notifications using Socket.IO with Redis adapter for horizontal scaling across multiple pods", created_at: "2025-09-25" },
        Mem { id: "lme-068", summary: "Implemented CQRS pattern for order service: write side uses PostgreSQL, read side uses Elasticsearch denormalized views updated via Kafka events", created_at: "2025-10-15" },
        Mem { id: "lme-069", summary: "Rate limiting implementation: token bucket algorithm with Redis backend, sliding window for more accurate limiting, Lua scripts for atomicity", created_at: "2025-10-25" },
        Mem { id: "lme-070", summary: "Implemented data pipeline: Kafka → Flink for real-time processing, Kafka → S3 → Spark for batch analytics, unified schema registry with Avro", created_at: "2025-11-05" },
        Mem { id: "lme-071", summary: "Zero-downtime database migration strategy: dual-write to old and new schema, backfill historical data, validate consistency, switch reads, drop old columns", created_at: "2025-11-12" },
        Mem { id: "lme-072", summary: "Implemented distributed tracing with OpenTelemetry: auto-instrumentation for Go services, manual spans for critical business logic paths", created_at: "2025-11-18" },

        // --- Security and compliance (20) ---
        Mem { id: "lme-073", summary: "Security audit finding: API keys stored in plaintext in environment variables on developer machines. Implementing HashiCorp Vault for all secrets", created_at: "2025-08-25" },
        Mem { id: "lme-074", summary: "GDPR compliance: implemented right-to-erasure endpoint that cascades deletion across all microservices within 72 hours of request", created_at: "2025-09-15" },
        Mem { id: "lme-075", summary: "Penetration test results: 2 critical (XSS in admin panel, IDOR in user API), 5 high, 12 medium findings. All critical fixed within 48 hours", created_at: "2025-10-20" },
        Mem { id: "lme-076", summary: "Implemented encryption at rest for all PII fields: AES-256-GCM with per-tenant keys, key rotation every 90 days via KMS", created_at: "2025-11-01" },
        Mem { id: "lme-077", summary: "Access control audit: revoked 23 unused service accounts, implemented just-in-time access for production database with 4-hour TTL", created_at: "2025-11-20" },
        Mem { id: "lme-078", summary: "Implemented Content Security Policy headers: strict CSP with nonce-based script loading, blocking inline styles, reporting violations to Sentry", created_at: "2025-12-05" },
        Mem { id: "lme-079", summary: "Set up automated dependency scanning with Snyk in CI pipeline. Blocking merges on critical/high vulnerabilities with SLA: critical=24h, high=7d", created_at: "2025-12-12" },
        Mem { id: "lme-080", summary: "SOC 2 evidence collection: automated screenshot collection for access reviews, change management logs exported from GitHub, incident tickets from PagerDuty", created_at: "2026-01-05" },

        // --- Team and process (20) ---
        Mem { id: "lme-081", summary: "Engineering onboarding: new hire gets local dev environment running on day 1, paired with a buddy for first 2 weeks, ships first PR in week 1", created_at: "2025-08-01" },
        Mem { id: "lme-082", summary: "Code review guidelines: all PRs require 1 approval minimum, critical paths require 2, max 400 lines per PR, review within 4 hours SLA", created_at: "2025-08-15" },
        Mem { id: "lme-083", summary: "Incident response process: page on-call via PagerDuty, declare severity within 5 minutes, status page update within 10 minutes, postmortem within 48 hours", created_at: "2025-09-01" },
        Mem { id: "lme-084", summary: "Architecture Decision Records: all significant technical decisions documented as ADRs in the monorepo, reviewed in weekly architecture meeting", created_at: "2025-09-15" },
        Mem { id: "lme-085", summary: "Testing strategy: unit tests required for all business logic (80% coverage), integration tests for API endpoints, E2E tests for critical user journeys only", created_at: "2025-10-01" },

        // --- Conversations and discussions (28) ---
        Mem { id: "lme-086", summary: "Discussion with Sarah about scaling the notification service: considered SNS fan-out pattern but decided on direct Kafka consumers for better ordering guarantees", created_at: "2025-10-10" },
        Mem { id: "lme-087", summary: "Debate in architecture review: should we build or buy for the analytics pipeline? Decided to build on Flink because our use case requires custom windowing logic", created_at: "2025-10-20" },
        Mem { id: "lme-088", summary: "Meeting with product team: users complaining about slow search. Agreed to prioritize Elasticsearch migration and add autocomplete/typo tolerance", created_at: "2025-11-01" },
        Mem { id: "lme-089", summary: "1:1 with Bob: he's interested in moving to the platform team. Discussed timeline — can transition after payment service migration is stable, estimated March 2026", created_at: "2025-11-10" },
        Mem { id: "lme-090", summary: "Design review: new checkout flow reduces steps from 5 to 3, adds Apple Pay and Google Pay, A/B test planned for January with 20% traffic allocation", created_at: "2025-12-01" },
        Mem { id: "lme-091", summary: "Discussion about technical debt: the legacy billing module has no tests and 5 known bugs. Decided to rewrite rather than patch, estimated 3 sprints", created_at: "2025-12-10" },
        Mem { id: "lme-092", summary: "Vendor evaluation: compared Twilio vs MessageBird vs Vonage for SMS. Twilio wins on reliability and API quality despite 15% higher cost", created_at: "2026-01-05" },
        Mem { id: "lme-093", summary: "AI feature discussion: evaluating embedding models for semantic search. BGE-M3 offers best multilingual support, but all-MiniLM is 10x faster for English-only use case", created_at: "2026-01-15" },
        Mem { id: "lme-094", summary: "Post-incident review for January 15 outage: root cause was a Kubernetes node running out of disk space due to container log rotation misconfiguration", created_at: "2026-01-20" },
        Mem { id: "lme-095", summary: "Decision to implement blue-green deployments for the payment service due to zero-tolerance for downtime in financial transactions", created_at: "2026-02-01" },
        Mem { id: "lme-096", summary: "Meeting with legal: new EU AI Act requirements may affect our recommendation engine. Need to document model inputs, outputs, and bias testing by June 2026", created_at: "2026-02-10" },
        Mem { id: "lme-097", summary: "Performance review of Redis cluster: 99.99% uptime over 6 months, average latency 0.5ms, peak at 2.1ms during Black Friday. No capacity concerns until 2027", created_at: "2026-02-15" },
        Mem { id: "lme-098", summary: "Frontend accessibility audit: 34 WCAG 2.1 violations found, 8 critical (missing alt text, insufficient color contrast). Sprint dedicated to fixes in March", created_at: "2026-02-20" },
        Mem { id: "lme-099", summary: "Discussion about adopting Rust for performance-critical services: the image processing pipeline and real-time matching engine are candidates", created_at: "2026-03-01" },
        Mem { id: "lme-100", summary: "Quarterly security review: 0 critical vulnerabilities in production, 2 medium in staging, all dependency patches applied within SLA", created_at: "2026-03-15" },

        // Pad to 200 with more diverse content
        Mem { id: "lme-101", summary: "Implemented connection pooling with PgBouncer in transaction mode, reducing PostgreSQL connection count from 500 to 50 active connections", created_at: "2025-08-10" },
        Mem { id: "lme-102", summary: "Set up canary deployments: 5% traffic to new version for 30 minutes, automated rollback if error rate exceeds 1% or p99 latency exceeds 500ms", created_at: "2025-08-25" },
        Mem { id: "lme-103", summary: "Implemented idempotency keys for all payment API endpoints using Redis with 24-hour TTL to prevent duplicate charges", created_at: "2025-09-10" },
        Mem { id: "lme-104", summary: "Load test results: system handles 10,000 concurrent users with p95 latency under 200ms. Bottleneck is database connection pool at 15,000 users", created_at: "2025-09-20" },
        Mem { id: "lme-105", summary: "Migrated from Docker Hub to ECR for container images: faster pulls within AWS, vulnerability scanning with Inspector, lifecycle policies for cleanup", created_at: "2025-10-05" },
        Mem { id: "lme-106", summary: "Implemented structured logging with JSON format across all services. Correlation IDs propagated via HTTP headers and Kafka message headers", created_at: "2025-10-15" },
        Mem { id: "lme-107", summary: "DNS migration from Route53 to Cloudflare for better DDoS protection and faster global resolution. Kept Route53 for internal service discovery", created_at: "2025-10-25" },
        Mem { id: "lme-108", summary: "Implemented graceful shutdown for all Go services: drain connections for 30s, finish in-flight requests, flush metrics, then exit", created_at: "2025-11-05" },
        Mem { id: "lme-109", summary: "Set up database read replicas: 2 replicas in us-east-1 for read scaling, 1 cross-region replica in eu-west-1 for disaster recovery", created_at: "2025-11-15" },
        Mem { id: "lme-110", summary: "Implemented API versioning: URL-based (/v1, /v2) for breaking changes, header-based for minor variations, sunset headers for deprecated versions", created_at: "2025-11-25" },
        Mem { id: "lme-111", summary: "Set up cost allocation tags in AWS: by team, environment, and project. Monthly Slack report showing cost trends and anomalies per team", created_at: "2025-12-05" },
        Mem { id: "lme-112", summary: "Implemented request throttling with circuit breaker: if downstream service returns 5xx for >50% of requests in 10s window, open circuit for 30s", created_at: "2025-12-15" },
        Mem { id: "lme-113", summary: "Set up automated compliance checks: CIS benchmarks for AWS, OWASP ZAP for web apps, Trivy for container images, all running in nightly pipeline", created_at: "2026-01-01" },
        Mem { id: "lme-114", summary: "Implemented data masking for non-production environments: PII fields (email, phone, address) replaced with realistic fake data using Faker library", created_at: "2026-01-10" },
        Mem { id: "lme-115", summary: "Set up monitoring dashboards: service health (golden signals), business metrics (orders, revenue, conversion), infrastructure (CPU, memory, disk, network)", created_at: "2026-01-20" },
        Mem { id: "lme-116", summary: "Implemented request coalescing for the product catalog API: multiple identical requests within 100ms window are served from a single backend call", created_at: "2026-02-01" },
        Mem { id: "lme-117", summary: "Set up chaos engineering: weekly Game Days with Chaos Monkey targeting random pod termination, network latency injection, and disk pressure simulation", created_at: "2026-02-10" },
        Mem { id: "lme-118", summary: "Implemented geo-routing: EU users routed to eu-west-1, US users to us-east-1, Asia users to ap-southeast-1, with automatic failover between regions", created_at: "2026-02-20" },
        Mem { id: "lme-119", summary: "Migrated from ELK stack to Grafana Loki for log aggregation: 60% cost reduction, simpler operations, native Grafana integration", created_at: "2026-03-01" },
        Mem { id: "lme-120", summary: "Implemented progressive image loading: LQIP (Low Quality Image Placeholder) with blur-up animation, WebP format with AVIF fallback, responsive srcset", created_at: "2026-03-10" },

        // More padding for diverse coverage
        Mem { id: "lme-121", summary: "Schema migration: added soft delete (deleted_at timestamp) to all tables, rewrote all DELETE queries to UPDATE, added retention policy for hard delete after 90 days", created_at: "2025-07-20" },
        Mem { id: "lme-122", summary: "Implemented multi-tenant isolation at the database level: row-level security policies in PostgreSQL, tenant_id required on every query, audit log for cross-tenant access", created_at: "2025-08-05" },
        Mem { id: "lme-123", summary: "Set up automated database backup: daily full backup to S3, hourly WAL archiving, point-in-time recovery capability, tested restore monthly", created_at: "2025-08-18" },
        Mem { id: "lme-124", summary: "Implemented saga pattern for distributed transactions across order, payment, and inventory services with compensating transactions for rollback", created_at: "2025-09-08" },
        Mem { id: "lme-125", summary: "Set up developer portal with Backstage: service catalog, API docs, onboarding guides, tech radar, and internal tool directory", created_at: "2025-09-22" },

        // Duplicate/updated facts for knowledge update testing
        Mem { id: "lme-126", summary: "Current primary database: PostgreSQL 16 on RDS with Multi-AZ deployment, 500GB storage, db.r6g.2xlarge instance, automated backups", created_at: "2026-03-01" },
        Mem { id: "lme-127", summary: "Current auth provider is Clerk. Previous provider Auth0 was decommissioned in February 2026. Migration took 3 months and affected 50K users", created_at: "2026-03-05" },
        Mem { id: "lme-128", summary: "Current deployment process: trunk-based development, feature flags via LaunchDarkly, continuous deployment to production within 30 minutes of merge", created_at: "2026-03-10" },
    ]
}

// ---------------------------------------------------------------------------
// Test cases: 80 queries across 5 LongMemEval task types
// ---------------------------------------------------------------------------

fn build_eval_cases() -> Vec<EvalCase> {
    vec![
        // ===== INFORMATION EXTRACTION (20 queries) =====
        EvalCase {
            query: "what database did we choose and why",
            expected: vec!["lme-002"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "Kafka vs RabbitMQ decision",
            expected: vec!["lme-003"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "which API gateway do we use",
            expected: vec!["lme-004"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "authentication provider decision",
            expected: vec!["lme-005", "lme-062"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "how do services communicate internally",
            expected: vec!["lme-006"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "what caching solution did we pick",
            expected: vec!["lme-007"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "search infrastructure technology choice",
            expected: vec!["lme-008"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "frontend state management library",
            expected: vec!["lme-009"],
            task_type: TaskType::InformationExtraction,
            difficulty: "medium",
        },
        EvalCase {
            query: "CI/CD pipeline platform",
            expected: vec!["lme-010"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "observability and monitoring stack",
            expected: vec!["lme-011"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "connection pool configuration for PostgreSQL",
            expected: vec!["lme-054"],
            task_type: TaskType::InformationExtraction,
            difficulty: "medium",
        },
        EvalCase {
            query: "API rate limiting thresholds",
            expected: vec!["lme-056"],
            task_type: TaskType::InformationExtraction,
            difficulty: "medium",
        },
        EvalCase {
            query: "who is on call this week",
            expected: vec!["lme-058"],
            task_type: TaskType::InformationExtraction,
            difficulty: "medium",
        },
        EvalCase {
            query: "testing requirements and coverage goals",
            expected: vec!["lme-085"],
            task_type: TaskType::InformationExtraction,
            difficulty: "medium",
        },
        EvalCase {
            query: "code review process and requirements",
            expected: vec!["lme-082"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "incident response procedure",
            expected: vec!["lme-083"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "secrets management approach",
            expected: vec!["lme-017"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "container orchestration platform",
            expected: vec!["lme-016"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "feature flag system",
            expected: vec!["lme-014"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "data warehouse technology",
            expected: vec!["lme-015"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        // ===== TEMPORAL REASONING (15 queries) =====
        EvalCase {
            query: "architecture decisions in July 2025",
            expected: vec!["lme-001", "lme-002", "lme-003"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "easy",
        },
        EvalCase {
            query: "what bugs were fixed in October 2025",
            expected: vec!["lme-029", "lme-030"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "medium",
        },
        EvalCase {
            query: "incidents and outages in September",
            expected: vec!["lme-023", "lme-025"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "medium",
        },
        EvalCase {
            query: "what happened in Q4 2025",
            expected: vec!["lme-044", "lme-045", "lme-046", "lme-047", "lme-048"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "security work done in November 2025",
            expected: vec!["lme-076", "lme-077"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "medium",
        },
        EvalCase {
            query: "what was decided in the first two weeks of January 2026",
            expected: vec!["lme-049", "lme-050", "lme-051"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "recent changes in March 2026",
            expected: vec!["lme-099", "lme-100", "lme-126"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "medium",
        },
        EvalCase {
            query: "what were the Q1 2026 plans",
            expected: vec!["lme-049"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "easy",
        },
        EvalCase {
            query: "production incidents in December",
            expected: vec!["lme-039", "lme-040"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "medium",
        },
        EvalCase {
            query: "mobile app release date and metrics",
            expected: vec!["lme-047"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "easy",
        },
        EvalCase {
            query: "what was done in the last quarter of 2025",
            expected: vec!["lme-044", "lme-045", "lme-046", "lme-047", "lme-048"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "deployment process changes over time",
            expected: vec!["lme-059", "lme-060", "lme-128"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "when did we migrate from Auth0 to Clerk",
            expected: vec!["lme-062", "lme-127"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "medium",
        },
        EvalCase {
            query: "database configuration changes",
            expected: vec!["lme-053", "lme-054"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "medium",
        },
        EvalCase {
            query: "rate limit changes",
            expected: vec!["lme-055", "lme-056"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "medium",
        },
        // ===== MULTI-HOP REASONING (15 queries) =====
        EvalCase {
            query: "technologies we evaluated but rejected",
            expected: vec!["lme-002", "lme-003", "lme-004", "lme-009"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "what infrastructure changes reduced costs",
            expected: vec!["lme-011", "lme-050", "lme-119"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "security improvements across the system",
            expected: vec!["lme-027", "lme-073", "lme-076", "lme-079"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "all the performance optimizations we made",
            expected: vec!["lme-024", "lme-028", "lme-065", "lme-101", "lme-116"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "decisions related to the payment service",
            expected: vec!["lme-021", "lme-045", "lme-095", "lme-103"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "everything about authentication and login",
            expected: vec!["lme-005", "lme-061", "lme-062", "lme-127"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "medium",
        },
        EvalCase {
            query: "how we handle data privacy and compliance",
            expected: vec!["lme-074", "lme-076", "lme-114"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "frontend technology decisions and changes",
            expected: vec!["lme-009", "lme-063", "lme-064", "lme-098"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "database related decisions and incidents",
            expected: vec!["lme-002", "lme-030", "lme-065", "lme-109"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "all Kubernetes and container related work",
            expected: vec!["lme-016", "lme-025", "lme-094", "lme-105"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "vendor evaluations and selections",
            expected: vec!["lme-004", "lme-018", "lme-092"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "medium",
        },
        EvalCase {
            query: "monitoring and alerting setup",
            expected: vec!["lme-011", "lme-072", "lme-106", "lme-115"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "API design decisions",
            expected: vec!["lme-001", "lme-013", "lme-069", "lme-110"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "team process improvements",
            expected: vec!["lme-052", "lme-082", "lme-084"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "medium",
        },
        EvalCase {
            query: "SOC 2 and compliance efforts",
            expected: vec!["lme-048", "lme-080", "lme-113"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "medium",
        },
        // ===== KNOWLEDGE UPDATE (15 queries) =====
        EvalCase {
            query: "what is our current authentication provider",
            expected: vec!["lme-062", "lme-127"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "medium",
        },
        EvalCase {
            query: "current database pool size",
            expected: vec!["lme-054"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "medium",
        },
        EvalCase {
            query: "what are the current API rate limits",
            expected: vec!["lme-056"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "medium",
        },
        EvalCase {
            query: "who handles on-call right now",
            expected: vec!["lme-058"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "medium",
        },
        EvalCase {
            query: "how do we deploy to production today",
            expected: vec!["lme-060", "lme-128"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "medium",
        },
        EvalCase {
            query: "current frontend build tooling",
            expected: vec!["lme-064"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "medium",
        },
        EvalCase {
            query: "what database are we using now",
            expected: vec!["lme-002", "lme-126"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "easy",
        },
        EvalCase {
            query: "current log aggregation solution",
            expected: vec!["lme-119"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "medium",
        },
        EvalCase {
            query: "latest penetration test findings",
            expected: vec!["lme-075"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "medium",
        },
        EvalCase {
            query: "current deployment cadence and process",
            expected: vec!["lme-060", "lme-128"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "medium",
        },
        EvalCase {
            query: "did we ever use Auth0",
            expected: vec!["lme-061", "lme-062", "lme-127"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "hard",
        },
        EvalCase {
            query: "what changed about our on-call rotation",
            expected: vec!["lme-057", "lme-058"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "hard",
        },
        EvalCase {
            query: "how has our build tool changed",
            expected: vec!["lme-063", "lme-064"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "hard",
        },
        EvalCase {
            query: "rate limit history",
            expected: vec!["lme-055", "lme-056"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "hard",
        },
        EvalCase {
            query: "connection pool size changes",
            expected: vec!["lme-053", "lme-054"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "hard",
        },
        // ===== ABSTRACTION (15 queries) =====
        EvalCase {
            query: "summarize our microservices architecture",
            expected: vec!["lme-001", "lme-006", "lme-013"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "what is our security posture",
            expected: vec!["lme-017", "lme-076", "lme-079", "lme-100"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "how mature is our CI/CD pipeline",
            expected: vec!["lme-010", "lme-060", "lme-102"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "what does our data pipeline look like",
            expected: vec!["lme-070", "lme-015"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "how do we handle high availability",
            expected: vec!["lme-095", "lme-109", "lme-118"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "describe our testing practices",
            expected: vec!["lme-085", "lme-104", "lme-117"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "what is our approach to developer experience",
            expected: vec!["lme-081", "lme-125", "lme-052"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "overall system reliability",
            expected: vec!["lme-066", "lme-108", "lme-112"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "how we handle data at scale",
            expected: vec!["lme-068", "lme-070", "lme-116"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "cost optimization efforts",
            expected: vec!["lme-011", "lme-050", "lme-111", "lme-119"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "compliance readiness assessment",
            expected: vec!["lme-048", "lme-074", "lme-080", "lme-113"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "production incident patterns",
            expected: vec!["lme-023", "lme-030", "lme-094"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "how we approach migrations",
            expected: vec!["lme-043", "lme-062", "lme-071"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "real-time features architecture",
            expected: vec!["lme-067", "lme-007", "lme-003"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "upcoming priorities and roadmap",
            expected: vec!["lme-049", "lme-096"],
            task_type: TaskType::Abstraction,
            difficulty: "medium",
        },
    ]
}

// ---------------------------------------------------------------------------
// Test runner
// ---------------------------------------------------------------------------

fn setup_corpus(conn: &Connection) {
    migration::runner::run_migrations(conn).unwrap();
    let corpus = build_longmemeval_corpus();

    for mem in &corpus {
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) VALUES (?1, ?2, ?3, 'clear', ?4)",
            rusqlite::params![mem.id, format!("hash-{}", mem.id), mem.summary, mem.created_at],
        ).unwrap();
    }
}

fn run_evaluation(
    conn: &Connection,
    lance: &LanceStorage,
    embedder: Option<&clearmemory::storage::embeddings::EmbeddingManager>,
    cases: &[EvalCase],
    summaries: &HashMap<String, String>,
    label: &str,
) {
    let resolver = HeuristicResolver;
    let reranker = PassthroughReranker;

    let config = RecallConfig {
        top_k: 10,
        temporal_boost: 0.4,
        entity_boost: 0.3,
        include_archived: false,
        stream_id: None,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut results: Vec<EvalResult> = Vec::new();
    let mut missed: Vec<(usize, &str, Vec<&str>)> = Vec::new();

    for (i, case) in cases.iter().enumerate() {
        let query_vec = embedder.and_then(|e| e.embed_query(case.query).ok());
        let query_slice = query_vec.as_deref();

        let recall_result = rt
            .block_on(retrieval::recall(
                case.query,
                conn,
                lance,
                query_slice,
                &resolver,
                &reranker,
                summaries,
                &config,
            ))
            .unwrap();

        let result_ids: Vec<String> = recall_result
            .results
            .iter()
            .map(|r| r.memory_id.clone())
            .collect();
        let found_by: Vec<Strategy> = recall_result
            .results
            .iter()
            .flat_map(|_r| {
                // The reranked results don't carry strategy info directly,
                // but we track which strategies contributed overall
                Vec::new() as Vec<Strategy>
            })
            .collect();

        // Compute metrics for this case
        let first_rank = case
            .expected
            .iter()
            .filter_map(|e| result_ids.iter().position(|r| r == *e))
            .min();

        let mrr = first_rank.map(|r| 1.0 / (r as f64 + 1.0)).unwrap_or(0.0);

        let recall_at = |k: usize| -> f64 {
            let found = case
                .expected
                .iter()
                .filter(|e| result_ids.iter().take(k).any(|r| r == **e))
                .count();
            if case.expected.is_empty() {
                0.0
            } else {
                found as f64 / case.expected.len() as f64
            }
        };

        let ndcg = compute_ndcg(&case.expected, &result_ids, 10);

        let r1 = recall_at(1);
        let r3 = recall_at(3);
        let r5 = recall_at(5);
        let r10 = recall_at(10);

        if r10 < 1.0 {
            let missing: Vec<&str> = case
                .expected
                .iter()
                .filter(|e| !result_ids.iter().take(10).any(|r| r == **e))
                .copied()
                .collect();
            missed.push((i, case.query, missing));
        }

        results.push(EvalResult {
            case_idx: i,
            task_type: case.task_type,
            difficulty: case.difficulty,
            mrr,
            recall_at_1: r1,
            recall_at_3: r3,
            recall_at_5: r5,
            recall_at_10: r10,
            ndcg_at_10: ndcg,
            found_by,
        });
    }

    // Aggregate and print results
    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  LONGMEMEVAL-STYLE BENCHMARK: {:<30} ║", label);
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!(
        "║  Corpus: {} memories | Queries: {:<26} ║",
        build_longmemeval_corpus().len(),
        cases.len()
    );
    println!("╠══════════════════════════════════════════════════════════════╣");

    // Overall metrics
    let n = results.len() as f64;
    let avg = |f: fn(&EvalResult) -> f64| -> f64 { results.iter().map(f).sum::<f64>() / n };

    let overall_mrr = avg(|r| r.mrr);
    let overall_r1 = avg(|r| r.recall_at_1);
    let overall_r3 = avg(|r| r.recall_at_3);
    let overall_r5 = avg(|r| r.recall_at_5);
    let overall_r10 = avg(|r| r.recall_at_10);
    let overall_ndcg = avg(|r| r.ndcg_at_10);

    println!("║                                                              ║");
    println!("║  OVERALL METRICS                                             ║");
    println!("║  ─────────────────────────────────────────────               ║");
    println!(
        "║  MRR:        {:.4}                                          ║",
        overall_mrr
    );
    println!(
        "║  Recall@1:   {:.4}                                          ║",
        overall_r1
    );
    println!(
        "║  Recall@3:   {:.4}                                          ║",
        overall_r3
    );
    println!(
        "║  Recall@5:   {:.4}                                          ║",
        overall_r5
    );
    println!(
        "║  Recall@10:  {:.4}                                          ║",
        overall_r10
    );
    println!(
        "║  NDCG@10:    {:.4}                                          ║",
        overall_ndcg
    );

    // Per task-type breakdown
    println!("║                                                              ║");
    println!("║  PER TASK TYPE                                               ║");
    println!("║  ─────────────────────────────────────────────               ║");
    println!(
        "║  {:<22} {:>6} {:>7} {:>7} {:>8}       ║",
        "Task Type", "Count", "MRR", "R@5", "R@10"
    );
    println!(
        "║  {:<22} {:>6} {:>7} {:>7} {:>8}       ║",
        "──────────────────────", "─────", "──────", "──────", "───────"
    );

    for task_type in &[
        TaskType::InformationExtraction,
        TaskType::TemporalReasoning,
        TaskType::MultiHopReasoning,
        TaskType::KnowledgeUpdate,
        TaskType::Abstraction,
    ] {
        let task_results: Vec<&EvalResult> = results
            .iter()
            .filter(|r| r.task_type == *task_type)
            .collect();
        if task_results.is_empty() {
            continue;
        }
        let tn = task_results.len() as f64;
        let t_mrr = task_results.iter().map(|r| r.mrr).sum::<f64>() / tn;
        let t_r5 = task_results.iter().map(|r| r.recall_at_5).sum::<f64>() / tn;
        let t_r10 = task_results.iter().map(|r| r.recall_at_10).sum::<f64>() / tn;
        println!(
            "║  {:<22} {:>6} {:>7.4} {:>7.4} {:>8.4}       ║",
            task_type,
            task_results.len(),
            t_mrr,
            t_r5,
            t_r10
        );
    }

    // Per difficulty breakdown
    println!("║                                                              ║");
    println!("║  PER DIFFICULTY                                              ║");
    println!("║  ─────────────────────────────────────────────               ║");
    for difficulty in &["easy", "medium", "hard"] {
        let diff_results: Vec<&EvalResult> = results
            .iter()
            .filter(|r| r.difficulty == *difficulty)
            .collect();
        if diff_results.is_empty() {
            continue;
        }
        let dn = diff_results.len() as f64;
        let d_r10 = diff_results.iter().map(|r| r.recall_at_10).sum::<f64>() / dn;
        println!(
            "║  {:<10} ({:>2} queries): Recall@10 = {:.4}                  ║",
            difficulty,
            diff_results.len(),
            d_r10
        );
    }

    // Failures
    if !missed.is_empty() {
        println!("║                                                              ║");
        println!(
            "║  MISSED QUERIES ({})                                       ║",
            missed.len()
        );
        println!("║  ─────────────────────────────────────────────               ║");
        for (idx, query, missing) in &missed {
            let truncated: String = if query.len() > 45 {
                format!("{}...", &query[..42])
            } else {
                query.to_string()
            };
            println!("║  #{:<3} {:<48} ║", idx, truncated);
            println!(
                "║       missing: {:?}{} ║",
                &missing[..missing.len().min(3)],
                " ".repeat(
                    45 - format!("{:?}", &missing[..missing.len().min(3)])
                        .len()
                        .min(44)
                )
            );
        }
    }

    println!("║                                                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // Assert minimum quality thresholds
    assert!(
        overall_r10 >= 0.50,
        "Recall@10 ({overall_r10:.4}) below minimum threshold 0.50 for {label}"
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_longmemeval_keyword_only() {
    let conn = Connection::open_in_memory().unwrap();
    setup_corpus(&conn);

    let corpus = build_longmemeval_corpus();
    let summaries: HashMap<String, String> = corpus
        .iter()
        .map(|m| (m.id.to_string(), m.summary.to_string()))
        .collect();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let lance = rt
        .block_on(LanceStorage::open(dir.path().join("vectors")))
        .unwrap();

    let cases = build_eval_cases();
    run_evaluation(
        &conn,
        &lance,
        None,
        &cases,
        &summaries,
        "Keyword + Temporal (no model)",
    );
}

#[test]
#[ignore] // Requires embedding model download
fn test_longmemeval_full_pipeline() {
    let conn = Connection::open_in_memory().unwrap();
    setup_corpus(&conn);

    let corpus = build_longmemeval_corpus();
    let summaries: HashMap<String, String> = corpus
        .iter()
        .map(|m| (m.id.to_string(), m.summary.to_string()))
        .collect();

    // Load embedding model and create vectors
    let manager = clearmemory::storage::embeddings::EmbeddingManager::new("bge-small-en").unwrap();
    let dim = manager.dimensions();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let lance = rt
        .block_on(LanceStorage::open_with_dim(
            dir.path().join("vectors"),
            dim as i32,
        ))
        .unwrap();

    // Embed and index all memories
    println!(
        "Indexing {} memories with BGE-Small-EN ({dim}-dim)...",
        corpus.len()
    );
    for mem in &corpus {
        let embedding = manager.embed_query(mem.summary).unwrap();
        rt.block_on(lance.insert(mem.id, &embedding, None)).unwrap();
    }
    println!("Indexing complete.");

    let cases = build_eval_cases();

    run_evaluation(
        &conn,
        &lance,
        Some(&manager),
        &cases,
        &summaries,
        "Full Pipeline (BGE-Small-EN 384d)",
    );
}

/// Publication-grade benchmark with BGE-M3 (1024 dimensions).
///
/// This is the production embedding model. ~600MB download on first run.
/// BGE-M3 scores ~15-20% higher than BGE-Small on MTEB retrieval benchmarks.
///
/// Run: `cargo test --test benchmark_longmemeval test_longmemeval_bge_m3 -- --nocapture --ignored`
#[test]
#[ignore] // Requires ~600MB model download
fn test_longmemeval_bge_m3() {
    let conn = Connection::open_in_memory().unwrap();
    setup_corpus(&conn);

    let corpus = build_longmemeval_corpus();
    let summaries: HashMap<String, String> = corpus
        .iter()
        .map(|m| (m.id.to_string(), m.summary.to_string()))
        .collect();

    let manager = clearmemory::storage::embeddings::EmbeddingManager::new("bge-m3").unwrap();
    let dim = manager.dimensions();
    assert_eq!(dim, 1024, "BGE-M3 should produce 1024-dim vectors");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let lance = rt
        .block_on(LanceStorage::open_with_dim(
            dir.path().join("vectors"),
            dim as i32,
        ))
        .unwrap();

    println!(
        "Indexing {} memories with BGE-M3 ({dim}-dim)...",
        corpus.len()
    );
    for mem in &corpus {
        let embedding = manager.embed_query(mem.summary).unwrap();
        rt.block_on(lance.insert(mem.id, &embedding, None)).unwrap();
    }
    println!("Indexing complete.");

    let cases = build_eval_cases();

    run_evaluation(
        &conn,
        &lance,
        Some(&manager),
        &cases,
        &summaries,
        "Full Pipeline (BGE-M3 1024d)",
    );
}

// ---------------------------------------------------------------------------
// Large corpus: 500 memories for scaled LongMemEval evaluation
// ---------------------------------------------------------------------------

fn build_large_corpus() -> Vec<Mem> {
    let mut corpus = build_longmemeval_corpus();

    let additional = vec![
        // --- More architecture decisions (50) ---
        Mem { id: "lme-129", summary: "Architecture decision: adopting Terraform for all infrastructure-as-code, replacing manual AWS Console provisioning across all environments", created_at: "2025-04-05" },
        Mem { id: "lme-130", summary: "Selected MongoDB Atlas for the product catalog service due to flexible schema requirements and native full-text search for catalog browsing", created_at: "2025-04-12" },
        Mem { id: "lme-131", summary: "Chose Istio service mesh over Linkerd for mTLS between microservices, traffic shaping, and canary routing at the network level", created_at: "2025-04-20" },
        Mem { id: "lme-132", summary: "Adopted ArgoCD for GitOps-style continuous delivery to Kubernetes clusters, replacing Helm-based manual deployments", created_at: "2025-05-01" },
        Mem { id: "lme-133", summary: "Decision to use Pulumi over Terraform for the data platform team because they prefer TypeScript over HCL for infrastructure definitions", created_at: "2025-05-10" },
        Mem { id: "lme-134", summary: "Selected CockroachDB for the multi-region inventory service requiring strong consistency with geo-distributed writes and automatic sharding", created_at: "2025-05-15" },
        Mem { id: "lme-135", summary: "Adopted OpenPolicy Agent (OPA) for centralized policy enforcement across Kubernetes admission control, API authorization, and Terraform plans", created_at: "2025-05-22" },
        Mem { id: "lme-136", summary: "Architecture review: decided to use NATS JetStream over Kafka for the IoT telemetry pipeline due to lower latency and simpler operations at edge nodes", created_at: "2025-06-01" },
        Mem { id: "lme-137", summary: "Chose TimescaleDB over InfluxDB for time-series metrics storage because it runs on PostgreSQL and our team already has deep Postgres expertise", created_at: "2025-06-08" },
        Mem { id: "lme-138", summary: "Adopted Temporal.io as the workflow orchestration engine for long-running business processes, replacing a custom state machine built on Redis", created_at: "2025-06-15" },
        Mem { id: "lme-139", summary: "Decision to use Deno instead of Node.js for new edge functions at CDN layer due to built-in TypeScript support and better security sandbox", created_at: "2025-06-22" },
        Mem { id: "lme-140", summary: "Selected MinIO for S3-compatible on-premises object storage in the air-gapped staging environment, mirroring production S3 bucket structure", created_at: "2025-07-02" },
        Mem { id: "lme-141", summary: "Adopted Dragonfly as a Redis-compatible in-memory store for the session service, achieving 25x throughput improvement on multi-core hardware", created_at: "2025-07-08" },
        Mem { id: "lme-142", summary: "Architecture decision: implementing event sourcing with Axon Framework for the order management domain to preserve full audit trail of state changes", created_at: "2025-07-12" },
        Mem { id: "lme-143", summary: "Chose Caddy over Nginx for reverse proxy on developer machines due to automatic HTTPS with Let's Encrypt and simpler Caddyfile configuration", created_at: "2025-08-22" },
        Mem { id: "lme-144", summary: "Adopted SurrealDB for the graph-relational hybrid data model in the recommendation engine, replacing separate Neo4j and PostgreSQL instances", created_at: "2025-09-18" },
        Mem { id: "lme-145", summary: "Selected Clickhouse for real-time analytics OLAP queries, replacing nightly batch jobs that were computing dashboard metrics from PostgreSQL", created_at: "2025-10-22" },
        Mem { id: "lme-146", summary: "Decision to adopt WebAssembly plugins via Extism for user-defined transformation logic in the data pipeline instead of embedding a Lua interpreter", created_at: "2025-11-22" },
        Mem { id: "lme-147", summary: "Chose Meilisearch over Typesense for the internal documentation search due to better typo tolerance and instant faceted filtering on document metadata", created_at: "2025-12-28" },
        Mem { id: "lme-148", summary: "Architecture decision: adopting the BFF (Backend for Frontend) pattern with dedicated GraphQL gateways per client platform (web, iOS, Android)", created_at: "2026-01-08" },
        Mem { id: "lme-149", summary: "Selected Crossplane for Kubernetes-native infrastructure provisioning, allowing developers to request cloud resources via custom resource definitions", created_at: "2026-01-18" },
        Mem { id: "lme-150", summary: "Adopted OpenTelemetry Collector as the universal telemetry pipeline, replacing separate Fluentd, StatsD, and Jaeger agent sidecars in each pod", created_at: "2026-01-25" },
        Mem { id: "lme-151", summary: "Decision to use SQLite with Litestream replication for the configuration service instead of etcd, reducing operational complexity for a read-heavy workload", created_at: "2026-02-05" },
        Mem { id: "lme-152", summary: "Chose Zig for the new high-performance image transcoding service after benchmarks showed 3x throughput over the existing Go implementation", created_at: "2026-02-12" },
        Mem { id: "lme-153", summary: "Adopted Buf for protobuf schema management and breaking change detection in CI, replacing manual proto review during code review", created_at: "2026-02-18" },
        Mem { id: "lme-154", summary: "Architecture decision: moving to cell-based architecture for the payment service to limit blast radius of failures to individual customer cohorts", created_at: "2026-02-25" },
        Mem { id: "lme-155", summary: "Selected Valkey over Redis for new deployments after the Redis license change, maintaining API compatibility while staying on open-source licensing", created_at: "2026-03-02" },
        Mem { id: "lme-156", summary: "Adopted Bazel for the monorepo build system to enable incremental builds and remote build caching, reducing full CI pipeline from 45 minutes to 8 minutes", created_at: "2026-03-08" },
        Mem { id: "lme-157", summary: "Decision to use DuckDB for local analytical queries in the CLI tools, replacing pandas-based Python scripts for data exploration", created_at: "2026-03-15" },
        Mem { id: "lme-158", summary: "Chose Tauri over Electron for the new internal admin dashboard desktop app to reduce memory footprint from 800MB to 90MB", created_at: "2026-03-22" },
        Mem { id: "lme-159", summary: "Architecture review: adopted sidecar proxy pattern with Envoy for all inter-service communication, delegating retry logic and circuit breaking to the mesh", created_at: "2025-04-28" },
        Mem { id: "lme-160", summary: "Selected Vitess for MySQL horizontal sharding in the legacy billing system that cannot be migrated to PostgreSQL within the current quarter", created_at: "2025-05-05" },
        Mem { id: "lme-161", summary: "Adopted Qdrant for vector similarity search in the product recommendation engine, replacing a custom FAISS wrapper that was difficult to maintain", created_at: "2025-05-18" },
        Mem { id: "lme-162", summary: "Decision to use Pkl configuration language for service configs, providing type safety and validation that YAML and TOML cannot offer", created_at: "2025-06-05" },
        Mem { id: "lme-163", summary: "Chose Fly.io for deploying the marketing site edge workers, complementing our primary AWS infrastructure for static content delivery", created_at: "2025-06-12" },
        Mem { id: "lme-164", summary: "Adopted Dapr as the distributed application runtime for the notification microservice to abstract away pub/sub and state store implementations", created_at: "2025-06-28" },
        Mem { id: "lme-165", summary: "Architecture decision: implementing the strangler fig pattern for incremental migration of the billing monolith, routing requests based on feature flags", created_at: "2025-07-05" },
        Mem { id: "lme-166", summary: "Selected ScyllaDB over Cassandra for the session store due to 10x lower tail latency at p99 and compatible CQL interface for easy migration", created_at: "2025-08-28" },
        Mem { id: "lme-167", summary: "Adopted Benthos for stream processing pipelines connecting Kafka to various sinks, replacing custom Go consumers with declarative YAML configuration", created_at: "2025-09-22" },
        Mem { id: "lme-168", summary: "Decision to use Encore.go framework for new microservices to get built-in tracing, pub/sub, and database provisioning with less boilerplate", created_at: "2025-10-28" },
        Mem { id: "lme-169", summary: "Chose Zitadel over Keycloak for the internal identity provider because of its Go-native implementation and better Kubernetes operator support", created_at: "2025-11-28" },
        Mem { id: "lme-170", summary: "Architecture decision: adopting the outbox pattern with Debezium CDC for reliable event publishing from PostgreSQL to Kafka without dual-write risks", created_at: "2025-12-08" },
        Mem { id: "lme-171", summary: "Selected Garage over MinIO for the distributed object storage cluster because it handles node failures gracefully without manual rebalancing", created_at: "2026-01-02" },
        Mem { id: "lme-172", summary: "Adopted Pkl over Jsonnet for Kubernetes manifest templating after the team found Jsonnet error messages incomprehensible during incident response", created_at: "2026-01-12" },
        Mem { id: "lme-173", summary: "Decision to implement read-your-writes consistency in the API gateway layer using sticky sessions and causal consistency tokens", created_at: "2026-02-08" },
        Mem { id: "lme-174", summary: "Chose Wasmer over Wasmtime for the WebAssembly plugin runtime because of better cross-compilation support for our ARM64 deployment targets", created_at: "2026-03-18" },
        Mem { id: "lme-175", summary: "Architecture decision: adopting trunk-based development with short-lived feature branches (max 24 hours) and automated merge queue via Mergify", created_at: "2026-03-25" },
        Mem { id: "lme-176", summary: "Selected Grafana Tempo over Jaeger for distributed tracing storage due to native integration with Grafana dashboards and lower storage costs using object storage backend", created_at: "2026-03-28" },
        Mem { id: "lme-177", summary: "Adopted gRPC-Web with Envoy transcoding for browser clients to call backend gRPC services without a separate REST translation layer", created_at: "2025-04-15" },
        Mem { id: "lme-178", summary: "Decision to use Apache Parquet for data lake storage format over ORC due to better ecosystem support in Spark, DuckDB, and Polars", created_at: "2025-05-25" },

        // --- More bug fixes (50) ---
        Mem { id: "lme-179", summary: "Fixed Terraform state lock contention: multiple CI pipelines were running terraform apply concurrently. Added DynamoDB locking with retry backoff", created_at: "2025-04-08" },
        Mem { id: "lme-180", summary: "Resolved MongoDB connection exhaustion in the catalog service: driver was not releasing connections from the pool on query timeout errors", created_at: "2025-04-18" },
        Mem { id: "lme-181", summary: "Fixed Istio sidecar injection failure: pods were being scheduled on nodes with incompatible kernel versions. Added node affinity rules for mesh-enabled workloads", created_at: "2025-05-03" },
        Mem { id: "lme-182", summary: "Resolved ArgoCD sync loop: Helm chart values were being regenerated on every sync due to non-deterministic map ordering in Go templates", created_at: "2025-05-12" },
        Mem { id: "lme-183", summary: "Fixed Prometheus alerting false positives: scrape interval mismatch between Prometheus and application metrics endpoint caused rate calculation spikes", created_at: "2025-05-20" },
        Mem { id: "lme-184", summary: "Resolved CockroachDB range split thrashing in the inventory table: hot key pattern from sequential order IDs. Switched to UUID-based primary keys with hash sharding", created_at: "2025-06-02" },
        Mem { id: "lme-185", summary: "Fixed OPA policy evaluation timeout: complex Rego rules with nested list comprehensions were taking 30 seconds. Rewrote to use indexed lookups instead of iteration", created_at: "2025-06-10" },
        Mem { id: "lme-186", summary: "Resolved NATS JetStream message redelivery storm: acknowledgment timeout was shorter than the consumer processing time, causing exponential retry amplification", created_at: "2025-06-18" },
        Mem { id: "lme-187", summary: "Fixed TimescaleDB chunk creation lock contention: hypertable was configured with 1-hour chunks but write volume needed 15-minute chunks to reduce lock duration", created_at: "2025-06-25" },
        Mem { id: "lme-188", summary: "Resolved Temporal workflow replay failure: non-deterministic activity implementation was reading the current time instead of using workflow.Now()", created_at: "2025-07-03" },
        Mem { id: "lme-189", summary: "Fixed Grafana dashboard variable loading: cascading variable queries were firing 200+ Prometheus queries on page load. Implemented query caching with 5-minute TTL", created_at: "2025-07-10" },
        Mem { id: "lme-190", summary: "Resolved Clickhouse materialized view lag: inserts to the source table were not triggering view updates when using async inserts. Switched to sync mode for critical tables", created_at: "2025-07-18" },
        Mem { id: "lme-191", summary: "Fixed Crossplane provider credential rotation: AWS credentials embedded in the provider config were not refreshing from the Vault secret store on rotation", created_at: "2025-08-02" },
        Mem { id: "lme-192", summary: "Resolved Envoy proxy memory leak: HTTP/2 connection window updates were not being properly acknowledged, causing unbounded buffer growth under sustained load", created_at: "2025-08-08" },
        Mem { id: "lme-193", summary: "Fixed Bazel remote cache poisoning: a corrupted action result in the shared cache was causing all CI builds to produce incorrect artifacts. Implemented cache key namespacing per branch", created_at: "2025-08-15" },
        Mem { id: "lme-194", summary: "Resolved Meilisearch index corruption after power loss: the LMDB storage engine did not have fsync enabled by default. Enabled sync writes for data integrity", created_at: "2025-09-02" },
        Mem { id: "lme-195", summary: "Fixed Qdrant vector index OOM during bulk import: batch size of 100K vectors exceeded available memory. Reduced to 5K batches with streaming upload", created_at: "2025-09-12" },
        Mem { id: "lme-196", summary: "Resolved ScyllaDB tombstone accumulation causing read timeouts: TTL-based expiration on the session table was generating 10M tombstones per day. Added compaction strategy tweak", created_at: "2025-09-28" },
        Mem { id: "lme-197", summary: "Fixed Debezium CDC connector losing events during PostgreSQL failover: the replication slot was not being transferred to the new primary. Added slot monitoring and auto-recreation", created_at: "2025-10-03" },
        Mem { id: "lme-198", summary: "Resolved Vitess vttablet crash during resharding: query serving was not paused during schema copy phase. Added explicit tablet drain before schema migration", created_at: "2025-10-09" },
        Mem { id: "lme-199", summary: "Fixed Benthos pipeline backpressure: when Kafka sink was slow, the entire pipeline would buffer in memory. Added bounded channel with disk spillover using badger DB", created_at: "2025-10-16" },
        Mem { id: "lme-200", summary: "Resolved Tauri app crash on macOS Sonoma: WebView2 initialization was failing due to new entitlement requirements. Updated info.plist with network client entitlement", created_at: "2025-10-22" },
        Mem { id: "lme-201", summary: "Fixed DuckDB query planner regression: complex joins with CTEs were choosing nested loop join instead of hash join after upgrading to 0.10. Worked around with explicit join hints", created_at: "2025-11-02" },
        Mem { id: "lme-202", summary: "Resolved Pulumi stack import failure: resources created outside Pulumi were not matching the expected state shape. Wrote a custom import script to reconcile drifted state", created_at: "2025-11-08" },
        Mem { id: "lme-203", summary: "Fixed Dapr pub/sub message ordering: messages published within the same transaction were being delivered out of order. Enabled ordering key propagation in the Kafka component", created_at: "2025-11-18" },
        Mem { id: "lme-204", summary: "Resolved Ansible playbook idempotency failure: the package installation task was not handling already-installed packages correctly on RHEL 9 due to dnf module changes", created_at: "2025-12-02" },
        Mem { id: "lme-205", summary: "Fixed Vault PKI certificate chain validation: intermediate CA certificates were not included in the issued certificate bundle, causing TLS verification failures in Java clients", created_at: "2025-12-12" },
        Mem { id: "lme-206", summary: "Resolved Litestream replication lag: WAL files were accumulating faster than S3 upload throughput during peak write hours. Increased concurrent upload workers from 1 to 4", created_at: "2025-12-20" },
        Mem { id: "lme-207", summary: "Fixed SurrealDB query timeout during graph traversal: recursive relationship query had no depth limit and was exploring the entire graph. Added max_depth=5 parameter", created_at: "2026-01-06" },
        Mem { id: "lme-208", summary: "Resolved Zitadel OIDC token validation failure: clock skew between Kubernetes nodes was causing JWT nbf (not before) validation to reject valid tokens. Added 30-second tolerance", created_at: "2026-01-16" },
        Mem { id: "lme-209", summary: "Fixed Mergify merge queue deadlock: two PRs with conflicting base branch requirements were blocking each other. Added priority-based queue ordering", created_at: "2026-01-22" },
        Mem { id: "lme-210", summary: "Resolved Garage distributed storage split-brain: network partition between nodes caused data inconsistency. Upgraded to version with improved Raft consensus handling", created_at: "2026-01-28" },
        Mem { id: "lme-211", summary: "Fixed Wasmer plugin sandboxing escape: WASI filesystem access was not properly restricted, allowing plugins to read host files. Applied stricter capability permissions", created_at: "2026-02-03" },
        Mem { id: "lme-212", summary: "Resolved Grafana Tempo trace search timeout: bloom filter configuration was suboptimal for our trace cardinality, causing full block scans. Tuned bloom shard size to 256KB", created_at: "2026-02-10" },
        Mem { id: "lme-213", summary: "Fixed OpenTelemetry Collector goroutine leak: batch exporter was not draining when the downstream OTLP endpoint was unreachable, accumulating thousands of goroutines", created_at: "2026-02-16" },
        Mem { id: "lme-214", summary: "Resolved Fly.io deployment rollback failure: the machine API returned stale state after rollback, causing health checks to report the old version as current", created_at: "2026-02-22" },
        Mem { id: "lme-215", summary: "Fixed Pkl configuration validation bypass: default values were not being validated against constraints, allowing invalid configs to reach production. Added strict mode", created_at: "2026-03-01" },
        Mem { id: "lme-216", summary: "Resolved Dragonfly eviction policy issue: allkeys-lru was evicting recently written keys during memory pressure because the LRU clock granularity was too coarse", created_at: "2026-03-06" },
        Mem { id: "lme-217", summary: "Fixed Encore.go service discovery race condition: newly deployed services were not registered before health checks started, causing transient 503 errors for 10 seconds", created_at: "2026-03-12" },
        Mem { id: "lme-218", summary: "Resolved Parquet file corruption during concurrent writes: multiple Spark executors were writing to the same partition path. Added partition locking via S3 conditional writes", created_at: "2026-03-20" },
        Mem { id: "lme-219", summary: "Fixed Buf protobuf schema registry sync failure: breaking change detection was not accounting for reserved field numbers, blocking valid schema evolution", created_at: "2025-04-25" },
        Mem { id: "lme-220", summary: "Resolved Valkey cluster slot migration hang: migrating slots between nodes would stall when keys contained large values (>1MB). Added chunked migration support", created_at: "2025-05-08" },
        Mem { id: "lme-221", summary: "Fixed Caddy automatic TLS renewal failure: ACME challenge was blocked by Cloudflare proxy. Switched to DNS-01 challenge with Cloudflare API token", created_at: "2025-06-20" },
        Mem { id: "lme-222", summary: "Resolved Deno edge function cold start regression: module graph analysis was re-running on every invocation. Pre-bundled dependencies with esbuild for instant startup", created_at: "2025-07-15" },
        Mem { id: "lme-223", summary: "Fixed NATS message deduplication window: duplicate detection was based on message ID only but two different messages could share the same ID across subjects. Added subject to dedup key", created_at: "2025-08-12" },
        Mem { id: "lme-224", summary: "Resolved Temporal activity heartbeat timeout: long-running data migration activities were being considered failed because heartbeat interval exceeded the configured timeout", created_at: "2025-09-15" },
        Mem { id: "lme-225", summary: "Fixed Clickhouse distributed query routing: queries hitting the wrong shard when using custom sharding key due to hash function mismatch between client and server", created_at: "2025-10-18" },
        Mem { id: "lme-226", summary: "Resolved Istio gateway TLS passthrough misconfiguration: SNI routing was not matching wildcard certificates, causing 404 for subdomains", created_at: "2025-11-25" },
        Mem { id: "lme-227", summary: "Fixed CockroachDB changefeed lag: resolved changefeed falling behind by switching from rangefeed-based to polling-based changefeed for high-churn tables", created_at: "2026-01-30" },
        Mem { id: "lme-228", summary: "Resolved Crossplane composition drift: managed resources were not reconciling after manual edits in the cloud console. Added drift detection webhook", created_at: "2026-02-28" },

        // --- More project updates (40) ---
        Mem { id: "lme-229", summary: "Sprint 15 review: completed Terraform module library for standard AWS resources. All new services can now spin up infrastructure in under 5 minutes", created_at: "2025-04-10" },
        Mem { id: "lme-230", summary: "Q2 2025 roadmap: (1) service mesh rollout, (2) MongoDB Atlas migration for catalog, (3) begin Ansible phase-out in favor of Terraform", created_at: "2025-04-01" },
        Mem { id: "lme-231", summary: "Sprint 16 retro: Istio rollout blocked by kernel version incompatibility on 30% of nodes. Need to schedule rolling OS upgrade first", created_at: "2025-04-22" },
        Mem { id: "lme-232", summary: "Project milestone: ArgoCD fully operational for all staging environments. Production rollout planned for next sprint after security review", created_at: "2025-05-15" },
        Mem { id: "lme-233", summary: "Sprint 17 review: CockroachDB proof of concept shows 4x throughput for multi-region inventory writes compared to PostgreSQL with Citus", created_at: "2025-05-28" },
        Mem { id: "lme-234", summary: "Budget approval: secured $120K annual budget for Temporal Cloud instead of self-hosting, reducing operational burden on the platform team by 2 FTE-months/year", created_at: "2025-06-10" },
        Mem { id: "lme-235", summary: "Sprint 19 retro: velocity dropped 30% due to three concurrent oncall incidents. Proposing dedicated incident response rotation separate from feature work", created_at: "2025-06-25" },
        Mem { id: "lme-236", summary: "Q3 planning addendum: Clickhouse evaluation added to roadmap after product team requested real-time analytics for the merchant dashboard", created_at: "2025-07-05" },
        Mem { id: "lme-237", summary: "Project milestone: all 12 microservices now reporting traces to Grafana Tempo. End-to-end request tracing from API gateway to database is fully operational", created_at: "2025-07-22" },
        Mem { id: "lme-238", summary: "Sprint 21 review: Meilisearch deployed for internal documentation search. Average query time 8ms, previously 2.5 seconds with the old Solr instance", created_at: "2025-08-05" },
        Mem { id: "lme-239", summary: "Technical spike: evaluated Zig vs Rust for the image transcoding service. Zig showed 15% better throughput but team has zero Zig experience. Decision deferred to Q1 2026", created_at: "2025-08-18" },
        Mem { id: "lme-240", summary: "Sprint 22 retro: Crossplane adoption slower than expected because the learning curve for writing custom compositions is steep. Scheduling training workshop", created_at: "2025-09-05" },
        Mem { id: "lme-241", summary: "Project cancellation: abandoned the Vitess migration after discovering that the legacy billing system can be retired entirely by Q2 2026 when new billing launches", created_at: "2025-09-20" },
        Mem { id: "lme-242", summary: "Sprint 24 review: Bazel migration complete for the three largest services. Build time reduction from 45 to 12 minutes observed, targeting sub-10 with remote cache", created_at: "2025-10-05" },
        Mem { id: "lme-243", summary: "Quarterly OKR review: hit 4 of 6 key results. Missed targets on API latency (p99 still at 280ms vs 200ms target) and documentation coverage (65% vs 80%)", created_at: "2025-10-15" },
        Mem { id: "lme-244", summary: "Project milestone: Debezium CDC pipeline operational for all PostgreSQL databases. Event lag under 500ms in steady state", created_at: "2025-10-25" },
        Mem { id: "lme-245", summary: "Sprint 26 review: Tauri admin dashboard v1 shipped to internal users. Memory usage 85MB vs 780MB for the old Electron version, launch time 0.8s vs 4.2s", created_at: "2025-11-05" },
        Mem { id: "lme-246", summary: "Roadmap change: deprioritized Deno edge functions after Cloudflare Workers proved sufficient for our CDN-level logic. Deno work shelved until Q3 2026", created_at: "2025-11-15" },
        Mem { id: "lme-247", summary: "Sprint 27 retro: merged 47 PRs this sprint, highest ever. Attributed to Mergify merge queue eliminating manual merge conflict resolution", created_at: "2025-11-25" },
        Mem { id: "lme-248", summary: "Project milestone: Vault PKI infrastructure fully operational. All internal TLS certificates now auto-rotated every 72 hours with zero manual intervention", created_at: "2025-12-05" },
        Mem { id: "lme-249", summary: "Q1 2026 planning: (1) cell-based payment architecture, (2) Valkey migration, (3) Pkl configuration rollout, (4) WebAssembly plugin system for data pipeline", created_at: "2025-12-15" },
        Mem { id: "lme-250", summary: "Sprint 30 review: DuckDB-based analytics CLI tool shipped. Product analysts reporting 10x faster ad-hoc queries compared to connecting to Snowflake", created_at: "2026-01-05" },
        Mem { id: "lme-251", summary: "Project risk assessment: cell-based architecture migration estimated at 4 months, 2 months longer than originally planned due to cross-cell routing complexity", created_at: "2026-01-15" },
        Mem { id: "lme-252", summary: "Sprint 31 retro: OpenTelemetry Collector rollout complete. Eliminated 6 different telemetry agents, saving 500MB RAM per node across the cluster", created_at: "2026-01-25" },
        Mem { id: "lme-253", summary: "Budget review Q1 2026: infrastructure spend up 12% due to CockroachDB and Temporal Cloud. Offset by 20% savings from Grafana stack migration", created_at: "2026-02-05" },
        Mem { id: "lme-254", summary: "Sprint 32 review: Parquet-based data lake operational. First batch analytics pipeline migrated from CSV to Parquet, reducing S3 storage by 70% and query time by 5x", created_at: "2026-02-15" },
        Mem { id: "lme-255", summary: "Project milestone: Valkey migration complete for all non-critical services. Redis still used for the session store pending ScyllaDB migration completion", created_at: "2026-02-25" },
        Mem { id: "lme-256", summary: "Sprint 33 review: Pkl configuration rollout to 8 of 12 services. Caught 3 production-bound misconfigurations during validation that YAML would have missed", created_at: "2026-03-05" },
        Mem { id: "lme-257", summary: "Roadmap revision: WebAssembly plugin system deprioritized after security audit found sandbox escapes in Wasmer. Will revisit after patches land in Q3", created_at: "2026-03-15" },
        Mem { id: "lme-258", summary: "Sprint 34 review: Grafana Tempo trace retention policy tuned. Reduced storage costs 40% by keeping detailed traces for 7 days and sampled traces for 30 days", created_at: "2026-03-22" },
        Mem { id: "lme-259", summary: "End of Q1 retrospective: shipped 85% of planned work. Biggest wins: OTel Collector, Parquet data lake, and Pkl configs. Biggest miss: cell-based architecture still in design", created_at: "2026-03-30" },
        Mem { id: "lme-260", summary: "Sprint 18 review: NATS JetStream deployed for IoT telemetry. Processing 50K messages/sec with p99 latency of 3ms, well within the 10ms target", created_at: "2025-07-01" },
        Mem { id: "lme-261", summary: "Project update: Benthos stream processing pipeline handling 200GB/day of Kafka events. Zero data loss incidents since deployment three months ago", created_at: "2025-12-10" },
        Mem { id: "lme-262", summary: "Sprint 20 review: SurrealDB recommendation engine in beta. A/B test showing 18% improvement in click-through rate compared to the collaborative filtering baseline", created_at: "2025-08-10" },
        Mem { id: "lme-263", summary: "Project status: Encore.go framework adopted by 2 of 5 product teams. Remaining teams blocked on gRPC interceptor support which lands in Encore 1.4", created_at: "2025-11-10" },
        Mem { id: "lme-264", summary: "Sprint 25 review: Zitadel identity provider operational for internal tooling. External customer auth still on Clerk, no plans to consolidate", created_at: "2025-10-10" },
        Mem { id: "lme-265", summary: "Project milestone: Ansible decommissioned for all server provisioning. 100% of infrastructure now managed through Terraform and Crossplane", created_at: "2026-02-20" },
        Mem { id: "lme-266", summary: "Sprint 29 review: Garage distributed storage handling 50TB of object data with 99.99% durability over 6-month measurement window", created_at: "2025-12-20" },
        Mem { id: "lme-267", summary: "Roadmap addition: adopting gRPC-Web for the new merchant portal to eliminate the REST translation layer. Estimated 2 sprints of work starting in Q2 2026", created_at: "2026-03-28" },
        Mem { id: "lme-268", summary: "Project status: Buf schema registry managing 47 protobuf definitions across 12 services. Zero breaking changes shipped to production since adoption", created_at: "2026-03-10" },

        // --- More technical deep-dives (50) ---
        Mem { id: "lme-269", summary: "Terraform module design: implemented a composable VPC module with optional NAT gateways, VPN endpoints, and flow logs. Reduces new environment setup from 2 days to 30 minutes", created_at: "2025-04-08" },
        Mem { id: "lme-270", summary: "MongoDB schema design for product catalog: used the subset pattern for embedding top-20 reviews directly in the product document, reducing read amplification by 80%", created_at: "2025-04-15" },
        Mem { id: "lme-271", summary: "Istio traffic management: configured weighted routing with 90/10 split for canary deployments, automatic retry on 503 errors, and 5-second timeout for inter-service calls", created_at: "2025-04-22" },
        Mem { id: "lme-272", summary: "ArgoCD application-of-apps pattern: top-level ArgoCD application manages per-environment app definitions, enabling single-commit promotion from staging to production", created_at: "2025-05-02" },
        Mem { id: "lme-273", summary: "Prometheus recording rules optimization: pre-computed heavy aggregation queries as recording rules, reducing dashboard load time from 12 seconds to 400ms", created_at: "2025-05-15" },
        Mem { id: "lme-274", summary: "CockroachDB zone configuration: pinned lease holders for the EU tenant data to the eu-west-1 region, reducing cross-region read latency from 120ms to 8ms", created_at: "2025-05-25" },
        Mem { id: "lme-275", summary: "OPA policy architecture: organized policies into base (deny-by-default), team (per-team overrides), and exception (time-boxed exceptions) layers with merge semantics", created_at: "2025-06-05" },
        Mem { id: "lme-276", summary: "NATS JetStream consumer design: implemented pull-based consumers with batch acknowledgment (100 messages per ack) for throughput, push-based for latency-sensitive workloads", created_at: "2025-06-15" },
        Mem { id: "lme-277", summary: "TimescaleDB continuous aggregates: set up hourly and daily materialized views for metrics rollup, reducing storage for 90-day retention from 800GB to 45GB", created_at: "2025-06-25" },
        Mem { id: "lme-278", summary: "Temporal workflow design: implemented the saga pattern with compensating activities for the multi-step order fulfillment process spanning 5 services and 3 external APIs", created_at: "2025-07-05" },
        Mem { id: "lme-279", summary: "Grafana Loki log pipeline: configured structured metadata extraction from JSON logs, enabling label-based queries on request_id, user_id, and service without full-text search", created_at: "2025-07-15" },
        Mem { id: "lme-280", summary: "Clickhouse table engine selection: used ReplacingMergeTree for the event analytics table to handle late-arriving duplicate events, with ver column for deduplication ordering", created_at: "2025-07-25" },
        Mem { id: "lme-281", summary: "Crossplane composition design: created a 'DatabaseInstance' XRD that provisions RDS instance, security group, parameter group, and monitoring alarms as a single resource", created_at: "2025-08-05" },
        Mem { id: "lme-282", summary: "Envoy rate limiting: implemented local rate limiting per-pod (10K req/s) plus global rate limiting via Redis (100K req/s across cluster) with graceful degradation on Redis failure", created_at: "2025-08-15" },
        Mem { id: "lme-283", summary: "Bazel build optimization: configured remote build cache on S3 with cache warming from main branch builds, achieving 95% cache hit rate on feature branch CI runs", created_at: "2025-08-25" },
        Mem { id: "lme-284", summary: "Meilisearch ranking rules: customized ranking to prioritize exactness, then recency, then word proximity for documentation search. Typo tolerance set to 2 for max 8-char words", created_at: "2025-09-05" },
        Mem { id: "lme-285", summary: "Qdrant vector index tuning: HNSW with ef_construct=256 and m=32 for the 768-dimension product embeddings. Search ef=128 gives 99.5% recall at 5ms p95 latency", created_at: "2025-09-15" },
        Mem { id: "lme-286", summary: "ScyllaDB data modeling: designed wide partition tables for the session store with partition key = user_id, clustering key = session_timestamp, TTL = 30 days per row", created_at: "2025-09-25" },
        Mem { id: "lme-287", summary: "Debezium CDC configuration: set up event flattening SMT to convert change events from before/after format to simple key-value, reducing Kafka message size by 40%", created_at: "2025-10-05" },
        Mem { id: "lme-288", summary: "Benthos pipeline optimization: implemented parallel processing with fan-out to 8 workers and fan-in with ordering guarantees using sequence IDs in message metadata", created_at: "2025-10-15" },
        Mem { id: "lme-289", summary: "Tauri IPC bridge design: implemented a type-safe command system with serde serialization between the Rust backend and TypeScript frontend, with streaming events via Tauri events API", created_at: "2025-10-25" },
        Mem { id: "lme-290", summary: "DuckDB extension development: wrote a custom table function in C++ that reads directly from our Parquet files in S3 with predicate pushdown for the analytics CLI", created_at: "2025-11-05" },
        Mem { id: "lme-291", summary: "Pulumi component resource: created a reusable 'MicroserviceStack' component that provisions ECS service, ALB target group, Route53 record, and CloudWatch alarms as a single unit", created_at: "2025-11-15" },
        Mem { id: "lme-292", summary: "Dapr state store configuration: set up actor-based state management with Dapr for the shopping cart service, using CockroachDB as the backing store with optimistic concurrency", created_at: "2025-11-25" },
        Mem { id: "lme-293", summary: "Vault dynamic database credentials: configured PostgreSQL database secret engine with 1-hour TTL, automatic rotation, and per-service credential isolation", created_at: "2025-12-05" },
        Mem { id: "lme-294", summary: "SurrealDB graph query optimization: rewrote the recommendation traversal from recursive subquery to native graph RELATE syntax, reducing query time from 800ms to 45ms", created_at: "2025-12-15" },
        Mem { id: "lme-295", summary: "Zitadel custom action scripting: implemented pre-authentication hooks that check user risk score from our fraud detection API before allowing login", created_at: "2025-12-25" },
        Mem { id: "lme-296", summary: "Encore.go service architecture: configured per-service databases with automatic migration, pub/sub topics with schema validation, and secrets injection from Vault", created_at: "2026-01-05" },
        Mem { id: "lme-297", summary: "Garage erasure coding configuration: set up 3-of-5 erasure coding for the archival tier, tolerating 2 node failures while using 67% less storage than 3x replication", created_at: "2026-01-15" },
        Mem { id: "lme-298", summary: "Pkl schema definitions: created strongly-typed config schemas for all services with cross-field validation, enum constraints, and conditional defaults based on environment", created_at: "2026-01-25" },
        Mem { id: "lme-299", summary: "Wasmer plugin API design: defined a host-guest interface using WIT (WebAssembly Interface Types) for data transformation plugins with memory-safe data passing", created_at: "2026-02-05" },
        Mem { id: "lme-300", summary: "Grafana Tempo backend optimization: configured S3 as the trace backend with compactor running every 5 minutes, bloom filter FP rate at 0.01, and block retention of 14 days", created_at: "2026-02-15" },
        Mem { id: "lme-301", summary: "OpenTelemetry Collector pipeline: configured tail-based sampling at 10% for healthy traces and 100% for error traces, reducing storage by 85% while keeping all failure data", created_at: "2026-02-25" },
        Mem { id: "lme-302", summary: "gRPC-Web transcoding rules: configured Envoy HTTP-to-gRPC transcoding with custom error mapping, streaming support via Server-Sent Events, and CORS headers for browser clients", created_at: "2026-03-05" },
        Mem { id: "lme-303", summary: "Parquet file optimization: configured row group size to 128MB with dictionary encoding for low-cardinality columns and delta encoding for timestamps, achieving 12:1 compression ratio", created_at: "2026-03-15" },
        Mem { id: "lme-304", summary: "Mergify merge queue configuration: priority-based queue with CI-required checks, automatic rebase, and batch merging of up to 5 non-conflicting PRs simultaneously", created_at: "2026-03-20" },
        Mem { id: "lme-305", summary: "Valkey cluster topology: 6-node cluster with 3 primaries and 3 replicas across 3 AZs, configured with maxmemory-policy allkeys-lfu for the caching workload", created_at: "2026-03-25" },
        Mem { id: "lme-306", summary: "Dragonfly memory optimization: enabled tiered storage with NVMe SSDs for cold data, keeping only hot keys in RAM. Reduced memory requirements by 60% for the rate limiting cache", created_at: "2025-07-10" },
        Mem { id: "lme-307", summary: "MinIO erasure coding benchmark: 4+4 erasure set configuration on NVMe drives achieving 2.8 GB/s write and 4.1 GB/s read throughput for the staging object store", created_at: "2025-07-20" },
        Mem { id: "lme-308", summary: "Ansible to Terraform migration strategy: documented a service-by-service migration plan with parallel running of both systems during transition, Terraform import for existing resources", created_at: "2025-04-05" },
        Mem { id: "lme-309", summary: "Litestream replication architecture: configured continuous WAL streaming to S3 with point-in-time restore capability. Recovery time objective under 30 seconds for the config service", created_at: "2026-02-08" },
        Mem { id: "lme-310", summary: "Vitess resharding plan: designed a two-phase shard split from 4 to 8 shards for the billing keyspace with vreplication-based backfill and cutover during maintenance window", created_at: "2025-05-10" },
        Mem { id: "lme-311", summary: "Caddy reverse proxy configuration: set up automatic HTTP/3 with QUIC, wildcard TLS via Cloudflare DNS challenge, and request matchers for path-based routing to different backends", created_at: "2025-06-15" },
        Mem { id: "lme-312", summary: "Deno Deploy edge function architecture: implemented request routing at CDN edge with geo-based A/B testing and personalized content injection before origin fetch", created_at: "2025-07-01" },
        Mem { id: "lme-313", summary: "Temporal visibility store migration: moved from Elasticsearch to PostgreSQL for workflow visibility queries, reducing infrastructure costs and operational complexity", created_at: "2025-08-10" },
        Mem { id: "lme-314", summary: "Clickhouse materialized views for real-time dashboards: configured AggregatingMergeTree engine with partial aggregation states for merchant analytics with 1-second refresh", created_at: "2025-09-10" },
        Mem { id: "lme-315", summary: "Buf connect-go adoption: migrated 3 internal APIs from REST to Connect protocol, providing gRPC, gRPC-Web, and JSON simultaneously from a single service definition", created_at: "2025-10-10" },
        Mem { id: "lme-316", summary: "Zig build system integration: configured Zig as a cross-compiler for the image transcoding service, targeting both ARM64 and x86_64 from the same CI pipeline without Docker", created_at: "2026-02-18" },
        Mem { id: "lme-317", summary: "Event sourcing snapshot strategy: configured automatic snapshots every 100 events for the order aggregate, reducing replay time from 2 seconds to 15ms on cold start", created_at: "2025-07-15" },
        Mem { id: "lme-318", summary: "DuckDB motherduck integration: connected local DuckDB instances to MotherDuck cloud for shared analytical queries across the data team without moving data to a central warehouse", created_at: "2025-11-20" },

        // --- More security/compliance (30) ---
        Mem { id: "lme-319", summary: "Vault policy audit: reviewed all 47 Vault policies, revoked 12 overly permissive paths, implemented least-privilege access with separate read and write policies per service", created_at: "2025-04-10" },
        Mem { id: "lme-320", summary: "Istio mTLS enforcement: enabled STRICT mode for all namespaces, blocking any plaintext inter-service communication. Verified with tcpdump that all traffic is encrypted", created_at: "2025-04-25" },
        Mem { id: "lme-321", summary: "OPA admission controller: deployed Gatekeeper with policies blocking privileged containers, host networking, and images from untrusted registries across all Kubernetes clusters", created_at: "2025-05-08" },
        Mem { id: "lme-322", summary: "MongoDB Atlas security: enabled audit logging, configured IP access list, set up VPC peering to eliminate public internet exposure, and enforced TLS 1.3 minimum", created_at: "2025-05-20" },
        Mem { id: "lme-323", summary: "CockroachDB encryption: enabled encryption at rest with customer-managed keys via AWS KMS, configured audit logging for all DDL operations and privilege changes", created_at: "2025-06-01" },
        Mem { id: "lme-324", summary: "Terraform security scanning: integrated Checkov in CI to detect misconfigurations in IaC. Blocked 8 PRs in the first month that would have created public S3 buckets", created_at: "2025-06-15" },
        Mem { id: "lme-325", summary: "ArgoCD RBAC implementation: configured project-level permissions so each team can only deploy to their own namespaces. Admin access requires break-glass procedure", created_at: "2025-07-01" },
        Mem { id: "lme-326", summary: "Security incident: detected unauthorized API calls from a leaked service account key. Rotated all service credentials, implemented 4-hour key rotation for all automated access", created_at: "2025-07-10" },
        Mem { id: "lme-327", summary: "NATS authentication: configured decentralized JWT-based auth with account isolation. Each microservice gets its own account with publish/subscribe permissions scoped to its topics only", created_at: "2025-08-01" },
        Mem { id: "lme-328", summary: "Clickhouse access control: implemented row-level security for the analytics tables so each merchant can only query their own data. Verified with penetration testing", created_at: "2025-08-20" },
        Mem { id: "lme-329", summary: "Compliance audit prep: documented all data flows for PII across 12 services. Created data flow diagrams showing where personal data is stored, processed, and transmitted", created_at: "2025-09-01" },
        Mem { id: "lme-330", summary: "Vault transit engine: migrated application-level encryption from per-service key management to centralized Vault transit engine with automatic key rotation every 30 days", created_at: "2025-09-15" },
        Mem { id: "lme-331", summary: "Security training: conducted tabletop exercise simulating a ransomware attack. Identified gaps in backup verification and cross-region recovery procedures", created_at: "2025-10-01" },
        Mem { id: "lme-332", summary: "Zitadel security hardening: configured brute-force protection with progressive delays, IP blocking after 10 failed attempts, and mandatory MFA for admin accounts", created_at: "2025-10-15" },
        Mem { id: "lme-333", summary: "ScyllaDB encryption: enabled internode encryption with mutual TLS and client-to-node encryption. All certificates managed by Vault PKI with 24-hour rotation", created_at: "2025-11-01" },
        Mem { id: "lme-334", summary: "Container image security: implemented image signing with Cosign and verification with Kyverno admission controller. Unsigned images are rejected from all clusters", created_at: "2025-11-15" },
        Mem { id: "lme-335", summary: "SIEM integration: forwarding all audit logs from Vault, Kubernetes, ArgoCD, and application services to Splunk Cloud via OpenTelemetry Collector log pipeline", created_at: "2025-12-01" },
        Mem { id: "lme-336", summary: "SOC 2 Type II evidence collection: automated continuous compliance monitoring with Vanta, covering 85% of controls automatically. Manual evidence for remaining 15%", created_at: "2026-01-01" },
        Mem { id: "lme-337", summary: "Penetration test Q1 2026: 0 critical findings, 1 high (Grafana admin panel accessible without SSO), 4 medium. High fixed within 24 hours by enabling Zitadel SSO for Grafana", created_at: "2026-01-20" },
        Mem { id: "lme-338", summary: "Wasmer sandbox audit: identified 3 WASI capability escapes in the plugin runtime. Disabled filesystem and network access for all plugins pending upstream fixes", created_at: "2026-02-01" },
        Mem { id: "lme-339", summary: "Encryption key rotation drill: performed end-to-end key rotation for all services, including Vault master key, KMS customer keys, and TLS certificates. Completed in 4 hours", created_at: "2026-02-15" },
        Mem { id: "lme-340", summary: "Compliance gap analysis for ISO 27001: identified 12 control gaps, mostly around asset management and supplier security. Remediation plan created with 6-month timeline", created_at: "2026-03-01" },
        Mem { id: "lme-341", summary: "Debezium security: configured SSL for Kafka connect, encrypted connector configurations containing database credentials, and restricted connector management API to admin role", created_at: "2025-10-08" },
        Mem { id: "lme-342", summary: "Temporal security: enabled mTLS between workers and Temporal server, configured namespace-level authorization, and set up audit logging for all workflow operations", created_at: "2025-06-20" },
        Mem { id: "lme-343", summary: "DuckDB security assessment: documented that DuckDB runs in-process with no network exposure, data is read-only from Parquet files, and no PII is stored in analytical datasets", created_at: "2025-11-10" },
        Mem { id: "lme-344", summary: "Crossplane security: restricted AWS provider credentials to minimum IAM permissions per composition, implemented drift detection alerts for any manual cloud console changes", created_at: "2025-12-10" },
        Mem { id: "lme-345", summary: "Grafana security audit: disabled anonymous access, enforced SSO-only authentication, configured RBAC with per-team folder permissions, and enabled audit logging", created_at: "2026-01-10" },
        Mem { id: "lme-346", summary: "Bazel supply chain security: configured bzlmod with module lockfile verification, signed module archives, and restricted registry access to approved module sources only", created_at: "2026-02-10" },
        Mem { id: "lme-347", summary: "Parquet data classification: implemented column-level encryption for PII fields in Parquet files using Apache Parquet modular encryption with per-column keys from Vault", created_at: "2026-03-10" },
        Mem { id: "lme-348", summary: "Incident response update: revised runbook to include CockroachDB, Temporal, and NATS failure scenarios. Conducted quarterly fire drill covering multi-region database failover", created_at: "2026-03-20" },

        // --- More team/process (30) ---
        Mem { id: "lme-349", summary: "Hired 3 senior SREs to form a dedicated platform reliability team. Focus areas: Kubernetes operations, observability, and incident management", created_at: "2025-04-15" },
        Mem { id: "lme-350", summary: "Reorg: merged the infrastructure and DevOps teams into a single Platform Engineering team with 12 engineers. Reporting to new VP of Engineering", created_at: "2025-05-01" },
        Mem { id: "lme-351", summary: "Introduced RFC process for cross-team architectural changes. Template includes problem statement, proposed solution, alternatives considered, and rollback plan", created_at: "2025-05-15" },
        Mem { id: "lme-352", summary: "New on-call compensation policy: engineers receive $500/week for primary on-call, $250/week for secondary. Overtime pay for incidents lasting more than 2 hours", created_at: "2025-06-01" },
        Mem { id: "lme-353", summary: "Adopted InnerSource model for shared libraries: any team can contribute to platform libraries via PRs, platform team reviews within 24 hours, bi-weekly sync meetings", created_at: "2025-06-15" },
        Mem { id: "lme-354", summary: "Engineering ladder update: added Staff Engineer and Principal Engineer levels with clear expectations around technical leadership, mentorship, and org-wide impact", created_at: "2025-07-01" },
        Mem { id: "lme-355", summary: "Implemented blameless postmortem culture: all incident reviews focus on systemic improvements, no individual blame. Published 15 postmortems in shared knowledge base", created_at: "2025-07-15" },
        Mem { id: "lme-356", summary: "Developer survey results: top 3 pain points are (1) slow CI pipeline (addressed by Bazel), (2) unclear service ownership (addressed by Backstage), (3) too many meetings", created_at: "2025-08-01" },
        Mem { id: "lme-357", summary: "Introduced Focus Fridays: no meetings allowed on Fridays, dedicated to deep work. Engineers report 35% more productivity on feature development tasks", created_at: "2025-08-15" },
        Mem { id: "lme-358", summary: "New hire ramp-up improvement: created service-specific onboarding modules in Backstage. Average time to first production deployment reduced from 3 weeks to 5 days", created_at: "2025-09-01" },
        Mem { id: "lme-359", summary: "Promoted 4 engineers to senior level in Q3 2025. Created mentorship program pairing each new senior with a staff engineer for their first quarter", created_at: "2025-10-01" },
        Mem { id: "lme-360", summary: "Adopted DORA metrics tracking: deployment frequency now at 12/day, lead time 2.3 hours, change failure rate 3.2%, MTTR 28 minutes. Categorized as Elite performers", created_at: "2025-10-15" },
        Mem { id: "lme-361", summary: "Team rotation program launched: engineers can rotate to a different team for one quarter per year to build cross-team knowledge and prevent silos", created_at: "2025-11-01" },
        Mem { id: "lme-362", summary: "Implemented team API contracts: each team publishes a team API defining how to request work, escalate issues, and access their services. Reviewed quarterly", created_at: "2025-11-15" },
        Mem { id: "lme-363", summary: "Hiring freeze lifted: approved headcount for 6 new engineers across platform (2), product (3), and data (1) teams. Interviews starting in January 2026", created_at: "2025-12-01" },
        Mem { id: "lme-364", summary: "Introduced Engineering Excellence Awards: quarterly recognition for engineers who demonstrate exceptional technical leadership, operational excellence, or mentorship", created_at: "2025-12-15" },
        Mem { id: "lme-365", summary: "Knowledge sharing sessions: bi-weekly tech talks where teams present their recent work. Topics so far include CockroachDB migration, Temporal workflows, and Zig performance tricks", created_at: "2026-01-01" },
        Mem { id: "lme-366", summary: "Remote work policy updated: fully remote with optional quarterly in-person offsites. Core collaboration hours set to 10am-2pm Pacific across all teams", created_at: "2026-01-15" },
        Mem { id: "lme-367", summary: "SRE rotation expanded: product engineers now participate in SRE on-call for their own services. Each product team has one engineer trained on production operations", created_at: "2026-02-01" },
        Mem { id: "lme-368", summary: "Annual developer experience survey: overall satisfaction 4.1/5.0. Biggest improvements from last year: CI speed (+1.2 points) and documentation (+0.8 points)", created_at: "2026-02-15" },
        Mem { id: "lme-369", summary: "Team topology change: moved from component teams (frontend, backend, infra) to stream-aligned teams (payments, catalog, logistics). Each team owns full stack for their domain", created_at: "2026-03-01" },
        Mem { id: "lme-370", summary: "Introduced Architecture Advisory Board: 5 senior/staff engineers who review all RFCs and provide non-blocking recommendations. Meets weekly for 1 hour", created_at: "2026-03-15" },
        Mem { id: "lme-371", summary: "New performance review cycle: switched from annual to semi-annual reviews with quarterly check-ins. Calibration across teams to ensure consistent leveling", created_at: "2025-04-01" },
        Mem { id: "lme-372", summary: "Bug bash event: dedicated 2-day company-wide bug bash fixed 89 bugs, 12 of which were customer-reported. Top bug fixer awarded extra PTO day", created_at: "2025-06-10" },
        Mem { id: "lme-373", summary: "Intern program: hired 4 summer interns across platform and product teams. Each intern paired with a mentor and assigned a self-contained project", created_at: "2025-06-20" },
        Mem { id: "lme-374", summary: "Engineering all-hands: presented 2025 technical strategy focusing on reliability (99.95% SLA), developer velocity (sub-10 minute CI), and data platform maturity", created_at: "2025-04-08" },
        Mem { id: "lme-375", summary: "Sprint planning improvement: adopted story mapping technique for quarterly planning. Each team creates a story map before breaking work into individual stories", created_at: "2025-09-10" },
        Mem { id: "lme-376", summary: "Workload distribution analysis: discovered that 3 engineers handle 60% of incident response. Redistributing on-call load and providing additional training to newer team members", created_at: "2025-10-20" },
        Mem { id: "lme-377", summary: "Pair programming initiative: introduced optional pair programming sessions for complex tasks. Teams reporting 25% fewer bugs in code written during pair sessions", created_at: "2025-12-05" },
        Mem { id: "lme-378", summary: "Exit interview insights: two departing engineers cited lack of career growth visibility. Responded by publishing the updated engineering ladder and creating individual growth plans", created_at: "2026-03-25" },

        // --- More incidents/postmortems (30) ---
        Mem { id: "lme-379", summary: "Incident: Terraform state file corruption after interrupted apply. Lost track of 15 AWS resources. Recovered by importing resources and reconciling with cloud inventory", created_at: "2025-04-12" },
        Mem { id: "lme-380", summary: "Postmortem: MongoDB Atlas cluster ran out of IOPS during Black Friday load test. Root cause was unoptimized aggregation pipeline scanning 10M documents without index", created_at: "2025-04-20" },
        Mem { id: "lme-381", summary: "Incident: Istio control plane OOM killed during large-scale deployment. 200 pods were updated simultaneously, overwhelming the sidecar injection webhook. Added rate limiting to rollouts", created_at: "2025-05-05" },
        Mem { id: "lme-382", summary: "Postmortem: ArgoCD accidentally deployed staging config to production due to overlapping Kustomize paths. Added environment label validation in the sync hook", created_at: "2025-05-18" },
        Mem { id: "lme-383", summary: "Incident: Prometheus storage full, 4-hour monitoring blackout. TSDB compaction was disabled during a migration and never re-enabled. Added alert for TSDB health", created_at: "2025-06-01" },
        Mem { id: "lme-384", summary: "Postmortem: CockroachDB multi-region cluster had 30-minute unavailability during AZ outage. Lease transfer took longer than expected because of large range sizes. Reduced to 512MB ranges", created_at: "2025-06-15" },
        Mem { id: "lme-385", summary: "Incident: OPA policy change blocked all deployments for 2 hours. A new deny-all-by-default policy was applied to the wrong namespace. Added policy staging environment", created_at: "2025-07-01" },
        Mem { id: "lme-386", summary: "Postmortem: NATS JetStream data loss for 500 messages during node replacement. Stream replication factor was set to 1 instead of 3 for the affected stream", created_at: "2025-07-15" },
        Mem { id: "lme-387", summary: "Incident: TimescaleDB continuous aggregate stopped updating silently. Background worker crashed and was not restarted by the scheduler. Added health check for background workers", created_at: "2025-08-01" },
        Mem { id: "lme-388", summary: "Postmortem: Temporal workflow execution stuck for 8 hours. Activity retry policy had infinite retries with no max interval, causing exponential backoff to reach 6-hour delays", created_at: "2025-08-15" },
        Mem { id: "lme-389", summary: "Incident: Grafana dashboards returning blank panels for 1 hour. Loki query frontend ran out of memory processing a regex query that matched 10M log lines. Added query limits", created_at: "2025-09-01" },
        Mem { id: "lme-390", summary: "Postmortem: Clickhouse distributed table returned incorrect aggregation results. The issue was a version mismatch between nodes causing different merge tree behavior. Upgraded all nodes", created_at: "2025-09-15" },
        Mem { id: "lme-391", summary: "Incident: Crossplane provider reconciliation storm consumed all API rate limits against AWS. A misconfigured polling interval of 1 second on 500 resources hit the EC2 API throttle", created_at: "2025-10-01" },
        Mem { id: "lme-392", summary: "Postmortem: Envoy proxy caused 30% latency increase after config update. New outlier detection settings were too aggressive, ejecting healthy hosts during normal load variance", created_at: "2025-10-15" },
        Mem { id: "lme-393", summary: "Incident: Bazel remote cache eviction during peak CI hours. Cache was sized for normal load but Black Friday prep caused 3x normal build volume. Doubled cache storage", created_at: "2025-11-01" },
        Mem { id: "lme-394", summary: "Postmortem: Meilisearch search latency degraded to 5 seconds after index grew to 2M documents. Default maximum indexing memory was too low. Increased to 4GB from 1GB default", created_at: "2025-11-15" },
        Mem { id: "lme-395", summary: "Incident: Qdrant cluster lost 1 of 3 nodes, causing 33% of vectors to become unavailable. Replication factor was 1 due to initial deployment misconfiguration. Rebuilt with RF=2", created_at: "2025-12-01" },
        Mem { id: "lme-396", summary: "Postmortem: ScyllaDB repair operation during peak hours caused 200ms latency spike. Scheduled repairs to 3am maintenance window and limited repair parallelism", created_at: "2025-12-15" },
        Mem { id: "lme-397", summary: "Incident: Debezium connector lost position in WAL after PostgreSQL maintenance restart. 4 hours of events were missed. Added WAL retention policy of 72 hours minimum", created_at: "2026-01-01" },
        Mem { id: "lme-398", summary: "Postmortem: Benthos pipeline dropped 10K messages during Kafka broker rolling restart. Consumer group rebalance timeout was too short. Increased to 2 minutes with static membership", created_at: "2026-01-15" },
        Mem { id: "lme-399", summary: "Incident: Tauri app crashed on Windows after update due to WebView2 runtime version mismatch. Added runtime version check on startup with auto-download fallback", created_at: "2026-02-01" },
        Mem { id: "lme-400", summary: "Postmortem: DuckDB analytics CLI returned stale data. Local Parquet file cache was not invalidated when upstream data was updated. Added ETag-based cache validation", created_at: "2026-02-15" },
        Mem { id: "lme-401", summary: "Incident: Vault seal event caused all dynamic credentials to stop rotating for 45 minutes. Auto-unseal with AWS KMS was not configured for the HA standby nodes", created_at: "2026-03-01" },
        Mem { id: "lme-402", summary: "Postmortem: SurrealDB query planner chose full table scan for a graph traversal query after a schema migration. Rebuilt all indexes and added query plan verification in CI", created_at: "2026-03-10" },
        Mem { id: "lme-403", summary: "Incident: Zitadel login page unresponsive for 20 minutes during traffic spike. Connection pool to PostgreSQL was exhausted. Increased pool size and added PgBouncer in front", created_at: "2025-12-20" },
        Mem { id: "lme-404", summary: "Postmortem: Mergify merge queue processed a PR that had a failing optional check as required. Check name was renamed in CI config but not updated in Mergify rules", created_at: "2026-01-05" },
        Mem { id: "lme-405", summary: "Incident: Garage storage node failed to rejoin cluster after maintenance. Disk UUID changed after firmware update, causing the node to be treated as new. Manual cluster membership fix applied", created_at: "2026-01-25" },
        Mem { id: "lme-406", summary: "Postmortem: OpenTelemetry Collector dropped 15% of traces during peak load. Memory ballast was not configured, causing frequent GC pauses. Added 512MB ballast", created_at: "2026-02-10" },
        Mem { id: "lme-407", summary: "Incident: Pkl configuration validation passed in CI but failed at runtime due to environment variable reference that existed in CI but not in production. Added runtime env check to validation", created_at: "2026-02-20" },
        Mem { id: "lme-408", summary: "Postmortem: Valkey cluster data loss during node replacement. CLUSTER FAILOVER was issued before replication was complete on the new replica. Added replication lag check to failover procedure", created_at: "2026-03-05" },

        // --- More conversations (42) ---
        Mem { id: "lme-409", summary: "Meeting with Terraform Cloud sales team: evaluated managed state backend. Decided against it because our S3+DynamoDB setup works well and avoids vendor lock-in", created_at: "2025-04-10" },
        Mem { id: "lme-410", summary: "1:1 with Maria: she wants to lead the Istio service mesh rollout. Has prior experience at Google with Istio. Assigned her as tech lead for the initiative", created_at: "2025-04-18" },
        Mem { id: "lme-411", summary: "Architecture review: debated MongoDB vs PostgreSQL JSONB for catalog service. MongoDB won because of native text search, change streams, and the catalog team's existing expertise", created_at: "2025-04-25" },
        Mem { id: "lme-412", summary: "Vendor call with MongoDB Atlas: negotiated dedicated cluster pricing at $2,400/month for M50 tier with NVMe storage. Includes 24/7 support and proactive monitoring", created_at: "2025-05-05" },
        Mem { id: "lme-413", summary: "Discussion with security team about OPA adoption: they want centralized policy management. Agreed to start with Kubernetes admission control and expand to API auth in Q3", created_at: "2025-05-15" },
        Mem { id: "lme-414", summary: "1:1 with Carlos: interested in learning Rust for the new high-performance services. Approved 20% time for Rust learning, paired with the Zig/Rust working group", created_at: "2025-05-25" },
        Mem { id: "lme-415", summary: "Meeting with Temporal.io solutions engineer: discussed workflow design patterns for our order fulfillment use case. Recommended child workflows for sub-processes", created_at: "2025-06-05" },
        Mem { id: "lme-416", summary: "Architecture review: TimescaleDB vs InfluxDB for metrics storage. TimescaleDB won because it runs on PostgreSQL, allowing SQL joins with business data for analytics", created_at: "2025-06-15" },
        Mem { id: "lme-417", summary: "1:1 with Lin: frustrated with on-call load distribution. Acknowledged the problem and committed to hiring dedicated SREs and redistributing on-call rotation", created_at: "2025-06-25" },
        Mem { id: "lme-418", summary: "Vendor evaluation: Clickhouse Cloud vs self-managed Clickhouse. Chose self-managed because we need custom merge tree configurations not available in managed service", created_at: "2025-07-05" },
        Mem { id: "lme-419", summary: "Discussion about Grafana dashboards as code: decided to manage all dashboards via Terraform grafana_dashboard resources with version control and PR-based changes", created_at: "2025-07-15" },
        Mem { id: "lme-420", summary: "Meeting with CockroachDB support: discussed our multi-region setup. They recommended locality-aware zone configs and increasing range cache size for our read-heavy workload", created_at: "2025-07-25" },
        Mem { id: "lme-421", summary: "1:1 with Priya: proposed creating an internal developer platform team. Discussed scope, headcount needs, and overlap with existing platform engineering team", created_at: "2025-08-05" },
        Mem { id: "lme-422", summary: "Architecture review: Meilisearch vs Algolia for documentation search. Meilisearch chosen for self-hosting requirement and comparable relevance quality at 10% of the cost", created_at: "2025-08-15" },
        Mem { id: "lme-423", summary: "Vendor call with Qdrant: discussed their managed cloud offering. Staying self-hosted for now but will revisit if operational overhead exceeds 10 hours/month", created_at: "2025-08-25" },
        Mem { id: "lme-424", summary: "Meeting with data team: they need real-time event replay for debugging. Evaluated Debezium CDC vs custom Kafka consumers. Debezium chosen for lower maintenance burden", created_at: "2025-09-05" },
        Mem { id: "lme-425", summary: "Discussion about ScyllaDB vs DynamoDB for session store: ScyllaDB chosen for predictable pricing (no per-request charges) and CQL compatibility with existing Cassandra code", created_at: "2025-09-15" },
        Mem { id: "lme-426", summary: "1:1 with Alex: wants to transition from backend to SRE. Created a 6-month development plan including Kubernetes certification and incident commander training", created_at: "2025-09-25" },
        Mem { id: "lme-427", summary: "Architecture review: Benthos vs custom Go consumers for stream processing. Benthos chosen for declarative configuration and built-in backpressure handling", created_at: "2025-10-05" },
        Mem { id: "lme-428", summary: "Meeting with Tauri team: discussed migration path from Electron admin dashboard. Key considerations: WebView compatibility, native API access, and bundle size reduction", created_at: "2025-10-15" },
        Mem { id: "lme-429", summary: "Vendor call with Pulumi: evaluated Pulumi vs Terraform for data platform. Data team prefers TypeScript over HCL. Agreed to let data team use Pulumi while infra stays on Terraform", created_at: "2025-10-25" },
        Mem { id: "lme-430", summary: "Discussion about DuckDB adoption: product analysts want fast local queries without connecting to Snowflake. Proof of concept showed 50x faster queries for under-100GB datasets", created_at: "2025-11-05" },
        Mem { id: "lme-431", summary: "1:1 with Kai: discussed his interest in contributing to open-source Temporal libraries. Approved 10% time for open-source contribution as professional development", created_at: "2025-11-15" },
        Mem { id: "lme-432", summary: "Architecture review: Encore.go framework evaluation. Pros: faster bootstrapping, built-in infra primitives. Cons: vendor lock-in risk, smaller community. Approved for 2 pilot services", created_at: "2025-11-25" },
        Mem { id: "lme-433", summary: "Meeting with Zitadel team: discussed custom identity provider requirements. They confirmed support for our SAML IdP integration and custom claims from external APIs", created_at: "2025-12-05" },
        Mem { id: "lme-434", summary: "Discussion about Garage vs MinIO: Garage chosen for production because of better multi-node failure handling. MinIO kept for air-gapped staging with simpler requirements", created_at: "2025-12-15" },
        Mem { id: "lme-435", summary: "1:1 with Sarah: discussing burnout from Q4 crunch. Agreed to reduce her scope in Q1 and pair her with a junior engineer to distribute knowledge", created_at: "2025-12-22" },
        Mem { id: "lme-436", summary: "Vendor evaluation: Buf vs protoc-gen-validate for protobuf validation. Buf provides better developer experience with CLI tooling, schema registry, and CI integration", created_at: "2026-01-05" },
        Mem { id: "lme-437", summary: "Architecture review: cell-based architecture proposal for payment service. Discussed blast radius isolation, routing layer design, and data partitioning strategy per cell", created_at: "2026-01-15" },
        Mem { id: "lme-438", summary: "Meeting with Valkey contributors: discussed migration path from Redis. They confirmed API compatibility for all our use cases except Redis Streams consumer groups (workaround available)", created_at: "2026-01-25" },
        Mem { id: "lme-439", summary: "Discussion about Pkl vs CUE for configuration: Pkl chosen for better IDE support, more intuitive syntax for non-programmers, and native module system", created_at: "2026-02-05" },
        Mem { id: "lme-440", summary: "1:1 with Jordan: promoted to Staff Engineer. New responsibilities include leading the Architecture Advisory Board and mentoring 3 senior engineers", created_at: "2026-02-15" },
        Mem { id: "lme-441", summary: "Vendor call with Wasmer: discussed the sandbox escape findings. They acknowledged the issues and committed to patches in version 5.2. We will re-evaluate after release", created_at: "2026-02-25" },
        Mem { id: "lme-442", summary: "Meeting with Grafana Labs: negotiated enterprise support contract for Grafana, Loki, and Tempo. $18K/year for 24/7 support with 1-hour response SLA for severity 1", created_at: "2026-03-05" },
        Mem { id: "lme-443", summary: "Architecture review: gRPC-Web adoption plan. Envoy transcoding proxy will handle protocol translation, allowing gradual migration of REST endpoints to gRPC without breaking clients", created_at: "2026-03-12" },
        Mem { id: "lme-444", summary: "Discussion about Parquet vs Avro for data lake: Parquet chosen for columnar storage efficiency on analytical queries. Avro kept for Kafka message serialization only", created_at: "2026-03-18" },
        Mem { id: "lme-445", summary: "1:1 with Lena: she completed the Kubernetes CKA certification. Transitioning her from application development to the platform SRE rotation starting next quarter", created_at: "2026-03-22" },
        Mem { id: "lme-446", summary: "Meeting with MotherDuck about DuckDB cloud integration: shared analytical queries would eliminate data duplication across team members. Piloting with 5 analysts", created_at: "2026-03-25" },
        Mem { id: "lme-447", summary: "Discussion about Mergify vs GitHub merge queue: Mergify chosen for batch merging capability, priority queues, and better configuration for monorepo with multiple CI pipelines", created_at: "2025-12-10" },
        Mem { id: "lme-448", summary: "Architecture review: Dragonfly as Redis replacement assessment. Benchmarks show 25x improvement for multi-core workloads. Approved for session service, monitoring for stability", created_at: "2025-07-05" },
        Mem { id: "lme-449", summary: "Vendor call with Fly.io: discussed edge function pricing for marketing site. $0.15/GB bandwidth with included SSL and DDoS protection. Cheaper than CloudFront for our traffic pattern", created_at: "2025-06-10" },
        Mem { id: "lme-450", summary: "1:1 with David: discussed his frustration with flaky integration tests. Agreed to dedicate a sprint to test infrastructure improvements including test isolation and parallelism", created_at: "2026-03-28" },

        // --- More infrastructure/devops (50) ---
        Mem { id: "lme-451", summary: "Terraform workspace strategy: one workspace per environment (dev, staging, prod) per service. State stored in S3 with DynamoDB locking and versioning enabled", created_at: "2025-04-05" },
        Mem { id: "lme-452", summary: "ArgoCD app-of-apps deployment: configured progressive sync waves — infrastructure (wave 1), databases (wave 2), services (wave 3), ingress (wave 4) — ensuring dependency ordering", created_at: "2025-04-15" },
        Mem { id: "lme-453", summary: "Prometheus federation setup: regional Prometheus instances scrape local targets, global Prometheus federates aggregated metrics for cross-region dashboards", created_at: "2025-04-25" },
        Mem { id: "lme-454", summary: "Kubernetes cluster upgrade strategy: in-place upgrade with PodDisruptionBudgets ensuring zero downtime. Validated on staging cluster 1 week before production", created_at: "2025-05-05" },
        Mem { id: "lme-455", summary: "Ansible decommissioning phase 1: migrated all EC2 instance provisioning to Terraform launch templates. Ansible retained only for legacy VM configuration management", created_at: "2025-05-15" },
        Mem { id: "lme-456", summary: "Grafana alerting rules migration: moved from Grafana alerting to Prometheus AlertManager for better routing, silencing, and PagerDuty integration", created_at: "2025-05-25" },
        Mem { id: "lme-457", summary: "CockroachDB backup strategy: scheduled full backup daily to S3, incremental backup hourly. Point-in-time recovery tested monthly with 5-minute RPO", created_at: "2025-06-05" },
        Mem { id: "lme-458", summary: "Istio canary deployment automation: Flagger watches Prometheus metrics during canary and automatically promotes or rolls back based on error rate and latency thresholds", created_at: "2025-06-15" },
        Mem { id: "lme-459", summary: "NATS cluster deployment: 3-node JetStream cluster across AZs with 3x replication for critical streams and 1x for best-effort telemetry streams", created_at: "2025-06-25" },
        Mem { id: "lme-460", summary: "Temporal Cloud migration: moved self-hosted Temporal to Temporal Cloud, eliminating 3 ElasticSearch nodes, 3 Cassandra nodes, and 2 Temporal server instances", created_at: "2025-07-05" },
        Mem { id: "lme-461", summary: "Clickhouse cluster deployment: 3-shard, 2-replica cluster on dedicated EC2 instances with NVMe storage. ZooKeeper replaced by Clickhouse Keeper for consensus", created_at: "2025-07-15" },
        Mem { id: "lme-462", summary: "Crossplane AWS provider configuration: separate provider configs for each AWS account (dev, staging, prod) with IAM roles assumed via IRSA for Kubernetes-native auth", created_at: "2025-07-25" },
        Mem { id: "lme-463", summary: "Bazel remote execution setup: configured BuildBarn cluster on spare EC2 instances for parallel build execution, reducing full monorepo build from 12 minutes to 3 minutes", created_at: "2025-08-05" },
        Mem { id: "lme-464", summary: "Meilisearch high availability: deployed 3-node cluster behind HAProxy with health checks. Automatic failover tested monthly with simulated node failure", created_at: "2025-08-15" },
        Mem { id: "lme-465", summary: "Qdrant deployment: 3-node cluster on Kubernetes with persistent volumes. Configured HNSW indexing on NVMe-backed storage for consistent low-latency vector search", created_at: "2025-08-25" },
        Mem { id: "lme-466", summary: "ScyllaDB cluster deployment: 6-node cluster across 3 AZs with NetworkTopologyStrategy replication factor 3. Monitoring via ScyllaDB Monitoring Stack (Grafana+Prometheus)", created_at: "2025-09-05" },
        Mem { id: "lme-467", summary: "Debezium connector deployment: running on Kafka Connect cluster with 3 workers. Separate connectors per database to isolate failure domains and enable independent scaling", created_at: "2025-09-15" },
        Mem { id: "lme-468", summary: "Benthos deployment: containerized with auto-scaling based on Kafka consumer lag metric from Prometheus. Scales from 2 to 20 pods based on processing backlog", created_at: "2025-09-25" },
        Mem { id: "lme-469", summary: "Vault HA deployment: 3-node cluster with Raft storage backend, auto-unseal via AWS KMS, and TLS termination at the load balancer. Audit logs shipped to Splunk", created_at: "2025-10-05" },
        Mem { id: "lme-470", summary: "SurrealDB deployment: single-node deployment with nightly backup to S3. Evaluating TiKV backend for distributed deployment once query volume exceeds current capacity", created_at: "2025-10-15" },
        Mem { id: "lme-471", summary: "Zitadel deployment: 2-instance deployment behind ALB with PostgreSQL RDS backend. Configured custom branding, login flows, and machine-to-machine OAuth for service auth", created_at: "2025-10-25" },
        Mem { id: "lme-472", summary: "Grafana Tempo deployment: single binary mode with S3 backend. Configured compactor, distributor, ingester, and querier components as a single process for simplicity", created_at: "2025-11-05" },
        Mem { id: "lme-473", summary: "OpenTelemetry Collector deployment: DaemonSet on every node for log/metrics collection, plus Deployment for trace processing with tail-based sampling", created_at: "2025-11-15" },
        Mem { id: "lme-474", summary: "Litestream deployment: configured as sidecar container alongside the SQLite-based config service. Continuous WAL replication to S3 with 1-second RPO", created_at: "2025-11-25" },
        Mem { id: "lme-475", summary: "Mergify configuration: branch protection rules require 2 approvals, CI pass, and merge queue check. Auto-merge enabled for dependency update PRs from Dependabot", created_at: "2025-12-05" },
        Mem { id: "lme-476", summary: "Dragonfly deployment: replaced Redis Sentinel with single Dragonfly instance. Multi-threaded architecture eliminates need for sharding at our current workload level", created_at: "2025-12-15" },
        Mem { id: "lme-477", summary: "MinIO staging deployment: 4-node erasure-coded cluster on bare metal servers in the office. Used for integration testing without incurring AWS S3 costs", created_at: "2025-12-25" },
        Mem { id: "lme-478", summary: "Garage production deployment: 5-node cluster with 3-of-5 erasure coding across 3 physical locations. Handles 50TB of archived data with automatic data rebalancing", created_at: "2026-01-05" },
        Mem { id: "lme-479", summary: "Pkl deployment pipeline: configuration changes go through the same PR review and CI pipeline as code. Pkl evaluate runs in CI to validate all configs before merge", created_at: "2026-01-15" },
        Mem { id: "lme-480", summary: "Valkey migration rollout: canary deployment with 10% traffic to Valkey, 90% to Redis. Application-level dual-write with consistency comparison for validation", created_at: "2026-01-25" },
        Mem { id: "lme-481", summary: "Caddy deployment for developer environments: configured local reverse proxy with automatic TLS certificates for *.local.dev domain using internal CA", created_at: "2025-06-10" },
        Mem { id: "lme-482", summary: "Deno Deploy configuration: edge functions deployed from GitHub Actions, environment variables from Vault, and custom domain with Cloudflare DNS", created_at: "2025-07-01" },
        Mem { id: "lme-483", summary: "Encore.go service deployment: Encore Cloud handles infrastructure provisioning. Each push to main triggers automatic deployment to staging, manual promotion to production", created_at: "2025-11-10" },
        Mem { id: "lme-484", summary: "Tauri app distribution: GitHub Releases with automatic update checking. Code-signed for macOS and Windows. Linux distributed via AppImage and Flatpak", created_at: "2025-10-28" },
        Mem { id: "lme-485", summary: "DuckDB binary distribution: compiled as static binary with no runtime dependencies. Distributed via internal Homebrew tap with version pinning per analytics team", created_at: "2025-11-08" },
        Mem { id: "lme-486", summary: "Buf schema registry deployment: self-hosted BSR instance on Kubernetes with PostgreSQL backend. CI pipelines push schema changes on merge to main", created_at: "2025-10-12" },
        Mem { id: "lme-487", summary: "Wasmer plugin deployment: WASM modules stored in OCI registry alongside container images. Plugin manifests specify required capabilities and resource limits", created_at: "2026-02-05" },
        Mem { id: "lme-488", summary: "Fly.io deployment: marketing site deployed across 4 regions (IAD, CDG, NRT, SYD). Auto-scaling from 1 to 10 machines per region based on request queue depth", created_at: "2025-06-15" },
        Mem { id: "lme-489", summary: "Parquet data lake on S3: organized with Hive-style partitioning by date/region/source. AWS Glue Crawler maintains the schema catalog for Athena and DuckDB queries", created_at: "2026-02-15" },
        Mem { id: "lme-490", summary: "Vitess deployment: 2 vtgate instances, 4 vttablet instances across 4 shards. MySQL protocol compatible, so the legacy billing application required zero code changes", created_at: "2025-05-10" },
        Mem { id: "lme-491", summary: "Dapr sidecar deployment: injected via Kubernetes annotations with component specs for Kafka pub/sub, CockroachDB state store, and Vault secret store", created_at: "2025-11-20" },
        Mem { id: "lme-492", summary: "Zig cross-compilation CI: GitHub Actions workflow builds ARM64 and x86_64 binaries on Linux and macOS from a single Zig source without Docker or QEMU", created_at: "2026-02-20" },
        Mem { id: "lme-493", summary: "Clickhouse Keeper deployment: replaced ZooKeeper with native Clickhouse Keeper for simpler operations. 3-node Keeper ensemble using Raft consensus", created_at: "2025-07-18" },
        Mem { id: "lme-494", summary: "MongoDB Atlas peering: configured VPC peering between our AWS VPC and Atlas dedicated cluster, eliminating public internet traffic and reducing latency by 15ms", created_at: "2025-04-18" },
        Mem { id: "lme-495", summary: "Terraform module registry: set up private registry on GitHub with semantic versioning. Teams consume modules by version tag, preventing unintended infrastructure changes", created_at: "2025-04-28" },
        Mem { id: "lme-496", summary: "ArgoCD notifications: configured Slack notifications for sync failures and degraded application health. PagerDuty integration for production sync failures during business hours", created_at: "2025-05-08" },
        Mem { id: "lme-497", summary: "Prometheus long-term storage: configured Thanos sidecar with S3 backend for metrics retention beyond 15 days. Query path goes through Thanos Query for seamless access", created_at: "2025-05-20" },
        Mem { id: "lme-498", summary: "CockroachDB changefeed to Kafka: configured rangefeed-based changefeeds for 3 critical tables, publishing row-level changes to Kafka topics for downstream consumers", created_at: "2025-06-01" },
        Mem { id: "lme-499", summary: "OPA bundle server: policies distributed via HTTP bundle served from S3. Gatekeeper syncs policies every 60 seconds with signature verification", created_at: "2025-06-20" },
        Mem { id: "lme-500", summary: "TimescaleDB deployment: single-node with 2TB NVMe storage on r6gd.2xlarge. Automated setup with pg_partman for non-timescale tables and scheduled compression policy", created_at: "2025-06-28" },
    ];

    corpus.extend(additional);
    corpus
}

// ---------------------------------------------------------------------------
// Large eval cases: 120 queries targeting the 500-memory corpus
// ---------------------------------------------------------------------------

fn build_large_eval_cases() -> Vec<EvalCase> {
    let mut cases = build_eval_cases();

    let additional = vec![
        // ===== INFORMATION EXTRACTION (8 new queries targeting new memories) =====
        EvalCase {
            query: "what service mesh did we choose and why",
            expected: vec!["lme-131"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "GitOps continuous delivery tool",
            expected: vec!["lme-132"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "workflow orchestration engine for business processes",
            expected: vec!["lme-138"],
            task_type: TaskType::InformationExtraction,
            difficulty: "medium",
        },
        EvalCase {
            query: "time-series database for metrics",
            expected: vec!["lme-137"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "policy enforcement across Kubernetes and API authorization",
            expected: vec!["lme-135"],
            task_type: TaskType::InformationExtraction,
            difficulty: "medium",
        },
        EvalCase {
            query: "real-time analytics OLAP engine",
            expected: vec!["lme-145"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "build system for the monorepo",
            expected: vec!["lme-156"],
            task_type: TaskType::InformationExtraction,
            difficulty: "easy",
        },
        EvalCase {
            query: "on-call compensation and overtime policy",
            expected: vec!["lme-352"],
            task_type: TaskType::InformationExtraction,
            difficulty: "medium",
        },
        // ===== TEMPORAL REASONING (8 new queries targeting new date ranges) =====
        EvalCase {
            query: "architecture decisions in April 2025",
            expected: vec!["lme-129", "lme-130", "lme-159"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "medium",
        },
        EvalCase {
            query: "incidents during June 2025",
            expected: vec!["lme-383", "lme-384"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "medium",
        },
        EvalCase {
            query: "what happened in May 2025",
            expected: vec!["lme-133", "lme-134", "lme-232", "lme-233"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "hiring and team changes in 2025",
            expected: vec!["lme-349", "lme-350", "lme-354", "lme-363"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "security work in Q1 2026",
            expected: vec!["lme-336", "lme-337", "lme-338", "lme-339", "lme-340"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "vendor evaluations in late 2025",
            expected: vec!["lme-418", "lme-422", "lme-423"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "sprint reviews in August 2025",
            expected: vec!["lme-238", "lme-239"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "medium",
        },
        EvalCase {
            query: "what was deployed in December 2025",
            expected: vec!["lme-248", "lme-249", "lme-261"],
            task_type: TaskType::TemporalReasoning,
            difficulty: "hard",
        },
        // ===== MULTI-HOP (8 new queries connecting old and new memories) =====
        EvalCase {
            query: "all infrastructure-as-code tools we use or evaluated",
            expected: vec!["lme-129", "lme-133", "lme-149"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "everything related to Vault and secrets across the system",
            expected: vec!["lme-017", "lme-073", "lme-293", "lme-330", "lme-401"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "distributed tracing and observability improvements",
            expected: vec!["lme-072", "lme-150", "lme-176", "lme-301"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "database technologies selected across the company",
            expected: vec!["lme-002", "lme-130", "lme-134", "lme-137", "lme-145"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "all Kubernetes-related incidents and fixes",
            expected: vec!["lme-025", "lme-094", "lme-381", "lme-385"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "Kafka ecosystem tools and configurations",
            expected: vec!["lme-003", "lme-170", "lme-287", "lme-467"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        EvalCase {
            query: "team promotions and career growth discussions",
            expected: vec!["lme-354", "lme-359", "lme-440"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "medium",
        },
        EvalCase {
            query: "protobuf and gRPC related decisions and tools",
            expected: vec!["lme-006", "lme-153", "lme-177", "lme-302"],
            task_type: TaskType::MultiHopReasoning,
            difficulty: "hard",
        },
        // ===== KNOWLEDGE UPDATE (8 new queries about updated facts) =====
        EvalCase {
            query: "current state of the Ansible infrastructure",
            expected: vec!["lme-265"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "medium",
        },
        EvalCase {
            query: "current status of the Vitess MySQL sharding",
            expected: vec!["lme-241"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "medium",
        },
        EvalCase {
            query: "are we using Redis or Valkey now",
            expected: vec!["lme-155", "lme-255"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "hard",
        },
        EvalCase {
            query: "current WebAssembly plugin system status",
            expected: vec!["lme-257", "lme-338"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "hard",
        },
        EvalCase {
            query: "what happened with the Deno edge functions project",
            expected: vec!["lme-246"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "medium",
        },
        EvalCase {
            query: "latest penetration test results",
            expected: vec!["lme-337"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "medium",
        },
        EvalCase {
            query: "current team structure and organization",
            expected: vec!["lme-369"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "medium",
        },
        EvalCase {
            query: "what is our current Bazel build performance",
            expected: vec!["lme-156", "lme-242"],
            task_type: TaskType::KnowledgeUpdate,
            difficulty: "hard",
        },
        // ===== ABSTRACTION (8 new queries synthesizing across old and new) =====
        EvalCase {
            query: "how has our data infrastructure evolved",
            expected: vec!["lme-070", "lme-145", "lme-170", "lme-178"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "overall incident management maturity",
            expected: vec!["lme-083", "lme-355", "lme-367", "lme-348"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "engineering culture and developer experience",
            expected: vec!["lme-356", "lme-357", "lme-358", "lme-368"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "supply chain security and software integrity",
            expected: vec!["lme-334", "lme-346", "lme-324"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "multi-region and global deployment strategy",
            expected: vec!["lme-118", "lme-134", "lme-274", "lme-384"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "stream processing and event-driven architecture",
            expected: vec!["lme-136", "lme-167", "lme-170", "lme-288"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "infrastructure cost management across the org",
            expected: vec!["lme-050", "lme-111", "lme-253", "lme-460"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
        EvalCase {
            query: "encryption and data protection strategy",
            expected: vec!["lme-076", "lme-320", "lme-323", "lme-347"],
            task_type: TaskType::Abstraction,
            difficulty: "hard",
        },
    ];

    cases.extend(additional);
    cases
}

// ---------------------------------------------------------------------------
// Helpers for the 500-memory tests
// ---------------------------------------------------------------------------

fn setup_large_corpus(conn: &Connection) {
    migration::runner::run_migrations(conn).unwrap();
    let corpus = build_large_corpus();

    for mem in &corpus {
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) VALUES (?1, ?2, ?3, 'clear', ?4)",
            rusqlite::params![mem.id, format!("hash-{}", mem.id), mem.summary, mem.created_at],
        ).unwrap();
    }
}

fn run_evaluation_large(
    conn: &Connection,
    lance: &LanceStorage,
    embedder: Option<&clearmemory::storage::embeddings::EmbeddingManager>,
    cases: &[EvalCase],
    summaries: &HashMap<String, String>,
    label: &str,
    corpus_size: usize,
) {
    let resolver = HeuristicResolver;
    let reranker = PassthroughReranker;

    let config = RecallConfig {
        top_k: 10,
        temporal_boost: 0.4,
        entity_boost: 0.3,
        include_archived: false,
        stream_id: None,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut results: Vec<EvalResult> = Vec::new();
    let mut missed: Vec<(usize, &str, Vec<&str>)> = Vec::new();

    for (i, case) in cases.iter().enumerate() {
        let query_vec = embedder.and_then(|e| e.embed_query(case.query).ok());
        let query_slice = query_vec.as_deref();

        let recall_result = rt
            .block_on(retrieval::recall(
                case.query,
                conn,
                lance,
                query_slice,
                &resolver,
                &reranker,
                summaries,
                &config,
            ))
            .unwrap();

        let result_ids: Vec<String> = recall_result
            .results
            .iter()
            .map(|r| r.memory_id.clone())
            .collect();
        let found_by: Vec<Strategy> = recall_result
            .results
            .iter()
            .flat_map(|_r| Vec::new() as Vec<Strategy>)
            .collect();

        let first_rank = case
            .expected
            .iter()
            .filter_map(|e| result_ids.iter().position(|r| r == *e))
            .min();

        let mrr = first_rank.map(|r| 1.0 / (r as f64 + 1.0)).unwrap_or(0.0);

        let recall_at = |k: usize| -> f64 {
            let found = case
                .expected
                .iter()
                .filter(|e| result_ids.iter().take(k).any(|r| r == **e))
                .count();
            if case.expected.is_empty() {
                0.0
            } else {
                found as f64 / case.expected.len() as f64
            }
        };

        let ndcg = compute_ndcg(&case.expected, &result_ids, 10);

        let r1 = recall_at(1);
        let r3 = recall_at(3);
        let r5 = recall_at(5);
        let r10 = recall_at(10);

        if r10 < 1.0 {
            let missing: Vec<&str> = case
                .expected
                .iter()
                .filter(|e| !result_ids.iter().take(10).any(|r| r == **e))
                .copied()
                .collect();
            missed.push((i, case.query, missing));
        }

        results.push(EvalResult {
            case_idx: i,
            task_type: case.task_type,
            difficulty: case.difficulty,
            mrr,
            recall_at_1: r1,
            recall_at_3: r3,
            recall_at_5: r5,
            recall_at_10: r10,
            ndcg_at_10: ndcg,
            found_by,
        });
    }

    // Aggregate and print results
    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  LONGMEMEVAL-STYLE BENCHMARK: {:<30} ║", label);
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!(
        "║  Corpus: {} memories | Queries: {:<26} ║",
        corpus_size,
        cases.len()
    );
    println!("╠══════════════════════════════════════════════════════════════╣");

    let n = results.len() as f64;
    let avg = |f: fn(&EvalResult) -> f64| -> f64 { results.iter().map(f).sum::<f64>() / n };

    let overall_mrr = avg(|r| r.mrr);
    let overall_r1 = avg(|r| r.recall_at_1);
    let overall_r3 = avg(|r| r.recall_at_3);
    let overall_r5 = avg(|r| r.recall_at_5);
    let overall_r10 = avg(|r| r.recall_at_10);
    let overall_ndcg = avg(|r| r.ndcg_at_10);

    println!("║                                                              ║");
    println!("║  OVERALL METRICS                                             ║");
    println!("║  ─────────────────────────────────────────────               ║");
    println!(
        "║  MRR:        {:.4}                                          ║",
        overall_mrr
    );
    println!(
        "║  Recall@1:   {:.4}                                          ║",
        overall_r1
    );
    println!(
        "║  Recall@3:   {:.4}                                          ║",
        overall_r3
    );
    println!(
        "║  Recall@5:   {:.4}                                          ║",
        overall_r5
    );
    println!(
        "║  Recall@10:  {:.4}                                          ║",
        overall_r10
    );
    println!(
        "║  NDCG@10:    {:.4}                                          ║",
        overall_ndcg
    );

    println!("║                                                              ║");
    println!("║  PER TASK TYPE                                               ║");
    println!("║  ─────────────────────────────────────────────               ║");
    println!(
        "║  {:<22} {:>6} {:>7} {:>7} {:>8}       ║",
        "Task Type", "Count", "MRR", "R@5", "R@10"
    );
    println!(
        "║  {:<22} {:>6} {:>7} {:>7} {:>8}       ║",
        "──────────────────────", "─────", "──────", "──────", "───────"
    );

    for task_type in &[
        TaskType::InformationExtraction,
        TaskType::TemporalReasoning,
        TaskType::MultiHopReasoning,
        TaskType::KnowledgeUpdate,
        TaskType::Abstraction,
    ] {
        let task_results: Vec<&EvalResult> = results
            .iter()
            .filter(|r| r.task_type == *task_type)
            .collect();
        if task_results.is_empty() {
            continue;
        }
        let tn = task_results.len() as f64;
        let t_mrr = task_results.iter().map(|r| r.mrr).sum::<f64>() / tn;
        let t_r5 = task_results.iter().map(|r| r.recall_at_5).sum::<f64>() / tn;
        let t_r10 = task_results.iter().map(|r| r.recall_at_10).sum::<f64>() / tn;
        println!(
            "║  {:<22} {:>6} {:>7.4} {:>7.4} {:>8.4}       ║",
            task_type,
            task_results.len(),
            t_mrr,
            t_r5,
            t_r10
        );
    }

    println!("║                                                              ║");
    println!("║  PER DIFFICULTY                                              ║");
    println!("║  ─────────────────────────────────────────────               ║");
    for difficulty in &["easy", "medium", "hard"] {
        let diff_results: Vec<&EvalResult> = results
            .iter()
            .filter(|r| r.difficulty == *difficulty)
            .collect();
        if diff_results.is_empty() {
            continue;
        }
        let dn = diff_results.len() as f64;
        let d_r10 = diff_results.iter().map(|r| r.recall_at_10).sum::<f64>() / dn;
        println!(
            "║  {:<10} ({:>2} queries): Recall@10 = {:.4}                  ║",
            difficulty,
            diff_results.len(),
            d_r10
        );
    }

    if !missed.is_empty() {
        println!("║                                                              ║");
        println!(
            "║  MISSED QUERIES ({})                                       ║",
            missed.len()
        );
        println!("║  ─────────────────────────────────────────────               ║");
        for (idx, query, missing) in &missed {
            let truncated: String = if query.len() > 45 {
                format!("{}...", &query[..42])
            } else {
                query.to_string()
            };
            println!("║  #{:<3} {:<48} ║", idx, truncated);
            println!(
                "║       missing: {:?}{} ║",
                &missing[..missing.len().min(3)],
                " ".repeat(
                    45 - format!("{:?}", &missing[..missing.len().min(3)])
                        .len()
                        .min(44)
                )
            );
        }
    }

    println!("║                                                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // Lower threshold for 500-memory corpus (harder retrieval task)
    assert!(
        overall_r10 >= 0.40,
        "Recall@10 ({overall_r10:.4}) below minimum threshold 0.40 for {label}"
    );
}

// ---------------------------------------------------------------------------
// 500-memory corpus tests
// ---------------------------------------------------------------------------

#[test]
fn test_longmemeval_500_keyword_only() {
    let conn = Connection::open_in_memory().unwrap();
    setup_large_corpus(&conn);

    let corpus = build_large_corpus();
    let corpus_size = corpus.len();
    let summaries: HashMap<String, String> = corpus
        .iter()
        .map(|m| (m.id.to_string(), m.summary.to_string()))
        .collect();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let lance = rt
        .block_on(LanceStorage::open(dir.path().join("vectors")))
        .unwrap();

    let cases = build_large_eval_cases();
    run_evaluation_large(
        &conn,
        &lance,
        None,
        &cases,
        &summaries,
        "500-mem Keyword + Temporal (no model)",
        corpus_size,
    );
}

#[test]
#[ignore] // Requires embedding model download
fn test_longmemeval_500_full_pipeline() {
    let conn = Connection::open_in_memory().unwrap();
    setup_large_corpus(&conn);

    let corpus = build_large_corpus();
    let corpus_size = corpus.len();
    let summaries: HashMap<String, String> = corpus
        .iter()
        .map(|m| (m.id.to_string(), m.summary.to_string()))
        .collect();

    let manager = clearmemory::storage::embeddings::EmbeddingManager::new("bge-small-en").unwrap();
    let dim = manager.dimensions();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let lance = rt
        .block_on(LanceStorage::open_with_dim(
            dir.path().join("vectors"),
            dim as i32,
        ))
        .unwrap();

    println!(
        "Indexing {} memories with BGE-Small-EN ({dim}-dim)...",
        corpus_size
    );
    for mem in &corpus {
        let embedding = manager.embed_query(mem.summary).unwrap();
        rt.block_on(lance.insert(mem.id, &embedding, None)).unwrap();
    }
    println!("Indexing complete.");

    let cases = build_large_eval_cases();

    run_evaluation_large(
        &conn,
        &lance,
        Some(&manager),
        &cases,
        &summaries,
        "500-mem Full Pipeline (BGE-Small-EN)",
        corpus_size,
    );
}
