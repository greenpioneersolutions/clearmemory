# retrieval/ — 4-Strategy Parallel Search and Reranking Pipeline

## Role in the Architecture

The `retrieval` module implements Clear Memory's core search pipeline: four retrieval strategies execute concurrently, their results are merged via Reciprocal Rank Fusion (RRF), and a cross-encoder reranker produces the final scored results. This module sits between the storage layer (which it queries) and the engine (which orchestrates full recall operations including curator filtering and context compilation).

Every `recall` operation in Clear Memory flows through this pipeline. The four strategies are complementary: semantic search catches conceptual similarity, keyword search catches exact terms, temporal search catches time-referenced queries, and entity graph traversal catches relationship-based queries. No single strategy covers all query types, which is why they run in parallel and merge.

## File-by-File Descriptions

### mod.rs

The orchestrator. Defines the `recall` function that coordinates all four strategies, merges results, and reranks.

**Key types:**

- `RecallConfig` — Configuration for a single recall operation: `top_k: usize`, `temporal_boost: f64`, `entity_boost: f64`, `include_archived: bool`, `stream_id: Option<String>`.
- `RecallResult` — Full recall output: `results: Vec<RerankedResult>`, `strategy_counts: HashMap<Strategy, usize>`, `total_candidates: usize`.

**Key function:**

- `recall(query, conn, lance, query_embedding, resolver, reranker, summaries, config)` — The main entry point. Uses `tokio::join!` to run all four strategies concurrently. Converts each strategy's results into `ScoredResult` structs, merges them with RRF (k=60), then reranks with the provided `Reranker` implementation. Parameters:
  - `conn: &Connection` — SQLite connection for keyword, temporal, and graph strategies
  - `lance: &LanceStorage` — LanceDB for semantic search
  - `query_embedding: Option<&[f32]>` — Pre-computed query embedding (if None, semantic search is skipped)
  - `resolver: &dyn EntityResolver` — Entity resolution for graph search
  - `reranker: &dyn Reranker` — Cross-encoder reranker
  - `summaries: &HashMap<String, String>` — Memory ID to summary text mapping (needed by reranker)

### semantic.rs

Semantic similarity search strategy. Thin wrapper around `LanceStorage::search`.

**Key types:**

- `SemanticResult` — Contains `memory_id: String` and `score: f64`.

**Key function:**

- `search(lance, query_embedding, top_k, stream_id, include_archived)` — Delegates to `LanceStorage::search` and maps `VectorSearchResult` to `SemanticResult`.

### keyword.rs

Keyword matching strategy using SQLite LIKE queries as a fallback for BGE-M3 sparse vectors.

**Key types:**

- `KeywordResult` — Contains `memory_id: String` and `score: f64`.

**Key function:**

- `search(conn, query, top_k, stream_id, include_archived)` — Splits query into whitespace-delimited keywords (filtering words with 3 or fewer characters). Scans all memory summaries via SQLite, counting case-insensitive keyword matches. Score = `match_count / total_keywords`. Results sorted by score descending, truncated to `top_k`.

**Note:** This is a fallback implementation. The spec calls for BGE-M3 sparse vector search, which is implemented in `storage::embeddings::SparseEmbeddingManager` but not yet wired into the retrieval pipeline because LanceDB Rust bindings lack native sparse vector support.

### temporal.rs

Time-aware retrieval strategy. Detects natural language time references in the query and finds memories created within that time range.

**Key types:**

- `TemporalResult` — Contains `memory_id: String` and `score: f64`.

**Key function:**

- `search(conn, query, top_k, temporal_boost, include_archived)` — Calls `detect_time_range` to parse the query for time references. If a range is found, queries SQLite for memories within that date range. Scores decrease by rank (0.05 per position) and are multiplied by `(1.0 + temporal_boost)`.

**Supported time expressions:**

- "last week", "this week", "last month"
- "yesterday", "today"
- "N days/weeks/months ago" (extracts the number and computes a range with +/-7 day padding)
- Month names: "in January", "in February", etc. (assumes current year, or previous year if the month is in the future)

**Internal functions:**

