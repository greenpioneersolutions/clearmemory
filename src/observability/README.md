# observability/ -- Metrics, Tracing, and Health Checks

## Role in Architecture

The observability module provides three capabilities: structured logging with distributed tracing (via the `tracing` crate and OpenTelemetry), in-process metrics counters (lock-free atomics), and a health check endpoint. Together these give operators visibility into Clear Memory's performance, throughput, token savings, and operational health.

When `OTEL_EXPORTER_OTLP_ENDPOINT` is set, traces and metrics export to any OpenTelemetry-compatible backend (Datadog, Grafana, Jaeger, etc.). When no endpoint is configured, structured logs go to stdout with zero network overhead. The metrics are always collected in-process and exposed via the `clearmemory_status` MCP tool and the `GET /health` HTTP endpoint.

## File-by-File Descriptions

### mod.rs

Module root. Re-exports: `health`, `metrics`, `tracing_setup`.

### tracing_setup.rs

Initializes the tracing subscriber for structured logging and optional OpenTelemetry export. Contains:

- **`init_tracing()`** -- Called once at startup. Sets up a `tracing_subscriber` with an `EnvFilter` (defaults to `clearmemory=info,warn` if `RUST_LOG` is not set) and a formatted output layer. If `OTEL_EXPORTER_OTLP_ENDPOINT` is set, initializes an OTLP span exporter via `opentelemetry_otlp::SpanExporter` with tonic (gRPC), creates a `TracerProvider` with batch exporting on the Tokio runtime, and adds a `tracing_opentelemetry::layer()` to the subscriber. Falls back to stdout-only if OTLP initialization fails.

- **`init_otel_tracer() -> Result<Tracer, TraceError>`** (private) -- Builds the OTLP exporter and tracer provider. Uses `opentelemetry_sdk::trace::TracerProvider::builder().with_batch_exporter()`. The tracer is named `"clearmemory"`.

- **`shutdown_tracing()`** -- Calls `opentelemetry::global::shutdown_tracer_provider()` to flush pending spans on process exit.

### metrics.rs

In-process metrics counters using lock-free atomics. All metrics described in the CLAUDE.md observability section are represented here. Contains:

- **`Metrics`** -- A struct with `AtomicU64` fields for every metric. Organized into groups:
  - **Operation counters**: `recall_count`, `retain_count`, `expand_count`, `forget_count`, `import_count`, `reflect_count`, `error_count`
  - **Latency tracking** (cumulative microseconds): `recall_latency_us_total`, `retain_latency_us_total`, `curator_latency_us_total`, `reflect_latency_us_total`, `embedding_latency_us_total`, `rerank_latency_us_total`
  - **Per-strategy latency**: `semantic_latency_us_total`, `keyword_latency_us_total`, `temporal_latency_us_total`, `graph_latency_us_total`
  - **Token optimization**: `context_tokens_injected_total`, `context_tokens_saved_total`, `curator_tokens_saved_total`
  - **Corpus metrics**: `corpus_size_bytes`, `corpus_memory_count`
  - **Retention events**: `retention_events_time`, `retention_events_size`, `retention_events_performance`
  - **Recall quality**: `recall_hits`, `recall_misses`

  Key methods:
  - `new()` / `Default` -- all counters start at zero
  - `record_recall(latency_us, hit: bool)` -- increments count, accumulates latency, tracks hit/miss
  - `record_retain(latency_us)` -- increments count and accumulates latency
  - `record_expand()`, `record_error()` -- simple counter increments
  - `record_tokens_injected(tokens)`, `record_tokens_saved(tokens)` -- token budget tracking
  - `update_corpus(size_bytes, memory_count)` -- overwrites corpus gauge values
  - `record_retention_event(trigger: &str)` -- increments the counter for "time", "size", or "performance" triggers
  - `snapshot() -> MetricsSnapshot` -- reads all atomics and computes derived values

- **`MetricsSnapshot`** -- A serializable (Serialize/Deserialize) struct that captures a point-in-time view of all metrics. Includes computed averages (`recall_avg_latency_us`, `retain_avg_latency_us`) and `recall_hit_rate_pct` (percentage of recalls that returned results). Used for JSON serialization in the status and health endpoints.

### health.rs

Health check endpoint logic. Contains:

- **`Status`** -- An enum with variants `Healthy`, `Degraded`, `Unhealthy`. Serialized as snake_case strings.

- **`HealthStatus`** -- A serializable struct with fields: `status` (Status), `uptime_secs` (u64), `memory_count` (u64), `tier` (Tier, imported from `crate::Tier`).

- **`check_health() -> HealthStatus`** -- Currently a placeholder that returns `Healthy` with zeroed metrics and `Tier::Offline`. The full implementation will check: SQLite accessibility, LanceDB consistency, embedding model status, curator/reflect model status (Tier 2+), and MCP/HTTP port availability.

## Key Public Types Other Modules Depend On

- `Metrics` -- instantiated once at engine startup, shared (via `Arc`) across all request handlers
- `MetricsSnapshot` -- returned by the status endpoint and health endpoint
- `HealthStatus` / `Status` -- returned by the `GET /health` HTTP endpoint and `clearmemory_status` MCP tool
- `init_tracing()` -- called once at the top of `main()`
- `shutdown_tracing()` -- called on graceful shutdown

## Relevant config.toml Keys

```toml
[observability]
otel_enabled = false                    # currently checked via OTEL_EXPORTER_OTLP_ENDPOINT env var
otel_endpoint = ""                      # set via env var, not yet read from config
otel_service_name = "clearmemory"       # hardcoded in init_otel_tracer as "clearmemory"
metrics_log_interval_secs = 60          # planned: periodic metric snapshots to log
health_endpoint_enabled = true          # planned: toggle for health endpoint
```

## Deferred / Planned Functionality

- **Full health check implementation**: `check_health()` should verify SQLite, LanceDB, models, and ports, returning `Degraded` or `Unhealthy` as appropriate
- **Uptime tracking**: The health status should track actual process uptime
- **Metrics export to OpenTelemetry**: The current `Metrics` struct uses in-process atomics. Bridging these to OpenTelemetry metric instruments (gauges, histograms, counters) for OTLP export is planned
- **Histogram support for latency percentiles**: Currently tracks cumulative totals for average calculation. P50/P95/P99 histograms require a histogram data structure (e.g., HdrHistogram)
- **Token expiry warning in health**: The health endpoint should include `token_expiry_warning` when the primary API token is within 14 days of expiration
- **Periodic metrics logging**: Configurable interval for writing `MetricsSnapshot` to structured logs
