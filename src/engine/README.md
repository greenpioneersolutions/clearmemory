# engine/ — Core Orchestration Engine

## Role in the Architecture

The `engine` module is the central coordinator of Clear Memory. It owns all subsystem instances (SQLite, LanceDB, verbatim storage, embedding model, encryption provider) and orchestrates the complete read and write paths described in the architecture spec. Every operation that a user or API client triggers -- retain, recall, expand, forget, status -- flows through the `Engine` struct.

The engine is the only module that composes the full pipeline. For writes (retain), it chains: secret scanning, classification, encryption, verbatim storage, SQLite insert, LanceDB vector append, entity resolution, fact extraction, conflict detection, and audit logging. For reads (recall), it chains: stream permission checks, query embedding, 4-strategy parallel retrieval, RRF merge, reranking, classification gating (Tier 3), curator filtering (Tier 2+), and audit logging. The server module (`server::http`) creates an `Engine` instance on startup and delegates all handler logic to it.

## File-by-File Descriptions

### mod.rs

The single file in this module. Contains the `Engine` struct, its initialization, and all core operation methods.

**Key types:**

- `Engine` — The main struct. Public fields:
  - `config: Arc<Config>` — Full configuration from `~/.clearmemory/config.toml`
  - `sqlite: SqliteStorage` — Structured data store
  - `verbatim: VerbatimStorage` — Raw transcript file store
  - `lance: LanceStorage` — Vector index for semantic search
  - `encryption: Arc<dyn EncryptionProvider>` — Encryption provider (Noop or real)
  - `embeddings: Option<Arc<EmbeddingManager>>` — Embedding model (None if loading failed)
  - Private: `start_time: Instant`, `data_dir: PathBuf`

- `RecallResponse` — Search result: `results: Vec<RecallHit>`, `total_candidates: usize`
- `RecallHit` — Individual search hit: `memory_id`, `summary: Option<String>`, `score: f64`, `created_at`
- `RetainResponse` — Store result: `memory_id`, `content_hash`
- `ExpandResponse` — Full content result: `memory_id`, `content`, `source_format`, `created_at`
- `StatusResponse` — Health check: `status`, `tier`, `memory_count`, `vector_count`, `uptime_secs`

**Key functions:**

- `Engine::init(config)` — Async initialization. Steps:
  1. Ensures `~/.clearmemory/` directory structure exists via `Config::ensure_directories()`
  2. Creates encryption provider (falls back to `NoopProvider` if passphrase not set)
  3. Opens SQLite database with encryption and configured write queue depth
  4. Creates VerbatimStorage with active and archive directories
  5. Loads embedding model in a blocking task (graceful degradation: warns and continues without it if loading fails)
  6. Opens LanceDB with the embedding model's dimension (or default 384)
  7. Returns fully initialized `Engine`

- `Engine::retain(content, tags, classification, stream_id)` — Full write path:
  1. **Secret scanning** via `SecretScanner::new()`. Behavior depends on `config.security.secret_scanning.mode`:
     - `"block"`: rejects the operation if secrets are detected
     - `"redact"`: calls `redactor::scan_and_redact`, upgrades classification to Confidential if secrets found
     - `"warn"` (default): stores as-is but upgrades classification to Confidential if secrets found
  2. **Verbatim storage**: encrypts and stores content, gets content hash
  3. **Summary generation**: first line of content, truncated to 200 chars
  4. **SQLite insert**: via `SqliteStorage::retain` with params including tags and classification
  5. **LanceDB vector append**: generates embedding via `EmbeddingManager::embed_query` (in blocking task), inserts into LanceDB. Warns on failure but does not abort.
  6. **Entity resolution and fact extraction** (in blocking task): opens a separate SQLite connection, calls `facts::extractor::extract_facts`, `facts::temporal::insert_fact`, `facts::conflict::detect_conflicts`, `facts::conflict::resolve_conflicts`
  7. **Audit logging**: records the retain operation via `AuditLogger`

