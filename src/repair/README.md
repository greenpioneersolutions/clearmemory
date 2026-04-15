# repair/ -- Integrity Checking and Index Rebuild

## Role in Architecture

The repair module provides tools for diagnosing and recovering from data corruption. It has two capabilities: SQLite integrity checking (detecting database corruption) and LanceDB index rebuilding (re-embedding all memories from SQLite and verbatim files when the vector index is damaged or needs to be recreated). This module backs the `clearmemory repair` CLI command and is also used during startup health checks.

When a user reports degraded retrieval quality, missing search results, or the engine refuses to start due to corruption, this module provides the diagnostic and recovery path. The integrity check is non-destructive (read-only), while the index rebuild is a write operation that creates a fresh LanceDB index from the authoritative data in SQLite and verbatim files.

## File-by-File Descriptions

### mod.rs

Module root. Re-exports: `integrity`, `rebuild`.

### integrity.rs

SQLite database integrity checking. Contains:

- **`IntegrityReport`** -- A serializable struct with a single field: `issues` (Vec<String>). Each string describes a corruption issue found by SQLite's integrity check. Provides an `is_ok()` method that returns `true` when the issues list is empty.

- **`check_integrity(conn) -> Result<IntegrityReport>`** -- Runs SQLite's `PRAGMA integrity_check` and collects the results. SQLite returns `"ok"` when the database is healthy; any other string indicates a specific corruption issue. The function filters out `"ok"` responses and returns only actual issues. This is a read-only operation that does not modify the database.

### rebuild.rs

LanceDB vector index rebuild from authoritative sources. Contains:

- **`rebuild_index() -> Result<()>`** -- Currently a **stub** that logs an info message and returns `Ok(())`. The full implementation will:
  1. Read all active (non-archived) memories from SQLite
  2. Load each memory's verbatim content from the `~/.clearmemory/verbatim/` directory
  3. Re-embed each memory using the configured embedding model (BGE-M3 or bge-small-en via fastembed)
  4. Write new dense and sparse vectors to a fresh LanceDB collection
  5. Atomically swap the old LanceDB index directory for the new one

  The operation is designed to be pausable/resumable (progress tracking is handled by `migration::reindex::ReindexState`), but this is not yet wired up. The stub exists so that the CLI command and repair pipeline have a callable interface.

## Key Public Types Other Modules Depend On

- `IntegrityReport` -- returned by `check_integrity`, used by the health check and the `clearmemory repair --check-only` CLI command
- `IntegrityReport::is_ok()` -- used to determine if the database is healthy during startup
- `check_integrity` -- called during startup health checks and by `clearmemory repair`
- `rebuild_index` -- called by `clearmemory repair --rebuild-index` and `clearmemory reindex`

## Relevant config.toml Keys

There are no direct config.toml keys for the repair module. It is influenced by:

```toml
[models]
embedding = "bge-m3"       # determines which model rebuild_index will use for re-embedding
model_path = ""             # where to find the model files
```

## Deferred / Planned Functionality

- **Full LanceDB index rebuild**: The `rebuild_index` function is a stub. It requires the LanceDB crate and fastembed crate integrations to be wired up. Once those are available, the function will iterate over all active memories, embed them, and write to a new LanceDB collection.
- **Pausable/resumable rebuild**: The `migration::reindex` module provides state tracking (`ReindexState`, `save_reindex_state`, `load_reindex_state`). Wiring this into `rebuild_index` will enable interruption and resumption of long-running reindex operations.
- **Atomic index swap**: On completion, the old LanceDB directory should be renamed and the new one put in place atomically, so queries never see a partially-built index.
- **Verbatim file integrity checking**: Verifying SHA-256 checksums of verbatim files against the `content_hash` column in the `memories` table. Currently only SQLite integrity is checked.
- **LanceDB consistency checking**: Verifying that every active memory in SQLite has a corresponding vector in LanceDB, and vice versa. Detecting orphaned vectors or missing embeddings.
- **Auto-repair mode**: Automatically rebuilding the LanceDB index when corruption is detected, rather than requiring manual intervention.
