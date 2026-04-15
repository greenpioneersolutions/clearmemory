# storage/ — Persistent Storage Backends

## Role in the Architecture

The `storage` module provides all persistent data access for Clear Memory. It sits at the bottom of the dependency graph: the engine, retrieval pipeline, import system, and server all depend on storage, but storage depends on nothing above it (only on `migration`, `security::encryption`, and core library types from `lib.rs`).

Storage is split into three independent backends that work in concert: SQLite for structured metadata and relational queries, LanceDB for vector similarity search, and a filesystem layer for raw verbatim transcript files. This separation means each backend can be optimized, backed up, and debugged independently. All three backends live under `~/.clearmemory/` and together form the complete data layer.

## File-by-File Descriptions

### mod.rs

Module declaration only. Re-exports the four submodules: `embeddings`, `lance`, `sqlite`, `verbatim`.

### sqlite.rs

The SQLite storage layer, using `rusqlite` (not sqlx) with optional SQLCipher encryption. This is the primary structured data store for memories, tags, facts, entities, streams, audit logs, and all relational data.

**Key types:**

- `Memory` — A stored memory record with fields: `id`, `content_hash`, `summary`, `source_format`, `classification` (`Classification` enum), `created_at`, `last_accessed_at`, `access_count`, `archived`, `owner_id`, `stream_id`. This is the core data type that flows through the entire system.
- `RetainParams` — Parameters for creating a new memory: `content_hash`, `summary`, `source_format`, `classification`, `owner_id`, `stream_id`, `tags: Vec<(String, String)>`.
- `SqliteStorage` — The main storage struct. Holds a read-only connection (`Arc<tokio::sync::Mutex<Connection>>`) and a write queue sender (`mpsc::Sender<WriteOp>`).
- `WriteOp` — Internal enum for serialized write operations: `Retain`, `Forget`, `UpdateAccessTime`, `Archive`, `RawSql`.

**Key functions:**

- `SqliteStorage::open(db_path, encryption, queue_depth)` — Opens or creates the database, runs migrations via `migration::runner::run_migrations`, enables WAL mode, starts the background writer task. Accepts an `Arc<dyn EncryptionProvider>` for SQLCipher key application.
- `SqliteStorage::open_in_memory()` — Opens an in-memory database for testing. Note: read and write connections are separate in-memory databases (they do not share state), so this is primarily useful for testing the write path.
- `SqliteStorage::retain(params)` — Stores a new memory via the write queue. Returns the generated UUID.
- `SqliteStorage::forget(memory_id, reason)` — Temporal invalidation: sets `valid_until` and `invalidated_at` on all facts for the memory. Does not delete anything.
- `SqliteStorage::get_memory(memory_id)` — Read path, bypasses write queue. Returns a `Memory` struct.
- `SqliteStorage::search_memories(stream_id, include_archived, limit)` — Read path. Filters by stream and archive status, returns up to `limit` memories ordered by `created_at DESC`.
- `SqliteStorage::memory_count()` — Returns count of active (non-archived) memories.
- `SqliteStorage::update_access_time(memory_id)` — Updates `last_accessed_at` and increments `access_count`. Used by the expand operation to reset the retention staleness clock.
- `SqliteStorage::execute_write(sql)` — Executes arbitrary SQL through the write queue (used for audit logging and similar operations).

**Concurrency model:** All writes go through a single `writer_task` (a tokio task consuming from an `mpsc` channel). Reads use a separate connection and bypass the queue entirely. This matches the WAL mode guarantee: concurrent readers with a single writer.

**Encryption:** On `open_connection`, if the `EncryptionProvider` is enabled, the SQLCipher PRAGMA key is applied before any other operations.

### lance.rs

LanceDB vector storage for semantic search. Stores dense embedding vectors alongside memory IDs and optional stream IDs.

**Key types:**

- `VectorSearchResult` — Contains `memory_id: String` and `score: f64`.
- `LanceStorage` — The main struct, holding the `vectors_dir` path, a `lancedb::Connection`, and `vector_dim: i32`. Implements `Clone`.

**Key constants:**

- `DEFAULT_VECTOR_DIM = 384` — Default dimension for bge-small-en-v1.5. Production BGE-M3 uses 1024; the dimension is configurable via `open_with_dim`.

**Key functions:**

- `LanceStorage::open(vectors_dir)` — Opens with default dimension (384).
- `LanceStorage::open_with_dim(vectors_dir, vector_dim)` — Opens with a specific vector dimension. Creates the directory if it does not exist.
- `LanceStorage::insert(memory_id, dense_vector, stream_id)` — Inserts a vector. Creates the "memories" table on first insert. Validates vector dimension. Empty vectors are silently ignored.
- `LanceStorage::search(query_vector, top_k, stream_id, include_archived)` — Approximate nearest neighbor search. Returns results sorted by similarity score (highest first). Score is computed as `1.0 / (1.0 + distance)`. Supports optional `stream_id` filtering via LanceDB's `only_if` clause.
- `LanceStorage::delete(memory_id)` — Deletes vectors for a memory.
- `LanceStorage::vector_count()` — Returns total vector count.