- `Engine::recall(query, stream_id, include_archived)` — Full read path:
  1. **Stream permission check**: if a stream_id is provided, checks read permission via `streams::security::can_read` (default user "local" for single-user deployments). Bails on access denied.
  2. **Collects summaries**: loads up to 100 memories from SQLite for the reranker
  3. **Query embedding**: generates embedding in a blocking task (skips if embedding model unavailable)
  4. **4-strategy parallel retrieval**: spawns a blocking task that opens its own SQLite connection and calls `retrieval::recall` with `HeuristicResolver` and `PassthroughReranker`
  5. **Classification gate (Tier 3 only)**: filters out PII/confidential memories when `tier == Cloud`, checking against `config.security.cloud_eligible_classifications`
  6. **Curator filtering (Tier 2+)**: currently uses `NoopCurator` (passes through all results). The code structure is in place for Qwen3-0.6B integration.
  7. **Audit logging**: records the recall operation with query and result count
  8. **Response building**: maps reranked results to `RecallHit` structs with summary and score

- `Engine::expand(memory_id)` — Full content retrieval:
  1. Gets memory metadata from SQLite
  2. Reads and decrypts verbatim content via content hash
  3. Updates access time (resets retention staleness clock)
  4. Audit logs the expand operation
  5. Returns full content with metadata

- `Engine::forget(memory_id, reason)` — Temporal invalidation. Delegates to `SqliteStorage::forget` which sets `valid_until` on associated facts.

- `Engine::status()` — Returns memory count, vector count, tier, uptime.

- `Engine::data_dir()` — Returns the `~/.clearmemory/` path.

## Key Public Types Other Modules Depend On

- `Engine` — The central type. Used by `server::http::AppState` (wrapped in `Arc`) and by tests.
- `RecallResponse` / `RecallHit` — Used by the HTTP recall handler to build API responses.
- `RetainResponse` — Used by the HTTP retain handler.
- `ExpandResponse` — Used by the HTTP expand handler.
- `StatusResponse` — Used by the HTTP status handler.

## Relevant config.toml Keys

The engine reads nearly every config section:

- `[general] tier` — Determines which pipeline stages are active (Offline/LocalLlm/Cloud)
- `[models] embedding` — Which embedding model to load
- `[encryption] enabled` — Whether at-rest encryption is active
- `[concurrency] write_queue_depth` — SQLite write queue size
- `[retrieval] top_k`, `temporal_boost`, `entity_boost` — Retrieval pipeline parameters
- `[security] secret_scanning.mode` — Secret scanning behavior (warn/redact/block)
- `[security] cloud_eligible_classifications` — Which classifications can go to Tier 3 cloud APIs
- `[compliance] default_classification` — Default classification for new memories

## Deferred / Planned Functionality

- **Qwen3-0.6B curator integration:** The recall path has the curator call structure in place (`NoopCurator` used as placeholder). When candle is integrated, replace `NoopCurator` with the real `QwenCurator` for Tier 2+ deployments.
- **FastembedReranker usage:** The recall path currently uses `PassthroughReranker`. The `FastembedReranker` is implemented in `retrieval::rerank` and can be swapped in when model loading is wired up.
- **Bulk import optimization:** The engine does not yet have a dedicated bulk import method. Each imported memory currently goes through the full `retain` path individually. A batch path could amortize embedding generation and SQLite transactions.
- **Context compilation in recall:** The engine's recall method returns raw results but does not invoke the `context::ContextCompiler`. The spec describes context compilation as part of the recall flow; wiring the compiler into the engine's recall or a dedicated `context` endpoint is needed.
- **Reflect operation:** Not yet implemented on the engine. The server stubs return "Reflect requires Tier 2 or higher". The reflect model (Qwen3-4B) via candle needs to be integrated.
- **Connection pooling:** The recall and retain paths open separate SQLite connections in blocking tasks for entity resolution and audit logging. These could be pooled or reuse the engine's existing connection infrastructure.
