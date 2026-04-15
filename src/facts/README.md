# facts/ -- Bi-Temporal Fact Extraction, Storage, and Conflict Resolution

## Role in the Architecture

The `facts` module manages the extraction, storage, and querying of temporal facts -- structured subject-predicate-object triples derived from stored memories. Facts are a progressive enhancement on top of verbatim storage: the system works perfectly fine with zero extracted facts (it still has verbatim storage + semantic search), but facts enable richer querying such as "What database does the auth service use?" or "Who was the team lead in January?"

Facts use **bi-temporal modeling**, which tracks two independent time dimensions:

1. **Real-world validity:** `valid_from` / `valid_until` -- when the fact was true in the real world. For example, "auth uses Auth0" might be valid from 2025-01-01 to 2026-03-01, and "auth uses Clerk" is valid from 2026-03-01 onward.
2. **System knowledge:** `ingested_at` / `invalidated_at` -- when Clear Memory learned about the fact and when it was marked as superseded. This allows distinguishing between "what was true at time T" and "what did we know at time T."

Old facts are invalidated, not deleted. Historical queries return historical truth. Current queries return current truth.

## File-by-File Description

### `mod.rs`

Module root. Re-exports:

- `pub mod conflict;`
- `pub mod extractor;`
- `pub mod temporal;`

### `temporal.rs`

Core fact data type and CRUD operations against SQLite.

**Key types:**

- **`Fact`** -- A temporal fact (subject-predicate-object triple with time bounds). Fields:
  - `id: String` -- UUID
  - `memory_id: String` -- The source memory this fact was extracted from (foreign key to `memories`)
  - `subject: String` -- e.g., "auth-service", "team"
  - `predicate: String` -- e.g., "uses", "decided", "switched_to"
  - `object: String` -- e.g., "Clerk", "migrate from Auth0 to Clerk"
  - `valid_from: Option<String>` -- When this became true in the real world (ISO 8601)
  - `valid_until: Option<String>` -- When this stopped being true (NULL = still true)
  - `ingested_at: String` -- When Clear Memory learned this
  - `invalidated_at: Option<String>` -- When this was marked as superseded in the system
  - `confidence: f64` -- Extraction confidence score (0.0 to 1.0)

**Key functions:**

- **`insert_fact(conn: &Connection, fact: &Fact) -> Result<(), rusqlite::Error>`** -- Inserts a new fact into the `facts` table.
- **`current_facts(conn: &Connection, subject: &str) -> Result<Vec<Fact>, rusqlite::Error>`** -- Returns all currently valid facts for a subject. Filters where `valid_until IS NULL AND invalidated_at IS NULL`. Case-insensitive subject match. Ordered by `ingested_at DESC`.
- **`facts_at(conn: &Connection, subject: &str, timestamp: &str) -> Result<Vec<Fact>, rusqlite::Error>`** -- Returns facts that were valid at a specific point in time. Uses the `valid_from` / `valid_until` bounds: includes facts where `valid_from <= timestamp` and `valid_until > timestamp` (or NULL). This is the bi-temporal point-in-time query.
- **`fact_history(conn: &Connection, subject: &str) -> Result<Vec<Fact>, rusqlite::Error>`** -- Returns the full history of facts for a subject, including invalidated ones. Ordered by `ingested_at ASC`. Useful for auditing and understanding how knowledge evolved.

### `extractor.rs`

Rule-based fact extraction from text content (Tier 1 implementation).

**Design philosophy:** Conservative extraction -- better to miss facts than to extract incorrect ones. The system works fine with zero extracted facts. Facts are a progressive enhancement.

**Key functions:**

- **`extract_facts(content: &str, memory_id: &str) -> Vec<Fact>`** -- Scans text content line by line and extracts facts using pattern matching. Returns a list of `Fact` structs with generated UUIDs, timestamps, and confidence scores. Currently detects two pattern families:

  1. **Uses/migration patterns** (confidence: 0.7):
     - "X uses Y"
     - "X switched to Y"
     - "X migrated to Y"
     - "X replaced by Y"
  
  2. **Decision patterns** (confidence: 0.6):
     - "decided to X"
     - "we decided to X"
     - "team decided to X"

  Subject and object lengths are capped at 100 characters (uses patterns) or 200 characters (decision patterns) to avoid false positives on long sentences.

- **`try_extract_uses_pattern(line: &str, memory_id: &str, now: &str) -> Option<Fact>`** (private) -- Attempts to extract a uses/migration fact from a single line.
- **`try_extract_decision_pattern(line: &str, memory_id: &str, now: &str) -> Option<Fact>`** (private) -- Attempts to extract a decision fact. If the line starts with "we" or "team", the subject is set to "team".

### `conflict.rs`

Detects and resolves contradictions between facts.

**Key types:**

- **`Conflict`** -- A detected conflict between two facts. Fields:
  - `existing_fact_id: String` -- The ID of the existing fact that conflicts
  - `new_fact: Fact` -- The new fact being ingested
  - `reason: String` -- Human-readable description of the conflict

**Key functions:**

- **`detect_conflicts(conn: &Connection, new_fact: &Fact) -> Result<Vec<Conflict>, rusqlite::Error>`** -- Checks for conflicts between a new fact and existing active facts. The conflict rule is: same subject + same predicate + different object = conflict. Only checks against facts where `valid_until IS NULL AND invalidated_at IS NULL` (currently active facts). Case-insensitive comparison on subject and predicate; case-insensitive comparison on object to determine if they differ.

- **`resolve_conflicts(conn: &Connection, conflicts: &[Conflict]) -> Result<usize, rusqlite::Error>`** -- Resolves conflicts by invalidating the older facts. Sets both `valid_until` and `invalidated_at` to the current timestamp on each conflicting existing fact. This is the Tier 1 resolution strategy (timestamp-based). Returns the count of resolved conflicts.

  **Planned:** Tier 2+ conflict resolution that uses the local LLM to verify whether a detected conflict is real (e.g., "auth uses Clerk for SSO" and "auth uses Auth0 for legacy" might not actually conflict).

## Key Public Types Other Modules Depend On

- **`Fact`** -- Used throughout the system: by the import pipeline (which calls `extract_facts` on ingested content), by the `clearmemory_forget` operation (which sets `valid_until` on associated facts), and by the MCP tools for fact queries.
- **`Conflict`** -- Used by the ingestion pipeline to detect and resolve contradictions when new memories are stored.
- **`extract_facts()`** -- Called during the `retain` / `import` operations to extract structured facts from raw text.
- **`detect_conflicts()` / `resolve_conflicts()`** -- Called after fact extraction to maintain fact consistency.

## Relevant config.toml Keys

```toml
[general]
tier = "offline"                # Tier 1 uses rule-based extraction + timestamp conflict resolution
                                # Tier 2+ will use LLM-enhanced extraction and conflict verification
```

## Deferred / Planned Functionality

- **LLM-enhanced fact extraction (Tier 2+):** Using the curator model to extract richer, more accurate facts from text beyond the current rule-based patterns.
- **LLM-enhanced conflict verification (Tier 2+):** Using the local LLM to verify whether detected conflicts are genuine contradictions or false positives (e.g., different contexts where both facts can be true simultaneously).
- **Additional extraction patterns:** The current extractor handles "uses", "switched to", "migrated to", "replaced by", and "decided to". More patterns (ownership, assignments, status changes) are planned.
- **Confidence calibration:** The current confidence scores (0.7 for uses patterns, 0.6 for decisions) are fixed. Future work may calibrate these based on extraction accuracy measurements.
