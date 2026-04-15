# migration/ -- Schema Versioning and Database Migrations

## Role in Architecture

The migration module is responsible for evolving the SQLite database schema over time. It tracks the current schema version, applies pending migrations in sequence on startup, prevents downgrade corruption (refuses to start if the database is newer than the binary), and supports pausable/resumable embedding model reindexing when the vector model changes. Every migration is logged to a `migration_log` table with status tracking (in_progress, success, failed, rolled_back).

This module runs early in the engine startup sequence -- before any other module accesses the database. The `run_migrations` function is called during `clearmemory init` and on every subsequent startup. Migration SQL files are embedded into the binary at compile time via `include_str!`, so the binary is fully self-contained.

## File-by-File Descriptions

### mod.rs

Module root. Re-exports: `reindex`, `runner`, `versioning`.

### versioning.rs

Schema version tracking and compatibility checks. Contains:

- **`CURRENT_VERSION: i64`** -- A constant set to `1`. This is the schema version the current binary expects. Bump this when adding new migrations.

- **`get_current_version(conn) -> Result<i64, rusqlite::Error>`** -- Reads the highest version from the `schema_version` table. Returns `0` if the table does not exist yet (fresh database). Safely checks for table existence via `sqlite_master` before querying.

- **`check_compatibility(conn) -> Result<(), String>`** -- Compares the database's schema version against `CURRENT_VERSION`. If the database version is higher than the binary expects, returns an error telling the user to upgrade the binary or restore from backup. This prevents downgrade corruption where an older binary misinterprets a newer schema.

### runner.rs

Migration execution engine. Contains:

- **`run_migrations(conn) -> Result<(), String>`** -- The main entry point. Calls `check_compatibility` first, then reads the current version. If the database is already at or above the target version, logs and returns. Otherwise, applies each migration version in sequence from `current_version + 1` to `CURRENT_VERSION`.

- **`apply_migration(conn, from_version, to_version) -> Result<(), String>`** (private) -- Applies a single migration. Generates a UUID for tracking, logs the attempt to `migration_log` (tolerates the table not existing for the first migration), executes the SQL batch, and logs the result (success or failure with error message). Uses `conn.execute_batch` for SQL execution. On failure, logs a warning via `tracing::warn`.

- **`get_migration_sql(version) -> Result<String, String>`** (private) -- Maps version numbers to embedded SQL strings. Version 1 maps to `migrations/001_initial_schema.sql` via `include_str!`. Returns an error for unknown versions.

- **`log_migration_start` / `log_migration_complete`** (private) -- Helper functions that write to the `migration_log` table. Use `INSERT OR IGNORE` for the start (tolerates table not yet existing) and `UPDATE` for completion.

### reindex.rs

Embedding model reindex state tracking. When the embedding model changes (e.g., from bge-small to BGE-M3), the entire corpus must be re-embedded. This module tracks progress in a JSON file on disk so the operation can be paused and resumed. Contains:

- **`ReindexState`** -- Serializable struct with fields: `total_memories` (u64), `processed` (u64), `last_memory_id` (Option<String>, for resume), `started_at` (ISO 8601), `model_name` (String, e.g., "bge-m3").

- **`ReindexProgress`** -- Computed progress information: `percentage` (f64, 0.0-100.0), `memories_remaining` (u64), `estimated_seconds` (Option<u64>, based on elapsed time and throughput).

- **`save_reindex_state(data_dir, state) -> Result<(), io::Error>`** -- Writes the state as pretty-printed JSON to `<data_dir>/reindex_state.json`.

- **`load_reindex_state(data_dir) -> Result<Option<ReindexState>, io::Error>`** -- Loads the state file. Returns `None` if no file exists.

- **`clear_reindex_state(data_dir) -> Result<(), io::Error>`** -- Removes the state file. Called on completion or cancellation. No error if the file does not exist.

- **`calculate_progress(state) -> ReindexProgress`** -- Computes percentage and remaining time. Handles zero-total edge case (returns 100%). Estimates remaining time by dividing remaining memories by the observed throughput (processed / elapsed seconds).

## Key Public Types Other Modules Depend On

- `runner::run_migrations` -- called by `engine` module during startup and by `clearmemory init`
- `versioning::get_current_version` -- used by the health check and status endpoints
- `versioning::CURRENT_VERSION` -- used for compatibility checks
- `versioning::check_compatibility` -- called at startup to prevent downgrade corruption
- `ReindexState` / `save_reindex_state` / `load_reindex_state` -- used by the `clearmemory reindex` CLI command
- `calculate_progress` -- used by the status endpoint to show reindex progress

## Relevant config.toml Keys

```toml
[migrations]
auto_migrate = true              # automatically apply pending migrations on startup
backup_before_migrate = true     # create a backup before applying migrations
```

## Deferred / Planned Functionality

- **Transactional migration rollback**: The CLAUDE.md specifies that migrations should be transactional with rollback on failure. The current implementation uses `execute_batch` which does not wrap in an explicit transaction. Adding `BEGIN/COMMIT/ROLLBACK` wrapping would provide atomicity guarantees.
- **Actual reindex execution**: The `reindex.rs` module tracks state but the actual re-embedding loop (read memory, embed with new model, write to LanceDB) is not yet implemented. It depends on the LanceDB and fastembed crate integrations being wired up. See also `repair/rebuild.rs`.
- **Migration version 2+**: Only migration 001 (initial schema) exists. Future migrations will be added as new SQL files in `migrations/` and new match arms in `get_migration_sql`.
