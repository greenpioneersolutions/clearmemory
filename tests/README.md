# tests/ — Clear Memory Test Suite

Test infrastructure for the Clear Memory engine. Tests are organized into benchmark suites (retrieval quality measurement) and fixture data. Unit tests live in-module under `#[cfg(test)]` blocks within `src/`.

---

## Test Files

### Retrieval Quality Benchmarks

These test files evaluate the accuracy and quality of the retrieval pipeline, not just that it runs without errors.

| File | Purpose | Scale |
|------|---------|-------|
| `benchmark_longmemeval.rs` | LongMemEval-style evaluation. 128-memory corpus, 80 queries across 5 task types (information extraction, temporal reasoning, multi-hop, knowledge update, abstraction). Reports MRR, Recall@K, NDCG@10 per task type. | Large |
| `benchmark_suite.rs` | Publication-quality benchmark. 500 memories, 100 queries across 8 categories including adversarial, negation, and paraphrase queries. The comprehensive quality gate. | Large |
| `benchmark_scale.rs` | Corpus scale testing. Runs the same 30 queries against corpora of 500, 1K, 2K, 3K, 4K, 5K, and 10K memories. Measures how retrieval quality degrades (or doesn't) as corpus grows. | Variable |
| `per_strategy_bench.rs` | Per-strategy precision isolation. Tests each of the 4 retrieval strategies (semantic, keyword, temporal, entity graph) independently to catch silent regressions in individual strategies. | Medium |
| `retrieval_regression.rs` | CI regression gate. 50+ realistic memories, 25 queries, fast pass/fail threshold: Recall@10 >= 0.90. Runs on every PR touching retrieval code. | Small (fast) |

### Running Tests

```bash
# Run all tests (unit + integration)
cargo test

# Run a specific benchmark test
cargo test --test benchmark_longmemeval

# Run the fast regression gate (what CI runs)
cargo test --test retrieval_regression

# Run Criterion benchmarks (in benchmarks/ directory, not here)
cargo bench
```

---

## Fixtures

Test fixture data lives in `tests/fixtures/`. See `fixtures/README.md` for details on each file.

All fixtures use **synthetic data** — fictional names, placeholder credentials, and fabricated conversations. No real user data, API keys, or proprietary content.

---

## Planned Test Directories

The following test categories are planned but not yet implemented:

- `integration/` — End-to-end flows: import, retrieval, retention, stream security, concurrency, backup/restore, migration, compliance
- `adversarial/` — Malformed inputs, unicode edge cases, corrupt DB/index recovery
- `security/` — Auth token scopes/expiration, rate limiting, secret scanning, encryption roundtrip, classification pipeline, purge authorization, audit chain integrity
- `stress/` — 500K memory corpus with concurrent queries, 50 simultaneous write operations
