# entities/ -- Entity Resolution and Knowledge Graph

## Role in the Architecture

The `entities` module manages the entity graph -- a knowledge graph of people, services, projects, and concepts that appear across stored memories. It is one of the four parallel retrieval strategies: when a user query mentions a known entity (e.g., "What did Kai work on?"), the entity graph traversal strategy follows relationships from that entity to connected memories.

Entity resolution maps natural language mentions to canonical entity nodes. In Tier 1 (offline), resolution is heuristic: exact string matching, case-insensitive lookup, and configurable aliases. In Tier 2+, the resolution is enhanced by a local LLM that can link fuzzy mentions like "the auth service", "our OAuth system", and "login microservice" to the same entity node. The LLM-enhanced resolution is **planned but not yet integrated**.

All entity data is stored in SQLite using three tables: `entities` (nodes), `entity_aliases` (alternative names that map to entities), and `entity_relationships` (directed edges between entities with temporal bounds and memory provenance).

## File-by-File Description

### `mod.rs`

Module root. Re-exports:

- `pub mod aliases;`
- `pub mod graph;`
- `pub mod resolver;`

### `resolver.rs`

Defines the entity resolution trait and the Tier 1 heuristic implementation.

**Key types:**

- **`EntityResolver` (trait)** -- The interface for entity resolution. Requires `Send + Sync`.
  - `fn resolve(&self, conn: &Connection, mention: &str) -> Option<String>` -- Takes a text mention and returns the entity ID if a match is found, or `None`.

- **`HeuristicResolver`** -- Tier 1 implementation. Resolution strategy:
  1. Try finding an entity by canonical name (case-insensitive) via `graph::find_entity`
  2. If not found, try alias lookup via `aliases::find_by_alias`
  3. Return `None` if neither matches

**Planned:** An `LlmResolver` for Tier 2+ that uses the curator model (Qwen3-0.6B) to perform fuzzy entity linking.

### `aliases.rs`

CRUD operations for entity aliases -- alternative names that resolve to the same entity.

**Key functions:**

- **`add_alias(conn: &Connection, alias: &str, entity_id: &str) -> Result<(), rusqlite::Error>`** -- Adds an alias for an entity. Uses `INSERT OR IGNORE` so duplicates are silently skipped.
- **`find_by_alias(conn: &Connection, alias: &str) -> Result<Option<String>, rusqlite::Error>`** -- Case-insensitive alias lookup. Returns the entity ID if found.
- **`get_aliases(conn: &Connection, entity_id: &str) -> Result<Vec<String>, rusqlite::Error>`** -- Returns all aliases for a given entity, ordered alphabetically.
- **`remove_alias(conn: &Connection, alias: &str, entity_id: &str) -> Result<(), rusqlite::Error>`** -- Removes a specific alias from an entity.

### `graph.rs`

Core entity graph operations: creating entities, finding them, adding relationships, and traversing the graph.

**Key types:**

- **`Entity`** -- An entity node. Fields:
  - `id: String` -- UUID
  - `canonical_name: String` -- The resolved display name
  - `entity_type: Option<String>` -- One of: "person", "service", "project", "concept" (or None)
  - `first_seen: String` -- ISO 8601 timestamp of first appearance
  - `last_seen: String` -- ISO 8601 timestamp of most recent appearance

- **`EntityRelationship`** -- A directed edge between two entities. Fields:
  - `source_entity_id: String`
  - `target_entity_id: String`
  - `relationship: String` -- e.g., "works_on", "decided", "owns", "related_to"
  - `memory_id: Option<String>` -- Provenance: which memory established this relationship
  - `valid_from: Option<String>` -- Temporal bound (when relationship started)
  - `valid_until: Option<String>` -- Temporal bound (when relationship ended; NULL = still active)

**Key functions:**

- **`create_entity(conn: &Connection, canonical_name: &str, entity_type: Option<&str>) -> Result<String, rusqlite::Error>`** -- Creates a new entity node with a generated UUID. Sets `first_seen` and `last_seen` to now.
- **`find_entity(conn: &Connection, name: &str) -> Result<Option<Entity>, rusqlite::Error>`** -- Case-insensitive lookup by canonical name.
- **`get_entity(conn: &Connection, entity_id: &str) -> Result<Option<Entity>, rusqlite::Error>`** -- Lookup by ID.
- **`add_relationship(conn: &Connection, source_id: &str, target_id: &str, relationship: &str, memory_id: Option<&str>) -> Result<(), rusqlite::Error>`** -- Creates a relationship edge. Uses `INSERT OR REPLACE`. Sets `valid_from` to now.
- **`traverse(conn: &Connection, entity_id: &str, max_hops: usize) -> Result<Vec<String>, rusqlite::Error>`** -- BFS traversal of the entity graph up to `max_hops` hops. Follows both outgoing and incoming relationships. Only follows active relationships (`valid_until IS NULL`). Returns deduplicated memory IDs collected from relationship edges. This is the core function used by the entity graph retrieval strategy.

## Key Public Types Other Modules Depend On

- **`EntityResolver` trait** -- Used by the retrieval pipeline to resolve entity mentions in queries before graph traversal.
- **`Entity`** and **`EntityRelationship`** -- Used by the MCP server for status reporting and by import pipelines that extract entities from ingested content.
- **`traverse()`** -- Called by the entity graph retrieval strategy (`src/retrieval/graph.rs`) to find memories connected to a query entity.

## Relevant config.toml Keys

```toml
[general]
tier = "offline"                # Tier 1 uses HeuristicResolver; Tier 2+ will use LLM-enhanced resolution

[retrieval]
entity_boost = 0.3              # Boost factor for entity graph matches during merge/rerank
```

## Deferred / Planned Functionality

- **LLM-enhanced entity resolution (Tier 2+):** Using the curator model to link fuzzy mentions to canonical entities.
- **Entity merge/split:** Administrative tools to merge two entities that were incorrectly separated, or split an entity that was incorrectly conflated.
- **Entity type inference:** Automatically determining entity types from context.
- **Updating `last_seen`:** The current code sets `last_seen` at creation time but does not update it on subsequent references. This should be updated when an entity is mentioned in a newly ingested memory.