**Arrow schema:** The memories table has three columns: `memory_id` (Utf8), `vector` (FixedSizeList of Float32), `stream_id` (Utf8, nullable).

### verbatim.rs

Manages raw transcript files on the filesystem. Files are content-addressed (named by SHA-256 hash) and encrypted before writing to disk.

**Key types:**

- `VerbatimStorage` — Holds `active_dir`, `archive_dir`, and `encryption: Arc<dyn EncryptionProvider>`.

**Key functions:**

- `VerbatimStorage::content_hash(content)` — Static method. Computes SHA-256 hex string of content bytes.
- `VerbatimStorage::store(content)` — Encrypts and writes content to `active_dir/{hash}`. Content-addressed deduplication: skips write if hash already exists. Returns the content hash.
- `VerbatimStorage::read(content_hash)` — Reads and decrypts content. Checks active directory first, then archive. Verifies integrity by comparing the hash of the decrypted content against the expected hash. Returns `StorageError::HashMismatch` if integrity check fails.
- `VerbatimStorage::archive(content_hash)` — Moves a file from active to archive directory (filesystem rename).
- `VerbatimStorage::delete(content_hash)` — Permanently deletes from both active and archive (for GDPR purge operations).
- `VerbatimStorage::exists(content_hash)` — Checks existence in either directory.

### embeddings.rs

Manages embedding model loading and inference using `fastembed-rs` with ONNX Runtime backend.

**Key types:**

- `EmbeddingManager` — Holds a `Mutex<TextEmbedding>`, the dimension count, and model name. Manually implements `Send + Sync` (ONNX Runtime is thread-safe; the Mutex serializes inference calls).
- `SparseEmbedding` — Sparse vector output: `indices: Vec<u32>` and `values: Vec<f32>`.
- `SparseEmbeddingManager` — Loads and runs BGE-M3 SPLADE sparse embedding model. Also manually `Send + Sync`.

**Supported models (dense):**

| Name | Enum | Dimensions | Size |
|------|------|------------|------|
| `bge-small-en` | BGESmallENV15 | 384 | ~50MB |
| `bge-base-en` | BGEBaseENV15 | 768 | ~130MB |
| `bge-large-en` | BGELargeENV15 | 1024 | ~335MB |
| `bge-m3` | BGEM3 | 1024 | ~600MB |

Unknown model names fall back to `bge-small-en-v1.5` with a warning.

**Key functions:**

- `EmbeddingManager::new(model_name)` — Downloads model on first use, initializes ONNX Runtime.
- `EmbeddingManager::embed_documents(texts)` — Batch embedding. Returns `Vec<Vec<f32>>`.
- `EmbeddingManager::embed_query(text)` — Single query embedding.
- `EmbeddingManager::dimensions()` — Returns vector dimension.
- `SparseEmbeddingManager::new()` — Loads BGE-M3 SPLADE model.
- `SparseEmbeddingManager::embed_query(text)` — Returns a `SparseEmbedding`.
- `SparseEmbeddingManager::sparse_similarity(query, doc)` — Merge-join dot product between two sparse vectors.

## Key Public Types Other Modules Depend On

- `SqliteStorage` — used by `engine::Engine` for all structured data operations
- `LanceStorage` — used by `engine::Engine` and `retrieval` for vector search
- `VerbatimStorage` — used by `engine::Engine` for raw content read/write
- `EmbeddingManager` — used by `engine::Engine` for embedding generation
- `Memory` — the core memory record type, used throughout retrieval, server, and engine
- `RetainParams` — used by the engine retain path
- `VectorSearchResult` — used by the semantic retrieval strategy
- `SparseEmbedding` / `SparseEmbeddingManager` — available for keyword search integration

## Relevant config.toml Keys

- `[models] embedding` — Which embedding model to load (default: `"bge-m3"`)
- `[encryption] enabled` — Whether at-rest encryption is active (default: `true`)
- `[encryption] cipher` — Cipher for verbatim files (default: `"aes-256-gcm"`)
- `[encryption] sqlite_cipher` — SQLCipher cipher (default: `"aes-256-cbc"`)
- `[concurrency] write_queue_depth` — Size of the write operation queue (default: `1000`)

## Deferred / Planned Functionality

- **BGE-M3 sparse vectors in LanceDB:** The LanceDB Rust bindings do not yet natively support sparse vector columns. The `SparseEmbeddingManager` is implemented and functional, but sparse vectors are not stored in LanceDB. The keyword retrieval strategy (`retrieval::keyword`) uses SQLite LIKE matching as a fallback. When LanceDB adds sparse vector support, `lance.rs` should add a sparse vector column and the keyword strategy should query it instead.
- **Application-level LanceDB encryption:** The CLAUDE.md spec calls for encrypting LanceDB data at the application level before writing to the columnar format. This is not yet implemented; LanceDB data is currently stored unencrypted on disk.
- **Shared cache for in-memory testing:** `SqliteStorage::open_in_memory()` creates separate in-memory databases for read and write connections. Integration tests that need read-after-write should use file-backed temporary databases instead.
