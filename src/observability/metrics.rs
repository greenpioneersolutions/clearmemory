//! Metrics tracking for Clear Memory — all metrics described in ENTERPRISE.md
//! and the CLAUDE.md observability section.
//!
//! Uses atomic counters for thread-safe, lock-free metric recording. These
//! metrics are exposed via the status endpoint, health endpoint, and
//! OpenTelemetry pipeline (when configured).

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// In-process metrics counters — all metrics promised in the enterprise docs.
///
/// Prefixed with `clearmemory.` when exported to OpenTelemetry.
#[derive(Debug, Default)]
pub struct Metrics {
    // Operation counters
    pub recall_count: AtomicU64,
    pub retain_count: AtomicU64,
    pub expand_count: AtomicU64,
    pub forget_count: AtomicU64,
    pub import_count: AtomicU64,
    pub reflect_count: AtomicU64,
    pub error_count: AtomicU64,

    // Latency tracking (cumulative microseconds for averaging)
    pub recall_latency_us_total: AtomicU64,
    pub retain_latency_us_total: AtomicU64,
    pub curator_latency_us_total: AtomicU64,
    pub reflect_latency_us_total: AtomicU64,
    pub embedding_latency_us_total: AtomicU64,
    pub rerank_latency_us_total: AtomicU64,

    // Per-strategy latency (cumulative microseconds)
    pub semantic_latency_us_total: AtomicU64,
    pub keyword_latency_us_total: AtomicU64,
    pub temporal_latency_us_total: AtomicU64,
    pub graph_latency_us_total: AtomicU64,

    // Token cost optimization
    pub context_tokens_injected_total: AtomicU64,
    pub context_tokens_saved_total: AtomicU64,
    pub curator_tokens_saved_total: AtomicU64,

    // Corpus metrics
    pub corpus_size_bytes: AtomicU64,
    pub corpus_memory_count: AtomicU64,

    // Retention
    pub retention_events_time: AtomicU64,
    pub retention_events_size: AtomicU64,
    pub retention_events_performance: AtomicU64,

    // Recall quality
    pub recall_hits: AtomicU64,   // recalls that returned results
    pub recall_misses: AtomicU64, // recalls that returned empty
}

