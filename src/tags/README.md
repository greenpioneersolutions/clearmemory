# tags/ -- Memory Tag Taxonomy (Team, Repo, Project, Domain)

## Role in the Architecture

The `tags` module implements the four-type tag taxonomy that organizes memories across organizational dimensions. Tags are the primary mechanism for scoping and filtering memories. They feed into streams (scoped views across tag intersections) and are used by the retrieval pipeline to narrow search results.

Clear Memory supports exactly four tag types:

| Tag Type | Description | Examples |
|----------|-------------|---------|
| **team** | Organizational team | `platform`, `frontend`, `security` |
| **repo** | Code repository | `auth-service`, `api-gateway` |
| **project** | Business initiative | `q1-migration`, `soc2-audit` |
| **domain** | Knowledge domain (nestable via `/`) | `security`, `security/auth`, `infrastructure/ci-cd` |

Tags are **optional**. The system works with zero tags -- everything goes into a default stream. Tags are a power-user feature that progressively improves retrieval as users invest in them. ClearPathAI can auto-tag based on the active workspace/repo context.

Tags are stored in the `memory_tags` SQLite table as a many-to-many relationship between memories and (tag_type, tag_value) pairs.

## File-by-File Description

### `mod.rs`

Module root. Re-exports:

- `pub mod taxonomy;`

### `taxonomy.rs`

All tag operations: validation, parsing, CRUD against SQLite, and querying.

**Key constants:**

- **`TAG_TYPES: &[&str]`** -- The four valid tag types: `["team", "repo", "project", "domain"]`.

**Key types:**

- **`Tag`** -- A tag attached to a memory. Fields:
  - `tag_type: String` -- One of the four valid types
  - `tag_value: String` -- The tag value (e.g., "platform", "security/auth")

**Key functions:**

- **`validate_tag_type(tag_type: &str) -> Result<(), String>`** -- Validates that a tag type is one of the four supported types. Returns a descriptive error string if invalid.

- **`parse_tag(s: &str) -> Result<(String, String), String>`** -- Parses a tag string in `type:value` format (e.g., "team:platform", "domain:security/auth"). Validates the type and requires a non-empty value. Returns `(tag_type, tag_value)`. Uses `splitn(2, ':')` so the value can contain colons.

- **`list_tags(conn: &Connection, tag_type: Option<&str>) -> Result<Vec<Tag>, rusqlite::Error>`** -- Lists all distinct tags in the system. Optionally filters by tag type. Returns unique (tag_type, tag_value) pairs ordered by tag_type then tag_value.

- **`add_tag(conn: &Connection, memory_id: &str, tag_type: &str, tag_value: &str) -> Result<(), rusqlite::Error>`** -- Adds a tag to a memory. Uses `INSERT OR IGNORE` so duplicate tags are silently skipped.

- **`remove_tag(conn: &Connection, memory_id: &str, tag_type: &str, tag_value: &str) -> Result<(), rusqlite::Error>`** -- Removes a specific tag from a specific memory.

- **`rename_tag(conn: &Connection, tag_type: &str, old_value: &str, new_value: &str) -> Result<usize, rusqlite::Error>`** -- Renames a tag value across all memories. Returns the number of rows updated. Useful for organizational changes (e.g., renaming a team).

- **`get_memory_tags(conn: &Connection, memory_id: &str) -> Result<Vec<Tag>, rusqlite::Error>`** -- Returns all tags for a specific memory, ordered by tag_type then tag_value.

## Key Public Types Other Modules Depend On

- **`Tag`** -- Used by the MCP `clearmemory_tags` tool, the import pipeline (which attaches tags during ingestion), and stream definitions (which are built from tag intersections).
- **`TAG_TYPES`** -- Referenced for validation anywhere tags are accepted (CLI parsing, MCP handlers, import pipelines).
- **`parse_tag()`** -- Used by CLI argument parsing to convert user-provided tag strings like `"team:platform"` into structured (type, value) pairs.
- **`get_memory_tags()`** -- Used by the retrieval pipeline when returning memory metadata and by stream filtering logic.

## Relevant config.toml Keys

Tags do not have dedicated config.toml keys. They are managed entirely through the CLI (`clearmemory tags`) and MCP (`clearmemory_tags`) interfaces. ClearPathAI may auto-assign tags based on workspace context via integration hooks.

## Deferred / Planned Functionality

- **Auto-tagging on import:** Using heuristics or the curator model to automatically suggest tags based on content analysis (e.g., detecting repository names, team references, or domain keywords in ingested text).
- **Tag hierarchy for domains:** The domain tag type supports `/`-separated nesting (e.g., `security/auth`), but there is no hierarchical query support yet (e.g., querying `security` to also match `security/auth`).
- **Tag statistics:** Showing memory counts per tag, most active tags, tag co-occurrence analysis for organizational insights.
- **Tag merge/deprecation:** Workflow for deprecating old tags and migrating memories to new ones (beyond the current `rename_tag` which is a simple value swap).
