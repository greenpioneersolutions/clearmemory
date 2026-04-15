# compliance/ -- Data Governance, Legal Hold, Purge, and Reporting

## Role in Architecture

The compliance module implements the data governance features required for enterprise deployments: data classification, permanent deletion (purge) for GDPR/CCPA right-to-delete, legal hold to freeze streams during litigation, and compliance reporting for auditors. It works closely with the audit module (all compliance events are logged), the security module (secret scanning drives classification), and the storage module (purge removes records from SQLite and associated data).

Classification is the foundation: every memory carries a label (public, internal, confidential, pii) that controls how it flows through the system. The `Classification` enum is defined in `lib.rs` and used here. Content containing detected secrets is auto-classified as `Confidential`. PII-classified content is blocked from Tier 3 cloud API calls. Legal holds prevent purge, forget, and archival operations on held streams. The compliance report aggregates all of this into a single auditor-friendly summary.

## File-by-File Descriptions

### mod.rs

Module root. Re-exports: `classification`, `legal_hold`, `purge`, `reporting`.

### classification.rs

Content classification based on secret scanning results. Contains:

- **`classify_content(content: &str, scanner: &SecretScanner) -> Classification`** -- Takes raw content and a `SecretScanner` (from `crate::security::secret_scanner`). If the scanner detects any secrets in the content, returns `Classification::Confidential`. Otherwise, returns `Classification::Internal` (the default classification). This is called on the retain path before storage.

Note: The `Classification` enum itself (`Public`, `Internal`, `Confidential`, `Pii`) is defined in `crate::lib.rs`, not in this module. It derives `PartialOrd`/`Ord` so classification levels can be compared (Public < Internal < Confidential < Pii).

### purge.rs

Hard deletion for GDPR/CCPA right-to-delete compliance. Contains:

- **`check_legal_hold(conn, memory_id) -> Result<()>`** -- Queries the database to determine whether the memory's stream has an active (unreleased) legal hold. Joins `legal_holds` with `memories` on `stream_id`. Returns `Ok(())` if purge is allowed, or returns an `anyhow::Error` wrapping `ComplianceError::LegalHoldActive { stream_id, reason }` if the stream is held. Callers must call this before `purge_memory`.

- **`purge_memory(conn, memory_id) -> Result<()>`** -- Permanently deletes a memory and all associated data from SQLite. Deletes from (in order): `entity_relationships`, `facts`, `memory_tags`, `memories`. Each delete is a separate `conn.execute` call. Does not error if the memory does not exist (DELETE affects 0 rows). Does NOT delete verbatim files or LanceDB vectors -- those must be handled separately by the caller. Does NOT check legal holds -- the caller must call `check_legal_hold` first.

- **`OptionalExt<T>`** (private trait) -- A helper trait on `Result<T, rusqlite::Error>` that converts `QueryReturnedNoRows` errors into `Ok(None)`. Used by `check_legal_hold` to handle the case where no hold exists.

### legal_hold.rs

Legal hold management -- freeze streams for litigation or compliance. Contains:

- **`LegalHold`** -- A serializable struct with fields: `id` (UUID), `stream_id`, `reason`, `held_by` (user who created the hold), `held_at` (ISO 8601), `released_at` (Option), `released_by` (Option). Derives `Serialize`/`Deserialize` for JSON output.

- **`create_hold(conn, stream_id, reason, held_by) -> Result<String>`** -- Inserts a new legal hold record. Returns the generated UUID. The stream must already exist in the `streams` table (foreign key constraint).

- **`release_hold(conn, hold_id, released_by) -> Result<()>`** -- Sets `released_at` and `released_by` on an active hold. Errors if the hold does not exist or is already released (UPDATE affects 0 rows).

- **`is_held(conn, stream_id) -> Result<bool>`** -- Returns `true` if the stream has at least one active (unreleased) hold. Multiple holds can exist on the same stream.

- **`list_holds(conn) -> Result<Vec<LegalHold>>`** -- Returns all holds (both active and released), ordered by `held_at` descending (newest first).

### reporting.rs

Compliance report generation for auditors. Contains:

- **`ComplianceReport`** -- A serializable struct with fields: `memory_count` (i64, total memories), `classification_counts` (HashMap<String, i64>, count per classification label), `pii_count` (i64, extracted from classification_counts), `active_holds_count` (i64, unreleased legal holds).

- **`generate_report(conn) -> Result<ComplianceReport>`** -- Queries the database and assembles the report. Counts total memories, groups by classification, counts active legal holds. Returns the assembled `ComplianceReport`.

## Key Public Types Other Modules Depend On

- `classify_content` -- called on the retain path by the storage/engine module
- `check_legal_hold` / `purge_memory` -- called by the `clearmemory purge` CLI command and the compliance handler
- `LegalHold` / `create_hold` / `release_hold` / `is_held` / `list_holds` -- called by `clearmemory hold` CLI commands and the MCP/HTTP handlers
- `ComplianceReport` / `generate_report` -- called by `clearmemory compliance report` CLI command
- `ComplianceError` (defined in `lib.rs`) -- `LegalHoldActive`, `PurgeRequiresConfirmation`, `PurgeRequiresApproval`, `PurgeRequestExpired`

## Relevant config.toml Keys

```toml
[compliance]
default_classification = "internal"             # default for memories without explicit classification
pii_detection_enabled = false                   # enable automatic PII detection
require_classification_on_retain = false        # require explicit classification on every retain
legal_hold_enabled = true                       # enable legal hold functionality
purge_requires_two_person = false               # require two-person authorization for purge
purge_request_ttl_hours = 72                    # pending purge requests expire after this
```

## Deferred / Planned Functionality

- **Two-person purge authorization**: The `ComplianceError::PurgeRequiresApproval` variant exists but the request/approve workflow (pending purge requests table, approval flow, TTL expiration) is not yet implemented.
- **Verbatim file and LanceDB vector deletion on purge**: `purge_memory` only deletes SQLite records. The caller must separately remove verbatim files from `~/.clearmemory/verbatim/` and `~/.clearmemory/archive/verbatim/`, and delete vectors from LanceDB.
- **PII detection**: Automatic PII detection (names, emails, addresses, SSNs) beyond the secret scanner patterns is planned but not implemented.
- **Classification-aware Tier 3 cloud filtering**: The classification pipeline tracing described in CLAUDE.md (tracking source classifications through curator and reflect outputs to block confidential/PII content from cloud APIs) is not yet implemented.
- **Per-stream compliance reporting**: The current report is corpus-wide. Per-stream breakdowns, age distribution, and recent purge history are planned.
- **CSV export format for compliance reports**: The `generate_report` function returns a struct; CSV serialization for auditor export is not yet implemented.
