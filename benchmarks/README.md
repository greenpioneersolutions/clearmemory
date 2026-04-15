# benchmarks/ — Clear Memory Performance Benchmarks

Criterion-based latency benchmarks for the Clear Memory engine. These measure raw performance (how fast), not retrieval quality (how accurate) — for quality benchmarks, see `tests/`.

---

## Benchmark Files

| File | What It Measures |
|------|-----------------|
| `retrieval_bench.rs` | End-to-end retrieval latency on a 20-memory corpus with 5 realistic queries. Measures the full pipeline: embedding, search, merge, rerank (excluding semantic search to isolate non-GPU components). |
| `latency_bench.rs` | Component-level latency: LanceDB single-vector insert, LanceDB search over 100 vectors, keyword search over 200 memories. Isolates storage layer performance from retrieval logic. |

## Running Benchmarks

```bash
# Run all Criterion benchmarks
cargo bench

# Run a specific benchmark
cargo bench --bench retrieval_bench
cargo bench --bench latency_bench
```

Criterion generates HTML reports in `target/criterion/` with statistical analysis, comparison to previous runs, and plots.

## Performance Targets

From `CLAUDE.md` — these are the targets the retention performance policy monitors:

| Corpus Size | Memories | Target p95 Recall (Tier 1) |
|-------------|----------|---------------------------|
| 100MB | ~2K | <50ms |
| 500MB | ~10K | <80ms |
| 2GB | ~40K | <150ms |
| 5GB | ~100K | <300ms |

The default performance threshold for retention policy action is 200ms p95.