/// Snapshot of all metrics for serialization and export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    // Operations
    pub recall_count: u64,
    pub retain_count: u64,
    pub expand_count: u64,
    pub forget_count: u64,
    pub import_count: u64,
    pub reflect_count: u64,
    pub error_count: u64,

    // Average latencies (microseconds)
    pub recall_avg_latency_us: u64,
    pub retain_avg_latency_us: u64,

    // Token optimization
    pub context_tokens_injected_total: u64,
    pub context_tokens_saved_total: u64,

    // Corpus
    pub corpus_size_bytes: u64,
    pub corpus_memory_count: u64,

    // Retention
    pub retention_events_total: u64,

    // Quality
    pub recall_hit_rate_pct: f64,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_recall(&self, latency_us: u64, hit: bool) {
        self.recall_count.fetch_add(1, Ordering::Relaxed);
        self.recall_latency_us_total
            .fetch_add(latency_us, Ordering::Relaxed);
        if hit {
            self.recall_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.recall_misses.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_retain(&self, latency_us: u64) {
        self.retain_count.fetch_add(1, Ordering::Relaxed);
        self.retain_latency_us_total
            .fetch_add(latency_us, Ordering::Relaxed);
    }

    pub fn record_expand(&self) {
        self.expand_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_tokens_injected(&self, tokens: u64) {
        self.context_tokens_injected_total
            .fetch_add(tokens, Ordering::Relaxed);
    }

    pub fn record_tokens_saved(&self, tokens: u64) {
        self.context_tokens_saved_total
            .fetch_add(tokens, Ordering::Relaxed);
    }

    pub fn update_corpus(&self, size_bytes: u64, memory_count: u64) {
        self.corpus_size_bytes.store(size_bytes, Ordering::Relaxed);
        self.corpus_memory_count
            .store(memory_count, Ordering::Relaxed);
    }

    pub fn record_retention_event(&self, trigger: &str) {
        match trigger {
            "time" => self.retention_events_time.fetch_add(1, Ordering::Relaxed),
            "size" => self.retention_events_size.fetch_add(1, Ordering::Relaxed),
            "performance" => self
                .retention_events_performance
                .fetch_add(1, Ordering::Relaxed),
            _ => 0,
        };
    }

    /// Take a snapshot of all metrics for export.
    pub fn snapshot(&self) -> MetricsSnapshot {
        let recall_count = self.recall_count.load(Ordering::Relaxed);
        let retain_count = self.retain_count.load(Ordering::Relaxed);
        let recall_hits = self.recall_hits.load(Ordering::Relaxed);
        let recall_total = recall_hits + self.recall_misses.load(Ordering::Relaxed);

        MetricsSnapshot {
            recall_count,
            retain_count,
            expand_count: self.expand_count.load(Ordering::Relaxed),
            forget_count: self.forget_count.load(Ordering::Relaxed),
            import_count: self.import_count.load(Ordering::Relaxed),
            reflect_count: self.reflect_count.load(Ordering::Relaxed),
            error_count: self.error_count.load(Ordering::Relaxed),
            recall_avg_latency_us: if recall_count > 0 {
                self.recall_latency_us_total.load(Ordering::Relaxed) / recall_count
            } else {
                0
            },
            retain_avg_latency_us: if retain_count > 0 {
                self.retain_latency_us_total.load(Ordering::Relaxed) / retain_count
            } else {
                0
            },
            context_tokens_injected_total: self
                .context_tokens_injected_total
                .load(Ordering::Relaxed),
            context_tokens_saved_total: self.context_tokens_saved_total.load(Ordering::Relaxed),
            corpus_size_bytes: self.corpus_size_bytes.load(Ordering::Relaxed),
            corpus_memory_count: self.corpus_memory_count.load(Ordering::Relaxed),
            retention_events_total: self.retention_events_time.load(Ordering::Relaxed)
                + self.retention_events_size.load(Ordering::Relaxed)
                + self.retention_events_performance.load(Ordering::Relaxed),
            recall_hit_rate_pct: if recall_total > 0 {
                (recall_hits as f64 / recall_total as f64) * 100.0
            } else {
                0.0
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_default_zero() {
        let m = Metrics::new();
        let snap = m.snapshot();
        assert_eq!(snap.recall_count, 0);
        assert_eq!(snap.retain_count, 0);
        assert_eq!(snap.error_count, 0);
        assert_eq!(snap.recall_hit_rate_pct, 0.0);
    }

    #[test]
    fn test_record_recall_with_hit_rate() {
        let m = Metrics::new();
        m.record_recall(1000, true);
        m.record_recall(2000, true);
        m.record_recall(500, false);

        let snap = m.snapshot();
        assert_eq!(snap.recall_count, 3);
        assert_eq!(snap.recall_avg_latency_us, 1166); // (1000+2000+500)/3
        assert!((snap.recall_hit_rate_pct - 66.66).abs() < 1.0);
    }

    #[test]
    fn test_token_savings() {
        let m = Metrics::new();
        m.record_tokens_injected(500);
        m.record_tokens_saved(3500);

        let snap = m.snapshot();
        assert_eq!(snap.context_tokens_injected_total, 500);
        assert_eq!(snap.context_tokens_saved_total, 3500);
    }

    #[test]
    fn test_retention_events() {
        let m = Metrics::new();
        m.record_retention_event("time");
        m.record_retention_event("time");
        m.record_retention_event("size");

        let snap = m.snapshot();
        assert_eq!(snap.retention_events_total, 3);
    }

    #[test]
    fn test_corpus_metrics() {
        let m = Metrics::new();
        m.update_corpus(1_000_000, 500);

        let snap = m.snapshot();
        assert_eq!(snap.corpus_size_bytes, 1_000_000);
        assert_eq!(snap.corpus_memory_count, 500);
    }

    #[test]
    fn test_snapshot_serialization() {
        let m = Metrics::new();
        m.record_recall(100, true);
        m.record_retain(200);
        let snap = m.snapshot();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("recall_count"));
        assert!(json.contains("recall_hit_rate_pct"));
        assert!(json.contains("context_tokens_saved_total"));
    }
}