- `detect_time_range(query)` — Returns `Option<(String, String)>` date range as ISO date strings.
- `parse_relative_time(lower, today)` — Handles "N days/weeks/months ago" patterns.
- `parse_month_reference(lower, today)` — Handles month name references.

### graph.rs

Entity graph traversal strategy. Extracts entity mentions from the query, resolves them to known entities, and traverses relationships to find connected memories.

**Key types:**

- `GraphResult` — Contains `memory_id: String` and `score: f64`.

**Key function:**

- `search(conn, query, resolver, entity_boost, top_k)` — Extracts mentions via `extract_mentions`, resolves each through the `EntityResolver` trait, traverses the entity graph 2 hops deep via `entities::graph::traverse`, deduplicates memory IDs, and scores with entity boost.

**Internal function:**

- `extract_mentions(query)` — Heuristic extraction: collects all words longer than 2 characters, plus consecutive word pairs longer than 3 characters. Returns `Vec<String>`.

### merge.rs

Reciprocal Rank Fusion (RRF) implementation for merging results from multiple strategies.

**Key types:**

- `ScoredResult` — A result from any strategy: `memory_id`, `score`, `strategy: Strategy`.
- `Strategy` — Enum: `Semantic`, `Keyword`, `Temporal`, `EntityGraph`. Implements `Hash`, `Eq`, `Copy`.
- `MergedResult` — Post-merge result: `memory_id`, `fused_score`, `contributing_strategies: Vec<Strategy>`.

**Key function:**

- `reciprocal_rank_fusion(strategy_results, k)` — RRF formula: `score(d) = sum(1 / (k + rank + 1))` across all strategies where `d` appears. Standard k=60. Results sorted by fused score descending. Memories appearing in multiple strategies get naturally boosted.

### rerank.rs

Cross-encoder reranking layer. Scores each (query, document) pair independently for true relevance, catching cases where bi-encoder search returns semantically similar but non-answering results.

**Key types:**

- `RerankedResult` — Contains `memory_id`, `rerank_score: f64`, `original_fused_score: f64`.
- `Reranker` — Trait: `fn rerank(&self, query, candidates: &[(String, String)]) -> Result<Vec<(String, f64)>>`. The candidates are `(memory_id, summary_text)` pairs.
- `PassthroughReranker` — Preserves existing fusion ranking with decreasing scores. Used when the BGE-Reranker-Base model is unavailable.
- `FastembedReranker` — Real cross-encoder using `fastembed::TextRerank` with `RerankerModel::BGERerankerBase` (~400MB model). Manually `Send + Sync` via Mutex.

**Key function:**

- `rerank_results(reranker, query, merged, summaries, top_k)` — Builds candidate list from merged results (filtering to those with summaries), calls the reranker, maps scores back to `RerankedResult`, sorts by rerank score descending, truncates to `top_k`.

## Key Public Types Other Modules Depend On

- `RecallConfig` / `RecallResult` — used by `engine::Engine::recall`
- `RerankedResult` — flows through the engine into the server response
- `Reranker` trait — allows swapping reranker implementations per tier
- `Strategy` enum — used for strategy-level metrics and debugging
- `MergedResult` — intermediate type between merge and rerank

## Relevant config.toml Keys

- `[retrieval] top_k` — Number of results per strategy before merge (default: `10`)
- `[retrieval] temporal_boost` — Max boost multiplier for temporal proximity (default: `0.4`)
- `[retrieval] entity_boost` — Boost multiplier for entity graph matches (default: `0.3`)
- `[models] reranker` — Which reranker model to use (default: `"bge-reranker-base"`)

## Deferred / Planned Functionality

- **Sparse vector keyword search:** Replace the SQLite LIKE-based keyword search with BGE-M3 sparse vector queries in LanceDB once the Rust bindings support sparse columns.
- **FastembedReranker integration in engine:** The `FastembedReranker` struct is implemented but the engine currently uses `PassthroughReranker`. Loading the BGE-Reranker-Base model and using `FastembedReranker` in production is straightforward but adds ~400MB model download and ~1.2s to startup.
- **Stream-aware graph traversal:** The graph search does not currently filter by stream. Cross-stream entity results may appear even when a stream filter is active.
