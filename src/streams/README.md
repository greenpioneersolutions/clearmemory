# streams/ -- Scoped Views, Access Control, and Stream Management

## Role in the Architecture

The `streams` module implements streams -- scoped views across tag intersections that organize how memories are grouped, accessed, and secured. A stream is not a separate data store; it is a named filter defined by a set of tag criteria. For example, a stream named "Platform Auth" might be defined by the intersection of `team:platform` and `domain:security/auth`, showing only memories that carry both tags.

Streams serve three purposes:

1. **Organization:** Group related memories across tag dimensions for focused retrieval. When a user searches within a stream, only memories matching the stream's tag filters are included.
2. **Context scoping:** The context compiler uses the active stream to determine what goes into L1 (working set) context injection.
3. **Access control:** Each stream has an owner, a visibility level (`private`, `team`, `org`), and an explicit list of authorized writers. This is the primary security boundary for multi-user deployments.

Stream data is stored across three SQLite tables: `streams` (core metadata), `stream_tags` (tag filters that define the stream's scope), and `stream_writers` (users with write access beyond the owner).

## File-by-File Description

### `mod.rs`

Module root. Re-exports:

- `pub mod manager;`
- `pub mod security;`

### `manager.rs`

Stream CRUD operations and tag filter management.

**Key types:**

- **`Stream`** -- A stream definition. Fields:
  - `id: String` -- UUID
  - `name: String` -- Human-readable stream name
  - `description: Option<String>` -- Optional description
  - `owner_id: String` -- User ID of the stream creator
  - `visibility: String` -- One of: `"private"`, `"team"`, `"org"`
  - `created_at: String` -- ISO 8601 creation timestamp

**Key functions:**

- **`create_stream(conn: &Connection, name: &str, description: Option<&str>, owner_id: &str, visibility: &str, tags: &[(String, String)]) -> Result<String, rusqlite::Error>`** -- Creates a new stream with a generated UUID and associates the given tag filters. Inserts into both the `streams` and `stream_tags` tables in a single operation. Returns the new stream ID.

- **`list_streams(conn: &Connection) -> Result<Vec<Stream>, rusqlite::Error>`** -- Lists all streams, ordered by name. Note: this does not filter by visibility -- the caller (server/handler layer) is responsible for applying access control using the `security` module.

- **`get_stream(conn: &Connection, id_or_name: &str) -> Result<Option<Stream>, rusqlite::Error>`** -- Looks up a stream by either its UUID or its name. Returns `None` if not found.

- **`grant_write_access(conn: &Connection, stream_id: &str, user_id: &str) -> Result<(), rusqlite::Error>`** -- Grants write access to a user on a stream. Uses `INSERT OR IGNORE` so duplicate grants are silently skipped. Write access implicitly grants read access for `team`-visibility streams.

- **`get_stream_tags(conn: &Connection, stream_id: &str) -> Result<Vec<(String, String)>, rusqlite::Error>`** -- Returns the tag filters that define a stream's scope as `(tag_type, tag_value)` pairs.

### `security.rs`

Permission checking for stream access. This is application-level enforcement -- the caller checks permissions before performing operations.

**Key functions:**

- **`can_read(conn: &Connection, user_id: &str, stream_id: &str) -> Result<bool, rusqlite::Error>`** -- Checks if a user can read from a stream. The rules are:
  - `org` visibility: anyone can read
  - `team` visibility: the owner can read, plus anyone with write access (write implies read)
  - `private` visibility: only the owner can read

- **`can_write(conn: &Connection, user_id: &str, stream_id: &str) -> Result<bool, rusqlite::Error>`** -- Checks if a user can write to a stream. The owner always has write access. Other users need an explicit entry in `stream_writers`.

- **`has_write_access(conn: &Connection, user_id: &str, stream_id: &str) -> Result<bool, rusqlite::Error>`** (private) -- Helper that checks the `stream_writers` table for an explicit grant.

## Key Public Types Other Modules Depend On

- **`Stream`** -- Used by MCP handlers (`clearmemory_streams` tool), the context compiler (to determine active stream context), and the retrieval pipeline (to scope searches).
- **`can_read()` / `can_write()`** -- Called by MCP/HTTP request handlers before any stream-scoped operation to enforce access control.
- **`create_stream()` / `get_stream()`** -- Used by CLI commands and MCP tools for stream management.

## Relevant config.toml Keys

```toml
[general]
default_stream = "default"      # The stream used when no stream is explicitly specified
```

## Deferred / Planned Functionality

- **Stream deletion:** There is no `delete_stream` function yet. Streams can be created but not removed.
- **Visibility update:** No function to change a stream's visibility after creation.
- **Revoke write access:** No function to remove a user's write access (only `grant_write_access` exists).
- **Stream-scoped queries:** The retrieval pipeline integration (filtering memories by stream tag filters during search) is handled at a higher level, not within this module.
- **Related stream discovery:** The architecture calls for checking "related streams" (adjacent tag intersections) when searching within a stream. This is not yet implemented.
- **Database-level encryption per stream:** Per the CLAUDE.md, per-stream encryption with separate keys is a v2 feature. V1 encrypts everything with one master key.
