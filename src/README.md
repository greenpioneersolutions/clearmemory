# src/ — Clear Memory Engine Source Code

This is the Rust source for the Clear Memory engine. It compiles to a single native binary (`clearmemory`) that provides a CLI, MCP server, and HTTP API for AI memory storage and retrieval.

---

## Entry Points

- **`main.rs`** — CLI entry point. Uses `clap` to parse commands (`init`, `import`, `recall`, `retain`, `forget`, `serve`, `status`, etc.) and dispatches to handler functions. Each `cmd_*` function orchestrates the relevant modules. This is the only file that directly handles user-facing I/O (terminal output, exit codes).

- **`lib.rs`** — Library root. Re-exports all 20 public modules and defines the shared error hierarchy (`ClearMemoryError`, `StorageError`, `RetrievalError`, etc.) plus the `Tier` and `Classification` enums used across the codebase.

- **`config.rs`** — Configuration loading. Reads `~/.clearmemory/config.toml`, provides typed defaults for all configuration sections (general, models, cloud, retrieval, retention, server, encryption, compliance, observability, security, backup, concurrency, migrations). Every configurable value has a sensible default so the engine works with zero configuration.

---

## Module Architecture

Data flows through the engine in this order:

```
User input (CLI / MCP / HTTP)
    │
    ▼
┌─────────┐     ┌──────────┐     ┌───────────┐
│ server/  │────▶│ engine/  │────▶│ storage/  │
│ (routes) │     │ (core    │     │ (sqlite,  │
│          │◀────│  logic)  │◀────│  lance,   │
└─────────┘     └──────────┘     │  verbatim)│
                    │            └───────────┘
                    │
            ┌───────┴────────┐
            ▼                ▼
     ┌────────────┐   ┌──────────┐
     │ retrieval/ │   │ context/ │
     │ (4-strategy│   │ (token   │
     │  search)   │   │  budget  │
     └────────────┘   │  compiler│
                      └──────────┘
```

### Modules by Role

**Core pipeline:**
| Module | Purpose |
|--------|---------|
| `engine/` | Central orchestrator — `retain()`, `recall()`, `expand()`, `forget()`, `status()`. Ties together storage, retrieval, and context. |
| `storage/` | Persistent storage: SQLite (via rusqlite with SQLCipher), LanceDB (vector embeddings), verbatim transcript files. |
| `retrieval/` | 4-strategy parallel search: semantic, keyword, temporal, entity graph. Merges results via reciprocal rank fusion, reranks with BGE-Reranker-Base. |
| `context/` | Assembles the context payload injected into LLM prompts. Manages L0–L3 tiers within a configurable token budget. Deduplicates against existing CLI context. |

**Intelligence (Tier 2+):**
| Module | Purpose |
|--------|---------|
| `curator/` | Qwen3-0.6B model parses retrieval results, extracts only relevant portions. Candle integration is planned — currently provides a noop trait and stub. |
| `reflect/` | Qwen3-4B model synthesizes across memories into coherent narratives and mental models. Candle integration is planned. |

**Data modeling:**
| Module | Purpose |
|--------|---------|
| `entities/` | Entity resolution, alias management, and relationship graph. Links mentions like "the auth service" / "OAuth system" / "login microservice" to a single entity. |
| `facts/` | Temporal fact extraction from text, bi-temporal queries (valid_from / valid_until), and contradiction detection. |
| `tags/` | CRUD for the four tag types: team, repo, project, domain. |
| `streams/` | Scoped views across tag intersections. Stream CRUD, visibility (private/team/org), and write access control. |

**Import & export:**
| Module | Purpose |
|--------|---------|
| `import/` | Parsers for 7 formats: Claude Code, Copilot CLI, ChatGPT export, Slack export, Markdown, Clear Format (.clear), plus CSV/Excel-to-Clear conversion. |

**Lifecycle management:**
| Module | Purpose |
|--------|---------|
| `retention/` | Three retention policies (time-based, size-based, performance-based) and the archiver that moves stale memories to `~/.clearmemory/archive/`. |
| `backup/` | SQLite Online Backup API snapshots, LanceDB snapshots, scheduled background backups, restore from `.cmb` files. |
| `migration/` | Schema versioning, sequential migration runner, embedding model reindex (pausable/resumable). |
| `repair/` | Integrity checks for SQLite and LanceDB, index rebuild from SQLite + verbatim files. |

**Security & compliance:**
| Module | Purpose |
|--------|---------|
| `security/` | API token auth (scoped, expiring), AES-256-GCM encryption, secret scanning/redaction, rate limiting, TLS, classification tracing, insider threat detection. |
| `audit/` | Append-only audit log with chained hashes for tamper evidence, CSV/JSON export. |
| `compliance/` | Data classification, GDPR/CCPA right-to-delete (purge), legal holds, compliance reporting. |

**Infrastructure:**
| Module | Purpose |
|--------|---------|
| `server/` | MCP server (JSON-RPC over stdio, 9 tools) and HTTP/JSON API (axum). Shared handler layer. |
| `observability/` | OpenTelemetry metrics and distributed tracing, health endpoint. |

---

## Key Design Decisions

1. **rusqlite, not sqlx** — We use `rusqlite` with the `bundled-sqlcipher` feature for at-rest encryption. SQLCipher provides AES-256-CBC encryption of the entire database file transparently.

2. **Write queue pattern** — All writes (retain, forget, import, tag mutations) funnel through a single async writer task via a `tokio::mpsc` channel. Reads bypass the queue entirely. This prevents SQLite/LanceDB inconsistency from interleaved writes.

3. **Error hierarchy** — `thiserror` enums in `lib.rs` for library-level errors that cross module boundaries. `anyhow` in `main.rs` for application-level error handling with context.

4. **Candle deferred** — The `curator/` and `reflect/` modules define traits and noop implementations. Actual Qwen3 inference via the `candle` framework will be integrated when the dependency is added to Cargo.toml.

---

## How to Work in This Codebase

- **Adding a new CLI command:** Add variant to `Commands` enum in `main.rs`, add `cmd_*` handler function, wire it in the main `match`.
- **Adding a new import format:** Create a parser in `import/`, implement the `ImportParser` trait, register in `import/mod.rs`.
- **Adding a new retrieval strategy:** Create module in `retrieval/`, return scored results, wire into `retrieval/mod.rs` parallel execution.
- **Modifying the schema:** Add a new migration file in `migrations/`, bump version in `migration/versioning.rs`.
