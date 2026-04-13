//! Publication-Grade Retrieval Benchmark Suite
//!
//! This is the rigorous quality benchmark for Clear Memory. It creates a corpus
//! of 500 realistic AI conversation memories spanning 10 categories and runs
//! 100 test queries across 8 difficulty categories.
//!
//! Metrics computed:
//! - MRR (Mean Reciprocal Rank)
//! - Recall@1, @3, @5, @10
//! - Precision@5, @10
//! - NDCG@10 (Normalized Discounted Cumulative Gain)
//! - Per-category breakdown of all metrics
//! - Per-strategy contribution analysis
//! - Failure analysis with detailed miss reports
//!
//! Two test entrypoints:
//! - `test_publication_benchmark_keyword_only` — fast CI test, no model download
//! - `test_publication_benchmark_full_pipeline` — full semantic pipeline, requires BGE model
//!
//! Run keyword-only:
//!   cargo test --test benchmark_suite test_publication_benchmark_keyword_only -- --nocapture
//!
//! Run full pipeline (requires model):
//!   cargo test --test benchmark_suite test_publication_benchmark_full_pipeline -- --nocapture --ignored

use clearmemory::entities::resolver::HeuristicResolver;
use clearmemory::migration;
use clearmemory::retrieval::merge::Strategy;
use clearmemory::retrieval::rerank::PassthroughReranker;
use clearmemory::retrieval::{self, RecallConfig};
use clearmemory::storage::lance::LanceStorage;
use rusqlite::Connection;
use std::collections::HashMap;

// ============================================================================
// DATA STRUCTURES
// ============================================================================

struct TestMemory {
    id: &'static str,
    summary: &'static str,
    created_at: &'static str,
    stream_id: Option<&'static str>,
    category: &'static str,
}

#[derive(Clone)]
struct TestCase {
    query: &'static str,
    expected_memory_ids: Vec<&'static str>,
    category: &'static str,
    /// Difficulty level for documentation and filtering: "easy", "medium", "hard", "extreme".
    #[allow(dead_code)]
    difficulty: &'static str,
}

struct BenchmarkResults {
    mrr: f64,
    recall_at_1: f64,
    recall_at_3: f64,
    recall_at_5: f64,
    recall_at_10: f64,
    precision_at_5: f64,
    precision_at_10: f64,
    ndcg_at_10: f64,
}

// ============================================================================
// CORPUS: 500 MEMORIES
// ============================================================================

