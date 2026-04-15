# audit/ -- Tamper-Evident Audit Logging

## Role in Architecture

The audit module provides an append-only, tamper-evident audit log for every operation performed by the Clear Memory engine. Every read, write, import, purge, legal hold, and administrative action is recorded with a chained hash that links each entry to the previous one, forming a verifiable chain of integrity. If any entry in the chain is modified or deleted after the fact, the chain breaks and the tampering is detectable via `verify_chain`.

This module is consumed by nearly every other module in the codebase. The compliance module depends on it for purge and legal hold event recording. The server module logs every MCP/HTTP request. The security module logs authentication events and anomaly flags. The export functionality allows auditors to extract the full log in JSON or CSV format for external review.

## File-by-File Descriptions

### mod.rs

Module root. Re-exports the three submodules: `chain`, `export`, and `logger`.

### logger.rs

The core audit logging implementation. Contains:

- **`AuditEntry`** -- A struct representing a single audit log row. Fields include `id` (UUID), `timestamp` (ISO 8601), `user_id`, `operation` (e.g., "retain", "recall", "forget", "purge", "import"), `memory_id`, `stream_id`, `details` (JSON blob), `classification`, `compliance_event` (bool), `anomaly_flag` (bool), `hash` (SHA-256 chain hash of this entry), and `previous_hash` (hash of the prior entry). Derives `Serialize` and `Deserialize` for JSON export.

- **`AuditParams<'a>`** -- A borrowed parameter struct used when logging a new entry. Avoids allocating owned strings for every log call. Contains the same fields as `AuditEntry` minus `id`, `timestamp`, `hash`, and `previous_hash`, which are computed at log time.

- **`AuditLogger`** -- The stateful logger that maintains the chain. Holds the last hash in a `Mutex<String>` for thread-safe access. Constructed via `AuditLogger::new(conn)` which reads the most recent hash from the database, or `AuditLogger::new_genesis()` which starts with the genesis hash (64 zeros). The `log()` method computes the chained hash via `compute_chain_hash`, inserts the row into the `audit_log` table using rusqlite, and updates the in-memory last hash. Instrumented with `tracing::instrument`.

- **`GENESIS_HASH`** -- The starting hash for a fresh chain: 64 hex zeros.

- **`query_entries(conn, from, to, operation, stream_id, limit)`** -- A standalone function that queries audit log entries with optional filters on timestamp range, operation type, and stream ID. Builds the SQL query dynamically with parameterized inputs. Returns `Vec<AuditEntry>`.

### chain.rs

Chained hash computation and verification. Contains:

- **`compute_chain_hash(previous_hash, content) -> String`** -- Computes `SHA-256(previous_hash + "|" + content)` and returns the hex-encoded digest. This is the core tamper-evidence primitive: each entry's hash depends on the previous entry's hash, so modifying any entry invalidates all subsequent hashes.

- **`verify_chain(entries: &[(String, String, String)]) -> Result<(), String>`** -- Takes a slice of `(id, hash, previous_hash)` tuples and verifies that each entry's `previous_hash` matches the preceding entry's `hash`. Returns `Ok(())` if valid, or `Err` with the ID of the first broken entry. Note: this verifies the chain linkage but does not recompute content hashes (that would require the full entry content).

### export.rs

Audit log export for compliance and auditing. Contains:

- **`ExportFormat`** -- An enum with variants `Json` and `Csv`.

- **`export_audit_log(conn, from, to, format) -> Result<String, String>`** -- Queries up to 100,000 audit entries in the given time range and serializes them to the requested format. JSON export uses `serde_json::to_string_pretty`. CSV export writes a header row followed by one row per entry with fields: id, timestamp, user_id, operation, memory_id, stream_id, classification, compliance_event, hash.

## Key Public Types Other Modules Depend On

- `AuditLogger` -- instantiated once at engine startup, shared across all operation handlers
- `AuditEntry` -- returned by log operations and used in compliance reporting
- `AuditParams` -- passed by callers when recording operations
- `query_entries` -- used by the export module and the compliance reporting module
- `compute_chain_hash` -- used internally by the logger; also available for external verification tools
- `verify_chain` -- used by `clearmemory audit verify` CLI command

## Relevant config.toml Keys

The audit module itself has no direct config.toml keys, but is influenced by:

- `[compliance] legal_hold_enabled` -- when true, hold/release operations are logged with `compliance_event = true`
- `[security.insider_detection] alert_on_anomaly` -- when true, anomalous access is logged with `anomaly_flag = true`

## Deferred / Planned Functionality

- **External checkpoint anchors**: Every N entries (or every 6 hours), the system should write a checkpoint hash to a separate file (`~/.clearmemory/audit_checkpoints.log`), to stdout for log aggregators, and to the observability metrics pipeline. This is described in CLAUDE.md but not yet implemented in code.
- **Full content hash recomputation in verify_chain**: The current `verify_chain` checks chain linkage only. A stronger verification would recompute `compute_chain_hash` for each entry using the full entry content and confirm the stored hash matches.
- **Audit log append protection at the database level**: Currently enforced by convention (append-only). Database-level triggers or write-ahead-log protections are not yet implemented.
