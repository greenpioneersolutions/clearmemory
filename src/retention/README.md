# retention/ -- Corpus Growth Management and Memory Archival

## Role in the Architecture

The `retention` module manages corpus growth through three complementary policies that identify memories eligible for archival. Clear Memory never deletes memories through retention -- it archives them by setting `archived = 1` in the database. Archived memories are excluded from normal search results but remain queryable with the `--include-archive` flag. Verbatim files for archived memories can be moved to `~/.clearmemory/archive/verbatim/` and LanceDB vectors removed from the active index.

The three retention triggers work together:

1. **Time-based:** Memories older than a threshold that have not been recently accessed are flagged as stale.
2. **Size-based:** When the corpus exceeds a disk space threshold, the oldest and least-accessed memories are identified as archival candidates.
3. **Performance-based:** When p95 retrieval latency degrades beyond a threshold, the system identifies which memories to archive to restore performance.

The archiver enforces legal hold checks before any archival operation -- if a memory's stream is under legal hold (managed by `src/compliance/legal_hold.rs`), archival is blocked.

## File-by-File Description

### `mod.rs`

Module root. Re-exports:

- `pub mod archiver;`
- `pub mod performance_policy;`
- `pub mod size_policy;`
- `pub mod time_policy;`

### `time_policy.rs`

Time-based retention: identifies stale memories based on age and access recency.

**Key functions:**

- **`find_stale_memories(conn: &Connection, threshold_days: i64) -> Result<Vec<String>>`** -- Finds memories older than `threshold_days` that have not been accessed recently. A memory is stale if:
  - It was created more than `threshold_days` ago, AND
  - Its `last_accessed_at` is either NULL or also older than `threshold_days`, AND
  - It is not already archived (`archived = 0`)
  
  Returns memory IDs ordered by `created_at ASC` (oldest first). The staleness clock resets on every access, so a 6-month-old memory recalled last week stays active. Uses SQLite's `datetime('now', '-N days')` for threshold calculation.

### `size_policy.rs`

Size-based retention: monitors corpus disk usage and identifies archival candidates.

**Key functions:**

- **`calculate_corpus_size(data_dir: &Path) -> Result<u64>`** -- Calculates the total size of files in the `verbatim/` subdirectory of the data directory (in bytes). Returns 0 if the directory does not exist. Only counts regular files, not subdirectories.

- **`find_archival_candidates(conn: &Connection, limit: u32) -> Result<Vec<String>>`** -- Returns up to `limit` memory IDs that are the best candidates for archival. Candidates are selected by: lowest `access_count` first, then oldest `created_at`. Excludes already-archived memories. This prioritizes memories that are both old and rarely accessed.

### `performance_policy.rs`

Performance-based retention: monitors p95 retrieval latency and detects degradation.

**Key functions:**

- **`measure_p95_latency(conn: &Connection) -> Result<f64, rusqlite::Error>`** -- Returns the most recent p95 recall latency from the `performance_baselines` table. Returns `0.0` if no baselines have been recorded yet.

- **`record_baseline(conn: &Connection, p95_ms: f64, corpus_size: i64, memory_count: i64) -> Result<(), rusqlite::Error>`** -- Records a new performance baseline measurement. Creates a new row in `performance_baselines` with a generated UUID and current timestamp. Called periodically during operation and on startup.

- **`check_degradation(conn: &Connection, threshold_ms: f64) -> Result<Option<f64>, rusqlite::Error>`** -- Checks whether the latest p95 latency exceeds the configured threshold. Returns `Some(current_p95)` if degraded, `None` if within threshold or no baselines exist.

### `archiver.rs`

Executes archival operations with safety checks.

**Key functions:**

- **`archive_memory(conn: &Connection, memory_id: &str) -> Result<()>`** -- Archives a single memory. The operation:
  1. Looks up the memory's `stream_id`
  2. If the memory belongs to a stream, checks if that stream has an active legal hold (via `crate::compliance::legal_hold::is_held`). If held, returns an error: "cannot archive: stream 'X' is under legal hold"
  3. Sets `archived = 1` on the memory (only if currently `archived = 0`)
  4. Logs a retention event in the `retention_events` table with `trigger_type = 'archive'`
  5. Returns an error if the memory does not exist or is already archived

## Key Public Types Other Modules Depend On

- **`find_stale_memories()`** -- Called by the CLI `archive --dry-run` command and retention background tasks.
- **`calculate_corpus_size()`** -- Called by the `clearmemory_status` MCP tool and the size-based retention check.
- **`find_archival_candidates()`** -- Called when size thresholds are exceeded to present candidates to the user.
- **`check_degradation()`** / **`record_baseline()`** -- Called by the engine's startup health check and periodic monitoring.
- **`archive_memory()`** -- Called by the CLI `archive --confirm` command after user approval.

## Relevant config.toml Keys

```toml
[retention]
time_threshold_days = 90        # Archive memories older than this if not accessed
size_threshold_gb = 2           # Warn and offer archival above this corpus size
performance_threshold_ms = 200  # Flag performance degradation above this p95
auto_archive = false            # If true, archive without confirmation (enterprise setting)
```

## Deferred / Planned Functionality

- **Verbatim file relocation:** The archiver currently only sets the `archived` flag in SQLite. Moving verbatim files from `~/.clearmemory/verbatim/` to `~/.clearmemory/archive/verbatim/` is not yet implemented.
- **LanceDB vector removal:** Removing archived memory vectors from the active LanceDB index to reduce search corpus size is not yet implemented.
- **User approval workflow:** The architecture describes a flow where the system warns the user before archiving, showing candidates and allowing approval, skip, or threshold adjustment. The current code provides the building blocks (find candidates, archive individual memories) but not the interactive workflow.
- **Background retention task:** A periodic task that runs during `clearmemory serve` to automatically evaluate retention policies is planned but not yet wired up.
- **Retention event reporting:** The `retention_events` table is populated but not yet surfaced through the CLI or MCP status tool.