fn build_corpus() -> Vec<TestMemory> {
    vec![
        // ====================================================================
        // CATEGORY 1: Architecture Decisions (50)
        // ====================================================================
        TestMemory { id: "mem-001", summary: "We decided to switch from REST to GraphQL for the public API because it reduces over-fetching and lets mobile clients request exactly the fields they need", created_at: "2025-07-15", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-002", summary: "Team agreed to use PostgreSQL instead of MongoDB for the user service because we need strong ACID guarantees for financial data and complex joins across tables", created_at: "2025-07-22", stream_id: Some("backend"), category: "architecture" },
        TestMemory { id: "mem-003", summary: "Architecture decision: adopt event sourcing for the order service to enable full audit trails and temporal queries on order state changes", created_at: "2025-07-29", stream_id: Some("backend"), category: "architecture" },
        TestMemory { id: "mem-004", summary: "Decided to use Clerk for authentication instead of Auth0 based on better pricing, developer experience with React integration, and built-in user management UI", created_at: "2025-08-05", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-005", summary: "We chose Redis for caching over Memcached because we need support for sorted sets, pub/sub for real-time features, and persistence for session data", created_at: "2025-08-12", stream_id: Some("backend"), category: "architecture" },
        TestMemory { id: "mem-006", summary: "Selected Kubernetes over Docker Swarm for container orchestration due to better auto-scaling, service mesh support with Istio, and wider industry adoption", created_at: "2025-08-19", stream_id: Some("infrastructure"), category: "architecture" },
        TestMemory { id: "mem-007", summary: "Decision to use gRPC for internal service-to-service communication while keeping GraphQL for external API clients. Protobuf schema registry will be in a shared repo", created_at: "2025-08-26", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-008", summary: "Evaluated Kafka vs RabbitMQ vs NATS for the event bus. Chose Kafka for its durability guarantees, partition-based ordering, and ability to replay event streams", created_at: "2025-09-02", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-009", summary: "Architecture review: moving from monolithic React app to micro-frontends using Module Federation so teams can deploy independently without coordinating releases", created_at: "2025-09-09", stream_id: Some("frontend"), category: "architecture" },
        TestMemory { id: "mem-010", summary: "Decided to implement CQRS pattern for the inventory service. Write side uses PostgreSQL, read side uses Elasticsearch for complex product search queries", created_at: "2025-09-16", stream_id: Some("backend"), category: "architecture" },
        TestMemory { id: "mem-011", summary: "Chose S3 for blob storage over Azure Blob Storage and GCS due to existing AWS infrastructure, lifecycle policies for cost optimization, and CDN integration with CloudFront", created_at: "2025-09-23", stream_id: Some("infrastructure"), category: "architecture" },
        TestMemory { id: "mem-012", summary: "We are adopting a cell-based architecture for the payment service to achieve fault isolation. Each cell handles a geographic region independently", created_at: "2025-09-30", stream_id: Some("backend"), category: "architecture" },
        TestMemory { id: "mem-013", summary: "Decision to use Terraform over Pulumi for infrastructure as code. Team already knows HCL, and the Terraform provider ecosystem for AWS is more mature", created_at: "2025-10-07", stream_id: Some("infrastructure"), category: "architecture" },
        TestMemory { id: "mem-014", summary: "Evaluated three API gateway options: Kong, AWS API Gateway, and Envoy. Chose Envoy because it integrates best with our Kubernetes service mesh and supports gRPC natively", created_at: "2025-10-14", stream_id: Some("infrastructure"), category: "architecture" },
        TestMemory { id: "mem-015", summary: "Architecture decision: all new services must use structured logging with OpenTelemetry traces. No more printf-style logging. Correlation IDs are mandatory across service boundaries", created_at: "2025-10-21", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-016", summary: "Decided against using GraphQL subscriptions for real-time updates. Going with Server-Sent Events instead because they are simpler, work through CDNs, and our use cases are server-to-client only", created_at: "2025-10-28", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-017", summary: "Moving our CI/CD from Jenkins to GitHub Actions. Jenkins maintenance overhead is too high and GitHub Actions native integration with our repos reduces context switching", created_at: "2025-11-04", stream_id: Some("infrastructure"), category: "architecture" },
        TestMemory { id: "mem-018", summary: "ADR: choosing SQLite over DynamoDB for the edge config service. SQLite can be embedded in the edge runtime, reducing latency from 50ms to under 1ms for config lookups", created_at: "2025-11-11", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-019", summary: "Decided to use a sidecar pattern for observability instead of instrumenting each service directly. Reduces coupling between application code and monitoring infrastructure", created_at: "2025-11-18", stream_id: Some("infrastructure"), category: "architecture" },
        TestMemory { id: "mem-020", summary: "Architecture decision: implementing circuit breakers using Istio service mesh rather than application-level libraries like Hystrix. Centralizes resilience policy management", created_at: "2025-11-25", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-021", summary: "Chose TimescaleDB extension for PostgreSQL over InfluxDB for time-series metrics storage. Keeps us on a single database engine and lets us join metrics with relational data", created_at: "2025-12-02", stream_id: Some("data"), category: "architecture" },
        TestMemory { id: "mem-022", summary: "Decision to use feature flags via LaunchDarkly instead of building our own system. Cost is justified by targeting rules, audit trail, and progressive rollout capabilities", created_at: "2025-12-09", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-023", summary: "Moving from REST webhooks to CloudEvents specification for inter-service event notifications. Standardized envelope format simplifies parsing across language boundaries", created_at: "2025-12-16", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-024", summary: "ADR: the notification service will use a fan-out pattern with SNS topics feeding SQS queues per consumer. Decouples producers from consumers and handles backpressure per consumer", created_at: "2025-12-23", stream_id: Some("backend"), category: "architecture" },
        TestMemory { id: "mem-025", summary: "Decided to use Vitess for MySQL sharding on the legacy user database rather than migrating everything to PostgreSQL at once. Reduces migration risk for the 200M row table", created_at: "2025-12-30", stream_id: Some("data"), category: "architecture" },
        TestMemory { id: "mem-026", summary: "Architecture review: adopting the BFF pattern (Backend for Frontend) so the mobile app and web app each get their own optimized API layer instead of sharing one generic API", created_at: "2026-01-06", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-027", summary: "We picked Argo Workflows for our data pipeline orchestration over Airflow because it runs natively on Kubernetes and handles container-based tasks more naturally", created_at: "2026-01-13", stream_id: Some("data"), category: "architecture" },
        TestMemory { id: "mem-028", summary: "Decision: implementing API versioning through URL path (/v1/, /v2/) rather than header-based versioning. More explicit, easier to route, and works better with API documentation tools", created_at: "2026-01-20", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-029", summary: "Chose to implement the strangler fig pattern for gradually replacing the legacy billing system. New features go through the new service while old endpoints proxy to legacy", created_at: "2026-01-27", stream_id: Some("backend"), category: "architecture" },
        TestMemory { id: "mem-030", summary: "ADR: all database schemas must support soft deletes with deleted_at timestamp columns. Hard deletes only allowed through explicit compliance purge operations", created_at: "2026-02-03", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-031", summary: "Moving to a monorepo structure using Nx for the frontend applications. Shared component library and consistent tooling across teams outweigh the build complexity cost", created_at: "2026-02-10", stream_id: Some("frontend"), category: "architecture" },
        TestMemory { id: "mem-032", summary: "Architecture decision: implementing saga pattern for distributed transactions across payment, inventory, and shipping services instead of two-phase commit", created_at: "2026-02-17", stream_id: Some("backend"), category: "architecture" },
        TestMemory { id: "mem-033", summary: "Chose gRPC-Web proxy over direct WebSocket connections for browser-to-backend communication in the admin dashboard. Simpler security model and works through corporate proxies", created_at: "2026-02-24", stream_id: Some("frontend"), category: "architecture" },
        TestMemory { id: "mem-034", summary: "Decided to use AWS Step Functions for long-running business workflows like order fulfillment instead of custom state machines. Reduces code and provides built-in retry/error handling", created_at: "2026-03-03", stream_id: Some("backend"), category: "architecture" },
        TestMemory { id: "mem-035", summary: "ADR: implementing database connection pooling at the infrastructure level with PgBouncer rather than per-service connection pools. Reduces total connections and simplifies configuration", created_at: "2026-03-10", stream_id: Some("infrastructure"), category: "architecture" },
        TestMemory { id: "mem-036", summary: "We will use OpenAPI 3.1 spec as the single source of truth for API contracts. Code generation for client SDKs and server stubs. Breaking changes require version bump", created_at: "2026-03-17", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-037", summary: "Decision to adopt Dapr for service invocation abstractions so we can swap underlying infrastructure without changing application code. Currently targeting Kubernetes but may move to serverless", created_at: "2026-03-24", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-038", summary: "Architecture review: implementing request coalescing in the API gateway for popular product catalog queries. Multiple identical requests within 50ms window are collapsed into one backend call", created_at: "2026-03-31", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-039", summary: "Chose to use Flyway for database migration management instead of custom scripts. Checksum validation prevents accidental modifications to applied migrations", created_at: "2025-08-01", stream_id: Some("backend"), category: "architecture" },
        TestMemory { id: "mem-040", summary: "ADR: the search service will use hybrid search combining BM25 keyword matching with vector similarity using reciprocal rank fusion. Neither strategy alone covers all query types", created_at: "2025-08-15", stream_id: Some("backend"), category: "architecture" },
        TestMemory { id: "mem-041", summary: "Decided to implement blue-green deployments for the core API services rather than rolling updates. Zero-downtime cutover with instant rollback capability justifies the extra infrastructure cost", created_at: "2025-09-05", stream_id: Some("infrastructure"), category: "architecture" },
        TestMemory { id: "mem-042", summary: "Architecture decision: using Apache Iceberg table format for our data lake instead of raw Parquet. Schema evolution and time travel queries are critical for compliance reporting", created_at: "2025-09-20", stream_id: Some("data"), category: "architecture" },
        TestMemory { id: "mem-043", summary: "We picked Backstage for our internal developer portal. Centralized service catalog, documentation, and scaffolding templates reduce onboarding time from 2 weeks to 3 days", created_at: "2025-10-10", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-044", summary: "ADR: implementing content-based routing in the API gateway for A/B testing. Traffic split at the gateway level is more reliable than client-side randomization", created_at: "2025-10-25", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-045", summary: "Decision to use Sealed Secrets for managing Kubernetes secrets rather than Vault for non-sensitive configs. Reduces operational complexity for the majority of our secret management needs", created_at: "2025-11-08", stream_id: Some("infrastructure"), category: "architecture" },
        TestMemory { id: "mem-046", summary: "Moving to trunk-based development with short-lived feature branches. Feature flags replace long-lived branches. PR merge queue enforces CI pass before merge", created_at: "2025-11-22", stream_id: Some("platform"), category: "architecture" },
        TestMemory { id: "mem-047", summary: "Architecture review: the recommendation engine will use a two-tower neural network architecture with item embeddings pre-computed nightly and user embeddings computed in real-time", created_at: "2025-12-05", stream_id: Some("data"), category: "architecture" },
        TestMemory { id: "mem-048", summary: "Chose to separate the read and write databases for the product catalog using read replicas with 200ms lag tolerance rather than full CQRS, since our write volume is low", created_at: "2025-12-20", stream_id: Some("backend"), category: "architecture" },
        TestMemory { id: "mem-049", summary: "ADR: implementing tenant isolation through schema-per-tenant in PostgreSQL rather than row-level security. Better data isolation guarantees for enterprise customers and simpler backup/restore", created_at: "2026-01-15", stream_id: Some("backend"), category: "architecture" },
        TestMemory { id: "mem-050", summary: "Decision to use WebAssembly plugins for the API gateway custom logic instead of Lua scripts. Type safety, better tooling, and ability to reuse Rust business logic", created_at: "2026-02-01", stream_id: Some("platform"), category: "architecture" },

        // ====================================================================
        // CATEGORY 2: Bug Reports & Fixes (50)
        // ====================================================================
        TestMemory { id: "mem-051", summary: "Fixed the authentication middleware timeout by increasing the JWT token verification timeout from 100ms to 500ms and adding retry logic with exponential backoff", created_at: "2025-07-18", stream_id: Some("platform"), category: "bugs" },
        TestMemory { id: "mem-052", summary: "Resolved the database connection pool exhaustion issue by setting max connections to 20, idle timeout to 30s, and adding connection health checks on checkout", created_at: "2025-07-25", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-053", summary: "Fixed memory leak in the WebSocket handler caused by event listeners not being cleaned up on client disconnect. Added proper teardown in the close handler", created_at: "2025-08-01", stream_id: Some("platform"), category: "bugs" },
        TestMemory { id: "mem-054", summary: "Resolved race condition in the payment processing pipeline where concurrent updates to the same order could result in double charging. Added optimistic locking with version column", created_at: "2025-08-08", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-055", summary: "Fixed CSS rendering issue on Safari where flexbox gap property was not supported in older versions. Added fallback margin-based spacing for Safari 13 and below", created_at: "2025-08-15", stream_id: Some("frontend"), category: "bugs" },
        TestMemory { id: "mem-056", summary: "Corrected the timezone handling in the scheduling service that was causing appointments to show at wrong times for users in different timezones. Storing all times as UTC with explicit timezone offsets", created_at: "2025-08-22", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-057", summary: "Fixed N+1 query problem in the orders list endpoint. Added eager loading with SQL joins to load order items and shipping addresses in a single query instead of hundreds", created_at: "2025-08-29", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-058", summary: "Resolved intermittent 502 errors from the load balancer. Root cause was health check endpoint timing out under load because it was querying the database. Changed to a simple in-memory check", created_at: "2025-09-05", stream_id: Some("infrastructure"), category: "bugs" },
        TestMemory { id: "mem-059", summary: "Fixed data corruption in the user preferences table caused by concurrent writes without proper serialization. Added advisory locks around the upsert operation", created_at: "2025-09-12", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-060", summary: "Resolved the infinite redirect loop on the login page that occurred when the session cookie had the SameSite=Strict attribute set while the auth provider used a different domain", created_at: "2025-09-19", stream_id: Some("frontend"), category: "bugs" },
        TestMemory { id: "mem-061", summary: "Fixed deadlock in the inventory reservation system. Two transactions were locking rows in opposite order. Enforced consistent lock ordering by always acquiring locks in product_id ascending order", created_at: "2025-09-26", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-062", summary: "Patched XSS vulnerability in the user profile page where bio field was rendered without sanitization. Added server-side HTML escaping and Content-Security-Policy headers", created_at: "2025-10-03", stream_id: Some("security"), category: "bugs" },
        TestMemory { id: "mem-063", summary: "Fixed the search autocomplete dropping characters when typing fast. The debounce implementation was canceling in-flight requests but not the UI state updates. Switched to an abort controller pattern", created_at: "2025-10-10", stream_id: Some("frontend"), category: "bugs" },
        TestMemory { id: "mem-064", summary: "Resolved OOM crashes in the image processing service. The sharp library was holding multiple 4K images in memory simultaneously. Added a semaphore to limit concurrent resize operations to 4", created_at: "2025-10-17", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-065", summary: "Fixed the email notification service silently dropping messages when the SMTP connection was reset. Added connection pool with health checks and dead letter queue for failed deliveries", created_at: "2025-10-24", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-066", summary: "Resolved cache stampede issue where thousands of requests simultaneously tried to regenerate an expired cache entry. Implemented probabilistic early expiration and distributed locking", created_at: "2025-10-31", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-067", summary: "Fixed broken file uploads larger than 10MB. The nginx proxy was using default 1MB client_max_body_size. Increased to 50MB and added proper streaming to avoid buffering entire file in memory", created_at: "2025-11-07", stream_id: Some("infrastructure"), category: "bugs" },
        TestMemory { id: "mem-068", summary: "Corrected the pagination bug where the last page would sometimes return duplicate items. The ORDER BY clause was using a non-unique column. Added secondary sort on id for deterministic ordering", created_at: "2025-11-14", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-069", summary: "Fixed Kubernetes pod crash loop caused by readiness probe failing during slow model loading. Increased initialDelaySeconds from 10 to 120 and added a startup probe with 300s failure threshold", created_at: "2025-11-21", stream_id: Some("infrastructure"), category: "bugs" },
        TestMemory { id: "mem-070", summary: "Resolved the GraphQL resolver returning stale data after mutations. The DataLoader cache was not being invalidated on writes. Added cache clearing in the mutation resolvers", created_at: "2025-11-28", stream_id: Some("platform"), category: "bugs" },
        TestMemory { id: "mem-071", summary: "Fixed race condition in user registration flow where duplicate accounts could be created for the same email. Added unique constraint on email column and idempotency key on the registration endpoint", created_at: "2025-12-05", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-072", summary: "Patched SQL injection vulnerability in the dynamic report builder. User-supplied column names were concatenated directly into queries. Switched to allowlisted column mapping", created_at: "2025-12-12", stream_id: Some("security"), category: "bugs" },
        TestMemory { id: "mem-073", summary: "Fixed the mobile app crash on iOS 16 when rotating the device during video playback. The layout constraint was being invalidated without a proper fallback. Added explicit constraint priorities", created_at: "2025-12-19", stream_id: Some("frontend"), category: "bugs" },
        TestMemory { id: "mem-074", summary: "Resolved the Elasticsearch index corruption after a failed cluster upgrade. Had to close affected indices, run segment repair, and re-index 2M documents from the source database", created_at: "2025-12-26", stream_id: Some("infrastructure"), category: "bugs" },
        TestMemory { id: "mem-075", summary: "Fixed the webhook delivery system dropping events during deployments. Added persistent queue with at-least-once delivery guarantees and idempotency keys on receiver side", created_at: "2026-01-02", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-076", summary: "Corrected the rate limiter that was incorrectly sharing counters across tenants. Each tenant was seeing limits hit by other tenants. Fixed by including tenant_id in the rate limit key", created_at: "2026-01-09", stream_id: Some("platform"), category: "bugs" },
        TestMemory { id: "mem-077", summary: "Fixed the Docker build cache invalidation issue where changing package.json always triggered a full npm install. Restructured Dockerfile to copy lock file first before source code", created_at: "2026-01-16", stream_id: Some("infrastructure"), category: "bugs" },
        TestMemory { id: "mem-078", summary: "Resolved intermittent test failures in CI caused by port conflicts. Tests were using hardcoded ports. Switched to dynamic port allocation with port 0 and reading the assigned port back", created_at: "2026-01-23", stream_id: Some("infrastructure"), category: "bugs" },
        TestMemory { id: "mem-079", summary: "Fixed the dashboard loading spinner that never resolved when the API returned an empty array. The condition checked for truthiness of the response but empty arrays are falsy in some contexts", created_at: "2026-01-30", stream_id: Some("frontend"), category: "bugs" },
        TestMemory { id: "mem-080", summary: "Patched critical privilege escalation bug where regular users could access admin endpoints by modifying the role claim in their JWT. Added server-side role verification against the database on every admin request", created_at: "2026-02-06", stream_id: Some("security"), category: "bugs" },
        TestMemory { id: "mem-081", summary: "Fixed the data export feature truncating CSV files at 65,536 rows due to using a 16-bit row counter. Changed to streaming CSV generation with no row limit", created_at: "2026-02-13", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-082", summary: "Resolved DNS resolution timeout causing cold start latency spikes in Lambda functions. Added DNS caching with 60s TTL and pre-resolved the RDS endpoint during initialization", created_at: "2026-02-20", stream_id: Some("infrastructure"), category: "bugs" },
        TestMemory { id: "mem-083", summary: "Fixed the search index drift where deleted products were still appearing in results. The delete event was not propagating to the search index consumer. Added a reconciliation job running every 6 hours", created_at: "2026-02-27", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-084", summary: "Corrected the billing calculation that was rounding intermediate values causing penny discrepancies on invoices. Switched all money calculations to integer cents and only formatted at display time", created_at: "2026-03-06", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-085", summary: "Fixed the API returning 500 instead of 413 when request body exceeded size limit. The error handler was not catching the Axum body limit rejection properly", created_at: "2026-03-13", stream_id: Some("platform"), category: "bugs" },
        TestMemory { id: "mem-086", summary: "Resolved the connection leak in the Redis client library. Connections were not being returned to the pool when pipeline commands failed. Wrapped pipeline execution in a try-finally block", created_at: "2026-03-20", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-087", summary: "Fixed the browser back button not working correctly in the SPA. History state was being replaced instead of pushed. Changed router configuration to use pushState consistently", created_at: "2026-03-27", stream_id: Some("frontend"), category: "bugs" },
        TestMemory { id: "mem-088", summary: "Patched SSRF vulnerability in the URL preview feature. User-supplied URLs could target internal services. Added URL validation against an allowlist of external domains and blocked private IP ranges", created_at: "2026-04-01", stream_id: Some("security"), category: "bugs" },
        TestMemory { id: "mem-089", summary: "Fixed the Kafka consumer lag alert that was firing false positives during rebalancing. Added a 5-minute grace period after consumer group rebalance events before evaluating lag thresholds", created_at: "2025-07-20", stream_id: Some("infrastructure"), category: "bugs" },
        TestMemory { id: "mem-090", summary: "Resolved the OpenTelemetry trace context not propagating across async boundaries in the Node.js services. Had to switch from manual context passing to the async hooks-based propagation", created_at: "2025-08-10", stream_id: Some("platform"), category: "bugs" },
        TestMemory { id: "mem-091", summary: "Fixed corrupted gzip responses when the CDN tried to compress already-compressed content. Added Vary: Accept-Encoding header and configured origin to skip compression for already-compressed types", created_at: "2025-09-01", stream_id: Some("infrastructure"), category: "bugs" },
        TestMemory { id: "mem-092", summary: "Corrected the migration script that was dropping the wrong index in production. The index name was different between dev and prod environments. Added explicit IF EXISTS checks and environment validation", created_at: "2025-09-15", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-093", summary: "Fixed the OAuth refresh token flow that silently failed when the original access token was expired. The refresh endpoint was validating the access token instead of just the refresh token", created_at: "2025-10-01", stream_id: Some("platform"), category: "bugs" },
        TestMemory { id: "mem-094", summary: "Resolved the flaky integration test that depended on insertion order in a HashMap. Tests passed on Linux but failed on macOS due to different hash seed defaults. Used BTreeMap for deterministic ordering", created_at: "2025-10-15", stream_id: Some("platform"), category: "bugs" },
        TestMemory { id: "mem-095", summary: "Fixed the audit log timestamps that were using local time instead of UTC causing confusion for distributed teams. Standardized all log timestamps to UTC with explicit timezone suffix", created_at: "2025-11-01", stream_id: Some("platform"), category: "bugs" },
        TestMemory { id: "mem-096", summary: "Patched the file download endpoint that was vulnerable to path traversal attacks. User-supplied filenames with ../ could access files outside the upload directory. Added strict filename sanitization", created_at: "2025-11-15", stream_id: Some("security"), category: "bugs" },
        TestMemory { id: "mem-097", summary: "Fixed the WebSocket reconnection logic that was creating duplicate connections on mobile when switching between WiFi and cellular. Added connection deduplication with a UUID per session", created_at: "2025-12-01", stream_id: Some("frontend"), category: "bugs" },
        TestMemory { id: "mem-098", summary: "Resolved the slow database vacuum operation that was locking the table for 20 minutes. Switched to concurrent vacuum which runs in the background without blocking reads or writes", created_at: "2025-12-15", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-099", summary: "Fixed the JSON serialization bug where NaN values from floating point calculations were causing the API to return invalid JSON. Added NaN check and replacement with null before serialization", created_at: "2026-01-05", stream_id: Some("backend"), category: "bugs" },
        TestMemory { id: "mem-100", summary: "Corrected the Helm chart values that were overriding environment-specific resource limits. Moved resource definitions to per-environment values files with proper merge strategy", created_at: "2026-01-20", stream_id: Some("infrastructure"), category: "bugs" },

        // ====================================================================
        // CATEGORY 3: Project Planning (50)
        // ====================================================================
        TestMemory { id: "mem-101", summary: "Q1 migration milestones: Phase 1 (Jan) schema migration scripts ready, Phase 2 (Feb) dual-write mode enabled with shadow traffic, Phase 3 (Mar) cutover to new database with rollback plan", created_at: "2025-07-10", stream_id: Some("backend"), category: "planning" },
        TestMemory { id: "mem-102", summary: "SOC2 audit preparation: need to document all data flows, access controls, encryption at rest, incident response procedures, and vendor risk assessments by end of Q3", created_at: "2025-07-17", stream_id: Some("security"), category: "planning" },
        TestMemory { id: "mem-103", summary: "Frontend rewrite roadmap: migrate from Create React App to Next.js with App Router for better SEO and server-side rendering. Estimated 3-month effort with 2 developers", created_at: "2025-07-24", stream_id: Some("frontend"), category: "planning" },
        TestMemory { id: "mem-104", summary: "Mobile app v2 launch plan: release beta by September 15, full launch October 1, targeting 95% crash-free rate and 4.5 star rating on both app stores", created_at: "2025-07-31", stream_id: Some("frontend"), category: "planning" },
        TestMemory { id: "mem-105", summary: "API versioning strategy: we will maintain v1 and v2 simultaneously for 12 months with deprecation notices, then sunset v1 with 6 months written notice to all consumers", created_at: "2025-08-07", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-106", summary: "Data platform modernization: replace legacy ETL jobs with streaming Flink pipelines over 6 months. Phase 1 targets the customer analytics pipeline, highest business impact", created_at: "2025-08-14", stream_id: Some("data"), category: "planning" },
        TestMemory { id: "mem-107", summary: "Kubernetes migration plan: lift and shift existing Docker Compose services to EKS. Start with stateless services, then tackle stateful workloads with persistent volumes", created_at: "2025-08-21", stream_id: Some("infrastructure"), category: "planning" },
        TestMemory { id: "mem-108", summary: "Performance optimization sprint planned for September: focus on p95 latency reduction for the top 10 API endpoints. Target: all endpoints under 200ms p95", created_at: "2025-08-28", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-109", summary: "Security hardening roadmap Q4: implement mTLS between all services, deploy WAF rules, enable database encryption at rest, and conduct red team exercise", created_at: "2025-09-04", stream_id: Some("security"), category: "planning" },
        TestMemory { id: "mem-110", summary: "Hiring plan discussion: need 2 senior backend engineers, 1 DevOps engineer, and 1 security engineer by Q1 2026 to support the platform scaling initiative", created_at: "2025-09-11", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-111", summary: "Sprint retrospective: completed 8/10 stories, carried over mobile push notifications and admin dashboard search. Main blocker was waiting on API approval from the payments team", created_at: "2025-09-18", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-112", summary: "Q4 roadmap priorities: 1) payment gateway integration with Stripe, 2) multi-tenant workspace support, 3) real-time collaboration features, 4) SOC2 Type II certification", created_at: "2025-09-25", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-113", summary: "Technical debt reduction plan: allocate 20% of each sprint to refactoring. Priority areas are the order processing module, user authentication layer, and test infrastructure", created_at: "2025-10-02", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-114", summary: "Internationalization project kickoff: support 5 languages by Q2 2026. Using react-intl for frontend, database-driven translations for API responses, RTL support for Arabic", created_at: "2025-10-09", stream_id: Some("frontend"), category: "planning" },
        TestMemory { id: "mem-115", summary: "Load testing roadmap: establish baseline metrics this month, then run weekly performance tests against staging with production-like traffic patterns. Automated alerts for regressions", created_at: "2025-10-16", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-116", summary: "Disaster recovery plan: RTO target 4 hours, RPO target 1 hour. Multi-region failover for critical services, daily cross-region database replication, quarterly DR drills", created_at: "2025-10-23", stream_id: Some("infrastructure"), category: "planning" },
        TestMemory { id: "mem-117", summary: "Feature flag cleanup sprint: remove 47 stale feature flags that have been fully rolled out for more than 3 months. Reducing code complexity and test matrix size", created_at: "2025-10-30", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-118", summary: "Machine learning pipeline plan: build recommendation engine using collaborative filtering, deploy with MLflow for model versioning, A/B test against current manual curation", created_at: "2025-11-06", stream_id: Some("data"), category: "planning" },
        TestMemory { id: "mem-119", summary: "API deprecation timeline for legacy REST endpoints: announcement Dec 1, migration guide published Dec 15, soft shutdown (warnings) Feb 1, hard shutdown Apr 1", created_at: "2025-11-13", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-120", summary: "Infrastructure cost optimization initiative: target 30% reduction in AWS spend. Focus areas: right-sizing instances, reserved capacity for predictable workloads, spot instances for batch jobs", created_at: "2025-11-20", stream_id: Some("infrastructure"), category: "planning" },
        TestMemory { id: "mem-121", summary: "Design system v2 planning: comprehensive token system, accessibility audit of all components, Storybook documentation for every component, visual regression testing pipeline", created_at: "2025-11-27", stream_id: Some("frontend"), category: "planning" },
        TestMemory { id: "mem-122", summary: "Compliance automation project: automate evidence collection for SOC2, ISO 27001, and GDPR audits. Pull access logs, configuration snapshots, and vulnerability scan results into a compliance dashboard", created_at: "2025-12-04", stream_id: Some("security"), category: "planning" },
        TestMemory { id: "mem-123", summary: "Backend service consolidation plan: merge 5 microservices that are always deployed together back into a single service. The operational overhead of separate deployments outweighs the modularity benefits", created_at: "2025-12-11", stream_id: Some("backend"), category: "planning" },
        TestMemory { id: "mem-124", summary: "Observability platform migration: moving from Datadog to Grafana Cloud for cost reduction. Phase 1: metrics (Prometheus), Phase 2: logs (Loki), Phase 3: traces (Tempo)", created_at: "2025-12-18", stream_id: Some("infrastructure"), category: "planning" },
        TestMemory { id: "mem-125", summary: "Customer onboarding flow redesign: reduce steps from 7 to 3, add social login options, implement progressive profiling to collect details over time rather than upfront", created_at: "2025-12-25", stream_id: Some("frontend"), category: "planning" },
        TestMemory { id: "mem-126", summary: "Sprint planning for the payments team: 2 weeks on PCI DSS compliance hardening, 1 week on refund automation, 1 week on multi-currency support for EU expansion", created_at: "2026-01-01", stream_id: Some("backend"), category: "planning" },
        TestMemory { id: "mem-127", summary: "Analytics data warehouse migration: move from Redshift to BigQuery. BigQuery serverless model eliminates cluster management and scales more cost-effectively for our bursty query patterns", created_at: "2026-01-08", stream_id: Some("data"), category: "planning" },
        TestMemory { id: "mem-128", summary: "End-to-end testing strategy overhaul: adopt Playwright, test critical user journeys in CI, run full regression nightly. Target 80% E2E coverage of revenue-critical paths", created_at: "2026-01-15", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-129", summary: "Multi-region deployment plan: primary in us-east-1, secondary in eu-west-1. Active-active for stateless services, active-passive for databases with asynchronous replication", created_at: "2026-01-22", stream_id: Some("infrastructure"), category: "planning" },
        TestMemory { id: "mem-130", summary: "Q2 OKR planning: Objective 1 - 99.95% uptime SLA for enterprise tier, Objective 2 - reduce mean time to resolution by 50%, Objective 3 - ship self-service workspace management", created_at: "2026-01-29", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-131", summary: "Migration from Heroku to AWS planned for March. The Heroku pricing changes make it 3x more expensive than equivalent AWS infrastructure. 6-week migration timeline estimated", created_at: "2026-02-05", stream_id: Some("infrastructure"), category: "planning" },
        TestMemory { id: "mem-132", summary: "Accessibility remediation roadmap: WCAG 2.1 AA compliance for all public-facing pages by June. Starting with the most-visited pages: login, dashboard, settings, billing", created_at: "2026-02-12", stream_id: Some("frontend"), category: "planning" },
        TestMemory { id: "mem-133", summary: "OpenTelemetry adoption plan: instrument all services with OTLP exporters, deploy Jaeger for trace visualization, create runbooks for common trace-based debugging workflows", created_at: "2026-02-19", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-134", summary: "Dependency upgrade sprint scheduled for March: update all major dependencies, resolve 12 Dependabot alerts, remove deprecated package-lock.json entries, audit transitive dependencies", created_at: "2026-02-26", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-135", summary: "Edge computing pilot: deploy lightweight API endpoints to Cloudflare Workers for latency-sensitive operations like geo-routing and auth token validation. 3-week POC", created_at: "2026-03-05", stream_id: Some("infrastructure"), category: "planning" },
        TestMemory { id: "mem-136", summary: "Customer data export feature planned for April release. Users can request full data export in JSON format within 72 hours. Required for GDPR data portability compliance", created_at: "2026-03-12", stream_id: Some("backend"), category: "planning" },
        TestMemory { id: "mem-137", summary: "Real-time collaboration roadmap: Phase 1 - presence indicators and cursor tracking, Phase 2 - operational transform for concurrent editing, Phase 3 - offline conflict resolution", created_at: "2026-03-19", stream_id: Some("frontend"), category: "planning" },
        TestMemory { id: "mem-138", summary: "Platform reliability initiative: implement chaos engineering practices using Gremlin, target one chaos experiment per sprint, build confidence in failover mechanisms", created_at: "2026-03-26", stream_id: Some("infrastructure"), category: "planning" },
        TestMemory { id: "mem-139", summary: "GraphQL federation rollout plan: extract user service subgraph first as a pilot, then product catalog, then order management. Full federation expected by end of H1", created_at: "2025-08-05", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-140", summary: "Database sharding strategy for the events table: shard by tenant_id using consistent hashing. Targeting the table that has grown to 500M rows and is causing query slowdowns", created_at: "2025-09-10", stream_id: Some("data"), category: "planning" },
        TestMemory { id: "mem-141", summary: "Incident management process improvement: adopt PagerDuty for alerting, implement severity classification (SEV1-SEV4), require blameless postmortems for SEV1 and SEV2 incidents", created_at: "2025-10-05", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-142", summary: "Frontend performance budget: First Contentful Paint under 1.5s, Largest Contentful Paint under 2.5s, Total Blocking Time under 200ms. Automated lighthouse CI checks on every PR", created_at: "2025-11-10", stream_id: Some("frontend"), category: "planning" },
        TestMemory { id: "mem-143", summary: "API rate limiting redesign: move from fixed window to sliding window algorithm, implement per-endpoint limits, add rate limit headers to all responses, create self-service limit increase request flow", created_at: "2025-12-08", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-144", summary: "Zero-trust networking migration: replace VPN-based access with BeyondCorp-style identity-aware proxy. Phased rollout starting with internal tools, then production access", created_at: "2026-01-12", stream_id: Some("security"), category: "planning" },
        TestMemory { id: "mem-145", summary: "Contract testing implementation plan: adopt Pact for consumer-driven contract tests between API gateway and all downstream services. Run as part of the PR check pipeline", created_at: "2026-02-08", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-146", summary: "Monolith decomposition Phase 2: extract the reporting module into its own service with dedicated read replica. The reporting queries are causing lock contention on the main database", created_at: "2026-03-01", stream_id: Some("backend"), category: "planning" },
        TestMemory { id: "mem-147", summary: "Developer experience improvement plan: reduce local setup time to under 10 minutes with Docker Compose, add hot reload for all services, create comprehensive API documentation with examples", created_at: "2026-03-15", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-148", summary: "Data retention policy implementation: personal data deleted after 2 years of inactivity, system logs retained for 90 days, financial records retained for 7 years per regulations", created_at: "2026-03-22", stream_id: Some("security"), category: "planning" },
        TestMemory { id: "mem-149", summary: "Mobile SDK release plan: provide native iOS and Android SDKs for third-party developers. Includes auth, push notifications, and real-time messaging. Beta SDK in June, GA in August", created_at: "2026-03-29", stream_id: Some("platform"), category: "planning" },
        TestMemory { id: "mem-150", summary: "On-call rotation restructuring: 1 week on, 3 weeks off cycle. Each service team owns their own rotation. Handoff document template required at every rotation change. Compensation: $500/week on-call", created_at: "2026-04-01", stream_id: Some("platform"), category: "planning" },

        // ====================================================================
        // CATEGORY 4: Code Reviews (50)
        // ====================================================================
        TestMemory { id: "mem-151", summary: "PR review: the middleware needs better error handling. Currently swallowing exceptions and returning generic 500. Each error type should map to a specific HTTP status code with a structured error body", created_at: "2025-07-12", stream_id: Some("platform"), category: "code_review" },
        TestMemory { id: "mem-152", summary: "Code review feedback: the DAO layer should be refactored to use repository pattern. Direct SQL in controller methods makes testing impossible and couples business logic to database schema", created_at: "2025-07-19", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-153", summary: "Review of the caching implementation: TTL values are hardcoded throughout the service. Extract to configuration so we can tune without redeploying. Also add cache hit/miss metrics", created_at: "2025-07-26", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-154", summary: "PR feedback on the authentication module: password hashing is using bcrypt with cost factor 10 which is too low for 2025. Increase to 12 and add Argon2id as an option for new accounts", created_at: "2025-08-02", stream_id: Some("security"), category: "code_review" },
        TestMemory { id: "mem-155", summary: "Code review: the React component tree is too deeply nested. Extract the form validation logic into a custom hook, move the API calls to a service layer, and split the 500-line component into smaller pieces", created_at: "2025-08-09", stream_id: Some("frontend"), category: "code_review" },
        TestMemory { id: "mem-156", summary: "Review comment: the database migration adds a NOT NULL column without a default value. This will fail on tables with existing rows. Add a default value or make it nullable first then backfill", created_at: "2025-08-16", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-157", summary: "PR review of the API rate limiter: the sliding window implementation has a race condition. Two concurrent requests at the boundary could both pass. Use Redis MULTI/EXEC for atomicity", created_at: "2025-08-23", stream_id: Some("platform"), category: "code_review" },
        TestMemory { id: "mem-158", summary: "Code review feedback: Terraform modules are not pinning provider versions. This caused a production incident when a provider auto-updated. Pin all providers to exact versions", created_at: "2025-08-30", stream_id: Some("infrastructure"), category: "code_review" },
        TestMemory { id: "mem-159", summary: "Review: the GraphQL schema exposes internal IDs that could be used for enumeration attacks. Switch to opaque cursor-based IDs using base64-encoded compound keys", created_at: "2025-09-06", stream_id: Some("platform"), category: "code_review" },
        TestMemory { id: "mem-160", summary: "PR feedback: the logging statements include PII (email addresses and phone numbers). All PII must be masked in logs. Use the structured logging sanitizer middleware before writing to output", created_at: "2025-09-13", stream_id: Some("security"), category: "code_review" },
        TestMemory { id: "mem-161", summary: "Code review: the test suite has no assertions on the response body, only status codes. Snapshot testing would catch regressions in response shape. Add JSON schema validation for API response tests", created_at: "2025-09-20", stream_id: Some("platform"), category: "code_review" },
        TestMemory { id: "mem-162", summary: "Review of the Docker Compose setup: services depend on each other but there are no health checks. The API starts before the database is ready. Add depends_on with condition: service_healthy", created_at: "2025-09-27", stream_id: Some("infrastructure"), category: "code_review" },
        TestMemory { id: "mem-163", summary: "PR review: the file upload handler does not validate content type. Users could upload executable files disguised as images. Add MIME type validation and magic byte checking", created_at: "2025-10-04", stream_id: Some("security"), category: "code_review" },
        TestMemory { id: "mem-164", summary: "Code review feedback on the state management: using global Redux store for local component state. This causes unnecessary re-renders across the entire app. Use local state or React Query for server state", created_at: "2025-10-11", stream_id: Some("frontend"), category: "code_review" },
        TestMemory { id: "mem-165", summary: "Review: the retry logic has no jitter and uses fixed intervals. Under load, all retries fire simultaneously causing thundering herd. Add exponential backoff with full jitter", created_at: "2025-10-18", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-166", summary: "PR feedback: environment variables are read at import time, not at function invocation time. This means config changes require a full restart. Wrap in a config loader that re-reads on access", created_at: "2025-10-25", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-167", summary: "Code review: the SQL query in the search endpoint is vulnerable to SQL injection through the sort column parameter. Use a whitelist of allowed sort columns instead of string interpolation", created_at: "2025-11-01", stream_id: Some("security"), category: "code_review" },
        TestMemory { id: "mem-168", summary: "Review of the monitoring dashboard: too many panels make it unusable. Apply the USE method (Utilization, Saturation, Errors) to organize metrics. Keep only actionable dashboards", created_at: "2025-11-08", stream_id: Some("infrastructure"), category: "code_review" },
        TestMemory { id: "mem-169", summary: "PR review: the pagination implementation uses OFFSET which degrades linearly with page depth. Switch to keyset pagination using the last seen id for O(1) performance on all pages", created_at: "2025-11-15", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-170", summary: "Code review feedback: the frontend is making 15 API calls on page load. Implement a BFF endpoint that aggregates the data server-side into a single response. Reduces waterfall latency significantly", created_at: "2025-11-22", stream_id: Some("frontend"), category: "code_review" },
        TestMemory { id: "mem-171", summary: "Review: the CORS configuration allows all origins in production. This must be restricted to our known domains. Use an environment-specific allowlist for Access-Control-Allow-Origin", created_at: "2025-11-29", stream_id: Some("security"), category: "code_review" },
        TestMemory { id: "mem-172", summary: "PR feedback: the event handler creates a new database connection per event instead of using the pool. Under 1000 events/sec this will exhaust available connections immediately", created_at: "2025-12-06", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-173", summary: "Code review: the error boundary in React is only at the app root level. If one widget crashes it takes down the entire page. Add error boundaries around each independent section of the dashboard", created_at: "2025-12-13", stream_id: Some("frontend"), category: "code_review" },
        TestMemory { id: "mem-174", summary: "Review of the CI pipeline: test and lint run sequentially taking 25 minutes. These are independent steps and should run in parallel. Also add a job dependency graph for better visualization", created_at: "2025-12-20", stream_id: Some("infrastructure"), category: "code_review" },
        TestMemory { id: "mem-175", summary: "PR review: the API endpoint returns the full user object including password hash and internal fields. Create a DTO that explicitly maps only the fields that should be exposed to clients", created_at: "2025-12-27", stream_id: Some("security"), category: "code_review" },
        TestMemory { id: "mem-176", summary: "Code review feedback: the batch processing job loads all records into memory at once. For 10M records this will OOM. Switch to cursor-based streaming that processes records in configurable chunks", created_at: "2026-01-03", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-177", summary: "Review: the Kubernetes manifests have no resource limits. A runaway pod could consume all node resources and affect other workloads. Add CPU/memory requests and limits to all deployments", created_at: "2026-01-10", stream_id: Some("infrastructure"), category: "code_review" },
        TestMemory { id: "mem-178", summary: "PR feedback: the date formatting is inconsistent across the UI. Some places use MM/DD/YYYY, others DD.MM.YYYY. Centralize date formatting through a utility that respects the user locale", created_at: "2026-01-17", stream_id: Some("frontend"), category: "code_review" },
        TestMemory { id: "mem-179", summary: "Code review: the webhook retry configuration uses max 3 retries with 1 second delay. For transient failures this is too aggressive. Use exponential backoff with 1min, 5min, 30min intervals and max 5 retries", created_at: "2026-01-24", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-180", summary: "Review of the database schema: the users table has 40 columns. Split into core user fields and separate profile, preferences, and billing tables. Wide tables cause cache inefficiency", created_at: "2026-01-31", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-181", summary: "PR review: the search feature uses synchronous re-indexing on every write. This blocks the write operation and degrades response times. Move to async indexing with a message queue", created_at: "2026-02-07", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-182", summary: "Code review feedback: the TypeScript types are using 'any' in 37 places. This defeats the purpose of type safety. Create proper interfaces for all API response types and shared data structures", created_at: "2026-02-14", stream_id: Some("frontend"), category: "code_review" },
        TestMemory { id: "mem-183", summary: "Review: the secrets in the deployment manifest are base64 encoded but not encrypted. Anyone with kubectl access can decode them. Use Sealed Secrets or external secret management", created_at: "2026-02-21", stream_id: Some("security"), category: "code_review" },
        TestMemory { id: "mem-184", summary: "PR feedback: the email template system uses string concatenation for building HTML. This is error-prone and an XSS risk. Switch to a proper templating engine with auto-escaping like Handlebars", created_at: "2026-02-28", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-185", summary: "Code review: the GraphQL resolvers are fetching data in a waterfall pattern. The user profile resolver waits for the org resolver which waits for the team resolver. Use DataLoader for batching and parallel execution", created_at: "2026-03-07", stream_id: Some("platform"), category: "code_review" },
        TestMemory { id: "mem-186", summary: "Review of the mobile app networking layer: all API calls go through a single serial queue. This creates a bottleneck when loading data-heavy screens. Use concurrent request queues with priority levels", created_at: "2026-03-14", stream_id: Some("frontend"), category: "code_review" },
        TestMemory { id: "mem-187", summary: "PR review: the integration tests create real network connections to external APIs. Mock these dependencies to make tests deterministic and fast. Use WireMock for HTTP service simulation", created_at: "2026-03-21", stream_id: Some("platform"), category: "code_review" },
        TestMemory { id: "mem-188", summary: "Code review feedback: the service discovery client caches results indefinitely. If a service moves to a new IP the client will never find it. Add a TTL of 60 seconds on cached service endpoints", created_at: "2026-03-28", stream_id: Some("infrastructure"), category: "code_review" },
        TestMemory { id: "mem-189", summary: "Review: the logging middleware logs the full request body including sensitive fields. Implement a field-level redaction filter that removes password, token, and credit_card fields before logging", created_at: "2025-07-28", stream_id: Some("security"), category: "code_review" },
        TestMemory { id: "mem-190", summary: "PR feedback: the job scheduler does not handle timezone changes for daylight saving time. Scheduled jobs fire at the wrong time twice a year. Use IANA timezone database and calculate next run in UTC", created_at: "2025-08-20", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-191", summary: "Code review: the HTTP client has no timeout configuration. A slow downstream service will hold threads indefinitely. Set connect timeout to 5s and read timeout to 30s with circuit breaker at 50% failure rate", created_at: "2025-09-08", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-192", summary: "Review of the message queue consumer: ack is sent before processing completes. If the process crashes mid-processing the message is lost. Move ack to after successful processing completion", created_at: "2025-10-08", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-193", summary: "PR review: the frontend bundle includes all locale data for 200+ languages even though we only support 5. Tree-shake the locale imports to reduce bundle size by approximately 800KB", created_at: "2025-11-05", stream_id: Some("frontend"), category: "code_review" },
        TestMemory { id: "mem-194", summary: "Code review feedback: the database connection string is hardcoded in the source code. Extract to environment variable and add to the secrets management system. Rotate the credentials immediately", created_at: "2025-12-03", stream_id: Some("security"), category: "code_review" },
        TestMemory { id: "mem-195", summary: "Review: the worker process has no graceful shutdown handler. When Kubernetes sends SIGTERM the process dies immediately dropping in-progress jobs. Add signal handler with drain timeout of 30 seconds", created_at: "2026-01-05", stream_id: Some("infrastructure"), category: "code_review" },
        TestMemory { id: "mem-196", summary: "PR feedback: the API response times vary by 10x between environments because the query planner uses different plans. Add query hints or force index usage for the critical product search query", created_at: "2026-02-01", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-197", summary: "Code review: using Math.random() for generating session tokens. This is not cryptographically secure. Switch to crypto.getRandomValues or a proper CSPRNG for security-sensitive random values", created_at: "2026-03-01", stream_id: Some("security"), category: "code_review" },
        TestMemory { id: "mem-198", summary: "Review of the caching strategy: cache keys include the full SQL query string which can exceed key length limits. Hash the query parameters into a fixed-length cache key using SHA-256", created_at: "2025-09-25", stream_id: Some("backend"), category: "code_review" },
        TestMemory { id: "mem-199", summary: "PR review: the frontend form validation runs on every keystroke causing lag on slower devices. Debounce validation to 300ms and only validate the changed field, not the entire form", created_at: "2025-10-20", stream_id: Some("frontend"), category: "code_review" },
        TestMemory { id: "mem-200", summary: "Code review feedback: the microservice has 15 external dependencies but no health check endpoint that verifies them. Add a structured health check that reports status of each dependency individually", created_at: "2025-11-20", stream_id: Some("platform"), category: "code_review" },

        // ====================================================================
        // CATEGORY 5: Debugging Sessions (50)
        // ====================================================================
        TestMemory { id: "mem-201", summary: "Debugged memory leak in Node.js notification service. Heap dump showed 50K orphaned Promise objects from unresolved callbacks in the email queue. Fixed by adding timeout and cleanup for stale promises", created_at: "2025-07-14", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-202", summary: "Traced the API timeout issue to DNS resolution. The service was resolving DNS on every request instead of caching. After enabling DNS caching with 30s TTL, p99 latency dropped from 2s to 200ms", created_at: "2025-07-21", stream_id: Some("infrastructure"), category: "debugging" },
        TestMemory { id: "mem-203", summary: "Investigated the intermittent 500 errors on the checkout endpoint. Flame graph showed contention on the database connection pool mutex. Root cause: health check queries were consuming connections meant for user traffic", created_at: "2025-07-28", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-204", summary: "Debugged the slow dashboard page load. Chrome DevTools showed 47 sequential API calls creating a 12-second waterfall. Refactored to 3 parallel batch API calls reducing total load time to 1.2 seconds", created_at: "2025-08-04", stream_id: Some("frontend"), category: "debugging" },
        TestMemory { id: "mem-205", summary: "Traced the CPU spike at 3am to the daily analytics aggregation job running with unbounded parallelism. Each worker spawned as many threads as cores, saturating the entire cluster. Added concurrency limit of 4 per pod", created_at: "2025-08-11", stream_id: Some("data"), category: "debugging" },
        TestMemory { id: "mem-206", summary: "Investigated the flaky end-to-end tests. Root cause was a shared test database without proper isolation. Tests running in parallel would see each other's data. Fixed by creating a fresh schema per test run", created_at: "2025-08-18", stream_id: Some("platform"), category: "debugging" },
        TestMemory { id: "mem-207", summary: "Debugged the authentication token refresh loop. After the access token expired, the refresh endpoint was returning a new access token but not updating the refresh token, causing an infinite loop of refreshes", created_at: "2025-08-25", stream_id: Some("platform"), category: "debugging" },
        TestMemory { id: "mem-208", summary: "Traced the data inconsistency between the order service and inventory service. Events were being processed out of order due to Kafka partition rebalancing. Fixed by adding idempotency keys and deduplication window", created_at: "2025-09-01", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-209", summary: "Investigated high error rate in the image processing pipeline. The worker pods were being OOM-killed because they loaded entire images into memory. Switched to streaming processing with memory-mapped files", created_at: "2025-09-08", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-210", summary: "Debugged the WebSocket connection dropping every 60 seconds. The cloud load balancer had an idle timeout of 60s. Added application-level ping/pong heartbeat at 30s interval to keep connections alive", created_at: "2025-09-15", stream_id: Some("infrastructure"), category: "debugging" },
        TestMemory { id: "mem-211", summary: "Traced the slow Elasticsearch queries to missing field data cache. The terms aggregation on a high-cardinality field was rebuilding the cache on every query. Pre-warmed the field data cache at index time", created_at: "2025-09-22", stream_id: Some("data"), category: "debugging" },
        TestMemory { id: "mem-212", summary: "Investigated the database replication lag spike. The replica was falling 30 seconds behind during peak hours. Root cause was a single long-running analytical query blocking WAL replay. Added query timeout of 10s on replicas", created_at: "2025-09-29", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-213", summary: "Debugged the mobile app battery drain issue. Profiling showed the app was polling the API every 5 seconds even when in the background. Implemented push notifications for real-time updates and reduced background polling to 5 minutes", created_at: "2025-10-06", stream_id: Some("frontend"), category: "debugging" },
        TestMemory { id: "mem-214", summary: "Traced the sporadic test failures to a time-dependent test that assumed UTC timezone. On CI machines in different regions the test failed because it formatted dates in the local timezone. Fixed by setting TZ=UTC in test runner", created_at: "2025-10-13", stream_id: Some("platform"), category: "debugging" },
        TestMemory { id: "mem-215", summary: "Investigated the Kubernetes pod eviction events happening daily around noon. The node was running out of ephemeral storage because container logs were not being rotated. Added log rotation config with 100MB max per container", created_at: "2025-10-20", stream_id: Some("infrastructure"), category: "debugging" },
        TestMemory { id: "mem-216", summary: "Debugged the payment webhook failures. The payment provider was sending webhooks within 2 seconds of the payment, but our service took 5 seconds to create the order record. Added a retry queue with exponential backoff on the webhook receiver", created_at: "2025-10-27", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-217", summary: "Traced the GraphQL query timeout for deeply nested queries. A user requested 5 levels of nested relationships causing 2^5 database calls. Added query depth limiting at 3 levels and query cost analysis", created_at: "2025-11-03", stream_id: Some("platform"), category: "debugging" },
        TestMemory { id: "mem-218", summary: "Investigated the Redis cluster failover that caused 30 seconds of downtime. The client library was not respecting MOVED redirections during failover. Upgraded to a cluster-aware client with automatic redirect handling", created_at: "2025-11-10", stream_id: Some("infrastructure"), category: "debugging" },
        TestMemory { id: "mem-219", summary: "Debugged the email delivery rate dropping to 60%. SPF and DKIM records were correctly configured but DMARC policy was set to reject. The issue was that some emails were being sent from a secondary IP not included in the SPF record", created_at: "2025-11-17", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-220", summary: "Traced the frontend hydration mismatch error to a component that rendered different HTML on server vs client because it used Date.now() directly. Replaced with a timestamp passed from the server as initial state", created_at: "2025-11-24", stream_id: Some("frontend"), category: "debugging" },
        TestMemory { id: "mem-221", summary: "Investigated the AWS Lambda cold start latency of 8 seconds. The function was loading a 200MB ML model from S3 on every cold start. Moved model to EFS with Lambda mount and reduced cold start to 800ms", created_at: "2025-12-01", stream_id: Some("infrastructure"), category: "debugging" },
        TestMemory { id: "mem-222", summary: "Debugged the search relevance regression after the index migration. The new index was using a different analyzer that stripped stop words differently. Restored the original analyzer configuration", created_at: "2025-12-08", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-223", summary: "Traced the cookie not being set on the production domain. The Secure flag was set but the staging environment used HTTP. Made cookie attributes environment-specific and added integration test for cookie behavior", created_at: "2025-12-15", stream_id: Some("frontend"), category: "debugging" },
        TestMemory { id: "mem-224", summary: "Investigated the Kafka consumer group rebalancing storm. One consumer was taking 45 seconds to process a batch, exceeding the session timeout. Reduced batch size and increased session timeout to 120 seconds", created_at: "2025-12-22", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-225", summary: "Debugged the CI build that started failing after the Dependabot update. A transitive dependency updated its minimum Node.js version to 18 but our CI was still running Node 16. Updated CI node version", created_at: "2025-12-29", stream_id: Some("infrastructure"), category: "debugging" },
        TestMemory { id: "mem-226", summary: "Traced the API response time degradation to the audit logging middleware. It was doing a synchronous database insert on every request. Changed to async fire-and-forget with a buffer that flushes every 5 seconds", created_at: "2026-01-05", stream_id: Some("platform"), category: "debugging" },
        TestMemory { id: "mem-227", summary: "Investigated the data pipeline producing duplicate records. The Flink checkpoint interval was longer than the processing window, so records were replayed from the last checkpoint on restart. Shortened checkpoint interval to 30 seconds", created_at: "2026-01-12", stream_id: Some("data"), category: "debugging" },
        TestMemory { id: "mem-228", summary: "Debugged the TLS handshake failure between services. One service was using an outdated CA bundle that did not include the renewed internal CA certificate. Updated CA bundle and added automated certificate renewal monitoring", created_at: "2026-01-19", stream_id: Some("security"), category: "debugging" },
        TestMemory { id: "mem-229", summary: "Traced the dashboard chart rendering bug to a floating point precision issue. Revenue values like $99.99 were displaying as $99.98999999999 after aggregation. Applied toFixed(2) at the display layer and used integer cents for calculations", created_at: "2026-01-26", stream_id: Some("frontend"), category: "debugging" },
        TestMemory { id: "mem-230", summary: "Investigated the Docker build taking 45 minutes. Build context included the entire node_modules directory and git history. Added proper .dockerignore file and reduced build time to 3 minutes", created_at: "2026-02-02", stream_id: Some("infrastructure"), category: "debugging" },
        TestMemory { id: "mem-231", summary: "Debugged the S3 upload timeout for files larger than 100MB. The single PUT request was timing out. Switched to multipart upload with 10MB parts and concurrent upload of 5 parts for a 4x speedup", created_at: "2026-02-09", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-232", summary: "Traced the intermittent auth failures to clock skew between the API server and the JWT issuer. The API server clock was 3 seconds behind, causing tokens with tight expiry windows to appear expired. Added NTP sync check to startup", created_at: "2026-02-16", stream_id: Some("platform"), category: "debugging" },
        TestMemory { id: "mem-233", summary: "Investigated the database connection leak. Active connections grew by 2 per hour and never returned to the pool. Found that error paths in the transaction handler were not closing connections. Added finally block for guaranteed cleanup", created_at: "2026-02-23", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-234", summary: "Debugged the gRPC service returning UNAVAILABLE errors intermittently. Envoy sidecar was doing L7 health checks that interfered with the gRPC protocol. Changed to TCP health checks on the gRPC port", created_at: "2026-03-02", stream_id: Some("infrastructure"), category: "debugging" },
        TestMemory { id: "mem-235", summary: "Traced the React app memory leak to a subscription in useEffect that was not cleaned up when the component unmounted. Added the cleanup function in the useEffect return statement and verified with memory profiler", created_at: "2026-03-09", stream_id: Some("frontend"), category: "debugging" },
        TestMemory { id: "mem-236", summary: "Investigated why the cron job ran twice at 2am on the DST transition day. The scheduler was using local time and the clock going back created a duplicate trigger. Switched all cron schedules to UTC", created_at: "2026-03-16", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-237", summary: "Debugged the Prometheus cardinality explosion. A metric label was using user_id creating millions of unique time series. Removed user-specific labels and added a separate counter for per-user tracking", created_at: "2026-03-23", stream_id: Some("infrastructure"), category: "debugging" },
        TestMemory { id: "mem-238", summary: "Traced the slow startup of the user service to eager loading of all feature flags from LaunchDarkly on initialization. Changed to lazy loading where flags are fetched on first access with local caching", created_at: "2026-03-30", stream_id: Some("platform"), category: "debugging" },
        TestMemory { id: "mem-239", summary: "Investigated the 403 forbidden errors on file downloads. CloudFront signed URLs were being generated with a 15-minute expiry, but the browser was caching the page HTML containing the URL for longer. Increased URL expiry to 4 hours", created_at: "2025-07-30", stream_id: Some("infrastructure"), category: "debugging" },
        TestMemory { id: "mem-240", summary: "Debugged the message queue deadletter growing rapidly. Consumers were rejecting messages because the schema had changed but old-format messages were still in the queue. Added schema versioning and backward-compatible deserialization", created_at: "2025-08-25", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-241", summary: "Traced the null pointer exception in the recommendation engine to a missing null check on the user profile preferences field. New users without preferences were crashing the batch job. Added Optional handling throughout the pipeline", created_at: "2025-09-20", stream_id: Some("data"), category: "debugging" },
        TestMemory { id: "mem-242", summary: "Investigated the load balancer returning 504 gateway timeout sporadically. The backend instances had different response times and the slow instance was hitting the 30s timeout. Added circuit breaking to route traffic away from slow instances", created_at: "2025-10-15", stream_id: Some("infrastructure"), category: "debugging" },
        TestMemory { id: "mem-243", summary: "Debugged the data export job that was producing corrupted CSV files. The writer was not flushing the buffer before closing the file when the export exceeded 1GB. Added explicit flush call before file close", created_at: "2025-11-12", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-244", summary: "Traced the frontend infinite re-render loop to a useEffect dependency array that included an object literal. Each render created a new object reference triggering the effect again. Moved object to useMemo", created_at: "2025-12-10", stream_id: Some("frontend"), category: "debugging" },
        TestMemory { id: "mem-245", summary: "Investigated the connection reset errors in the gRPC streaming endpoint. The HTTP/2 keep-alive settings on the load balancer were shorter than the gRPC stream duration. Aligned keep-alive intervals across all layers", created_at: "2026-01-08", stream_id: Some("infrastructure"), category: "debugging" },
        TestMemory { id: "mem-246", summary: "Debugged the Terraform plan showing 200+ resource changes after a minor config update. A module version bump changed the default tags structure. Pinned module version and updated state to match the new defaults", created_at: "2026-02-05", stream_id: Some("infrastructure"), category: "debugging" },
        TestMemory { id: "mem-247", summary: "Traced the high garbage collection pause times in the JVM-based order service. The heap was configured at 8GB with default GC. Switched to ZGC with 4GB heap and reduced p99 GC pause from 500ms to 2ms", created_at: "2026-03-05", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-248", summary: "Investigated the pod scheduling failures in Kubernetes. Nodes had sufficient CPU but were low on memory. Resource requests were underestimated because the profiling was done during low traffic. Re-measured during peak traffic and adjusted", created_at: "2025-08-15", stream_id: Some("infrastructure"), category: "debugging" },
        TestMemory { id: "mem-249", summary: "Debugged the websocket message ordering issue. Messages sent rapidly were arriving out of order because the async handler was processing them concurrently. Added a per-client serial message queue", created_at: "2025-10-01", stream_id: Some("backend"), category: "debugging" },
        TestMemory { id: "mem-250", summary: "Traced the increased error rate to a misconfigured feature flag that routed 100% of traffic to the experimental code path. The rollout percentage was set to 100 instead of 10. Rolled back the flag and added guardrail of max 50% for new experiments", created_at: "2025-11-25", stream_id: Some("platform"), category: "debugging" },

        // ====================================================================
        // CATEGORY 6: Team Discussions (50)
        // ====================================================================
        TestMemory { id: "mem-251", summary: "Team standup: blocked on API approval from the payments team. The contract review has been pending for 2 weeks. Escalating to engineering manager to expedite the process", created_at: "2025-07-11", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-252", summary: "Design review feedback on new dashboard layout: stakeholders want more data density, fewer clicks to reach key metrics, and the ability to customize widget arrangement per user role", created_at: "2025-07-18", stream_id: Some("frontend"), category: "team" },
        TestMemory { id: "mem-253", summary: "Team retrospective: deployment pipeline too slow, need to parallelize test suites and add better caching for CI/CD builds. Action item assigned to DevOps team lead", created_at: "2025-07-25", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-254", summary: "Architecture review meeting: proposal to extract the notification system into a standalone service. Currently embedded in 3 different services causing inconsistent behavior. Team approved the extraction", created_at: "2025-08-01", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-255", summary: "Cross-team sync: data engineering needs access to the product event stream for the recommendation engine. Platform team to provide read-only access to the Kafka topic with a new consumer group", created_at: "2025-08-08", stream_id: Some("data"), category: "team" },
        TestMemory { id: "mem-256", summary: "On-call rotation discussion: switching from weekly to bi-weekly rotations. Adding secondary on-call for major incidents. Compensating on-call engineers with extra PTO days", created_at: "2025-08-15", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-257", summary: "Sprint planning disagreement on scope: product wants 12 story points but team capacity is 8 after accounting for on-call duties and tech debt allocation. Agreed to cut the admin reporting feature to next sprint", created_at: "2025-08-22", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-258", summary: "Knowledge sharing session on Kubernetes debugging: demonstrated kubectl debug, explained pod eviction priorities, showed how to read resource quotas and limit ranges. Recorded for new team members", created_at: "2025-08-29", stream_id: Some("infrastructure"), category: "team" },
        TestMemory { id: "mem-259", summary: "Team morale discussion: engineers feel context-switching between too many projects. Proposal to dedicate engineers to specific domains for at least one full sprint before rotating. Manager agreed to try this approach", created_at: "2025-09-05", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-260", summary: "Post-release review: the v3.2 release went smoothly. Zero customer-reported bugs in the first 48 hours. The investment in pre-release testing paid off. Team celebrated with pizza", created_at: "2025-09-12", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-261", summary: "Code review guidelines updated: all PRs require at least one approval, security-sensitive changes require two approvals, maximum 400 lines per PR, description template is mandatory", created_at: "2025-09-19", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-262", summary: "Team all-hands: CTO announced focus shift to enterprise features. Multi-tenancy, SSO, audit logging, and SLA dashboards are now top priority. Some engineers will be reallocated from consumer features", created_at: "2025-09-26", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-263", summary: "Backend team offsite planning: 2-day event focused on architectural vision for 2026. Topics include serverless migration assessment, AI feature integration, and technical debt reduction strategy", created_at: "2025-10-03", stream_id: Some("backend"), category: "team" },
        TestMemory { id: "mem-264", summary: "Pair programming experiment results: after 4 weeks of pairing on complex tasks, bugs in paired code dropped by 40% but velocity decreased by 15%. Team decided to pair selectively on critical features and bug fixes", created_at: "2025-10-10", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-265", summary: "Discussion on documentation standards: every service must have a README with setup instructions, architecture diagram, API documentation, and runbook. Quarterly documentation reviews added to sprint calendar", created_at: "2025-10-17", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-266", summary: "New team member onboarding feedback: Priya said the setup process took 3 days because documentation was outdated and several services needed manual configuration steps not documented anywhere", created_at: "2025-10-24", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-267", summary: "Cross-functional meeting on feature prioritization: sales team requesting custom reporting, support team needs bulk actions UI, engineering wants to invest in observability. Agreed to split capacity across all three", created_at: "2025-10-31", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-268", summary: "Retrospective action item follow-up: the automated deployment pipeline is now 3x faster after parallelization work. Test reliability improved from 92% to 99.5% after fixing flaky tests. Both action items closed", created_at: "2025-11-07", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-269", summary: "Team decided to deprecate the old REST API v1 endpoints by June 2026 and migrate all internal consumers to GraphQL. External consumers get 12 months notice with migration support", created_at: "2025-11-14", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-270", summary: "Technical interview debrief: candidate showed strong system design skills but limited hands-on experience with Kubernetes. Team consensus is strong hire with expectation of 3-month K8s ramp-up", created_at: "2025-11-21", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-271", summary: "Incident review meeting: the recent outage exposed a gap in our monitoring. We had metrics for individual services but no end-to-end synthetic monitoring. Action: implement Synthetic checks for critical user flows", created_at: "2025-11-28", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-272", summary: "Quarterly planning: Q1 2026 themes are reliability (target 99.95% uptime), performance (all p95 under 200ms), and developer productivity (setup time under 10 minutes)", created_at: "2025-12-05", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-273", summary: "Team skill assessment: identified gaps in distributed systems expertise and security engineering. Budget approved for 2 conference attendance slots and an online learning platform subscription per engineer", created_at: "2025-12-12", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-274", summary: "Engineering culture discussion: team wants more open-source contributions and conference talks. Manager agreed to allow 10% time for open-source work and will support speaking proposals with preparation time", created_at: "2025-12-19", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-275", summary: "Service ownership matrix review: 4 services currently have no designated owner. Assigned primary and secondary owners for each. Added ownership metadata to the service catalog in Backstage", created_at: "2025-12-26", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-276", summary: "Standup: frontend team blocked on API changes from backend team. Backend team has a 3-day lead time for schema changes due to contract test pipeline. Working on reducing turnaround time", created_at: "2026-01-02", stream_id: Some("frontend"), category: "team" },
        TestMemory { id: "mem-277", summary: "War room for the search index rebuild: all hands on deck to rebuild the Elasticsearch index after the corrupt shard incident. Estimated 6 hours for full re-index of 5M documents", created_at: "2026-01-09", stream_id: Some("backend"), category: "team" },
        TestMemory { id: "mem-278", summary: "Team agreement on coding standards: adopt the company style guide with one modification - use 4-space indentation instead of 2 for better readability in complex nested code", created_at: "2026-01-16", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-279", summary: "New engineer mentorship program launched: each new hire gets paired with a senior engineer for their first 90 days. Weekly 1-on-1s, shared code reviews, and a structured learning path", created_at: "2026-01-23", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-280", summary: "Discussion about reducing meeting load: agreed to make standup async via Slack bot updates, cancel recurring meetings with no clear agenda, and designate Wednesday as meeting-free focus day", created_at: "2026-01-30", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-281", summary: "Platform team offsite recap: agreed on 3 technical bets for 2026 - edge computing for latency, AI-assisted code review, and automated performance testing. Each bet gets a 2-week prototype sprint", created_at: "2026-02-06", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-282", summary: "Cross-team dependency mapping exercise: identified 23 cross-team dependencies. Top 3 bottlenecks are auth service changes, database schema migrations, and API contract updates. Created SLA for each", created_at: "2026-02-13", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-283", summary: "Team health survey results: high scores on technical challenge and autonomy, low scores on work-life balance and career growth clarity. Action plan: reduce after-hours pages and create engineering ladder document", created_at: "2026-02-20", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-284", summary: "Blameless postmortem culture reinforcement: reminded team that postmortems focus on systems not people. Updated the template to explicitly remove names and focus on process improvements", created_at: "2026-02-27", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-285", summary: "Discussion on technical debt prioritization: ranked debt items by impact and effort. Top priority is replacing the custom ORM with SQLAlchemy, followed by migrating from Webpack to Vite for faster builds", created_at: "2026-03-06", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-286", summary: "Sprint demo went well - showed the new real-time notification system to stakeholders. Product team excited about the push notification support for mobile. Follow-up meeting scheduled to discuss analytics hooks", created_at: "2026-03-13", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-287", summary: "Data team standup: the ETL pipeline for customer churn prediction is 70% complete. Blocked on getting access to the payment history table from the billing team. Expected unblock by end of week", created_at: "2026-03-20", stream_id: Some("data"), category: "team" },
        TestMemory { id: "mem-288", summary: "Security team sync: completed the quarterly access review. Revoked 15 unused service accounts and 8 ex-employee GitHub accesses. Implementing automated access review going forward", created_at: "2026-03-27", stream_id: Some("security"), category: "team" },
        TestMemory { id: "mem-289", summary: "Frontend guild meeting: shared learnings from React 19 migration pilot. Key finding: the new use() hook simplifies data fetching significantly but requires rethinking error boundary placement", created_at: "2025-07-20", stream_id: Some("frontend"), category: "team" },
        TestMemory { id: "mem-290", summary: "Manager 1-on-1 notes: discussed promotion path for Kai. Needs to demonstrate cross-team influence and lead at least one major technical initiative. Agreed on the API gateway migration as the promotion project", created_at: "2025-08-20", stream_id: None, category: "team" },
        TestMemory { id: "mem-291", summary: "Team lunch discussion about AI coding assistants: most engineers use Copilot daily, some prefer Claude. Agreed to standardize on a shared config and prompt library to share best practices", created_at: "2025-09-20", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-292", summary: "Incident commander rotation training: walked through the ICS framework, communication templates, and escalation matrix. 6 engineers now certified as incident commanders", created_at: "2025-10-20", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-293", summary: "Team decision: implement feature flags for all new features going forward. No more big-bang releases. This supports trunk-based development and reduces deployment risk significantly", created_at: "2025-11-20", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-294", summary: "Hackathon results: winning project was an AI-powered log analyzer that clusters error patterns and suggests fixes. Runner-up was a visual query builder for non-technical users. Both approved for productization", created_at: "2025-12-20", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-295", summary: "Tech lead sync: agreed to implement a service mesh evaluation in Q2. Comparing Istio, Linkerd, and Cilium. Main requirements are mTLS, traffic splitting, and circuit breaking without application code changes", created_at: "2026-01-20", stream_id: Some("infrastructure"), category: "team" },
        TestMemory { id: "mem-296", summary: "New team formation: spinning up a dedicated platform reliability team with 4 engineers. Focus areas: SLO management, automated incident response, capacity planning, and chaos engineering", created_at: "2026-02-20", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-297", summary: "Engineering all-hands: VP announced company is going carbon neutral. All new infrastructure must consider energy efficiency. Prefer ARM instances, right-size resources, shut down dev environments after hours", created_at: "2026-03-20", stream_id: Some("infrastructure"), category: "team" },
        TestMemory { id: "mem-298", summary: "Team retro: too many context switches between projects. Solution: implement a Kanban WIP limit of 2 items per engineer and protect focus time blocks in calendar", created_at: "2025-08-30", stream_id: Some("platform"), category: "team" },
        TestMemory { id: "mem-299", summary: "Design system council meeting: approved 5 new components for the shared library. Rejected the custom date picker in favor of the existing one with accessibility improvements. Versioning strategy: semver with breaking change freeze during release candidates", created_at: "2025-10-30", stream_id: Some("frontend"), category: "team" },
        TestMemory { id: "mem-300", summary: "Cross-team alignment on API naming conventions: all endpoints use kebab-case, all query params use snake_case, all JSON fields use camelCase. Linting rule added to the CI pipeline to enforce this", created_at: "2025-12-30", stream_id: Some("platform"), category: "team" },

        // ====================================================================
        // CATEGORY 7: Technical Specs (50)
        // ====================================================================
        TestMemory { id: "mem-301", summary: "Database schema for the users table: id (UUID PK), email (unique), display_name, avatar_url, created_at, updated_at, last_login_at, status (active/suspended/deleted), org_id (FK)", created_at: "2025-07-13", stream_id: Some("backend"), category: "specs" },
        TestMemory { id: "mem-302", summary: "API rate limiting specification: 100 requests per minute per API key for standard tier, 1000 for premium tier. Burst allowance of 20 requests above limit. Rate limit headers: X-RateLimit-Limit, X-RateLimit-Remaining, X-RateLimit-Reset", created_at: "2025-07-20", stream_id: Some("platform"), category: "specs" },
        TestMemory { id: "mem-303", summary: "Nginx reverse proxy configuration: rate limiting at 100 req/s per IP, SSL termination with Let's Encrypt auto-renewal, WebSocket upgrade support on /ws path, gzip compression for text and JSON responses", created_at: "2025-07-27", stream_id: Some("infrastructure"), category: "specs" },
        TestMemory { id: "mem-304", summary: "Docker image optimization spec: multi-stage builds, distroless base images for production, separate debug image with shell access. Target production image size under 200MB for all services", created_at: "2025-08-03", stream_id: Some("infrastructure"), category: "specs" },
        TestMemory { id: "mem-305", summary: "Prometheus monitoring specification: scrape interval 15s, retention 30 days, alerting rules for p95 latency above 200ms and error rate above 1%. Custom metrics for business KPIs exported by each service", created_at: "2025-08-10", stream_id: Some("infrastructure"), category: "specs" },
        TestMemory { id: "mem-306", summary: "Kafka topic configuration for the event bus: 12 partitions per topic, replication factor 3, retention 7 days for event topics and 30 days for audit topics, compression type lz4, min.insync.replicas=2", created_at: "2025-08-17", stream_id: Some("platform"), category: "specs" },
        TestMemory { id: "mem-307", summary: "GraphQL schema design spec: using federation with Apollo Gateway, each service owns its own subgraph. Shared types defined in a schema registry. Breaking changes require RFC and 2-week migration window", created_at: "2025-08-24", stream_id: Some("platform"), category: "specs" },
        TestMemory { id: "mem-308", summary: "Elasticsearch cluster specification: 3 data nodes (64GB RAM, 2TB SSD each), 2 dedicated master-eligible nodes, index lifecycle management with hot-warm-cold tiers. ILM policy: hot 7 days, warm 30 days, cold 90 days, delete after 180 days", created_at: "2025-08-31", stream_id: Some("infrastructure"), category: "specs" },
        TestMemory { id: "mem-309", summary: "Redis configuration spec: cluster mode with 6 nodes (3 primary, 3 replica), maxmemory 4GB per node with allkeys-lru eviction, persistence via AOF with fsync every second, TLS enabled for all connections", created_at: "2025-09-07", stream_id: Some("infrastructure"), category: "specs" },
        TestMemory { id: "mem-310", summary: "JWT token specification: RS256 signing algorithm, access token TTL 15 minutes, refresh token TTL 7 days with rotation. Claims: sub, email, roles, org_id, iat, exp. Token size target under 1KB", created_at: "2025-09-14", stream_id: Some("security"), category: "specs" },
        TestMemory { id: "mem-311", summary: "Database connection pool configuration: max_connections=20 per service instance, idle_timeout=30s, max_lifetime=300s, min_connections=5, connection_timeout=5s with health check on checkout", created_at: "2025-09-21", stream_id: Some("backend"), category: "specs" },
        TestMemory { id: "mem-312", summary: "Logging standard specification: structured JSON format with fields timestamp, level, service, trace_id, span_id, message, and optional metadata. Log levels: ERROR for failures, WARN for degradation, INFO for significant events, DEBUG for troubleshooting", created_at: "2025-09-28", stream_id: Some("platform"), category: "specs" },
        TestMemory { id: "mem-313", summary: "S3 bucket configuration spec: server-side encryption with AWS KMS customer-managed key, versioning enabled, lifecycle policy to transition to Glacier after 90 days. Bucket policy restricts access to VPC endpoint only", created_at: "2025-10-05", stream_id: Some("infrastructure"), category: "specs" },
        TestMemory { id: "mem-314", summary: "gRPC service specification: deadline propagation with 30s max, retry policy of 3 attempts for UNAVAILABLE and DEADLINE_EXCEEDED, max message size 4MB, server reflection enabled for debugging", created_at: "2025-10-12", stream_id: Some("platform"), category: "specs" },
        TestMemory { id: "mem-315", summary: "Kubernetes resource limits specification: all pods must set requests and limits. Default: CPU request 100m/limit 500m, memory request 256Mi/limit 512Mi. Services with custom needs must document justification", created_at: "2025-10-19", stream_id: Some("infrastructure"), category: "specs" },
        TestMemory { id: "mem-316", summary: "CI/CD pipeline specification: build triggers on push to any branch, deploy to staging on merge to main, deploy to production on tag. All stages must complete in under 15 minutes. Rollback triggered by health check failure within 5 minutes of deploy", created_at: "2025-10-26", stream_id: Some("infrastructure"), category: "specs" },
        TestMemory { id: "mem-317", summary: "API error response specification: all errors return {status, code, message, details, trace_id}. Codes follow domain-specific enumeration. 4xx errors include user-friendly message. 5xx errors log details server-side but return generic client message", created_at: "2025-11-02", stream_id: Some("platform"), category: "specs" },
        TestMemory { id: "mem-318", summary: "Database backup specification: automated daily snapshots retained for 30 days, point-in-time recovery enabled with 5-minute granularity, cross-region replication to eu-west-1, monthly restore test drill", created_at: "2025-11-09", stream_id: Some("infrastructure"), category: "specs" },
        TestMemory { id: "mem-319", summary: "WebSocket protocol specification: binary frames with protobuf encoding, ping/pong heartbeat every 30 seconds, max message size 64KB, reconnection with exponential backoff starting at 1s up to 60s", created_at: "2025-11-16", stream_id: Some("platform"), category: "specs" },
        TestMemory { id: "mem-320", summary: "Feature flag specification: all flags must have an owner, creation date, and expected removal date. Maximum lifespan is 90 days. Stale flags are reported weekly. Kill switch flags have no expiration", created_at: "2025-11-23", stream_id: Some("platform"), category: "specs" },
        TestMemory { id: "mem-321", summary: "TLS configuration specification: minimum TLS 1.2 for all external connections, TLS 1.3 preferred. mTLS required for all internal service-to-service communication. Certificate rotation every 30 days via cert-manager", created_at: "2025-11-30", stream_id: Some("security"), category: "specs" },
        TestMemory { id: "mem-322", summary: "Data pipeline specification: source data arrives in S3 as JSON, Flink processes in real-time with exactly-once semantics, output goes to both PostgreSQL for serving and BigQuery for analytics with max 5 minute latency", created_at: "2025-12-07", stream_id: Some("data"), category: "specs" },
        TestMemory { id: "mem-323", summary: "API pagination specification: cursor-based pagination using opaque base64 tokens, default page size 25, maximum page size 100, response includes total_count, has_next_page, and next_cursor fields", created_at: "2025-12-14", stream_id: Some("platform"), category: "specs" },
        TestMemory { id: "mem-324", summary: "Service health check specification: /health returns {status, version, uptime, dependencies: [{name, status, latency_ms}]}. Status values: healthy, degraded, unhealthy. Response time must be under 200ms", created_at: "2025-12-21", stream_id: Some("platform"), category: "specs" },
        TestMemory { id: "mem-325", summary: "CDN configuration specification: CloudFront distribution with edge caching, TTL 24 hours for static assets, 5 minutes for API responses with Cache-Control headers. Custom error pages for 404 and 5xx", created_at: "2025-12-28", stream_id: Some("infrastructure"), category: "specs" },
        TestMemory { id: "mem-326", summary: "Webhook specification: HTTP POST with JSON body, HMAC-SHA256 signature in X-Signature header, 30-second timeout, retry on 5xx with exponential backoff (1m, 5m, 30m, 2h, 8h), dead letter after 5 failed attempts", created_at: "2026-01-04", stream_id: Some("platform"), category: "specs" },
        TestMemory { id: "mem-327", summary: "Secrets management specification: all secrets stored in AWS Secrets Manager, rotated every 90 days, accessed via IAM roles not API keys. No secrets in environment variables or config files. Application reads from Secrets Manager SDK on startup", created_at: "2026-01-11", stream_id: Some("security"), category: "specs" },
        TestMemory { id: "mem-328", summary: "Load balancer specification: Application Load Balancer with health check interval 10s, healthy threshold 3, unhealthy threshold 2, deregistration delay 30s, sticky sessions disabled, cross-zone load balancing enabled", created_at: "2026-01-18", stream_id: Some("infrastructure"), category: "specs" },
        TestMemory { id: "mem-329", summary: "Caching strategy specification: L1 in-process cache with 100 items TTL 60s, L2 Redis cache with TTL 5 minutes, L3 CDN cache with TTL 1 hour. Cache invalidation via event-driven approach through Kafka topic", created_at: "2026-01-25", stream_id: Some("backend"), category: "specs" },
        TestMemory { id: "mem-330", summary: "Audit logging specification: every state-changing operation produces an audit event with actor, action, resource, timestamp, IP address, and before/after values. Events are immutable and stored for 7 years", created_at: "2026-02-01", stream_id: Some("security"), category: "specs" },
        TestMemory { id: "mem-331", summary: "Database naming conventions: tables in snake_case plural, columns in snake_case singular, indexes named idx_{table}_{columns}, foreign keys named fk_{table}_{ref_table}. Timestamps always use _at suffix", created_at: "2026-02-08", stream_id: Some("backend"), category: "specs" },
        TestMemory { id: "mem-332", summary: "Container orchestration spec: all services run as non-root, read-only filesystem, seccomp profile default, no privilege escalation, network policy restricting ingress to only expected sources", created_at: "2026-02-15", stream_id: Some("infrastructure"), category: "specs" },
        TestMemory { id: "mem-333", summary: "Email notification specification: templates stored in S3, rendered server-side with Handlebars, sent via SES with dedicated IP pool, DKIM/SPF/DMARC configured. Bounce rate threshold: 2% triggers alert", created_at: "2026-02-22", stream_id: Some("backend"), category: "specs" },
        TestMemory { id: "mem-334", summary: "Search service specification: supports full-text search across product title, description, and tags. Fuzzy matching with edit distance 2, synonym expansion, stemming for English. Response time p95 under 100ms", created_at: "2026-03-01", stream_id: Some("backend"), category: "specs" },
        TestMemory { id: "mem-335", summary: "API versioning specification: URL-based versioning (/api/v1/, /api/v2/), backward compatibility required within major version, deprecation header added 6 months before removal, migration guide required for breaking changes", created_at: "2026-03-08", stream_id: Some("platform"), category: "specs" },
        TestMemory { id: "mem-336", summary: "Mobile push notification specification: uses APNs for iOS and FCM for Android, payload max 4KB, priority levels (critical, high, normal), topic-based subscription, silent push for background data sync", created_at: "2026-03-15", stream_id: Some("backend"), category: "specs" },
        TestMemory { id: "mem-337", summary: "Service mesh specification: Istio with strict mTLS, traffic management via VirtualService and DestinationRule, retry budget of 20% of base traffic, circuit breaker with 5 consecutive 5xx triggers", created_at: "2026-03-22", stream_id: Some("infrastructure"), category: "specs" },
        TestMemory { id: "mem-338", summary: "Observability specification: all services export metrics via OpenTelemetry, traces sampled at 10% for normal traffic and 100% for errors. Alert channels: PagerDuty for SEV1/2, Slack for SEV3/4", created_at: "2026-03-29", stream_id: Some("platform"), category: "specs" },
        TestMemory { id: "mem-339", summary: "File upload specification: max file size 100MB, accepted types image/jpeg image/png application/pdf, virus scanning via ClamAV before storage, metadata extraction, thumbnail generation for images", created_at: "2025-07-15", stream_id: Some("backend"), category: "specs" },
        TestMemory { id: "mem-340", summary: "OAuth2 authorization server specification: support authorization code flow with PKCE, client credentials flow for service-to-service. Token introspection endpoint for resource servers. Scopes: read, write, admin", created_at: "2025-08-15", stream_id: Some("security"), category: "specs" },
        TestMemory { id: "mem-341", summary: "Data classification specification: four levels - Public (marketing content), Internal (employee docs), Confidential (customer data, financial records), Restricted (credentials, encryption keys). Each level has defined storage and transmission requirements", created_at: "2025-09-15", stream_id: Some("security"), category: "specs" },
        TestMemory { id: "mem-342", summary: "Queue processing specification: SQS with visibility timeout 5x average processing time, max receive count 3 before dead letter queue, consumer batch size 10, long polling with 20s wait time", created_at: "2025-10-15", stream_id: Some("backend"), category: "specs" },
        TestMemory { id: "mem-343", summary: "DNS configuration specification: Route53 with health check routing policy, TTL 60 seconds for service endpoints, alias records for CloudFront and ALB, failover routing to secondary region with 30s health check interval", created_at: "2025-11-15", stream_id: Some("infrastructure"), category: "specs" },
        TestMemory { id: "mem-344", summary: "Database migration specification: all migrations must be backward compatible, must include both up and down scripts, tested against production data snapshot in staging, applied during maintenance window for large tables", created_at: "2025-12-15", stream_id: Some("backend"), category: "specs" },
        TestMemory { id: "mem-345", summary: "Batch processing specification: jobs scheduled via cron with exactly-once semantics using distributed lock, progress tracking via Redis, alerting on jobs exceeding 2x expected duration, automatic retry with dead letter for failures", created_at: "2026-01-15", stream_id: Some("backend"), category: "specs" },
        TestMemory { id: "mem-346", summary: "API authentication specification: Bearer token in Authorization header, token validation on every request, 401 for missing/invalid token, 403 for insufficient scope. Public endpoints listed explicitly in allowlist", created_at: "2026-02-15", stream_id: Some("security"), category: "specs" },
        TestMemory { id: "mem-347", summary: "Image processing specification: on upload, generate thumbnails at 150x150, 300x300, and 600x600. Format: WebP with JPEG fallback. Quality: 85%. Strip EXIF data except orientation. Max processing time: 10 seconds per image", created_at: "2026-03-15", stream_id: Some("backend"), category: "specs" },
        TestMemory { id: "mem-348", summary: "Network security specification: VPC with public and private subnets, NAT gateway for outbound traffic from private subnets, security groups following least privilege, VPC flow logs enabled and shipped to SIEM", created_at: "2025-08-01", stream_id: Some("security"), category: "specs" },
        TestMemory { id: "mem-349", summary: "A/B testing specification: using Statsig for feature flags and experimentation, minimum 2 weeks per experiment with 95% statistical significance. Metrics defined upfront: primary metric, guardrail metrics, and expected lift", created_at: "2025-09-01", stream_id: Some("data"), category: "specs" },
        TestMemory { id: "mem-350", summary: "Content delivery specification: static assets served from S3 through CloudFront, immutable filenames with hash for cache busting. API responses use Cache-Control: no-store for authenticated endpoints, max-age=300 for public", created_at: "2025-10-01", stream_id: Some("infrastructure"), category: "specs" },

        // ====================================================================
        // CATEGORY 8: Incident Reports (50)
        // ====================================================================
        TestMemory { id: "mem-351", summary: "Postmortem: 2-hour outage caused by a misconfigured database migration that locked the users table during peak hours. All API calls touching user data timed out. Mitigation: killed the lock, rolled back migration, rescheduled for maintenance window", created_at: "2025-07-16", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-352", summary: "Incident report: 45-minute outage on the API gateway caused by certificate expiration. The auto-renewal cron job had silently failed 3 weeks ago. Implementing monitoring for certificate expiration with 14-day advance warning", created_at: "2025-07-23", stream_id: Some("infrastructure"), category: "incidents" },
        TestMemory { id: "mem-353", summary: "Alert fired at 3am: CPU above 95% on all 6 database nodes. Root cause was a runaway query from the analytics team that was doing a full table scan on the 200M-row events table without an index. Killed the query and added index", created_at: "2025-07-30", stream_id: Some("data"), category: "incidents" },
        TestMemory { id: "mem-354", summary: "SEV1 incident: payment processing completely down for 90 minutes. The payment gateway vendor had a partial outage affecting our region. Implemented failover to secondary payment processor for future incidents", created_at: "2025-08-06", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-355", summary: "Postmortem: search functionality returned empty results for 4 hours. The Elasticsearch cluster ran out of disk space because the ILM policy was not deleting old indices. Increased disk allocation and fixed the ILM policy", created_at: "2025-08-13", stream_id: Some("infrastructure"), category: "incidents" },
        TestMemory { id: "mem-356", summary: "Incident: customer data exposure. A bug in the access control logic allowed users to see other tenants' data by manipulating the tenant_id query parameter. Patched within 30 minutes, affected 12 customers, notified per GDPR requirements", created_at: "2025-08-20", stream_id: Some("security"), category: "incidents" },
        TestMemory { id: "mem-357", summary: "SEV2 incident: mobile app crash loop affecting iOS users on version 16.5. A nil dereference in the networking layer when the API returned an unexpected empty response body. Hotfix released within 2 hours via expedited App Store review", created_at: "2025-08-27", stream_id: Some("frontend"), category: "incidents" },
        TestMemory { id: "mem-358", summary: "Alert: Redis cluster memory usage at 98%. The user session cache was growing unbounded because TTL was not set on some session keys. Fixed the code path that created sessions without TTL and expired the orphaned keys", created_at: "2025-09-03", stream_id: Some("infrastructure"), category: "incidents" },
        TestMemory { id: "mem-359", summary: "Postmortem: 6-hour degradation of the notification service. A Kafka consumer group rebalance storm caused by one consumer repeatedly crashing and rejoining. Root cause was an unhandled message format change. Added schema validation", created_at: "2025-09-10", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-360", summary: "Incident report: staging environment leaked into production. A misconfigured environment variable pointed the staging deployment to the production database. No data loss but 200 test records were written to production. Cleaned up and added environment validation checks", created_at: "2025-09-17", stream_id: Some("infrastructure"), category: "incidents" },
        TestMemory { id: "mem-361", summary: "SEV1: complete service outage for 3 hours due to AWS us-east-1 regional degradation. Our disaster recovery plan was not tested recently and the failover to eu-west-1 had configuration drift. Restored service manually. Action: monthly DR drill", created_at: "2025-09-24", stream_id: Some("infrastructure"), category: "incidents" },
        TestMemory { id: "mem-362", summary: "Alert: error rate spike to 15% on the user registration endpoint. Root cause was a third-party email verification API returning 500 errors. Implemented graceful degradation: allow registration without email verification and verify asynchronously", created_at: "2025-10-01", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-363", summary: "Postmortem: billing system charged customers twice for their monthly subscription. A race condition in the billing job allowed duplicate charge creation. Refunded all affected customers within 24 hours and added idempotency key to prevent duplicate charges", created_at: "2025-10-08", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-364", summary: "Incident: DNS resolution failure for internal services lasting 90 minutes. The custom CoreDNS configuration in Kubernetes had a caching bug that returned stale NXDOMAIN responses. Rolled back to default CoreDNS config", created_at: "2025-10-15", stream_id: Some("infrastructure"), category: "incidents" },
        TestMemory { id: "mem-365", summary: "SEV2: API response times degraded to 5-10 seconds for 2 hours. The connection pool was exhausted because a new feature introduced a slow query without the team realizing. Added query timeout of 5 seconds and slow query logging", created_at: "2025-10-22", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-366", summary: "Alert: disk space critically low on 2 of 3 Kafka brokers. The log retention configuration was set to unlimited for a new topic created for debugging. Fixed retention and cleaned up the oversized topic partitions", created_at: "2025-10-29", stream_id: Some("infrastructure"), category: "incidents" },
        TestMemory { id: "mem-367", summary: "Postmortem: failed database migration in production corrupted the orders table. The migration assumed column values were non-null but 5000 rows had nulls. Restored from backup and rewrote migration with null handling", created_at: "2025-11-05", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-368", summary: "Incident: unauthorized access attempt detected. An API key with read-only permissions was used to attempt write operations 10,000 times in 5 minutes. Revoked the key, blocked the source IP, and investigated the access pattern", created_at: "2025-11-12", stream_id: Some("security"), category: "incidents" },
        TestMemory { id: "mem-369", summary: "SEV2: the CDN started serving stale content after a cache invalidation failure. Users saw outdated pricing on the marketing site for 4 hours. Root cause was a CloudFront invalidation that exceeded the path limit. Switched to versioned asset URLs", created_at: "2025-11-19", stream_id: Some("infrastructure"), category: "incidents" },
        TestMemory { id: "mem-370", summary: "Alert: SSL certificate for api.example.com expired at midnight. The renewal bot failed because the DNS challenge could not be completed due to a Route53 API permission change. Emergency manual renewal within 15 minutes", created_at: "2025-11-26", stream_id: Some("infrastructure"), category: "incidents" },
        TestMemory { id: "mem-371", summary: "Postmortem: data loss incident. A background job that archives old records accidentally deleted 50,000 active records due to a wrong date filter. Restored from the daily backup with 4-hour data gap. Added dry-run mode for all archival jobs", created_at: "2025-12-03", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-372", summary: "Incident: third-party SSO provider had 2-hour outage. Users could not log in. We had no fallback authentication method. Implemented emergency local auth bypass that can be activated by admin for SSO outages", created_at: "2025-12-10", stream_id: Some("security"), category: "incidents" },
        TestMemory { id: "mem-373", summary: "SEV1: the message queue backed up to 500,000 unprocessed messages causing a 6-hour delay in order processing. A code deployment introduced a bug that caused consumers to reject all messages. Rolled back deployment and consumers caught up in 2 hours", created_at: "2025-12-17", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-374", summary: "Alert: memory usage on the API servers reached 95%. A memory leak in the new caching layer was holding references to response objects indefinitely. Deployed fix with WeakRef-based cache and memory returned to normal within 30 minutes", created_at: "2025-12-24", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-375", summary: "Postmortem: customer-facing dashboard showed incorrect analytics data for 2 days. The data pipeline had a timezone conversion bug that shifted all timestamps by 8 hours. Corrected the pipeline and backfilled affected data", created_at: "2025-12-31", stream_id: Some("data"), category: "incidents" },
        TestMemory { id: "mem-376", summary: "Incident report: CI/CD pipeline compromised. A malicious dependency was introduced via a typosquatted npm package. The package exfiltrated environment variables including API keys. All affected keys rotated within 1 hour", created_at: "2026-01-07", stream_id: Some("security"), category: "incidents" },
        TestMemory { id: "mem-377", summary: "SEV2: webhook delivery delayed by 8 hours due to a deadlock in the webhook processor. Two threads were waiting for each other's database locks. Fixed by establishing consistent lock ordering and adding deadlock detection", created_at: "2026-01-14", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-378", summary: "Alert: Kubernetes cluster autoscaler reached maximum node count during a traffic spike. The autoscaler was configured with a max of 10 nodes but the traffic required 15. Increased max to 20 and added predictive scaling based on historical patterns", created_at: "2026-01-21", stream_id: Some("infrastructure"), category: "incidents" },
        TestMemory { id: "mem-379", summary: "Postmortem: email notifications sent to wrong recipients. A batch job used the wrong field mapping and sent personal account summaries to random email addresses. Affected 3,400 users. Notified all affected users and regulators within 72 hours", created_at: "2026-01-28", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-380", summary: "Incident: production database replica lag exceeded 5 minutes during a large batch import. Read queries returned stale data. Temporarily routed reads to primary until import completed. Added replica lag monitoring with automatic failover", created_at: "2026-02-04", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-381", summary: "SEV1: complete API outage for 45 minutes caused by a Kubernetes node failure that was running the single replica of the API gateway. Added anti-affinity rules to spread pods across nodes and increased replicas to 3", created_at: "2026-02-11", stream_id: Some("infrastructure"), category: "incidents" },
        TestMemory { id: "mem-382", summary: "Alert: unusual traffic pattern detected. 50x increase in API calls from a single IP range targeting the user search endpoint. Automated rate limiting kicked in. Investigation revealed a customer was scraping our user directory. Blocked and contacted customer", created_at: "2026-02-18", stream_id: Some("security"), category: "incidents" },
        TestMemory { id: "mem-383", summary: "Postmortem: mobile push notifications stopped working for Android users for 12 hours. The FCM server key was rotated by Google without notice. Updated the key and added FCM key expiration monitoring", created_at: "2026-02-25", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-384", summary: "Incident: gradual memory leak in the GraphQL gateway caused it to restart every 6 hours via OOM kill. A subscription handler was accumulating client state without cleanup. Added proper subscription lifecycle management", created_at: "2026-03-04", stream_id: Some("platform"), category: "incidents" },
        TestMemory { id: "mem-385", summary: "SEV2: image upload service was storing unencrypted user photos. Discovered during security audit. All 150,000 existing images encrypted in-place using server-side encryption. Encryption now enforced at the bucket policy level", created_at: "2026-03-11", stream_id: Some("security"), category: "incidents" },
        TestMemory { id: "mem-386", summary: "Alert: scheduled maintenance job ran during business hours instead of the configured 2am window. The cron timezone was set to UTC but the team expected local time. Standardized all cron schedules to UTC with clear documentation", created_at: "2026-03-18", stream_id: Some("infrastructure"), category: "incidents" },
        TestMemory { id: "mem-387", summary: "Postmortem: search rankings degraded silently over 2 weeks. The ML model serving the search relevance scores was not retrained after a schema change in the feature store. Added automated model quality monitoring with threshold alerts", created_at: "2026-03-25", stream_id: Some("data"), category: "incidents" },
        TestMemory { id: "mem-388", summary: "Incident report: rate limiter misconfiguration allowed 100x the intended request rate for premium tier customers. A config change used requests per second instead of per minute. No service degradation but cost overrun detected", created_at: "2026-04-01", stream_id: Some("platform"), category: "incidents" },
        TestMemory { id: "mem-389", summary: "SEV2: deployment rollback failed because the previous Docker image was garbage collected from the container registry. Deployment stuck in a half-deployed state for 40 minutes. Now retaining last 10 images and validating rollback target exists before starting deployment", created_at: "2025-07-25", stream_id: Some("infrastructure"), category: "incidents" },
        TestMemory { id: "mem-390", summary: "Alert: PostgreSQL WAL disk usage at 90%. Replication slot for the analytics consumer was preventing WAL cleanup because the consumer had been down for 3 days. Dropped the stale replication slot and added monitoring for inactive slots", created_at: "2025-08-25", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-391", summary: "Postmortem: A/B test showed 15% conversion improvement but was invalid. The test group had a sampling bias toward returning users. Revised the experimentation framework to use stratified sampling and added pre-experiment balance checks", created_at: "2025-09-25", stream_id: Some("data"), category: "incidents" },
        TestMemory { id: "mem-392", summary: "Incident: cross-site request forgery attack attempted against the admin panel. No data was compromised because the attacker did not have valid session cookies, but the CSRF token validation was missing on 3 endpoints. Patched all endpoints", created_at: "2025-10-25", stream_id: Some("security"), category: "incidents" },
        TestMemory { id: "mem-393", summary: "SEV1: complete data pipeline failure for 8 hours. The Flink job checkpointing was failing silently and the job eventually ran out of memory trying to hold all state. Root cause: S3 bucket for checkpoint storage had a permission change. Fixed permissions and added checkpoint health monitoring", created_at: "2025-11-25", stream_id: Some("data"), category: "incidents" },
        TestMemory { id: "mem-394", summary: "Alert: abnormal spike in 401 responses. A bot was attempting credential stuffing against the login endpoint with 100K email/password combinations per hour. Enabled IP-based rate limiting on the login endpoint and added CAPTCHA after 3 failed attempts", created_at: "2025-12-25", stream_id: Some("security"), category: "incidents" },
        TestMemory { id: "mem-395", summary: "Postmortem: the daily report email sent blank PDFs to 5000 subscribers. The PDF generation service had a font rendering issue after a container base image update. Pinned the base image version and added PDF content validation before send", created_at: "2026-01-25", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-396", summary: "Incident: Terraform state lock stuck for 6 hours preventing all infrastructure changes. A previous apply command was killed mid-execution. Manually released the DynamoDB lock and added a Slack notification for lock acquisition", created_at: "2026-02-25", stream_id: Some("infrastructure"), category: "incidents" },
        TestMemory { id: "mem-397", summary: "SEV2: GraphQL API returning 500 errors for all mutation operations. A schema deployment introduced a breaking change that removed a required field from the input type. Rolled back the schema and added backward compatibility tests", created_at: "2026-03-25", stream_id: Some("platform"), category: "incidents" },
        TestMemory { id: "mem-398", summary: "Alert: Elasticsearch response times degraded to 30 seconds. A customer had created a regex search query that caused catastrophic backtracking. Added regex query timeout of 1 second and disabled regex queries for unprivileged users", created_at: "2025-08-05", stream_id: Some("backend"), category: "incidents" },
        TestMemory { id: "mem-399", summary: "Postmortem: deployment to production went unmonitored on Friday at 5pm. The canary metrics showed elevated error rates but no one was watching. New policy: no deployments after 3pm on Fridays, canary alerts go to the on-call channel", created_at: "2025-09-05", stream_id: Some("platform"), category: "incidents" },
        TestMemory { id: "mem-400", summary: "Incident: customer API keys exposed in public GitHub repository. A developer accidentally committed a test configuration file containing production API keys. Rotated all affected keys within 30 minutes and added git pre-commit hooks for secret scanning", created_at: "2025-10-05", stream_id: Some("security"), category: "incidents" },

        // ====================================================================
        // CATEGORY 9: Security & Compliance (50)
        // ====================================================================
        TestMemory { id: "mem-401", summary: "Security audit findings: need to implement rate limiting on the login endpoint, add CSRF tokens to all state-changing forms, and upgrade from TLS 1.2 to TLS 1.3 for all external connections", created_at: "2025-07-17", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-402", summary: "Implemented API key rotation: all service-to-service API keys now rotate every 90 days automatically via HashiCorp Vault. Old keys have a 24-hour grace period before revocation", created_at: "2025-07-24", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-403", summary: "PII data handling policy: encrypt all PII at rest with AES-256, mask in application logs, purge after 2 years unless user opts in to extended retention. PII fields: name, email, phone, address, payment info", created_at: "2025-07-31", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-404", summary: "Penetration test results: found stored XSS in user profile page, blind SQL injection in search endpoint, and insecure direct object reference in the document download API. All patched within 48 hours", created_at: "2025-08-07", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-405", summary: "GDPR compliance review: implemented data subject access request workflow. Users can request all their data in JSON format, delivered within 30 days. Also implemented right to erasure with 72-hour processing SLA", created_at: "2025-08-14", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-406", summary: "SOC2 Type II audit completed successfully. Zero critical findings, two observations: need to formalize the change management process and add multi-factor authentication for all production access", created_at: "2025-08-21", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-407", summary: "Implemented content security policy headers across all web applications. Default-src self, script-src with nonce-based allowlist, no inline styles except from trusted CDN. Report-URI configured for violation monitoring", created_at: "2025-08-28", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-408", summary: "Security review of third-party dependencies: identified 3 critical CVEs in transitive dependencies. Updated axios to patch SSRF vulnerability, upgraded lodash for prototype pollution fix, replaced deprecated crypto library", created_at: "2025-09-04", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-409", summary: "Implemented database encryption at rest using AWS KMS customer-managed keys. All RDS instances now use encrypted storage. Key rotation scheduled every 365 days. Access to KMS keys restricted via IAM policies", created_at: "2025-09-11", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-410", summary: "Multi-factor authentication rolled out for all employee accounts. Supported methods: TOTP authenticator apps, hardware security keys (FIDO2), and SMS as fallback. SMS will be deprecated in 6 months", created_at: "2025-09-18", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-411", summary: "Vulnerability scanning automated in CI pipeline using Snyk and Trivy. Every PR is scanned for dependency vulnerabilities and container image CVEs. High/critical findings block merge", created_at: "2025-09-25", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-412", summary: "Implemented network segmentation: separated production, staging, and development environments into distinct VPCs. Cross-VPC access requires explicit peering connection and security group approval", created_at: "2025-10-02", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-413", summary: "CCPA compliance implementation: added do-not-sell flag to user profiles, created opt-out mechanism for data sharing, implemented 45-day response SLA for data deletion requests", created_at: "2025-10-09", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-414", summary: "Security training completed for all engineering staff: topics covered OWASP Top 10, secure coding practices, social engineering awareness, and incident response procedures. Quarterly refresher scheduled", created_at: "2025-10-16", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-415", summary: "Implemented secret scanning in all Git repositories using GitHub Advanced Security. Detects API keys, passwords, certificates, and tokens before they reach the remote repository. 15 secrets found and rotated in existing repos", created_at: "2025-10-23", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-416", summary: "Access review completed: revoked 23 unused IAM roles, 15 stale SSH keys, and 8 API keys belonging to former employees. Implementing quarterly automated access reviews going forward", created_at: "2025-10-30", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-417", summary: "Implemented Web Application Firewall rules: blocking SQL injection patterns, XSS payloads, path traversal attempts, and known malicious user agents. Custom rules for our specific API patterns", created_at: "2025-11-06", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-418", summary: "Data loss prevention policy implemented: monitoring outbound network traffic for patterns matching credit card numbers, SSNs, and other PII. Alerts sent to security team for investigation", created_at: "2025-11-13", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-419", summary: "Implemented SAML-based SSO for enterprise customers. Identity providers supported: Okta, Azure AD, Google Workspace. SCIM provisioning for automated user lifecycle management", created_at: "2025-11-20", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-420", summary: "Security incident response plan updated: defined severity levels, escalation paths, communication templates, and post-incident review process. Tabletop exercise conducted with 12 participants", created_at: "2025-11-27", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-421", summary: "Implemented RBAC with attribute-based access control for fine-grained permissions. Roles: viewer, editor, admin, owner. Resources: project, team, billing, settings. Evaluated and denied all cross-tenant access patterns", created_at: "2025-12-04", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-422", summary: "Container image hardening: all production images based on distroless, no shell access, non-root user, read-only filesystem. Security scanning with Trivy on build and admission controller on deploy", created_at: "2025-12-11", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-423", summary: "Bug bounty program launched: paying $500-$5000 for valid security vulnerabilities. Using HackerOne platform. In first month received 47 reports, 3 valid high-severity findings rewarded", created_at: "2025-12-18", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-424", summary: "Implemented certificate pinning in the mobile application to prevent MITM attacks. Pins are for the intermediate CA, not the leaf certificate, to allow certificate rotation without app updates", created_at: "2025-12-25", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-425", summary: "Data retention audit completed: identified 3TB of data beyond retention period. Automated purge of expired data with compliance team sign-off. Quarterly data retention review scheduled", created_at: "2026-01-01", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-426", summary: "Implemented IP allowlisting for admin API access. Only office IP ranges and VPN exit nodes can reach admin endpoints. Emergency bypass available via hardware security key authentication", created_at: "2026-01-08", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-427", summary: "Security review of the microservices communication: all internal traffic now encrypted with mTLS. Service identities managed by SPIFFE/SPIRE. Authorization policies enforce least-privilege access between services", created_at: "2026-01-15", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-428", summary: "Compliance dashboard implemented: real-time view of SOC2 control status, open vulnerabilities by severity, access review completion percentage, and encryption coverage across all data stores", created_at: "2026-01-22", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-429", summary: "Implemented API request signing for webhook deliveries. Each webhook includes HMAC-SHA256 signature computed over the payload with a per-customer secret key. Receivers verify signature before processing", created_at: "2026-01-29", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-430", summary: "Privacy impact assessment completed for the new recommendation engine. Determined that user behavior data must be anonymized before training. Implemented k-anonymity with k=50 for training datasets", created_at: "2026-02-05", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-431", summary: "Deployed runtime application self-protection for Java services: SQL injection detection, command injection prevention, path traversal blocking, and deserialization attack mitigation at the application layer", created_at: "2026-02-12", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-432", summary: "Implemented key management system using AWS KMS with envelope encryption. Data keys are generated per-record, encrypted with a master key, and stored alongside the encrypted data. Master key rotation every 90 days", created_at: "2026-02-19", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-433", summary: "Security architecture review of the event-driven system: identified that Kafka messages containing PII were not encrypted in transit. Enabled TLS for all Kafka connections and added message-level encryption for PII events", created_at: "2026-02-26", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-434", summary: "Implemented automated compliance evidence collection: daily snapshots of IAM policies, security group rules, encryption configurations, and access logs. Stored in tamper-evident S3 bucket for audit purposes", created_at: "2026-03-05", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-435", summary: "Zero-trust access model implemented: no implicit trust based on network location. Every request authenticated and authorized regardless of source. Context-aware access decisions based on device posture, location, and time", created_at: "2026-03-12", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-436", summary: "Implemented data masking for non-production environments. All PII is replaced with realistic fake data using a deterministic masking function. Production data never copied to dev or staging without masking", created_at: "2026-03-19", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-437", summary: "Supply chain security hardening: signed all container images with cosign, implemented SLSA Level 2 for build provenance, pinned all base images to digest instead of tag, enabled Dependabot with auto-merge for patch versions", created_at: "2026-03-26", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-438", summary: "Implemented encryption of secrets in transit: all secrets fetched from Vault are encrypted with a session key before transmission, even over TLS, as defense-in-depth against TLS interception", created_at: "2025-07-20", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-439", summary: "Security assessment of the API gateway: identified 5 findings including missing rate limiting on auth endpoints, overly permissive CORS policy, and no request size limits. All remediated within 2 weeks", created_at: "2025-08-20", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-440", summary: "ISO 27001 certification preparation: completed asset inventory, risk assessment, and statement of applicability. 114 controls mapped, 97 applicable and implemented, 17 not applicable with documented justification", created_at: "2025-09-20", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-441", summary: "Implemented cross-origin resource sharing (CORS) lockdown: removed wildcard origins in production, created allowlist of known client domains, added preflight caching for 24 hours to reduce OPTIONS request overhead", created_at: "2025-10-20", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-442", summary: "Vendor security assessment program launched: all third-party vendors handling customer data must complete security questionnaire, provide SOC2 report, and agree to data processing agreement before integration", created_at: "2025-11-20", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-443", summary: "Implemented session management hardening: absolute timeout 8 hours, idle timeout 30 minutes, session invalidation on password change, concurrent session limit of 5, session binding to user agent and IP range", created_at: "2025-12-20", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-444", summary: "HIPAA compliance assessment for healthcare vertical: identified 12 gaps in technical safeguards. Priority items: audit controls for PHI access, encryption for data in transit between services, and emergency access procedure", created_at: "2026-01-20", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-445", summary: "Implemented HTTP security headers across all services: Strict-Transport-Security with max-age 2 years and includeSubDomains, X-Content-Type-Options nosniff, X-Frame-Options DENY, Referrer-Policy strict-origin-when-cross-origin", created_at: "2026-02-20", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-446", summary: "Red team exercise completed: team simulated advanced persistent threat targeting customer data. Found 2 privilege escalation paths via service account key exposure and unpatched internal service. Both paths closed", created_at: "2026-03-20", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-447", summary: "Implemented API security gateway with OAuth2 token introspection, JWT validation, request signing verification, and payload schema validation. Replaces the custom middleware approach in individual services", created_at: "2025-08-10", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-448", summary: "Data privacy impact assessment for the new analytics pipeline: determined that aggregated behavioral data requires consent under GDPR legitimate interest basis. Implemented consent management UI and backend tracking", created_at: "2025-09-10", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-449", summary: "Implemented intrusion detection system for the Kubernetes cluster: Falco monitors system calls, detects container escapes, privilege escalation, and suspicious network connections. Alerts routed to security team Slack channel", created_at: "2025-10-10", stream_id: Some("security"), category: "security" },
        TestMemory { id: "mem-450", summary: "PCI DSS Level 1 compliance achieved: cardholder data environment isolated, all traffic encrypted, access logging complete, quarterly vulnerability scans passed. Annual assessment scheduled for Q1 2026", created_at: "2025-11-10", stream_id: Some("security"), category: "security" },

        // ====================================================================
        // CATEGORY 10: Onboarding & Docs (50)
        // ====================================================================
        TestMemory { id: "mem-451", summary: "New engineer guide: local development environment setup requires Docker, Node.js 18+, Python 3.11, and Rust toolchain. Run make setup to install all dependencies and seed the database with test data", created_at: "2025-07-19", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-452", summary: "Architecture overview for new team members: the system is composed of 12 microservices communicating via gRPC and Kafka events. The API gateway handles authentication and routes to the appropriate service", created_at: "2025-07-26", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-453", summary: "Database schema documentation: the core tables are users, organizations, projects, tasks, and comments. Each table uses UUID primary keys and includes created_at and updated_at timestamps with automatic triggers", created_at: "2025-08-02", stream_id: Some("backend"), category: "onboarding" },
        TestMemory { id: "mem-454", summary: "API documentation updated: all endpoints now have OpenAPI 3.1 specs with request/response examples. Interactive documentation available at /docs. Authentication section explains JWT flow and API key usage", created_at: "2025-08-09", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-455", summary: "Deployment guide: production deployments use ArgoCD with GitOps. Merge to main triggers staging deploy. Production deploy requires creating a release tag. Rollback is automatic on health check failure", created_at: "2025-08-16", stream_id: Some("infrastructure"), category: "onboarding" },
        TestMemory { id: "mem-456", summary: "Monitoring and alerting guide: Grafana dashboards at monitoring.internal. Key dashboards: service overview, database performance, API latency. PagerDuty integration for SEV1/SEV2 alerts", created_at: "2025-08-23", stream_id: Some("infrastructure"), category: "onboarding" },
        TestMemory { id: "mem-457", summary: "Testing strategy documentation: unit tests use Jest, integration tests use Supertest with a test database, E2E tests use Playwright against staging. Run npm test for unit tests, npm run test:integration for integration tests", created_at: "2025-08-30", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-458", summary: "Code style guide: TypeScript strict mode enabled, ESLint with Airbnb config, Prettier for formatting. Import order: external packages, internal modules, relative imports. No default exports except for React components", created_at: "2025-09-06", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-459", summary: "Git workflow documentation: trunk-based development with short-lived feature branches. Branch naming: feat/JIRA-123-description, fix/JIRA-456-description. Squash merge to main. Commit message follows Conventional Commits", created_at: "2025-09-13", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-460", summary: "Incident response runbook: page the on-call engineer, join the war room Slack channel, assess severity, communicate status every 30 minutes, resolve and write postmortem within 48 hours for SEV1/SEV2", created_at: "2025-09-20", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-461", summary: "Service catalog documentation: all services registered in Backstage with owner, team, dependencies, SLO targets, and runbook links. Each service has a README with architecture diagram and API documentation", created_at: "2025-09-27", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-462", summary: "Frontend architecture guide: Next.js with App Router, React Query for server state, Zustand for client state. Components organized by feature, shared UI in packages/ui. Storybook for component development", created_at: "2025-10-04", stream_id: Some("frontend"), category: "onboarding" },
        TestMemory { id: "mem-463", summary: "Backend service template: every new service starts from the service-template repo. Includes health check endpoint, structured logging, OpenTelemetry instrumentation, Dockerfile, Helm chart, and CI pipeline", created_at: "2025-10-11", stream_id: Some("backend"), category: "onboarding" },
        TestMemory { id: "mem-464", summary: "Data engineering onboarding: our data stack is Kafka for ingestion, Flink for real-time processing, S3 as the data lake, dbt for transformations, and BigQuery as the warehouse. Airflow orchestrates batch jobs", created_at: "2025-10-18", stream_id: Some("data"), category: "onboarding" },
        TestMemory { id: "mem-465", summary: "Security onboarding checklist: set up 2FA, review access permissions, complete security training module, read the incident response plan, and verify your development environment does not contain production secrets", created_at: "2025-10-25", stream_id: Some("security"), category: "onboarding" },
        TestMemory { id: "mem-466", summary: "Infrastructure documentation: AWS account structure uses separate accounts for prod, staging, and dev via AWS Organizations. Terraform manages all infrastructure. State stored in S3 with DynamoDB locking", created_at: "2025-11-01", stream_id: Some("infrastructure"), category: "onboarding" },
        TestMemory { id: "mem-467", summary: "API versioning documentation: current version is v2 for all public endpoints. v1 is deprecated and will be removed June 2026. Internal APIs are unversioned and use protocol buffers for backward compatibility", created_at: "2025-11-08", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-468", summary: "Database migration guide: use Flyway for migrations. Files named V{number}__{description}.sql in the migrations/ directory. Test against production snapshot in staging before applying. Never modify applied migrations", created_at: "2025-11-15", stream_id: Some("backend"), category: "onboarding" },
        TestMemory { id: "mem-469", summary: "Monitoring guide for new engineers: key metrics to watch are request rate, error rate, latency percentiles, and saturation metrics like CPU/memory/disk. Dashboard links bookmarked in the team wiki", created_at: "2025-11-22", stream_id: Some("infrastructure"), category: "onboarding" },
        TestMemory { id: "mem-470", summary: "Feature flag documentation: we use LaunchDarkly for all feature toggles. Flags are created in the admin UI with a description and owner. Targeting rules allow percentage rollout, user segment targeting, and kill switch behavior", created_at: "2025-11-29", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-471", summary: "Performance testing guide: use k6 for load testing. Test scripts are in the perf-tests/ directory. Run against staging with production-like traffic patterns. Baseline: 1000 RPS with p99 under 500ms", created_at: "2025-12-06", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-472", summary: "Debugging guide for production issues: start with the Grafana dashboard for the affected service, check error logs in Loki, trace the request through Jaeger using the trace_id from the error log", created_at: "2025-12-13", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-473", summary: "Onboarding buddy system documentation: each new hire is assigned a buddy from their team. Buddies help with setup, answer questions, and pair on first tasks. Weekly check-in for the first 90 days", created_at: "2025-12-20", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-474", summary: "Release process documentation: release candidate cut every Thursday, QA sign-off by Friday noon, production deploy Monday morning. Hotfix process: branch from main, cherry-pick fix, deploy with expedited QA", created_at: "2025-12-27", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-475", summary: "Mobile development setup guide: Xcode 15+ for iOS, Android Studio Hedgehog for Android. Run fastlane setup to configure signing certificates and provisioning profiles. Emulators and simulators are configured via the setup script", created_at: "2026-01-03", stream_id: Some("frontend"), category: "onboarding" },
        TestMemory { id: "mem-476", summary: "Service ownership documentation: every service has a primary owner team and a secondary team. Owner responsibilities include on-call, monitoring, dependency upgrades, and capacity planning. Ownership tracked in Backstage", created_at: "2026-01-10", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-477", summary: "Kubernetes debugging guide: use kubectl logs for container logs, kubectl describe for events, kubectl exec for shell access. For networking issues use kubectl port-forward and tcpdump in the debug container", created_at: "2026-01-17", stream_id: Some("infrastructure"), category: "onboarding" },
        TestMemory { id: "mem-478", summary: "Data privacy training documentation: all engineers must complete annual privacy training. Topics: data classification (public, internal, confidential, restricted), PII handling, GDPR rights, and data breach notification requirements", created_at: "2026-01-24", stream_id: Some("security"), category: "onboarding" },
        TestMemory { id: "mem-479", summary: "GraphQL development guide: use Apollo Server for the gateway, type-graphql for service subgraphs. Schema-first development: write the schema, generate types, then implement resolvers. Federation docs in the wiki", created_at: "2026-01-31", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-480", summary: "Local development troubleshooting FAQ: Docker containers not starting - check Docker Desktop memory allocation (minimum 8GB), port conflicts - run make ports to see what is using common ports, database errors - run make db-reset", created_at: "2026-02-07", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-481", summary: "Team communication norms: async-first communication via Slack. Use threads for discussions. Channels: #eng-general for announcements, #eng-help for questions, #incidents for active incidents. No DMs for technical decisions", created_at: "2026-02-14", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-482", summary: "Continuous integration documentation: GitHub Actions runs on every PR. Pipeline stages: lint, type check, unit tests, build, integration tests. All stages must pass for merge. Workflow files in .github/workflows/", created_at: "2026-02-21", stream_id: Some("infrastructure"), category: "onboarding" },
        TestMemory { id: "mem-483", summary: "Service mesh documentation for new engineers: Istio handles mTLS, traffic management, and observability between services. VirtualService resources control routing. DestinationRule resources control circuit breaking", created_at: "2026-02-28", stream_id: Some("infrastructure"), category: "onboarding" },
        TestMemory { id: "mem-484", summary: "Design system documentation: component library at storybook.internal. Tokens for color, spacing, typography defined in packages/tokens. All new UI must use design system components. Custom CSS only with design team approval", created_at: "2026-03-07", stream_id: Some("frontend"), category: "onboarding" },
        TestMemory { id: "mem-485", summary: "Secrets management guide: never store secrets in code or environment files. Use AWS Secrets Manager for runtime secrets, Sealed Secrets for Kubernetes deployments. Rotate credentials quarterly or immediately after exposure", created_at: "2026-03-14", stream_id: Some("security"), category: "onboarding" },
        TestMemory { id: "mem-486", summary: "Technical interview process documentation: phone screen (45 min system design), onsite day 1 (coding exercise, architecture deep dive), onsite day 2 (cultural fit, team presentation). Rubric in the hiring wiki", created_at: "2026-03-21", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-487", summary: "Database access documentation: production database access requires VPN plus IAM authentication. Read access via read replica endpoint. Write access restricted to service accounts only. Human write access requires break-glass procedure", created_at: "2026-03-28", stream_id: Some("backend"), category: "onboarding" },
        TestMemory { id: "mem-488", summary: "Cost management documentation: each team has a monthly AWS budget allocation. Use cost allocation tags on all resources. Review weekly cost report in the #cost-optimization channel. Spike alerts at 80% of monthly budget", created_at: "2025-07-22", stream_id: Some("infrastructure"), category: "onboarding" },
        TestMemory { id: "mem-489", summary: "Microservices communication patterns guide: synchronous calls via gRPC with deadline propagation, asynchronous events via Kafka for eventual consistency. Prefer events for cross-domain communication, gRPC for intra-domain queries", created_at: "2025-08-22", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-490", summary: "Error handling documentation: all services use structured error types with error codes, HTTP status mapping, and user-friendly messages. Error hierarchy: ServiceError > DomainError > InfrastructureError. Log stack traces for 5xx only", created_at: "2025-09-22", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-491", summary: "Observability onboarding: instrument new services with OpenTelemetry SDK. Auto-instrumentation covers HTTP, gRPC, and database calls. Add custom spans for business-critical operations. Export to Grafana Cloud via OTLP", created_at: "2025-10-22", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-492", summary: "Pull request review guide: reviewer should check for correctness, security, performance, test coverage, and documentation. Use conventional comments (suggestion, nitpick, issue, question). Approve with minor suggestions, request changes for blockers", created_at: "2025-11-22", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-493", summary: "Sprint ceremony documentation: planning on Monday (2 hours), daily standup at 10am (15 minutes async in Slack), mid-sprint check Thursday (30 minutes), demo and retro on Friday (1 hour each)", created_at: "2025-12-22", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-494", summary: "Backend development guide: all services use TypeScript with strict mode. Database interactions via Prisma ORM. API handlers follow controller-service-repository pattern. Business logic in the service layer only", created_at: "2026-01-22", stream_id: Some("backend"), category: "onboarding" },
        TestMemory { id: "mem-495", summary: "Chaos engineering guide for new SREs: start with Gremlin experiments in staging. First experiment: pod kill for stateless services. Graduate to CPU stress, network partition, and disk fill. Document results in the chaos log", created_at: "2026-02-22", stream_id: Some("infrastructure"), category: "onboarding" },
        TestMemory { id: "mem-496", summary: "Analytics implementation guide: use Segment for event tracking, events forwarded to Mixpanel for product analytics and BigQuery for data warehouse. Event naming convention: object_action (e.g., user_signed_up, project_created)", created_at: "2026-03-22", stream_id: Some("data"), category: "onboarding" },
        TestMemory { id: "mem-497", summary: "Environment management documentation: three environments - dev (auto-deploy on push), staging (deploy on merge to main), production (deploy on release tag). Environment-specific configs in config/env/ directory", created_at: "2025-08-05", stream_id: Some("infrastructure"), category: "onboarding" },
        TestMemory { id: "mem-498", summary: "API design guidelines: use RESTful conventions, resource-based URLs, HTTP methods for CRUD, consistent error responses, pagination for list endpoints, HATEOAS links for discoverability. OpenAPI spec required before implementation", created_at: "2025-09-05", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-499", summary: "Logging best practices guide: log at appropriate levels (ERROR for failures requiring action, WARN for degradation, INFO for business events, DEBUG for troubleshooting). Include trace_id in every log. Never log credentials or PII", created_at: "2025-10-05", stream_id: Some("platform"), category: "onboarding" },
        TestMemory { id: "mem-500", summary: "First week checklist for new engineers: Day 1 - access setup and team intro, Day 2 - local environment setup, Day 3 - codebase walkthrough with buddy, Day 4 - first PR (documentation fix), Day 5 - first code review", created_at: "2025-11-05", stream_id: Some("platform"), category: "onboarding" },
    ]
}

// ============================================================================
// TEST CASES: 100 QUERIES ACROSS 8 CATEGORIES
// ============================================================================

fn build_test_cases() -> Vec<TestCase> {
    vec![
        // ====================================================================
        // CATEGORY 1: Direct Keyword (20 queries)
        // ====================================================================
        TestCase {
            query: "GraphQL API migration decision",
            expected_memory_ids: vec!["mem-001"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "PostgreSQL MongoDB database decision",
            expected_memory_ids: vec!["mem-002"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "Clerk Auth0 authentication decision",
            expected_memory_ids: vec!["mem-004"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "Kubernetes Docker Swarm container orchestration",
            expected_memory_ids: vec!["mem-006"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "Kafka event bus topic configuration",
            expected_memory_ids: vec!["mem-008", "mem-306"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "JWT token verification timeout fix",
            expected_memory_ids: vec!["mem-051"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "database connection pool exhaustion",
            expected_memory_ids: vec!["mem-052", "mem-311"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "payment processing race condition double charging",
            expected_memory_ids: vec!["mem-054"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "WebSocket memory leak event listener cleanup",
            expected_memory_ids: vec!["mem-053"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "Q1 migration milestones schema dual-write cutover",
            expected_memory_ids: vec!["mem-101"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "SOC2 audit preparation documentation",
            expected_memory_ids: vec!["mem-102", "mem-406"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "PR review middleware error handling",
            expected_memory_ids: vec!["mem-151"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "DAO layer repository pattern refactoring",
            expected_memory_ids: vec!["mem-152"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "Node.js memory leak Promise orphaned",
            expected_memory_ids: vec!["mem-201"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "DNS resolution timeout latency API",
            expected_memory_ids: vec!["mem-202"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "Redis caching implementation TTL eviction",
            expected_memory_ids: vec!["mem-005", "mem-309"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "Terraform infrastructure as code Pulumi",
            expected_memory_ids: vec!["mem-013"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "Docker image multi-stage build distroless",
            expected_memory_ids: vec!["mem-304"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "Prometheus monitoring alerting scrape interval",
            expected_memory_ids: vec!["mem-305"],
            category: "keyword",
            difficulty: "easy",
        },
        TestCase {
            query: "certificate expiration outage API gateway",
            expected_memory_ids: vec!["mem-352"],
            category: "keyword",
            difficulty: "easy",
        },
        // ====================================================================
        // CATEGORY 2: Paraphrase (15 queries)
        // ====================================================================
        TestCase {
            query: "login system change provider switch",
            expected_memory_ids: vec!["mem-004"],
            category: "paraphrase",
            difficulty: "medium",
        },
        TestCase {
            query: "choosing a relational database for money data",
            expected_memory_ids: vec!["mem-002"],
            category: "paraphrase",
            difficulty: "medium",
        },
        TestCase {
            query: "container packaging optimization smaller images",
            expected_memory_ids: vec!["mem-304"],
            category: "paraphrase",
            difficulty: "medium",
        },
        TestCase {
            query: "server cluster management scaling solution",
            expected_memory_ids: vec!["mem-006"],
            category: "paraphrase",
            difficulty: "medium",
        },
        TestCase {
            query: "message broker evaluation for events",
            expected_memory_ids: vec!["mem-008"],
            category: "paraphrase",
            difficulty: "medium",
        },
        TestCase {
            query: "concurrent transaction safety in orders",
            expected_memory_ids: vec!["mem-054"],
            category: "paraphrase",
            difficulty: "medium",
        },
        TestCase {
            query: "scheduled task wrong time daylight saving",
            expected_memory_ids: vec!["mem-236"],
            category: "paraphrase",
            difficulty: "medium",
        },
        TestCase {
            query: "web connection dropping every minute",
            expected_memory_ids: vec!["mem-210"],
            category: "paraphrase",
            difficulty: "medium",
        },
        TestCase {
            query: "app store rating crash free launch plan",
            expected_memory_ids: vec!["mem-104"],
            category: "paraphrase",
            difficulty: "medium",
        },
        TestCase {
            query: "code quality guidelines pull request size limit",
            expected_memory_ids: vec!["mem-261"],
            category: "paraphrase",
            difficulty: "medium",
        },
        TestCase {
            query: "new hire first week setup walkthrough",
            expected_memory_ids: vec!["mem-500"],
            category: "paraphrase",
            difficulty: "medium",
        },
        TestCase {
            query: "breaking up the big app into smaller services",
            expected_memory_ids: vec!["mem-123", "mem-146"],
            category: "paraphrase",
            difficulty: "medium",
        },
        TestCase {
            query: "tracking user behavior experiments feature testing",
            expected_memory_ids: vec!["mem-349"],
            category: "paraphrase",
            difficulty: "medium",
        },
        TestCase {
            query: "getting data out of the system for users GDPR portability",
            expected_memory_ids: vec!["mem-136", "mem-405"],
            category: "paraphrase",
            difficulty: "medium",
        },
        TestCase {
            query: "fixing the slow build pipeline parallel tests",
            expected_memory_ids: vec!["mem-253", "mem-174"],
            category: "paraphrase",
            difficulty: "medium",
        },
        // ====================================================================
        // CATEGORY 3: Conceptual/Semantic (15 queries)
        // ====================================================================
        TestCase {
            query: "improving API response times",
            expected_memory_ids: vec!["mem-329", "mem-108"],
            category: "semantic",
            difficulty: "hard",
        },
        TestCase {
            query: "keeping customer data safe",
            expected_memory_ids: vec!["mem-403", "mem-409"],
            category: "semantic",
            difficulty: "hard",
        },
        TestCase {
            query: "making the system more reliable",
            expected_memory_ids: vec!["mem-138", "mem-020"],
            category: "semantic",
            difficulty: "hard",
        },
        TestCase {
            query: "reducing infrastructure costs",
            expected_memory_ids: vec!["mem-120"],
            category: "semantic",
            difficulty: "hard",
        },
        TestCase {
            query: "handling traffic spikes gracefully",
            expected_memory_ids: vec!["mem-378", "mem-066"],
            category: "semantic",
            difficulty: "hard",
        },
        TestCase {
            query: "preventing data corruption in concurrent access",
            expected_memory_ids: vec!["mem-059", "mem-061"],
            category: "semantic",
            difficulty: "hard",
        },
        TestCase {
            query: "making deployments safer",
            expected_memory_ids: vec!["mem-041", "mem-293"],
            category: "semantic",
            difficulty: "hard",
        },
        TestCase {
            query: "protecting against web attacks",
            expected_memory_ids: vec!["mem-417", "mem-407"],
            category: "semantic",
            difficulty: "hard",
        },
        TestCase {
            query: "developer productivity and tooling",
            expected_memory_ids: vec!["mem-147", "mem-043"],
            category: "semantic",
            difficulty: "hard",
        },
        TestCase {
            query: "testing strategy for quality assurance",
            expected_memory_ids: vec!["mem-128", "mem-457"],
            category: "semantic",
            difficulty: "hard",
        },
        TestCase {
            query: "how services talk to each other",
            expected_memory_ids: vec!["mem-007", "mem-489"],
            category: "semantic",
            difficulty: "hard",
        },
        TestCase {
            query: "managing user permissions and access",
            expected_memory_ids: vec!["mem-421"],
            category: "semantic",
            difficulty: "hard",
        },
        TestCase {
            query: "getting new team members up to speed",
            expected_memory_ids: vec!["mem-451", "mem-473"],
            category: "semantic",
            difficulty: "hard",
        },
        TestCase {
            query: "ensuring our system handles failures",
            expected_memory_ids: vec!["mem-116", "mem-361"],
            category: "semantic",
            difficulty: "hard",
        },
        TestCase {
            query: "tracking what goes wrong in production",
            expected_memory_ids: vec!["mem-338", "mem-472"],
            category: "semantic",
            difficulty: "hard",
        },
        // ====================================================================
        // CATEGORY 4: Multi-hop (10 queries)
        // ====================================================================
        TestCase {
            query: "what technology decisions affected the mobile team",
            expected_memory_ids: vec!["mem-001", "mem-009"],
            category: "multi_hop",
            difficulty: "hard",
        },
        TestCase {
            query: "security improvements since the penetration test",
            expected_memory_ids: vec!["mem-404", "mem-417"],
            category: "multi_hop",
            difficulty: "hard",
        },
        TestCase {
            query: "how did we fix database performance problems",
            expected_memory_ids: vec!["mem-052", "mem-311"],
            category: "multi_hop",
            difficulty: "hard",
        },
        TestCase {
            query: "what changes were made to the authentication flow",
            expected_memory_ids: vec!["mem-004", "mem-051"],
            category: "multi_hop",
            difficulty: "hard",
        },
        TestCase {
            query: "all Kubernetes-related decisions and incidents",
            expected_memory_ids: vec!["mem-006", "mem-381"],
            category: "multi_hop",
            difficulty: "hard",
        },
        TestCase {
            query: "how did the payment system evolve",
            expected_memory_ids: vec!["mem-054", "mem-354"],
            category: "multi_hop",
            difficulty: "hard",
        },
        TestCase {
            query: "frontend performance work and outcomes",
            expected_memory_ids: vec!["mem-142", "mem-204"],
            category: "multi_hop",
            difficulty: "hard",
        },
        TestCase {
            query: "Elasticsearch problems and solutions",
            expected_memory_ids: vec!["mem-355", "mem-211"],
            category: "multi_hop",
            difficulty: "hard",
        },
        TestCase {
            query: "data pipeline architecture and incidents",
            expected_memory_ids: vec!["mem-322", "mem-393"],
            category: "multi_hop",
            difficulty: "hard",
        },
        TestCase {
            query: "compliance requirements across GDPR SOC2 PCI",
            expected_memory_ids: vec!["mem-405", "mem-406", "mem-450"],
            category: "multi_hop",
            difficulty: "hard",
        },
        // ====================================================================
        // CATEGORY 5: Temporal (10 queries)
        // ====================================================================
        TestCase {
            query: "decisions made in January 2026",
            expected_memory_ids: vec!["mem-026", "mem-028"],
            category: "temporal",
            difficulty: "medium",
        },
        TestCase {
            query: "what happened in August 2025",
            expected_memory_ids: vec!["mem-004", "mem-008"],
            category: "temporal",
            difficulty: "medium",
        },
        TestCase {
            query: "incidents from October 2025",
            expected_memory_ids: vec!["mem-363", "mem-365"],
            category: "temporal",
            difficulty: "medium",
        },
        TestCase {
            query: "bug fixes from September 2025",
            expected_memory_ids: vec!["mem-058", "mem-059", "mem-061"],
            category: "temporal",
            difficulty: "medium",
        },
        TestCase {
            query: "planning discussions from November 2025",
            expected_memory_ids: vec!["mem-118", "mem-119", "mem-120"],
            category: "temporal",
            difficulty: "medium",
        },
        TestCase {
            query: "security work in March 2026",
            expected_memory_ids: vec!["mem-435", "mem-436", "mem-437"],
            category: "temporal",
            difficulty: "medium",
        },
        TestCase {
            query: "code reviews from December 2025",
            expected_memory_ids: vec!["mem-173", "mem-174", "mem-175"],
            category: "temporal",
            difficulty: "medium",
        },
        TestCase {
            query: "recent architecture decisions from February 2026",
            expected_memory_ids: vec!["mem-031", "mem-032", "mem-033"],
            category: "temporal",
            difficulty: "medium",
        },
        TestCase {
            query: "onboarding updates from early 2026",
            expected_memory_ids: vec!["mem-477", "mem-479", "mem-480"],
            category: "temporal",
            difficulty: "medium",
        },
        TestCase {
            query: "what was discussed in the last quarter",
            expected_memory_ids: vec!["mem-384", "mem-397"],
            category: "temporal",
            difficulty: "medium",
        },
        // ====================================================================
        // CATEGORY 6: Adversarial/Vague (10 queries)
        // ====================================================================
        TestCase {
            query: "that thing we talked about last time",
            expected_memory_ids: vec![],
            category: "adversarial",
            difficulty: "extreme",
        },
        TestCase {
            query: "the bug",
            expected_memory_ids: vec![],
            category: "adversarial",
            difficulty: "extreme",
        },
        TestCase {
            query: "performance",
            expected_memory_ids: vec![],
            category: "adversarial",
            difficulty: "extreme",
        },
        TestCase {
            query: "the decision",
            expected_memory_ids: vec![],
            category: "adversarial",
            difficulty: "extreme",
        },
        TestCase {
            query: "what happened",
            expected_memory_ids: vec![],
            category: "adversarial",
            difficulty: "extreme",
        },
        TestCase {
            query: "fix the thing",
            expected_memory_ids: vec![],
            category: "adversarial",
            difficulty: "extreme",
        },
        TestCase {
            query: "something about security",
            expected_memory_ids: vec![],
            category: "adversarial",
            difficulty: "extreme",
        },
        TestCase {
            query: "that meeting",
            expected_memory_ids: vec![],
            category: "adversarial",
            difficulty: "extreme",
        },
        TestCase {
            query: "stuff from before",
            expected_memory_ids: vec![],
            category: "adversarial",
            difficulty: "extreme",
        },
        TestCase {
            query: "the problem we had",
            expected_memory_ids: vec![],
            category: "adversarial",
            difficulty: "extreme",
        },
        // ====================================================================
        // CATEGORY 7: Negation/Contrast (10 queries)
        // ====================================================================
        TestCase {
            query: "why we didn't choose MongoDB",
            expected_memory_ids: vec!["mem-002"],
            category: "negation",
            difficulty: "hard",
        },
        TestCase {
            query: "alternatives we rejected for message queue",
            expected_memory_ids: vec!["mem-008"],
            category: "negation",
            difficulty: "hard",
        },
        TestCase {
            query: "why not Docker Swarm for containers",
            expected_memory_ids: vec!["mem-006"],
            category: "negation",
            difficulty: "hard",
        },
        TestCase {
            query: "reasons against using Auth0",
            expected_memory_ids: vec!["mem-004"],
            category: "negation",
            difficulty: "hard",
        },
        TestCase {
            query: "why GraphQL subscriptions were rejected",
            expected_memory_ids: vec!["mem-016"],
            category: "negation",
            difficulty: "hard",
        },
        TestCase {
            query: "why we moved away from Jenkins",
            expected_memory_ids: vec!["mem-017"],
            category: "negation",
            difficulty: "hard",
        },
        TestCase {
            query: "why not Pulumi for infrastructure",
            expected_memory_ids: vec!["mem-013"],
            category: "negation",
            difficulty: "hard",
        },
        TestCase {
            query: "what we decided against for the API gateway",
            expected_memory_ids: vec!["mem-014"],
            category: "negation",
            difficulty: "hard",
        },
        TestCase {
            query: "why Memcached was not chosen for caching",
            expected_memory_ids: vec!["mem-005"],
            category: "negation",
            difficulty: "hard",
        },
        TestCase {
            query: "drawbacks of the previous REST approach",
            expected_memory_ids: vec!["mem-001", "mem-269"],
            category: "negation",
            difficulty: "hard",
        },
        // ====================================================================
        // CATEGORY 8: Incident/Debugging (10 queries)
        // ====================================================================
        TestCase {
            query: "production outage root cause",
            expected_memory_ids: vec!["mem-351", "mem-361"],
            category: "incident",
            difficulty: "medium",
        },
        TestCase {
            query: "memory leak investigation",
            expected_memory_ids: vec!["mem-201", "mem-053"],
            category: "incident",
            difficulty: "medium",
        },
        TestCase {
            query: "customer data exposed security breach",
            expected_memory_ids: vec!["mem-356"],
            category: "incident",
            difficulty: "medium",
        },
        TestCase {
            query: "double billing charging customers twice",
            expected_memory_ids: vec!["mem-363"],
            category: "incident",
            difficulty: "medium",
        },
        TestCase {
            query: "Kafka consumer rebalancing storm",
            expected_memory_ids: vec!["mem-359", "mem-224"],
            category: "incident",
            difficulty: "medium",
        },
        TestCase {
            query: "SSL TLS certificate problem",
            expected_memory_ids: vec!["mem-352", "mem-370"],
            category: "incident",
            difficulty: "medium",
        },
        TestCase {
            query: "API keys leaked in source code",
            expected_memory_ids: vec!["mem-400", "mem-376"],
            category: "incident",
            difficulty: "medium",
        },
        TestCase {
            query: "slow database queries causing timeouts",
            expected_memory_ids: vec!["mem-365", "mem-353"],
            category: "incident",
            difficulty: "medium",
        },
        TestCase {
            query: "deployment rollback failure",
            expected_memory_ids: vec!["mem-389"],
            category: "incident",
            difficulty: "medium",
        },
        TestCase {
            query: "data loss accidental deletion",
            expected_memory_ids: vec!["mem-371"],
            category: "incident",
            difficulty: "medium",
        },
    ]
}

// ============================================================================
// METRICS COMPUTATION
// ============================================================================

fn compute_mrr(results: &[(Vec<String>, &TestCase)]) -> f64 {
    let mut rr_sum = 0.0;
    let mut count = 0;

    for (result_ids, tc) in results {
        if tc.expected_memory_ids.is_empty() {
            continue; // Skip adversarial queries with no expected results
        }
        let mut best_rank = None;
        for exp_id in &tc.expected_memory_ids {
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
        0.0
    } else {
        rr_sum / count as f64
    }
}

fn compute_recall_at_k(results: &[(Vec<String>, &TestCase)], k: usize) -> f64 {
    let mut total_expected = 0;
    let mut total_found = 0;

    for (result_ids, tc) in results {
        if tc.expected_memory_ids.is_empty() {
            continue;
        }
        let top_k: Vec<&String> = result_ids.iter().take(k).collect();
        for exp_id in &tc.expected_memory_ids {
            total_expected += 1;
            if top_k.iter().any(|r| r.as_str() == *exp_id) {
                total_found += 1;
            }
        }
    }

    if total_expected == 0 {
        0.0
    } else {
        total_found as f64 / total_expected as f64
    }
}

fn compute_precision_at_k(results: &[(Vec<String>, &TestCase)], k: usize) -> f64 {
    let mut precision_sum = 0.0;
    let mut count = 0;

    for (result_ids, tc) in results {
        if tc.expected_memory_ids.is_empty() {
            continue;
        }
        let top_k: Vec<&String> = result_ids.iter().take(k).collect();
        if top_k.is_empty() {
            continue;
        }
        let hits = top_k
            .iter()
            .filter(|r| tc.expected_memory_ids.iter().any(|e| e == &r.as_str()))
            .count();
        precision_sum += hits as f64 / top_k.len() as f64;
        count += 1;
    }

    if count == 0 {
        0.0
    } else {
        precision_sum / count as f64
    }
}

/// Compute NDCG@K (Normalized Discounted Cumulative Gain).
/// Relevance is binary: 1 if the result is in the expected set, 0 otherwise.
fn compute_ndcg_at_k(results: &[(Vec<String>, &TestCase)], k: usize) -> f64 {
    let mut ndcg_sum = 0.0;
    let mut count = 0;

    for (result_ids, tc) in results {
        if tc.expected_memory_ids.is_empty() {
            continue;
        }

        // DCG: sum of rel_i / log2(i+2) for i in 0..k
        let mut dcg = 0.0;
        for (i, rid) in result_ids.iter().take(k).enumerate() {
            let rel = if tc.expected_memory_ids.iter().any(|e| e == &rid.as_str()) {
                1.0
            } else {
                0.0
            };
            dcg += rel / (i as f64 + 2.0).log2();
        }

        // Ideal DCG: all expected results at the top positions
        let mut idcg = 0.0;
        let n_relevant = tc.expected_memory_ids.len().min(k);
        for i in 0..n_relevant {
            idcg += 1.0 / (i as f64 + 2.0).log2();
        }

        if idcg > 0.0 {
            ndcg_sum += dcg / idcg;
        }
        count += 1;
    }

    if count == 0 {
        0.0
    } else {
        ndcg_sum / count as f64
    }
}

fn compute_all_metrics(results: &[(Vec<String>, &TestCase)]) -> BenchmarkResults {
    BenchmarkResults {
        mrr: compute_mrr(results),
        recall_at_1: compute_recall_at_k(results, 1),
        recall_at_3: compute_recall_at_k(results, 3),
        recall_at_5: compute_recall_at_k(results, 5),
        recall_at_10: compute_recall_at_k(results, 10),
        precision_at_5: compute_precision_at_k(results, 5),
        precision_at_10: compute_precision_at_k(results, 10),
        ndcg_at_10: compute_ndcg_at_k(results, 10),
    }
}

// ============================================================================
// CORPUS SETUP
// ============================================================================

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

// ============================================================================
// REPORTING
// ============================================================================

fn print_metrics(label: &str, metrics: &BenchmarkResults) {
    println!("  {label}:");
    println!("    MRR:            {:.4}", metrics.mrr);
    println!("    Recall@1:       {:.4}", metrics.recall_at_1);
    println!("    Recall@3:       {:.4}", metrics.recall_at_3);
    println!("    Recall@5:       {:.4}", metrics.recall_at_5);
    println!("    Recall@10:      {:.4}", metrics.recall_at_10);
    println!("    Precision@5:    {:.4}", metrics.precision_at_5);
    println!("    Precision@10:   {:.4}", metrics.precision_at_10);
    println!("    NDCG@10:        {:.4}", metrics.ndcg_at_10);
}

fn print_report(
    pipeline_label: &str,
    corpus_size: usize,
    test_case_count: usize,
    overall: &BenchmarkResults,
    category_metrics: &HashMap<String, BenchmarkResults>,
    strategy_contributions: &HashMap<Strategy, usize>,
    failures: &[(String, String, Vec<String>, Vec<String>)], // (query, category, expected, got_top5)
) {
    println!();
    println!("================================================================");
    println!("  PUBLICATION-GRADE BENCHMARK RESULTS ({pipeline_label})");
    println!("================================================================");
    println!("  Corpus:           {corpus_size} memories");
    println!("  Test queries:     {test_case_count}");
    println!();

    // Overall metrics table
    println!("  --- Overall Metrics ---");
    println!("  +---------------+---------+");
    println!("  | Metric        | Value   |");
    println!("  +---------------+---------+");
    println!("  | MRR           | {:.4}  |", overall.mrr);
    println!("  | Recall@1      | {:.4}  |", overall.recall_at_1);
    println!("  | Recall@3      | {:.4}  |", overall.recall_at_3);
    println!("  | Recall@5      | {:.4}  |", overall.recall_at_5);
    println!("  | Recall@10     | {:.4}  |", overall.recall_at_10);
    println!("  | Precision@5   | {:.4}  |", overall.precision_at_5);
    println!("  | Precision@10  | {:.4}  |", overall.precision_at_10);
    println!("  | NDCG@10       | {:.4}  |", overall.ndcg_at_10);
    println!("  +---------------+---------+");
    println!();

    // Per-category breakdown
    println!("  --- Per-Category Breakdown ---");
    let categories = [
        "keyword",
        "paraphrase",
        "semantic",
        "multi_hop",
        "temporal",
        "adversarial",
        "negation",
        "incident",
    ];
    for cat in categories {
        if let Some(m) = category_metrics.get(cat) {
            print_metrics(cat, m);
        }
    }
    println!();

    // Strategy contributions
    println!("  --- Strategy Contributions ---");
    let mut sorted_strategies: Vec<_> = strategy_contributions.iter().collect();
    sorted_strategies.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    for (strategy, count) in sorted_strategies {
        println!("    {strategy:?}: {count} results contributed");
    }
    println!();

    // Failure analysis
    let non_adversarial_failures: Vec<_> = failures
        .iter()
        .filter(|(_, cat, expected, _)| cat != "adversarial" && !expected.is_empty())
        .collect();
    println!(
        "  --- Failure Analysis ({} misses on non-adversarial queries) ---",
        non_adversarial_failures.len()
    );
    for (query, category, expected, got_top5) in non_adversarial_failures.iter().take(20) {
        println!("    [{category}] \"{query}\"");
        println!("      Expected: {expected:?}");
        println!("      Got top5: {got_top5:?}");
    }
    if non_adversarial_failures.len() > 20 {
        println!(
            "    ... and {} more failures",
            non_adversarial_failures.len() - 20
        );
    }

    println!("================================================================");
    println!();
}

// ============================================================================
// BENCHMARK RUNNER
// ============================================================================

#[allow(clippy::type_complexity)]
async fn run_benchmark(
    conn: &Connection,
    lance: &LanceStorage,
    corpus: &[TestMemory],
    test_cases: &[TestCase],
    embed_fn: Option<&dyn Fn(&str) -> Vec<f32>>,
    pipeline_label: &str,
) {
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

    let mut all_results: Vec<(Vec<String>, &TestCase)> = Vec::new();
    let mut strategy_contributions: HashMap<Strategy, usize> = HashMap::new();
    let mut category_results: HashMap<String, Vec<(Vec<String>, &TestCase)>> = HashMap::new();
    let mut failures: Vec<(String, String, Vec<String>, Vec<String>)> = Vec::new();

    for tc in test_cases {
        let query_embedding = embed_fn.map(|f| f(tc.query));
        let query_vec_ref = query_embedding.as_deref();

        let result = retrieval::recall(
            tc.query,
            conn,
            lance,
            query_vec_ref,
            &resolver,
            &reranker,
            &summaries,
            &config,
        )
        .await
        .unwrap();

        let result_ids: Vec<String> = result.results.iter().map(|r| r.memory_id.clone()).collect();

        // Track strategy contributions
        for (strategy, count) in &result.strategy_counts {
            *strategy_contributions.entry(*strategy).or_insert(0) += count;
        }

        // Check for failures (only on non-adversarial queries)
        if !tc.expected_memory_ids.is_empty() {
            let missing: Vec<&str> = tc
                .expected_memory_ids
                .iter()
                .filter(|exp| !result_ids.iter().take(10).any(|r| r == **exp))
                .copied()
                .collect();

            if !missing.is_empty() {
                failures.push((
                    tc.query.to_string(),
                    tc.category.to_string(),
                    tc.expected_memory_ids
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    result_ids.iter().take(5).cloned().collect(),
                ));
            }
        }

        // Group by category
        category_results
            .entry(tc.category.to_string())
            .or_default()
            .push((result_ids.clone(), tc));

        all_results.push((result_ids, tc));
    }

    // Compute overall metrics
    let overall = compute_all_metrics(&all_results);

    // Compute per-category metrics
    let mut cat_metrics: HashMap<String, BenchmarkResults> = HashMap::new();
    for (cat, results) in &category_results {
        cat_metrics.insert(cat.clone(), compute_all_metrics(results));
    }

    print_report(
        pipeline_label,
        corpus.len(),
        test_cases.len(),
        &overall,
        &cat_metrics,
        &strategy_contributions,
        &failures,
    );
}

// ============================================================================
// TESTS
// ============================================================================

/// Keyword-only benchmark. Runs without model download.
/// Tests keyword + temporal + entity graph strategies.
/// This is the fast CI test.
#[test]
fn test_publication_benchmark_keyword_only() {
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

        assert_eq!(corpus.len(), 500, "Corpus must contain exactly 500 memories");
        assert_eq!(
            test_cases.len(),
            100,
            "Test suite must contain exactly 100 queries"
        );

        // Verify category distribution
        let mut cat_counts: HashMap<&str, usize> = HashMap::new();
        for tc in &test_cases {
            *cat_counts.entry(tc.category).or_insert(0) += 1;
        }
        assert_eq!(cat_counts["keyword"], 20);
        assert_eq!(cat_counts["paraphrase"], 15);
        assert_eq!(cat_counts["semantic"], 15);
        assert_eq!(cat_counts["multi_hop"], 10);
        assert_eq!(cat_counts["temporal"], 10);
        assert_eq!(cat_counts["adversarial"], 10);
        assert_eq!(cat_counts["negation"], 10);
        assert_eq!(cat_counts["incident"], 10);

        // Verify memory category distribution
        let mut mem_cats: HashMap<&str, usize> = HashMap::new();
        for m in &corpus {
            *mem_cats.entry(m.category).or_insert(0) += 1;
        }
        for (cat, count) in &mem_cats {
            assert_eq!(
                *count, 50,
                "Category '{cat}' has {count} memories, expected 50"
            );
        }

        run_benchmark(
            &conn,
            &lance,
            &corpus,
            &test_cases,
            None,
            "Keyword + Temporal + Entity Graph",
        )
        .await;

        // Compute metrics for assertions
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

        let mut results_for_assert: Vec<(Vec<String>, &TestCase)> = Vec::new();
        for tc in &test_cases {
            let result = retrieval::recall(
                tc.query,
                &conn,
                &lance,
                None,
                &resolver,
                &reranker,
                &summaries,
                &config,
            )
            .await
            .unwrap();
            let ids: Vec<String> = result.results.iter().map(|r| r.memory_id.clone()).collect();
            results_for_assert.push((ids, tc));
        }

        // Filter to only queries with expected results for threshold checks
        let non_adversarial: Vec<_> = results_for_assert
            .iter()
            .filter(|(_, tc)| !tc.expected_memory_ids.is_empty())
            .map(|(ids, tc)| (ids.clone(), *tc))
            .collect();

        let keyword_recall_10 = compute_recall_at_k(&non_adversarial, 10);
        let keyword_mrr = compute_mrr(&non_adversarial);

        // Quality assertions for keyword-only pipeline
        // With 500 memories and keyword matching, we expect reasonable retrieval
        // for direct keyword queries but lower performance on semantic/paraphrase
        assert!(
            keyword_recall_10 > 0.25,
            "Keyword-only Recall@10 should be > 0.25 on non-adversarial queries, got {keyword_recall_10:.4}"
        );
        assert!(
            keyword_mrr > 0.15,
            "Keyword-only MRR should be > 0.15 on non-adversarial queries, got {keyword_mrr:.4}"
        );
    });
}

/// Full pipeline benchmark with semantic search.
/// Requires BGE-Small-EN model download (~50MB).
///
/// Run with:
///   cargo test --test benchmark_suite test_publication_benchmark_full_pipeline -- --nocapture --ignored
#[test]
#[ignore]
fn test_publication_benchmark_full_pipeline() {
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

        // Embed all memories
        println!("Embedding {} memories...", corpus.len());
        let start = std::time::Instant::now();
        for mem in &corpus {
            let vector = emb.embed_query(mem.summary).unwrap();
            lance.insert(mem.id, &vector, mem.stream_id).await.unwrap();
        }
        let embed_duration = start.elapsed();
        println!(
            "Embedded {} memories in {:.1}s ({:.0} memories/sec)",
            corpus.len(),
            embed_duration.as_secs_f64(),
            corpus.len() as f64 / embed_duration.as_secs_f64()
        );
        println!("Vectors stored: {}", lance.vector_count().await.unwrap());

        let embed_fn = |query: &str| -> Vec<f32> { emb.embed_query(query).unwrap() };

        run_benchmark(
            &conn,
            &lance,
            &corpus,
            &test_cases,
            Some(&embed_fn),
            "Full Pipeline (Semantic + Keyword + Temporal + Entity Graph)",
        )
        .await;

        // Compute metrics for assertions
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

        let mut results_for_assert: Vec<(Vec<String>, &TestCase)> = Vec::new();
        for tc in &test_cases {
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
            let ids: Vec<String> = result.results.iter().map(|r| r.memory_id.clone()).collect();
            results_for_assert.push((ids, tc));
        }

        let non_adversarial: Vec<_> = results_for_assert
            .iter()
            .filter(|(_, tc)| !tc.expected_memory_ids.is_empty())
            .map(|(ids, tc)| (ids.clone(), *tc))
            .collect();

        let full_recall_10 = compute_recall_at_k(&non_adversarial, 10);
        let full_mrr = compute_mrr(&non_adversarial);
        let full_recall_5 = compute_recall_at_k(&non_adversarial, 5);
        let full_ndcg_10 = compute_ndcg_at_k(&non_adversarial, 10);

        // Full pipeline quality assertions
        // With semantic search, quality should be significantly higher
        assert!(
            full_recall_10 > 0.55,
            "Full pipeline Recall@10 should be > 0.55, got {full_recall_10:.4}"
        );
        assert!(
            full_recall_5 > 0.45,
            "Full pipeline Recall@5 should be > 0.45, got {full_recall_5:.4}"
        );
        assert!(
            full_mrr > 0.35,
            "Full pipeline MRR should be > 0.35, got {full_mrr:.4}"
        );
        assert!(
            full_ndcg_10 > 0.35,
            "Full pipeline NDCG@10 should be > 0.35, got {full_ndcg_10:.4}"
        );
    });
}
