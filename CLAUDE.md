# Clear Memory — Project Constitution

## Mission

Clear Memory is a high-performance, local-first AI memory engine built in Rust. It stores every AI conversation verbatim, retrieves relevant context via multi-strategy search, and injects optimized context into LLM prompts — keeping token costs minimal and data local.

**Tagline:** Store everything. Send only what matters. Pay for less.

Clear Memory is a standalone open-source project with its own repository. Its primary integration target is ClearPathAI (an Electron.js desktop app that wraps GitHub Copilot CLI and Claude Code CLI), but it is designed to be used independently via CLI, MCP, or local HTTP API by any tool or developer.

---

## Core Principles

1. **Verbatim storage + structured extraction.** Store raw transcripts in full fidelity — the original is never summarized, truncated, or replaced. At ingestion time, Clear Memory also extracts structured subject-predicate-object facts with temporal validity (valid_from / valid_until) and indexes them for knowledge update and temporal queries. The verbatim transcript is always preserved alongside the structured facts, giving you both audibility (the exact words) and queryability (the extracted knowledge). This hybrid approach — keeping everything while also structuring what matters — achieves 96.6% recall on LongMemEval in research, outperforming systems that use LLMs to decide what to keep.
2. **Multi-strategy retrieval.** Every search runs four strategies in parallel: semantic similarity, keyword matching (BM25), temporal proximity, and entity graph traversal. Results are merged and reranked. No single strategy covers all query types.
3. **Tiered context injection.** Memory is organized into tiers: always-loaded identity (~200 tokens), project working set (~500 tokens), on-demand semantic search, and deep cross-project retrieval. Each tier has a configurable token budget.
4. **Temporal awareness.** Every fact tracks when it became true (valid_from) and when it was superseded (valid_until). Old facts are invalidated, not deleted. Historical queries return historical truth. Current queries return current truth.
5. **Local-first.** All data stays on the user's machine by default. Tier 1 requires zero external calls. Tier 2 uses a bundled local LLM. Tier 3 optionally connects to cloud APIs. The user chooses their security posture.
6. **Token cost optimization.** The context compiler assembles the minimum viable context within a configurable token budget before any prompt reaches the LLM. Under token-based pricing, this is the ROI engine.

---

## Architecture Overview

```
User prompt
    │
    ▼
┌─────────────────────────────────────────────────┐
│  ClearPathAI wrapper (or CLI / MCP client)      │
│  Intercepts prompt before it reaches the model  │
└─────────────────────┬───────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│  CLEAR MEMORY ENGINE (Rust binary)                          │
│                                                             │
│  ┌──────────────────────┐  ┌────────────────────────────┐   │
│  │  Verbatim Storage    │  │  Tag Taxonomy              │   │
│  │  ├─ SQLite facts     │  │  ├─ Teams                  │   │
│  │  ├─ LanceDB vectors  │  │  ├─ Repos                  │   │
│  │  ├─ Entity graph     │  │  ├─ Projects               │   │
│  │  └─ Bi-temporal meta │  │  └─ Domains (nested)       │   │
│  └──────────┬───────────┘  └─────────────┬──────────────┘   │
│             │                            │                  │
│             └──────────┬─────────────────┘                  │
│                        ▼                                    │
│           ┌────────────────────────┐                        │
│           │  Streams               │                        │
│           │  Scoped tag views      │                        │
│           └────────────┬───────────┘                        │
│                        ▼                                    │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  4-Strategy Parallel Retrieval                       │   │
│  │  semantic + keyword + temporal + entity graph        │   │
│  └──────────────────────┬───────────────────────────────┘   │
│                         ▼                                   │
│           ┌─────────────────────────┐                       │
│           │  Curator Model          │  ◄── Tier 2+ only     │
│           │  Qwen3-0.6B (bundled)   │                       │
│           │  Parses, filters,       │                       │
│           │  extracts relevant parts│                       │
│           └─────────────┬───────────┘                       │
│                         ▼                                   │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Context Compiler (token budget)                     │   │
│  │  L0 identity │ L1 working set │ L2 recall │ L3 deep  │   │
│  └──────────────────────┬───────────────────────────────┘   │
│                         │                                   │
│  ┌──────────────────────┴───────────────────────────────┐   │
│  │  Reflect Engine       │  ◄── Tier 2+ only             │   │
│  │  Qwen3-4B (bundled)   │                               │   │
│  │  Synthesizes across   │                               │   │
│  │  memories into mental │                               │   │
│  │  models               │                               │   │
│  └───────────────────────────────────────────────────────┘   │
│                                                             │
│  Interfaces: MCP Server │ Local HTTP API │ CLI              │
└─────────────────────────┬───────────────────────────────────┘
                          │
              ┌───────────┼───────────┐
              ▼           ▼           ▼
         Copilot CLI  Claude Code  Local LLM
                          │
                          ▼
                  Session auto-save
                  (back to storage)
```

---

## Technology Stack

### Language & Runtime
- **Rust** — entire engine compiles to a single native binary
- No Python runtime, no Node.js, no external dependencies at runtime
- Target platforms: macOS (ARM64 + x86_64), Linux (x86_64), Windows (x86_64)

### Storage
- **SQLite** — structured data: temporal facts, entity graph, tag taxonomy, retention metadata, audit log
- **LanceDB** (embedded, Rust bindings) — vector index for semantic search. Columnar format optimized for similarity search with metadata filtering. Entire database is a single portable directory.
- All data lives in `~/.clearmemory/` — one folder, fully portable, backupable

### Embedding Model
- **Default: BGE-Small-EN-v1.5** via `fastembed-rs` (ONNX Runtime backend)
  - 384 dimensions, ~50MB quantized
  - English-specialized — outperforms BGE-M3 on English-only corpora (76.8% vs 69.4% Recall@10 in benchmarks)
  - Fast inference, low memory footprint
- **Alternative: BGE-M3** — 1024 dimensions, ~600MB quantized, 100+ languages supported
  - Use for multilingual corpora or deployments mixing multiple languages
  - Dense + sparse retrieval from a single model
  - Note: BGE-M3 scores lower than BGE-Small-EN on English-only corpora. This is a known phenomenon — multilingual models trade English-specific discrimination for language breadth.
- Models are downloaded on first run and cached in `~/.clearmemory/models/`
- Config: `embedding_model` field in `~/.clearmemory/config.toml` — values: `bge-small-en` (default), `bge-m3`

### Curator Model (Tier 2+)
- **Qwen3-0.6B** quantized (~1.2GB) via `candle` framework (candle integration is planned — see `src/curator/qwen.rs` for current status)
- Purpose: parse retrieval results, extract only relevant portions before context injection
- Bundled with the binary, downloaded on first run
- Fast: ~1 second per curator call on typical laptop CPU

### Reflect Model (Tier 2+)
- **Qwen3-4B** quantized (~2.5GB) via `candle` framework (candle integration is planned — see `src/reflect/` for current status)
- Purpose: synthesize across multiple memories to produce coherent project narratives, mental models, and summaries
- Bundled with the binary, downloaded on first run
- Slower than curator (~5-10 seconds) but significantly higher quality synthesis
- 4B is the minimum size for coherent multi-document synthesis — do NOT downgrade to 0.6B for reflect

### Reranking (Tier 1)
- **BGE-Reranker-Base** via `fastembed-rs` — cross-encoder reranker that runs locally, no LLM needed
- Used in all tiers as the final scoring step after multi-strategy retrieval merge

---

## Three Deployment Tiers

All three tiers share the same storage engine, retrieval pipeline, and binary. The difference is whether LLM intelligence is applied during entity resolution, curation, and synthesis — and where that inference runs.

### Tier 1: Fully Offline (Zero External Calls)
- Verbatim storage + BGE-M3 embeddings + 4-strategy retrieval + BGE-Reranker-Base
- Entity resolution via heuristic matching (exact string, case-insensitive, configurable aliases)
- Conflict detection via timestamp comparison
- No curator model — retrieval results go directly to context compiler with fusion scoring
- Reflect tool returns: "Reflect requires Tier 2 or higher"
- **Measured accuracy:** 76.8% Recall@10 (LongMemEval-style, 80 hard queries); 93.3% Recall@10 (scale test, 500-10K memories). See [docs/benchmarks.md](docs/benchmarks.md).
- **Use case: air-gapped environments, regulated industries, privacy-critical deployments**

### Tier 2: Offline + Bundled Local LLM
- Everything in Tier 1, plus:
- Curator model (Qwen3-0.6B) parses retrieval results before injection
- Reflect model (Qwen3-4B) synthesizes across memories
- Entity resolution enhanced by local LLM (links "the auth service" / "our OAuth system" / "login microservice")
- Conflict detection verified by local LLM
- **Target accuracy: ~99%** (not yet measured — curator model integration in progress)
- **Use case: enterprise teams with GPU-capable hardware, security-conscious but quality-focused**

### Tier 3: Cloud-Connected (Maximum Quality)
- Everything in Tier 2, plus:
- Curator and reflect can use cloud APIs (Claude, GPT, Gemini) for highest quality
- Entity resolution uses best available model
- Profile generation (stable facts + recent activity summary)
- **Target accuracy: 99%+** (not yet measured — cloud API integration in progress)
- **Use case: cloud-connected teams with API budgets who want maximum memory quality**

Config: `tier` field in `~/.clearmemory/config.toml` — values: `offline`, `local_llm`, `cloud`

---

## Tag Taxonomy & Streams

### Tags
Every memory can be tagged with one or more of four first-class tag types:

| Tag Type | Description | Examples |
|----------|-------------|---------|
| **Team** | Organizational team | `platform-team`, `frontend`, `security` |
| **Repo** | Code repository | `auth-service`, `api-gateway`, `frontend-app` |
| **Project** | Business initiative | `q1-migration`, `rebrand-2026`, `soc2-audit` |
| **Domain** | Knowledge domain (nestable) | `security`, `security/auth`, `infrastructure/ci-cd` |

Tags are **optional**. The system works with zero tags — everything goes into a default stream. Tags are a power-user feature that progressively improves retrieval as users invest in them. ClearPathAI can auto-tag based on active workspace/repo.

### Streams
A **stream** is a scoped view across tag intersections. Examples:
- `Platform Team + auth-service + Security` — shows only memories at that intersection
- `All Teams + q1-migration` — shows all team contributions to a project
- `Default` — everything, no filtering

Streams are created explicitly by users or inferred by ClearPathAI from the active workspace context. The system always checks for **related streams** (adjacent tag intersections) when searching within a stream.

### Related Stream Detection

When a user searches within a stream, Clear Memory identifies and checks **adjacent streams** for potentially relevant results. Two streams are considered related when they share one or more tags in their filter definitions.

**Algorithm:**
1. Load the active stream's tag set (e.g., `team:platform` + `domain:security/auth`)
2. Query all other streams the user has read access to
3. A stream is **related** if it shares at least one tag with the active stream
4. Rank related streams by tag overlap count (more shared tags = more related)
5. Include top-N related streams in the search (default: 3)
6. Related stream results are included in the merge/rerank pipeline but scored slightly lower than primary stream results

**Example:** Searching within stream "Platform Auth" (`team:platform` + `domain:security/auth`):

| Related Stream | Shared Tags | Why It's Checked |
|---|---|---|
| "Q1 Migration" (`project:q1-migration` + `team:platform`) | `team:platform` | Same team may have discussed auth in a migration context |
| "All Security" (`domain:security/*`) | `domain:security/auth` (nested match) | Security-domain stream captures auth decisions from other teams |
| "Auth Service" (`repo:auth-service` + `domain:security/auth`) | `domain:security/auth` | Same domain, different organizational scope |

Streams with zero tag overlap (e.g., `team:frontend` + `domain:ui`) are not checked. This prevents cross-team information leakage while still surfacing relevant adjacent context.

**Nested domain matching:** `domain:security/auth` matches streams tagged with `domain:security/*` (parent domain) and `domain:security/auth/oauth` (child domain).

### Stream Security (v1)
Every stream has three properties:
- **Owner** — the user who created it
- **Visibility** — `private` (owner only), `team` (authorized team members can read), `org` (everyone can read)
- **Write access** — owner + explicitly authorized users

Enforcement is at the application level (ClearPathAI checks permissions). Audit log records every memory read/write with user ID. Database-level encryption per-stream is a v2 hardening step.

---

## Storage Schema

### SQLite Tables

```sql
-- Memories: the core record linking to verbatim content
CREATE TABLE memories (
    id TEXT PRIMARY KEY,           -- UUID
    content_hash TEXT NOT NULL,     -- SHA-256 of verbatim content
    summary TEXT,                  -- short description for progressive loading
    source_format TEXT NOT NULL,    -- 'claude_code', 'copilot', 'chatgpt', 'slack', 'markdown', 'clear'
    created_at TEXT NOT NULL,       -- ISO 8601
    last_accessed_at TEXT,          -- updated on every recall hit (retention policy)
    access_count INTEGER DEFAULT 0,
    archived INTEGER DEFAULT 0,    -- 0 = active, 1 = archived by retention policy
    owner_id TEXT,                 -- user who created this memory
    stream_id TEXT                 -- primary stream assignment
);

-- Tags: many-to-many relationship between memories and tags
CREATE TABLE memory_tags (
    memory_id TEXT NOT NULL,
    tag_type TEXT NOT NULL,        -- 'team', 'repo', 'project', 'domain'
    tag_value TEXT NOT NULL,
    FOREIGN KEY (memory_id) REFERENCES memories(id)
);

-- Facts: extracted temporal assertions
CREATE TABLE facts (
    id TEXT PRIMARY KEY,
    memory_id TEXT NOT NULL,       -- source memory
    subject TEXT NOT NULL,
    predicate TEXT NOT NULL,
    object TEXT NOT NULL,
    valid_from TEXT,               -- when this became true in the real world
    valid_until TEXT,              -- when this stopped being true (NULL = still true)
    ingested_at TEXT NOT NULL,     -- when Clear Memory learned this
    invalidated_at TEXT,           -- when this was marked as superseded
    confidence REAL DEFAULT 1.0,
    FOREIGN KEY (memory_id) REFERENCES memories(id)
);

-- Entities: resolved entity nodes for the entity graph
CREATE TABLE entities (
    id TEXT PRIMARY KEY,
    canonical_name TEXT NOT NULL,  -- resolved name
    entity_type TEXT,              -- 'person', 'service', 'project', 'concept'
    first_seen TEXT NOT NULL,
    last_seen TEXT NOT NULL
);

-- Entity aliases: multiple names that resolve to the same entity
CREATE TABLE entity_aliases (
    alias TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    FOREIGN KEY (entity_id) REFERENCES entities(id)
);

-- Entity relationships: edges in the entity graph
CREATE TABLE entity_relationships (
    source_entity_id TEXT NOT NULL,
    target_entity_id TEXT NOT NULL,
    relationship TEXT NOT NULL,    -- 'works_on', 'decided', 'owns', 'related_to'
    memory_id TEXT,               -- source memory for provenance
    valid_from TEXT,
    valid_until TEXT,
    FOREIGN KEY (source_entity_id) REFERENCES entities(id),
    FOREIGN KEY (target_entity_id) REFERENCES entities(id)
);

-- Streams: scoped views across tags
CREATE TABLE streams (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    owner_id TEXT NOT NULL,
    visibility TEXT DEFAULT 'private',  -- 'private', 'team', 'org'
    created_at TEXT NOT NULL
);

-- Stream tag filters: which tags define this stream's scope
CREATE TABLE stream_tags (
    stream_id TEXT NOT NULL,
    tag_type TEXT NOT NULL,
    tag_value TEXT NOT NULL,
    FOREIGN KEY (stream_id) REFERENCES streams(id)
);

-- Stream write access: who can write to this stream beyond the owner
CREATE TABLE stream_writers (
    stream_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    FOREIGN KEY (stream_id) REFERENCES streams(id)
);

-- Audit log: every read/write operation (tamper-evident chain)
CREATE TABLE audit_log (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    user_id TEXT,
    operation TEXT NOT NULL,           -- 'retain', 'recall', 'expand', 'reflect', 'forget', 'import', 'purge', 'auth'
    memory_id TEXT,
    stream_id TEXT,
    classification TEXT,               -- classification of affected memory
    compliance_event INTEGER DEFAULT 0,-- 1 for purge, legal hold, audit export
    anomaly_flag INTEGER DEFAULT 0,    -- 1 if insider detection flagged this access
    hash TEXT NOT NULL,                -- SHA-256(previous_hash + this_entry)
    previous_hash TEXT,                -- hash of the previous audit entry (chained)
    details TEXT                       -- JSON blob with query, results count, latency, etc.
);

-- Retention events: track archival actions
CREATE TABLE retention_events (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    trigger_type TEXT NOT NULL,   -- 'time', 'size', 'performance'
    memories_archived INTEGER,
    details TEXT                  -- JSON with specifics
);

-- Performance baselines: for performance-based retention
CREATE TABLE performance_baselines (
    id TEXT PRIMARY KEY,
    measured_at TEXT NOT NULL,
    p95_recall_ms REAL NOT NULL,
    corpus_size_bytes INTEGER NOT NULL,
    memory_count INTEGER NOT NULL
);
```

### LanceDB Collections

```
~/.clearmemory/
├── config.toml
├── clearmemory.db          (SQLite)
├── vectors/                (LanceDB directory)
│   └── memories/           (vector collection)
│       ├── dense/          (BGE-M3 dense vectors, 1024 dimensions)
│       └── sparse/         (BGE-M3 sparse vectors for keyword matching)
├── verbatim/               (raw transcript files, referenced by content_hash)
│   ├── abc123.txt
│   └── def456.txt
├── archive/                (archived memories moved here by retention)
│   └── verbatim/
├── models/                 (downloaded on first run)
│   ├── bge-m3-onnx/
│   ├── bge-reranker-base-onnx/
│   ├── qwen3-0.6b/        (curator, Tier 2+)
│   └── qwen3-4b/          (reflect, Tier 2+)
└── mental_models/          (generated by reflect, markdown files)
    ├── project-auth-migration.md
    └── team-platform-overview.md
```

---

## Retrieval Pipeline

### 4-Strategy Parallel Search

On every `recall` operation, all four strategies execute concurrently (Rust async):

1. **Semantic similarity** — query embedding vs stored memory embeddings in LanceDB. Uses BGE-M3 dense vectors. Returns top-K by cosine similarity.
2. **Keyword matching** — BGE-M3 sparse vectors enable term-level matching without a separate BM25 index. Catches exact terminology ("GraphQL", "Clerk", specific error messages).
3. **Temporal proximity** — memories near a detected time reference in the query get a distance reduction (up to 40%). Parses "last month", "in January", "3 weeks ago" into date ranges.
4. **Entity graph traversal** — if the query mentions a known entity, traverse relationships to find connected memories. "What did Kai work on?" follows Kai → works_on → [projects] → related memories.

### Merge & Rerank

Results from all four strategies are merged via reciprocal rank fusion, then reranked by BGE-Reranker-Base (cross-encoder). The reranker scores each (query, memory) pair for relevance, not just similarity. This catches cases where a memory is semantically similar but doesn't actually answer the question.

### Progressive Loading

Recall returns **summaries** (the `summary` field from the memories table). The AI then calls `clearmemory_expand` with a specific memory ID to get the full verbatim content. This two-step pattern minimizes token usage — the AI only loads full content for memories it determines are relevant.

### Curator Layer (Tier 2+)

After retrieval and reranking, the Qwen3-0.6B curator model receives the retrieved memory excerpts and the original query. It:
- Identifies which portions of each memory are relevant to the query
- Strips irrelevant context (other topics discussed in the same session)
- Returns only the targeted excerpts

This further reduces token count before injection.

---

## Context Compiler

The context compiler assembles the final payload that gets injected into the LLM prompt. It operates within a configurable **token budget** (default: 4096 tokens for context injection).

### Memory Tiers (filled in priority order)

| Tier | What | Size | When |
|------|------|------|------|
| **L0** | Identity — who is the user, what CLI is active, current project | ~50 tokens | Always loaded |
| **L1** | Working set — active stream context, recent decisions, current project state | ~200-500 tokens | Always loaded |
| **L2** | Recall — relevant memories from semantic search within active stream | On demand | When query triggers relevance signal |
| **L3** | Deep search — cross-stream, cross-project retrieval | On demand | When explicitly requested or L2 insufficient |

The compiler fills L0, then L1, then L2 (if triggered), then L3 (if triggered), stopping when the token budget is exhausted. Highest-priority memories fill first; marginal memories are cut.

### Deduplication

The compiler checks what the CLI already has in context (CLAUDE.md contents, file contents passed via --add-dir) by hashing known context sources. If a memory's content overlaps with existing context, it's deprioritized or skipped.

---

## Retention Policies

Three triggers work together to manage corpus growth. Archived memories are moved to `~/.clearmemory/archive/` — never deleted. A `--include-archive` flag on any recall query includes them.

### Time-Based
- Memories older than `retention.time_threshold` (default: 90 days) that have not been accessed are flagged for auto-archive
- The staleness clock **resets on every access** — a 6-month-old memory recalled last week stays active
- Configurable in `config.toml`: `retention.time_threshold_days = 90`

### Size-Based
- When total corpus exceeds `retention.size_threshold` (default: 2GB), the system identifies oldest, least-accessed memories for archival
- Warns the user before archiving, showing candidates and allowing approval, skip, or threshold adjustment
- Configurable: `retention.size_threshold_gb = 2`

### Performance-Based
- Clear Memory benchmarks its own p95 retrieval latency on startup and periodically during use
- Stores baselines in `performance_baselines` table
- If p95 degrades beyond `retention.performance_threshold_ms` (default: 200ms), the system:
  1. Identifies the largest, oldest, least-accessed streams as candidates
  2. Notifies the user with specific recommendations
  3. Offers auto-archive with one-click approval
- Configurable: `retention.performance_threshold_ms = 200`

### Archive Behavior
- Archived verbatim files move to `~/.clearmemory/archive/verbatim/`
- SQLite records remain (with `archived = 1`) so metadata queries still work
- LanceDB vectors are removed from the active index (reduces search corpus)
- `clearmemory recall "query" --include-archive` searches both active and archived

---

## MCP Server

Clear Memory exposes an MCP server for integration with any MCP-compatible tool (Claude Code, Copilot, Cursor, Windsurf, etc.).

### Setup
```bash
# Claude Code
claude mcp add clearmemory -- clearmemory serve

# Generic MCP
clearmemory serve --port 9700
```

### 9 MCP Tools

#### Read Operations

| Tool | Purpose | Tiers |
|------|---------|-------|
| `clearmemory_recall` | Search with stream/tag filters. Returns summaries for progressive loading. Accepts query string, optional stream ID, optional tag filters, optional time range. | All |
| `clearmemory_expand` | Get full verbatim content for a specific memory ID returned by recall. The progressive loading primitive. | All |
| `clearmemory_reflect` | Synthesize across memories. Accepts a query or topic, returns a coherent narrative drawing from all relevant memories. Generates/updates mental models. | Tier 2+ |
| `clearmemory_status` | Overview: corpus size, stream count, memory count, model status, tier, retention health, performance metrics. | All |

#### Write Operations

| Tool | Purpose | Tiers |
|------|---------|-------|
| `clearmemory_retain` | Store a memory with optional tags (team, repo, project, domain). Accepts content string, optional metadata, optional stream assignment. | All |
| `clearmemory_import` | Bulk import from a file or directory. Accepts path and format hint. Supports: claude_code, copilot, chatgpt, slack, markdown, clear. | All |
| `clearmemory_forget` | Invalidate a memory with temporal marking. Sets valid_until on associated facts. Memory is not deleted — it's marked as superseded and excluded from current queries but available in historical queries. | All |

#### Organization

| Tool | Purpose | Tiers |
|------|---------|-------|
| `clearmemory_streams` | List, create, switch, or describe streams. Accepts subcommand (list, create, describe, switch) and relevant parameters. | All |
| `clearmemory_tags` | Manage team/repo/project/domain tags. Accepts subcommand (list, add, remove, rename) and tag type/value. | All |

---

## CLI Reference

```bash
# Setup
clearmemory init                           # guided onboarding, creates ~/.clearmemory/
clearmemory init --tier local_llm          # initialize with Tier 2 (downloads curator + reflect models)

# Import
clearmemory import ~/chats/ --format auto  # auto-detect format
clearmemory import ~/chats/ --format claude_code --stream my-project
clearmemory import ~/chats/ --format copilot
clearmemory import ~/chats/ --format chatgpt
clearmemory import ~/chats/ --format slack
clearmemory import ~/chats/ --format markdown
clearmemory import data.clear              # Clear Format import
clearmemory import data.csv --format clear --mapping auto  # CSV to Clear conversion

# Convert (Clear Format tooling)
clearmemory convert csv-to-clear input.csv --mapping auto
clearmemory convert excel-to-clear input.xlsx
clearmemory convert csv-to-clear input.csv --mapping "date=Column A,author=Column B,notes=Column D"
clearmemory validate myfile.clear

# Search
clearmemory recall "why did we switch to GraphQL"
clearmemory recall "auth decisions" --stream platform-team
clearmemory recall "what happened last week" --tag team:frontend
clearmemory recall "old auth pattern" --include-archive
clearmemory expand MEMORY_ID              # get full verbatim content

# Synthesis (Tier 2+)
clearmemory reflect "summarize the auth migration project"
clearmemory reflect --stream q1-migration  # generate mental model for a stream

# Memory management
clearmemory forget MEMORY_ID --reason "decision reversed"
clearmemory retain "We decided to use Clerk for auth" --tag project:q1-migration --tag team:platform

# Organization
clearmemory streams list
clearmemory streams create "Platform Auth" --tag team:platform --tag domain:security/auth
clearmemory tags list --type team
clearmemory tags add --type repo --value auth-service

# Status & maintenance
clearmemory status                         # corpus overview, health, performance
clearmemory status --retention             # show retention policy status and candidates
clearmemory archive --dry-run              # preview what retention would archive
clearmemory archive --confirm              # execute archival

# Server
clearmemory serve                          # start MCP server (default port 9700)
clearmemory serve --http --port 8080       # start HTTP API server
clearmemory serve --both                   # both MCP and HTTP

# Context (for manual use or local LLM integration)
clearmemory context                        # output L0 + L1 context payload to stdout
clearmemory context --stream my-project    # project-specific context
clearmemory context --budget 2000          # limit to 2000 tokens
```

---

## Import Formats

### Supported Formats (v1)

| Format | Flag | Source | Notes |
|--------|------|--------|-------|
| Claude Code | `--format claude_code` | `~/.claude/` session transcripts | Primary format for ClearPathAI |
| Copilot CLI | `--format copilot` | Copilot CLI session logs | Primary format for ClearPathAI |
| ChatGPT | `--format chatgpt` | ChatGPT export JSON (`conversations.json`) | Standard OpenAI export |
| Slack | `--format slack` | Slack workspace export (JSON per channel) | Enterprise integration |
| Markdown | `--format markdown` | Any `.md` or `.txt` files | Catch-all for notes, docs, meeting minutes |
| **Clear Format** | `--format clear` | `.clear` files (JSON with defined schema) | Our custom enterprise format |
| Auto-detect | `--format auto` | Any of the above | Inspects file structure to determine format |

### The Clear Format (.clear)

A `.clear` file is JSON with a defined schema designed for enterprise integration. Non-technical users create data in CSV or Excel, convert with `clearmemory convert`, and import.

```json
{
  "clear_format_version": "1.0",
  "source": "jira-export",
  "exported_at": "2026-04-12T10:00:00Z",
  "memories": [
    {
      "date": "2026-03-15",
      "author": "Sarah Chen",
      "type": "decision",
      "content": "Team decided to migrate auth from Auth0 to Clerk based on pricing and DX.",
      "tags": {
        "team": "platform",
        "repo": "auth-service",
        "project": "q1-migration",
        "domain": "security/auth"
      },
      "related_memories": [],
      "metadata": {
        "source_ticket": "AUTH-234",
        "participants": ["Sarah Chen", "Kai Rivera", "Priya Sharma"]
      }
    }
  ]
}
```

**All tag fields are optional.** A minimal .clear file needs only `date`, `author`, and `content` per memory. Auto-tagging can be applied on import via `--auto-tag`.

**Clear Format tooling ships with the binary:**
- `clearmemory convert csv-to-clear` — maps CSV columns to Clear schema fields
- `clearmemory convert excel-to-clear` — same for .xlsx
- `clearmemory validate` — schema validation with error reporting
- Excel template downloadable from docs with pre-labeled columns and example rows

---

## ClearPathAI Integration (Slice 31)

Clear Memory integrates with ClearPathAI as a sidecar process communicating over a local socket.

### Startup
1. ClearPathAI launches `clearmemory serve --both` on app start
2. CLIManager calls Clear Memory's `context` endpoint to get L0 + L1 payload
3. Payload is injected into the CLI's system prompt (CLAUDE.md for Claude Code, custom instructions for Copilot)

### During Session
1. User sends prompt through ClearPathAI
2. ClearPathAI sends prompt to Clear Memory's recall endpoint for relevance check
3. If relevant memories found, append to prompt before sending to CLI
4. Progressive loading: if initial summaries warrant deeper context, ClearPathAI calls expand for specific memories

### After Session
1. On session end (or save hook trigger), full transcript is sent to `clearmemory_retain`
2. Tags are auto-assigned based on active workspace, repo, and project context
3. If Tier 2+, entity resolution runs on the new transcript to update the entity graph

### Analytics Integration
- ClearPathAI's analytics dashboard (Slice 19) shows:
  - Tokens saved by memory optimization per session/day/week/month
  - Memory corpus health (size, growth rate, retention status)
  - Most-accessed memories and streams
  - Retrieval latency trends

---

## Configuration

### `~/.clearmemory/config.toml`

```toml
[general]
tier = "local_llm"                    # "offline", "local_llm", "cloud"
default_stream = "default"

[models]
embedding = "bge-m3"                  # "bge-m3" (default) or "bge-small-en"
curator = "qwen3-0.6b"               # Tier 2+ only
reflect = "qwen3-4b"                 # Tier 2+ only
reranker = "bge-reranker-base"       # used in all tiers

[cloud]                               # Tier 3 only
api_provider = "anthropic"            # "anthropic", "openai", "google"
api_key_env = "ANTHROPIC_API_KEY"     # env var containing the API key
curator_model = "claude-haiku-4-5-20251001"
reflect_model = "claude-sonnet-4-6"

[retrieval]
top_k = 10                            # number of results per strategy before merge
temporal_boost = 0.4                  # max distance reduction for temporal proximity
entity_boost = 0.3                    # boost for entity graph matches
token_budget = 4096                   # max tokens for context injection

[retention]
time_threshold_days = 90              # archive memories older than this if not accessed
size_threshold_gb = 2                 # warn and offer archival above this corpus size
performance_threshold_ms = 200        # flag performance degradation above this p95
auto_archive = false                  # if true, archive without confirmation (enterprise setting)

[server]
mcp_enabled = true
http_enabled = true
http_port = 8080
mcp_port = 9700
```

---

## Development Conventions

### Rust Conventions
- Edition 2021, MSRV 1.75+
- Use `tokio` for async runtime
- Use `rusqlite` for SQLite (with bundled-sqlcipher for at-rest encryption)
- Use `fastembed` crate for embeddings and reranking
- Use `candle-core` + `candle-transformers` for local LLM inference (curator, reflect) — planned, not yet integrated
- Use `lancedb` crate for vector storage
- Use `axum` for HTTP API server
- Use `clap` for CLI argument parsing
- Error handling: `anyhow` for application errors, `thiserror` for library errors
- Logging: `tracing` crate with structured logging
- Tests: unit tests in-module, integration tests in `tests/` directory

### Code Structure

```
clearmemory/
├── CLAUDE.md                    ← you are here
├── Cargo.toml
├── src/
│   ├── main.rs                  ← CLI entry point (clap)
│   ├── lib.rs                   ← library root, re-exports
│   ├── config.rs                ← config loading from TOML
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── sqlite.rs            ← SQLite operations (rusqlite)
│   │   ├── lance.rs             ← LanceDB vector operations
│   │   └── verbatim.rs          ← raw transcript file I/O
│   ├── retrieval/
│   │   ├── mod.rs
│   │   ├── semantic.rs          ← BGE-M3 dense vector search
│   │   ├── keyword.rs           ← BGE-M3 sparse vector search
│   │   ├── temporal.rs          ← time-aware scoring
│   │   ├── graph.rs             ← entity graph traversal
│   │   ├── merge.rs             ← reciprocal rank fusion
│   │   └── rerank.rs            ← BGE-Reranker-Base cross-encoder
│   ├── curator/
│   │   ├── mod.rs
│   │   └── qwen.rs              ← Qwen3-0.6B inference (candle integration planned)
│   ├── reflect/
│   │   ├── mod.rs
│   │   ├── synthesizer.rs       ← multi-memory synthesis
│   │   └── mental_models.rs     ← generate/update mental model files
│   ├── context/
│   │   ├── mod.rs
│   │   ├── compiler.rs          ← token-budget-aware context assembly
│   │   ├── layers.rs            ← L0/L1/L2/L3 tier logic
│   │   └── dedup.rs             ← deduplication against known CLI context
│   ├── import/
│   │   ├── mod.rs
│   │   ├── claude_code.rs       ← Claude Code transcript parser
│   │   ├── copilot.rs           ← Copilot CLI session parser
│   │   ├── chatgpt.rs           ← ChatGPT export JSON parser
│   │   ├── slack.rs             ← Slack export parser
│   │   ├── markdown.rs          ← generic markdown/text parser
│   │   ├── clear_format.rs      ← .clear file parser and validator
│   │   └── converter.rs         ← CSV/Excel to .clear conversion
│   ├── retention/
│   │   ├── mod.rs
│   │   ├── time_policy.rs       ← time-based retention
│   │   ├── size_policy.rs       ← size-based retention
│   │   ├── performance_policy.rs ← latency-based retention
│   │   └── archiver.rs          ← move memories to archive
│   ├── entities/
│   │   ├── mod.rs
│   │   ├── resolver.rs          ← entity resolution (heuristic + optional LLM)
│   │   ├── graph.rs             ← entity relationship graph operations
│   │   └── aliases.rs           ← alias management
│   ├── tags/
│   │   ├── mod.rs
│   │   └── taxonomy.rs          ← team/repo/project/domain CRUD
│   ├── streams/
│   │   ├── mod.rs
│   │   ├── manager.rs           ← stream CRUD, visibility, access control
│   │   └── security.rs          ← permission checks
│   ├── server/
│   │   ├── mod.rs
│   │   ├── mcp.rs               ← MCP server (9 tools)
│   │   ├── http.rs              ← HTTP/JSON API (axum)
│   │   └── handlers.rs          ← shared request handlers
│   ├── audit/
│   │   ├── mod.rs
│   │   ├── logger.rs            ← audit log operations
│   │   ├── chain.rs             ← chained hash tamper-evident log
│   │   └── export.rs            ← audit log export (CSV, JSON)
│   ├── facts/
│   │   ├── mod.rs
│   │   ├── extractor.rs         ← extract temporal facts from text
│   │   ├── conflict.rs          ← detect contradictions, manage invalidation
│   │   └── temporal.rs          ← bi-temporal query logic
│   ├── compliance/
│   │   ├── mod.rs
│   │   ├── classification.rs   ← data classification (public/internal/confidential/pii)
│   │   ├── purge.rs             ← hard delete for GDPR/CCPA right-to-delete
│   │   ├── legal_hold.rs        ← freeze streams for litigation
│   │   └── reporting.rs         ← compliance report generation
│   ├── backup/
│   │   ├── mod.rs
│   │   ├── snapshot.rs          ← SQLite Online Backup + LanceDB snapshot
│   │   ├── restore.rs           ← restore from .cmb backup file
│   │   └── scheduler.rs         ← scheduled background backups
│   ├── migration/
│   │   ├── mod.rs
│   │   ├── versioning.rs        ← schema version tracking
│   │   ├── runner.rs            ← apply migrations in sequence
│   │   └── reindex.rs           ← embedding model migration (pause/resume)
│   ├── observability/
│   │   ├── mod.rs
│   │   ├── metrics.rs           ← OpenTelemetry metric definitions
│   │   ├── tracing_setup.rs     ← distributed tracing spans
│   │   └── health.rs            ← health endpoint logic
│   ├── security/
│   │   ├── mod.rs
│   │   ├── auth.rs              ← API token generation, validation, rotation, expiration
│   │   ├── tls.rs               ← TLS configuration for shared deployments
│   │   ├── cloud_filter.rs      ← block PII/confidential from Tier 3 cloud calls
│   │   ├── secret_scanner.rs    ← detect credentials/secrets in retain path; v1.1 planned: entropy-based detection for high-entropy strings in key-value contexts
│   │   ├── redactor.rs          ← redact detected secrets before storage
│   │   ├── rate_limiter.rs      ← per-client rate limiting on MCP/HTTP endpoints
│   │   ├── encryption.rs        ← at-rest encryption (SQLCipher, AES-256-GCM for files)
│   │   ├── insider_detection.rs ← access anomaly detection for shared deployments
│   │   └── classification_tracer.rs ← trace classification through content pipeline
│   └── repair/
│       ├── mod.rs
│       ├── integrity.rs         ← SQLite + LanceDB integrity checks
│       └── rebuild.rs           ← rebuild LanceDB index from SQLite + verbatim
├── migrations/
│   └── 001_initial_schema.sql   ← v1 schema creation
├── tests/
│   ├── benchmark_longmemeval.rs ← LongMemEval-style evaluation suite
│   ├── benchmark_suite.rs       ← publication-quality benchmark (500 memories, 100 queries)
│   ├── benchmark_scale.rs       ← corpus scale testing (500–10K memories)
│   ├── per_strategy_bench.rs    ← per-strategy precision isolation
│   ├── retrieval_regression.rs  ← CI regression gate (25 queries, Recall@10 ≥ 0.90)
│   ├── integration/             ← planned
│   ├── adversarial/             ← planned
│   ├── security/                ← planned
│   ├── stress/                  ← planned
│   └── fixtures/
│       ├── sample_claude_code_session.json
│       ├── sample_copilot_session.log
│       ├── sample_chatgpt_export.json
│       ├── sample_slack_export/
│       ├── sample.clear
│       ├── sample.csv
│       └── corrupt_fixtures/    ← intentionally broken files for recovery tests
├── benchmarks/
│   ├── retrieval_bench.rs       ← Criterion end-to-end retrieval latency
│   └── latency_bench.rs         ← LanceDB insert/search + keyword latency
├── templates/                   ← planned
│   └── clear_format_template.xlsx  ← planned
└── docs/
    ├── architecture.md
    ├── security.md
    ├── ENTERPRISE.md
    ├── benchmarks.md
    ├── clear_format_spec.md     ← planned
    ├── retention_policies.md    ← planned
    ├── stream_security.md       ← planned
    ├── clearpathAI_integration.md ← planned
    ├── runbook.md               ← planned
    ├── incident_response.md     ← planned
    ├── security_model.md        ← planned
    ├── integration_guide.md     ← planned
    ├── mcp_tools_schema.json    ← planned
    └── adr/                     ← planned
        ├── 001-verbatim-over-extraction.md
        ├── 002-rust-over-python.md
        ├── 003-bge-m3-embedding.md
        ├── 004-lancedb-over-sqlite-vss.md
        ├── 005-tiered-deployment.md
        ├── 006-streams-over-flat-projects.md
        ├── 007-at-rest-encryption-v1.md
        └── 008-secret-scanning-pipeline.md
```

### Naming Conventions
- Project name: **Clear Memory** (two words, capitalized)
- Binary name: `clearmemory` (one word, lowercase)
- Crate name: `clearmemory`
- MCP tool prefix: `clearmemory_` (e.g., `clearmemory_recall`)
- Config directory: `~/.clearmemory/`
- File extension: `.clear`
- Environment variables: `CLEARMEMORY_` prefix (e.g., `CLEARMEMORY_TIER`)

### Git Conventions
- Branch naming: `feature/`, `fix/`, `refactor/`, `docs/`
- Commit messages: conventional commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `bench:`)
- All PRs must pass: `cargo clippy -- -D warnings`, `cargo test`, `cargo fmt --check`

---

## Build & Run

```bash
# Development
cargo build
cargo run -- init
cargo run -- serve --both
cargo test
cargo bench

# Release
cargo build --release
# Binary at target/release/clearmemory

# First run downloads models
./clearmemory init              # downloads BGE-M3 (~600MB)
./clearmemory init --tier local_llm  # also downloads Qwen3-0.6B + Qwen3-4B (~3.7GB additional)
```

---

## Brand & Naming

- **Product:** Clear Memory
- **Binary:** `clearmemory`
- **Parent brand:** ClearPathAI
- **Brand colors:** Purple `#5B4FC4` (primary), Teal `#1D9E75` (AI accent), `#5DCAA5` (secondary teal)
- **File extension:** `.clear`
- **Namespace:** No known conflicts with `.clear` extension or "Clear Memory" as a product name in the AI/ML space

---

## Competitive Reference

Built by studying and synthesizing the best ideas from seven leading memory systems:

| System | What we took | What we skipped |
|--------|-------------|-----------------|
| **MemPalace** | Verbatim storage philosophy, 4-layer memory stack, wing/room → tag/stream mapping | AAAK compression (regresses benchmarks), Python dependency |
| **Mem0** | Self-editing memory with conflict resolution, dynamic forgetting, three scopes (user/session/agent) | Cloud dependency, LLM-extracted-only memories |
| **Zep/Graphiti** | Bi-temporal modeling (valid_from/valid_until), automatic fact invalidation | Neo4j dependency, cloud-first architecture |
| **Letta/MemGPT** | Core/recall/archival tiered architecture, OS-inspired context management | Full runtime replacement (we're a library, not a framework) |
| **Hindsight** | 4-strategy parallel retrieval, retain/recall/reflect pattern, mental models | PostgreSQL dependency (we use SQLite + LanceDB) |
| **Supermemory** | Profile generation, project-scoped isolation via container tags, MCP plugins | Closed source, cloud-first |
| **Cognee** | Proof that graph memory works with SQLite + local stores, no cloud needed | Python-only, no enterprise features |

---

## Observability & Monitoring

Clear Memory emits structured metrics and traces compatible with enterprise observability stacks via OpenTelemetry.

### OpenTelemetry Integration
- The binary includes an OTLP exporter (`opentelemetry-otlp` crate)
- If `OTEL_EXPORTER_OTLP_ENDPOINT` is set, metrics and traces export automatically
- If no endpoint is configured, metrics log to `tracing` as structured JSON — zero overhead

### Emitted Metrics (prefixed `clearmemory.`)

| Metric | Type | Description |
|--------|------|-------------|
| `corpus.size_bytes` | Gauge | Total active corpus size |
| `corpus.memory_count` | Gauge | Total active memories |
| `recall.latency_ms` | Histogram | End-to-end recall latency (p50, p95, p99) |
| `recall.strategy.{name}_ms` | Histogram | Per-strategy latency (semantic, keyword, temporal, graph) |
| `recall.rerank_ms` | Histogram | Reranker latency |
| `retain.latency_ms` | Histogram | Memory storage latency |
| `curator.latency_ms` | Histogram | Curator model inference time (Tier 2+) |
| `curator.tokens_saved` | Counter | Tokens removed by curator before injection |
| `reflect.latency_ms` | Histogram | Reflect model inference time (Tier 2+) |
| `context.injected_tokens` | Histogram | Tokens injected per session |
| `context.tokens_saved` | Counter | Cumulative tokens saved vs naive approach |
| `retention.events` | Counter (labels: trigger_type) | Retention policy trigger count |
| `embedding.inference_ms` | Histogram | Embedding model inference latency |
| `errors` | Counter (labels: component, error_type) | Error count by component |

### Distributed Tracing
- Every MCP/HTTP request creates a trace span with child spans for: retrieval strategies (parallel), merge, rerank, curator, context compilation
- Trace IDs propagate through the audit log for end-to-end correlation

### Health Endpoint
- `GET /health` returns JSON: status (healthy/degraded/unhealthy), uptime, corpus size, p95 latency, model status, tier, disk usage
- MCP equivalent: `clearmemory_status` returns the same data
- Compatible with Kubernetes liveness/readiness probes and enterprise monitoring agents

### Configuration
```toml
[observability]
otel_enabled = false
otel_endpoint = ""
otel_service_name = "clearmemory"
metrics_log_interval_secs = 60
health_endpoint_enabled = true
```

---

## Disaster Recovery & Backup

### Backup Command
```bash
clearmemory backup ~/backups/clearmemory-2026-04-12.cmb
clearmemory backup ~/backups/ --auto-name
clearmemory backup --scheduled --interval 24h    # background task in serve mode
```

### Implementation
- **SQLite:** Online Backup API (`sqlite3_backup_*`) — consistent snapshot under concurrent access, no locking
- **LanceDB:** Copies current version snapshot (immutable append-only files), no interference with active writes
- **Verbatim files:** Filesystem hardlinks where supported (instant, zero extra disk), fallback to copy
- **Output:** Single `.cmb` file — compressed tar with SQLite, LanceDB snapshot, verbatim files, config, and checksums manifest

### Restore
```bash
clearmemory restore ~/backups/clearmemory-2026-04-12.cmb
clearmemory restore ~/backups/clearmemory-2026-04-12.cmb --target ~/.clearmemory-restored/
clearmemory restore --verify ~/backups/clearmemory-2026-04-12.cmb
```
- Validates checksums before restoring
- Can restore to alternate directory for side-by-side verification
- Automatically rebuilds derived indexes after restore

### Configuration
```toml
[backup]
auto_backup_enabled = false
auto_backup_interval_hours = 24
backup_directory = "~/.clearmemory/backups"
backup_retention_count = 7
encrypt_backups = true                  # encrypt .cmb files (default true)
```

### Backup Encryption
Backup files (`.cmb`) are encrypted by default using the same master passphrase that protects at-rest data:
- The backup archive is encrypted with AES-256-GCM after compression
- Restore requires the passphrase: `clearmemory restore backup.cmb` prompts for passphrase (or reads from `CLEARMEMORY_PASSPHRASE` env var)
- Unencrypted backups can be created with `--no-encrypt` for environments where the backup destination is already encrypted (e.g., encrypted enterprise NAS)
- Backup files stored on network shares, cloud storage, or external drives are protected even if the storage is compromised

---

## Data Migration & Upgrade Paths

### Schema Versioning
- `schema_version` table tracks current version
- Migrations in `migrations/` directory, auto-applied on startup in sequence
- Migrations are transactional — failure triggers rollback
- Migration history recorded in `migration_log` table

```sql
CREATE TABLE schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL,
    description TEXT
);

CREATE TABLE migration_log (
    id TEXT PRIMARY KEY,
    from_version INTEGER,
    to_version INTEGER,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    status TEXT NOT NULL,       -- 'success', 'failed', 'rolled_back'
    error_message TEXT
);
```

### Embedding Model Migration
- No automatic re-embedding on model change — existing index continues working
- `clearmemory reindex --model <new_model>` re-embeds entire corpus with new model
- Old index stays active during reindex; atomic swap on completion
- Reindex is pausable/resumable: `clearmemory reindex --resume`
- Estimated: ~1 hour per 100K memories on laptop CPU

### Version Compatibility
- Semver: patch = no schema change, minor = additive only, major = breaking (ships migration scripts)
- Binary refuses to start if schema version is newer than binary version (prevents downgrade corruption)

### Configuration
```toml
[migrations]
auto_migrate = true
backup_before_migrate = true
```

---

## Error Handling & Degradation Modes

Every component has a defined fallback. The system always provides a useful response.

| Component | Failure | Fallback | Impact |
|-----------|---------|----------|--------|
| Reflect model (4B) | OOM / corrupt / timeout | Return error: "Reflect unavailable" | Reflect tool errors, all other tools unaffected |
| Curator model (0.6B) | OOM / corrupt / timeout | Skip curator, pass raw results to context compiler (Tier 1 behavior) | Slightly more tokens, slightly lower quality |
| Embedding model | Corrupt model file | Refuse to start with remediation: `clearmemory models download --force` | Critical — binary won't start |
| LanceDB index | Corrupted | Fall back to keyword-only search, auto-rebuild index in background | Degraded retrieval with ETA for recovery |
| SQLite | Locked by other process | Retry with exponential backoff (5 retries, 100ms→1600ms), then error with PID | Transient write delays |
| SQLite | Corrupted | Refuse to start with remediation: `clearmemory restore` or `clearmemory repair` | Critical — provides recovery path |
| Verbatim file | Missing | `expand` errors for that memory only, all others unaffected | Single memory inaccessible |
| MCP/HTTP port | In use | Scan next ports (9701, 9702...), log actual port | Consuming app discovers via stdout |
| Network (model download) | No internet | Error with offline path: `clearmemory models install --path` | Cannot run until models available |

### Startup Health Checks
Runs on startup, reports status per component:
1. SQLite accessible + schema current
2. LanceDB accessible + consistent with SQLite
3. Embedding model loaded + correct dimensions
4. Curator model loaded (Tier 2+ only)
5. Reflect model loaded (Tier 2+ only)
6. MCP/HTTP ports available

Non-critical failures → start in degraded mode. Critical failures → refuse to start with clear remediation.

### Repair Command
```bash
clearmemory repair                      # full integrity check + auto-repair
clearmemory repair --check-only         # report without fixing
clearmemory repair --rebuild-index      # rebuild LanceDB from SQLite + verbatim files
```

---

## Multi-User Concurrency

### SQLite: WAL Mode
- WAL (Write-Ahead Logging) enabled on creation — concurrent readers with single writer
- All writes use IMMEDIATE transactions to prevent write starvation
- Read operations never block and are never blocked by writes

### Write Queue Architecture
```
User A: retain() ──┐
                    ├──▶ Write Queue (tokio mpsc) ──▶ Writer Task ──▶ SQLite + LanceDB + Verbatim
User B: retain() ──┘
User C: recall() ─────── reads bypass queue, direct to database
```

- All writes (retain, forget, import, tag mutations) funnel through a single async writer task
- Guarantees ordering, prevents SQLite/LanceDB inconsistency from interleaved writes
- Reads bypass the queue entirely — zero coordination with writes
- Queue depth configurable (default: 1000). Backpressure on overflow.

### LanceDB Concurrency
- Append-only format is safe for concurrent reads
- Writes serialized through same write queue as SQLite
- Background compaction merges segments without blocking reads or writes

### Shared Deployment
- Server runs as long-lived process (systemd, launchd, or supervised by ClearPathAI)
- Multiple MCP/HTTP clients connect concurrently
- Each request carries `user_id` for audit logging and stream permission checks
- Isolation enforced at query level via stream visibility filters

### Configuration
```toml
[concurrency]
read_pool_size = 4
write_queue_depth = 1000
compaction_interval_secs = 300
```

---

## Compliance & Data Governance

### Data Classification
Every memory carries a classification label:

| Classification | Behavior |
|----------------|----------|
| `public` | No restrictions. Default searchable. |
| `internal` | Authenticated users only. Logged. |
| `confidential` | Stream owner + authorized users only. Requires private/team visibility. |
| `pii` | Flagged in audit log. Subject to right-to-delete. Blocked from Tier 3 cloud API calls. |

Set on retain: `clearmemory retain "..." --classification confidential`
Default: `internal`. ClearPathAI can auto-set via policy profiles (Slice 17).

```sql
-- Added to memories table
classification TEXT DEFAULT 'internal'
```

### Right to Delete (GDPR / CCPA)
`forget` does temporal invalidation. `purge` does permanent deletion:

```bash
clearmemory purge --user "kai@company.com"            # all memories by user
clearmemory purge --memory-id abc123 --hard            # specific memory
clearmemory purge --stream old-project --hard --confirm # entire stream
```

Purge removes: SQLite records, LanceDB vectors, verbatim files (active + archive), facts, entity relationships, tags. Writes purge event to audit log (records deletion occurred, not content). Auto-backup created before execution. Requires `--confirm`.

### Legal Hold
```bash
clearmemory hold --stream q1-migration --reason "Litigation: Case #2026-1234"
clearmemory hold --release --stream q1-migration
clearmemory hold --list
```
Held streams: cannot be forgotten, purged, archived, or modified. New memories can still be added. Hold recorded in audit log.

```sql
CREATE TABLE legal_holds (
    id TEXT PRIMARY KEY,
    stream_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    held_by TEXT NOT NULL,
    held_at TEXT NOT NULL,
    released_at TEXT,
    released_by TEXT,
    FOREIGN KEY (stream_id) REFERENCES streams(id)
);
```

### Compliance Reporting
```bash
clearmemory compliance report                        # full report
clearmemory compliance report --format csv            # for auditors
```
Includes: memory count by classification, age distribution, per-stream breakdown, PII count, active holds, recent purges, retention config.

### Audit Log Enhancements
- `classification` field on operations involving classified memories
- `compliance_event` flag for purge and legal hold operations
- Append-only — cannot be modified or deleted
- Chained hashes (each entry hashes previous entry + current content) for tamper evidence
- **External checkpoint anchors:** every 1000 entries (or every 6 hours, whichever comes first), the system writes a checkpoint hash to: (a) a separate checkpoint file outside the database (`~/.clearmemory/audit_checkpoints.log`), (b) stdout/stderr so enterprise log aggregators (Splunk, Datadog, syslog) capture it, and (c) the observability metrics pipeline. If the audit chain is replaced entirely, the checkpoint mismatch is detectable from external records.
- Export: `clearmemory audit export --from 2026-01-01 --to 2026-04-12 --format csv`
- Verify: `clearmemory audit verify` — validates the entire chain from first entry to last, reports any broken links or missing checkpoints

### Configuration
```toml
[compliance]
default_classification = "internal"
pii_detection_enabled = false
pii_detection_mode = "warn"              # "warn", "redact", "block"
require_classification_on_retain = false
legal_hold_enabled = true
```

### PII Detection (v1.x — planned)

When `pii_detection_enabled = true`, the retain path runs PII pattern detection alongside secret scanning. Detected PII auto-classifies the memory as `pii`. Patterns detected: email addresses, phone numbers, SSNs, credit card numbers, IP addresses, names in key-value context, and date-of-birth patterns. Supports `warn`/`redact`/`block` modes matching secret scanning behavior. Implementation planned for `src/security/secret_scanner.rs` as an extension of the existing pattern matching infrastructure.

### Classification Roadmap

| Phase | Version | Capability |
|-------|---------|-----------|
| Manual + auto-escalation | v1 (current) | User sets classification on retain. Secrets auto-escalate to `confidential`. |
| PII pattern detection | v1.x (planned) | Regex-based PII detection on retain path. Auto-classify as `pii`. |
| LLM-based classification | v2 (planned) | Curator model classifies content by topic sensitivity, not just pattern matching. |

---

## Security Model & Threat Mitigation

### Threat Model

| # | Threat | Vector | Mitigation |
|---|--------|--------|------------|
| 1 | Unauthorized MCP/HTTP access | Malicious local process or network client | API token authentication with scopes on all interfaces |
| 2 | Sensitive data to cloud APIs | PII/confidential content reaching Tier 3 | Classification-aware filtering on ALL content in the pipeline (raw memories, curator output, reflect input) |
| 3 | Verbatim file tampering | Direct filesystem modification | SHA-256 checksums verified on every `expand` |
| 4 | Audit log tampering | Replacing or modifying log entries | Append-only with chained hashes + external checkpoint anchors |
| 5 | DoS via API flooding | Compromised MCP client flooding queries | Per-client rate limiting on all MCP/HTTP endpoints |
| 6 | DoS via large imports | Malicious .clear file with millions of records | Size caps per operation + rate limiting on retain/import |
| 7 | Data exfiltration via stolen device | Laptop theft, directory copy | At-rest encryption of all stored data (SQLite via SQLCipher, verbatim files via AES-256-GCM) |
| 8 | Model supply chain poisoning | Compromised model on Hugging Face | Pinned model revisions, self-hosted manifest with checksums, benchmark verification gate |
| 9 | Credential exposure in memories | API keys, tokens, passwords in stored transcripts | Secret scanning pipeline on retain path with redaction |
| 10 | Insider threat / unauthorized access | Legitimate user querying streams they shouldn't | Access anomaly detection, mandatory justification for confidential access, audit alerting |
| 11 | Backup exfiltration | Unencrypted backup on network share | Backup encryption with user-provided or derived key |
| 12 | Derived content classification bypass | Confidential memory excerpts laundered through curator into Tier 3 | Classification tracing through entire content pipeline |
| 13 | Unauthorized destructive operations | Malicious purge of another user's data | Purge requires dedicated `purge` scope + two-person authorization for shared deployments |
| 14 | Permanent credential reuse | Stolen API token used indefinitely | Token expiration with configurable TTL, automatic expiry warnings |

### At-Rest Encryption (v1)

All stored data is encrypted at rest. This is NOT a v2 item — it ships in v1.

**SQLite:** Uses SQLCipher (via `rusqlite` with the `bundled-sqlcipher` feature). The database is AES-256-CBC encrypted. The encryption key is derived from a master passphrase set on `clearmemory init` using Argon2id key derivation.

**Verbatim files:** Each verbatim transcript file is encrypted with AES-256-GCM before writing to disk. The key is derived from the same master passphrase. File names are the content hash (opaque), so directory listing reveals nothing about content.

**LanceDB:** LanceDB files are encrypted at the application level — data is encrypted before writing to the Lance columnar format and decrypted on read. This adds ~5% overhead to read/write operations.

**Key management:**
- On `clearmemory init`, the user sets a master passphrase (or one is auto-generated and displayed once)
- The passphrase derives an encryption key via Argon2id (memory-hard, resistant to GPU attacks)
- The derived key is cached in memory during runtime — the passphrase is never stored on disk
- On startup, if encryption is enabled, the user provides the passphrase (or it's read from an environment variable `CLEARMEMORY_PASSPHRASE` for automated deployments)
- Key rotation: `clearmemory auth rotate-key` re-encrypts all data with a new key derived from a new passphrase

```toml
[encryption]
enabled = true                          # default true for new installations
cipher = "aes-256-gcm"                  # verbatim files and LanceDB
sqlite_cipher = "aes-256-cbc"           # SQLCipher default
kdf = "argon2id"                        # key derivation function
kdf_memory_mb = 64                      # Argon2id memory parameter
kdf_iterations = 3                      # Argon2id time parameter
passphrase_env_var = "CLEARMEMORY_PASSPHRASE"  # env var for automated deployments
```

### Authentication & Token Lifecycle

**Token generation:**
- `clearmemory init` generates a 256-bit API token stored (hashed) in config
- All MCP/HTTP requests require token (`Authorization: Bearer <token>`)
- Invalid tokens rejected with 401 and logged

**Token scopes:**

| Scope | Permissions |
|-------|------------|
| `read` | recall, expand, status, streams list, tags list |
| `read-write` | Everything in read + retain, import, forget, streams create, tags manage |
| `admin` | Everything in read-write + auth management, config changes, repair |
| `purge` | Dedicated scope for destructive operations — purge, hard delete. Separate from admin. |

**Token expiration:**
- Every token has a configurable TTL (default: 90 days)
- Expired tokens are rejected with 401 and a clear message: "Token expired, rotate with `clearmemory auth rotate`"
- 14 days before expiration, the health endpoint includes a warning: `"token_expiry_warning": "primary token expires in 12 days"`
- The system logs a warning daily once a token is within 14 days of expiration
- `clearmemory auth status` shows all tokens with their expiration dates

**Token management commands:**
```bash
clearmemory auth create --scope read --ttl 30d --label "monitoring"
clearmemory auth rotate                     # rotate primary token
clearmemory auth rotate-key                 # rotate encryption passphrase (re-encrypts all data)
clearmemory auth revoke --id monitoring     # revoke specific token
clearmemory auth status                     # show all tokens with scopes, expiry, last used
```

```toml
[auth]
require_token = true
default_token_ttl_days = 90
tokens = [
    { id = "primary", token_hash = "sha256:...", scope = "admin", created_at = "...", expires_at = "..." },
    { id = "readonly", token_hash = "sha256:...", scope = "read", created_at = "...", expires_at = "..." },
    { id = "purge-auth", token_hash = "sha256:...", scope = "purge", created_at = "...", expires_at = "..." }
]
```

### Purge Authorization (Two-Person Rule)

Purge operations (permanent, irreversible deletion) require stronger authorization than normal operations:

**Single-user deployment:** Purge requires the `purge` scope token + `--confirm` flag. The `admin` scope alone cannot purge.

**Shared deployment:** Purge requires two-person authorization:
1. User A requests purge: `clearmemory purge --user "kai@company.com" --request`
2. This creates a pending purge request logged in the audit trail
3. User B (with `purge` scope) approves: `clearmemory purge --approve --request-id <id>`
4. Only after approval does the purge execute
5. Pending requests expire after 72 hours if not approved

```bash
# Single-user: direct purge with purge-scope token
CLEARMEMORY_TOKEN=<purge-token> clearmemory purge --memory-id abc123 --hard --confirm

# Shared deployment: request + approve workflow
clearmemory purge --user "kai@company.com" --request --reason "Employee departure"
# → "Purge request PR-2026-0412 created. Requires approval from purge-scope holder."

clearmemory purge --approve --request-id PR-2026-0412
# → "Purge approved. 847 memories permanently deleted. Backup created at ~/.clearmemory/backups/pre-purge-PR-2026-0412.cmb"
```

```toml
[compliance]
purge_requires_two_person = false       # set true for shared deployments
purge_request_ttl_hours = 72
```

### Secret Scanning & Redaction

A secret scanning pipeline runs on the `retain` path before content is stored. This prevents Clear Memory from becoming a long-term credential store.

**Detection patterns (built-in):**

| Pattern | Examples |
|---------|----------|
| AWS keys | `AKIA...`, `aws_secret_access_key` |
| GitHub tokens | `ghp_`, `gho_`, `ghs_`, `github_pat_` |
| Generic API keys | `api_key=`, `apikey:`, `x-api-key` |
| Database connection strings | `postgres://`, `mysql://`, `mongodb://`, `redis://` |
| Private keys | `-----BEGIN RSA PRIVATE KEY-----`, `-----BEGIN OPENSSH PRIVATE KEY-----` |
| JWT tokens | `eyJ...` (base64-encoded JSON with alg/typ headers) |
| Generic passwords | `password=`, `passwd:`, `secret=` (followed by non-whitespace) |
| Anthropic API keys | `sk-ant-` |
| OpenAI API keys | `sk-proj-`, `sk-` (followed by 40+ chars) |

**Behavior on detection:**

| Mode | Behavior |
|------|----------|
| `warn` (default) | Store the memory as-is but flag it in metadata. The memory gets `contains_secrets = true` in SQLite. A warning is logged. The memory is auto-classified as `confidential` regardless of user-specified classification. |
| `redact` | Replace detected secrets with `[REDACTED:<pattern_type>]` before storage. The verbatim file contains the redacted version. Original content is never stored. |
| `block` | Reject the retain operation entirely. Return an error: "Memory contains detected secrets. Remove credentials and retry." |

```bash
# Check what secrets exist in the corpus
clearmemory security scan                   # scan all stored memories for secrets
clearmemory security scan --stream my-project  # scan specific stream
clearmemory security scan --remediate       # redact secrets in existing memories
```

```toml
[security.secret_scanning]
enabled = true
mode = "warn"                           # "warn", "redact", "block"
custom_patterns = []                    # additional regex patterns
exclude_patterns = []                   # pattern names to disable
```

### Rate Limiting

All MCP and HTTP endpoints are rate-limited per client to prevent DoS:

| Operation Type | Default Limit | Configurable |
|---------------|---------------|-------------|
| Read operations (recall, expand, status) | 1000 req/min per client | Yes |
| Write operations (retain, forget, import) | 100 req/min per client | Yes |
| Reflect operations | 10 req/min per client | Yes |
| Auth operations | 10 req/min per client | Yes |
| Purge operations | 5 req/hour per client | Yes |

Rate limit exceeded returns 429 with `Retry-After` header. All rate limit hits are logged with client identifier.

```toml
[security.rate_limiting]
enabled = true
read_rpm = 1000
write_rpm = 100
reflect_rpm = 10
auth_rpm = 10
purge_rph = 5
max_request_body_mb = 50                # global HTTP body size limit
```

### Tier 3 Classification Pipeline Tracing

The classification check applies to the ENTIRE content pipeline, not just raw memories:

```
Memory (classified: confidential)
    │
    ▼
Retrieval results ──▶ classification check ──▶ if blocked, exclude from pipeline
    │
    ▼ (only eligible content passes)
Curator output ──▶ classification trace ──▶ curator output inherits highest classification of its source memories
    │
    ▼
Reflect input ──▶ classification check ──▶ if any source memory is PII/confidential AND tier = cloud, fall back to local inference
    │
    ▼
Cloud API call ──▶ final classification gate ──▶ reject if classification not in cloud_eligible_classifications
```

Every piece of derived content (curator output, reflect input) carries a `source_classifications` field that tracks the classification levels of all source memories that contributed to it. If any source is above the cloud-eligible threshold, the derived content is treated as if it carries that classification.

### Insider Threat Detection

For shared deployments, Clear Memory monitors access patterns for anomalies:

**Access anomaly detection:**
- Tracks per-user access patterns: which streams they query, how often, at what times
- Flags anomalies: a user suddenly querying streams they've never accessed before, burst access to confidential memories, access outside normal hours
- Anomaly events are logged to the audit log with `anomaly_flag = true`
- Configurable alert threshold (default: 3 standard deviations from the user's normal pattern)

**Confidential access justification:**
- When `require_justification_for_confidential = true`, any recall or expand on a `confidential`-classified memory prompts the caller to provide an access reason
- The reason is recorded in the audit log alongside the access event
- This doesn't block access — it creates an accountability record that can be reviewed

**Separation of duties:**
- Stream creators can grant others access but cannot escalate their own access to streams they don't own
- Admin-scope tokens can manage auth but require `purge` scope for destructive operations (separate token)
- No single token scope grants unrestricted access to all operations

```toml
[security.insider_detection]
enabled = false                         # enable for shared deployments
anomaly_threshold_stddev = 3.0
require_justification_for_confidential = false
alert_on_anomaly = true                 # emit metric + audit log entry
```

### Transport Security
- Unix domain sockets (macOS/Linux) — filesystem-permission protected
- HTTP binds to `127.0.0.1` by default — not network accessible
- TLS supported via `--tls-cert` and `--tls-key` for shared deployments
- Certificate pinning configurable for mutual TLS in zero-trust environments

```toml
[security]
bind_address = "127.0.0.1"
tls_cert_path = ""
tls_key_path = ""
tls_client_ca_path = ""                 # mutual TLS: require client certificates
cloud_eligible_classifications = ["public", "internal"]
max_import_size_mb = 500
max_memory_size_mb = 10
```

---

## Testing Strategy

Three layers: correctness, quality, resilience.

### Layer 1: Correctness (cargo test)
- Unit tests in every module
- Integration tests: end-to-end flows across all 7 import formats
- Migration tests: apply each migration to previous-version DB, verify integrity
- Concurrency tests: concurrent reads + writes, verify no corruption or deadlocks

### Layer 2: Quality (retrieval benchmarks)
- **LongMemEval runner** — runs on every release. Score must meet or exceed previous release. Regression = release blocker.
- **Retrieval regression suite** — 200 curated query/expected-memory pairs. Runs on every PR touching retrieval code.
- **Per-strategy benchmarks** — each strategy has its own precision@5 test to catch silent regressions
- **Reranker validation** — verify reranker consistently improves over fusion-only baseline

### Layer 3: Resilience (adversarial & stress)
- **Malformed inputs** — invalid JSON, missing fields, 100MB single memories, unicode edge cases, null bytes
- **Corpus stress** — 500K memories, 100 concurrent queries, verify latency and correctness
- **Concurrent write stress** — 50 simultaneous retains to same stream, verify all stored
- **Recovery tests** — corrupt SQLite, corrupt LanceDB, delete verbatim files, verify repair recovers gracefully
- **Retention tests** — trigger all three policies, verify correct archival behavior

### CI/CD Pipeline
```yaml
# Every PR
- cargo fmt --check
- cargo clippy -- -D warnings
- cargo test
- retrieval regression suite (200 queries)
- security test suite (auth, rate limiting, secret scanning, classification pipeline, audit chain)

# Every release
- LongMemEval full benchmark
- adversarial test suite
- 500K corpus stress test
- concurrent write stress test
- migration test against previous version
- encryption roundtrip test (encrypt → backup → restore → decrypt → verify)
- model integrity verification against published manifest
```

---

## Documentation Requirements

### API Documentation
- **HTTP:** OpenAPI 3.1 spec via `utoipa` crate, served at `GET /docs`
- **MCP:** JSON Schema for all 9 tools in `docs/mcp_tools_schema.json`

### Operations Runbook (`docs/runbook.md`)
Procedures for: setup (single + shared), backup/restore, migration, troubleshooting (won't start, slow queries, corrupt index), retention tuning, legal hold, audit export, token rotation, reindexing

### Integration Guide (`docs/integration_guide.md`)
MCP integration (Claude Code, Copilot, Cursor), HTTP API usage, ClearPathAI Slice 31, Clear Format with CSV/Excel examples, auto-tagging strategies

### Architecture Decision Records (`docs/adr/`)
- `001-verbatim-over-extraction.md`
- `002-rust-over-python.md`
- `003-bge-m3-embedding.md`
- `004-lancedb-over-sqlite-vss.md`
- `005-tiered-deployment.md`
- `006-streams-over-flat-projects.md`
- New ADRs for any decision a future developer would question

---

## Capacity Planning

### Disk Usage

| Usage Level | Sessions/Day | Monthly Growth | 6-Month Corpus |
|-------------|-------------|----------------|----------------|
| Light (individual) | 5 | ~50MB | ~300MB |
| Moderate (developer) | 15 | ~150MB | ~900MB |
| Heavy (power user) | 30 | ~300MB | ~1.8GB |
| Team (10 devs, shared) | 150 | ~1.5GB | ~9GB |

### RAM Requirements

| Configuration | RAM |
|---------------|-----|
| Tier 1 (binary + embedding + reranker) | ~1.2GB |
| Tier 2 (+ curator resident, reflect on-demand) | ~2.4GB resident, ~4.9GB peak during reflect |
| Tier 2 (all models resident) | ~4.9GB |

Reflect model (4B) loads on demand by default, unloads after inference. Configurable:
```toml
[models]
reflect_resident = false    # true = keep in RAM, false = load/unload on demand
curator_resident = true     # small model, stays loaded
```

### CPU & Performance at Scale

| Corpus Size | Memories | p95 Recall (Tier 1) | p95 Recall (Tier 2) |
|-------------|----------|---------------------|---------------------|
| 100MB | ~2K | <50ms | <1.2s |
| 500MB | ~10K | <80ms | <1.5s |
| 2GB | ~40K | <150ms | <2s |
| 5GB | ~100K | <300ms | <3s |
| 10GB | ~200K | ~500ms | ~4s |

Tier 2 dominated by curator inference (~1s constant). GPU not required but improves reflect latency (candle supports CUDA + Metal). Retrieval scales sub-linearly via LanceDB indexing.

---

## Model Distribution for Enterprise

### Strategy 1: Online Download (Default)
Models download from Hugging Face on `clearmemory init`. Requires internet.

### Strategy 2: Admin Pre-Stage
```bash
# Admin machine
clearmemory models download --all --output ./clearmemory-models/
tar -czf clearmemory-models-v1.0.tar.gz ./clearmemory-models/
# Distribute via Artifactory, Nexus, S3, network share
```
Developer config:
```toml
[models]
model_path = "/shared/tools/clearmemory-models/"
```

### Strategy 3: Bundled Installer
For air-gapped environments — binary + all models in single package (~4.5GB):
```bash
clearmemory package --include-models --output clearmemory-full-v1.0.tar.gz
```
Distributable via USB, network share, SCCM, Jamf.

### Strategy 4: Container Image
For shared server deployments:
```bash
docker pull ghcr.io/clearpathai/clearmemory:v1.0-full
docker run -d -p 8080:8080 -p 9700:9700 \
  -v /data/clearmemory:/root/.clearmemory \
  ghcr.io/clearpathai/clearmemory:v1.0-full serve --both
```

### Model Integrity & Supply Chain Security
- All model files include SHA-256 checksums in `models.manifest`
- Checksums verified on first load — corrupted/tampered files rejected
- Manifest is ed25519-signed for provenance verification
- **Pinned model revisions:** the manifest references exact Hugging Face commit hashes, not just model names. Example: `BAAI/bge-m3@a1b2c3d4` not just `BAAI/bge-m3`. This prevents a compromised Hugging Face repo from silently substituting a poisoned model.
- **Checksums are published in the Clear Memory repository**, not derived from Hugging Face at download time. The verification flow is: download model → check against Clear Memory's published checksums → reject if mismatch. An attacker would need to compromise both Hugging Face AND the Clear Memory repository.
- **Model verification gate:** when a new model version is adopted in a Clear Memory release, it must pass the full LongMemEval benchmark suite before its checksums are added to the manifest. This is part of the release CI/CD pipeline.
- **Enterprise model mirror:** for organizations that want full supply chain control, the admin pre-stage workflow (`clearmemory models download`) pulls from Hugging Face to an internal mirror. Developer machines are configured to pull from the internal mirror only (`auto_download = false` + `model_path` set). The enterprise never trusts Hugging Face directly.

```bash
# Verify installed model integrity at any time
clearmemory models verify                   # check all models against manifest
clearmemory models verify --verbose         # show per-file checksums
```

### Configuration
```toml
[models]
model_path = ""                 # empty = HuggingFace download. Set for pre-staged.
verify_checksums = true
auto_download = true            # false = prevent network downloads
```

---

## Incident Response Playbook

Enterprise security teams need documented procedures for security events. Clear Memory provides built-in tooling for each phase of incident response.

### Incident Type 1: Device Lost or Stolen

**Detection:** User or IT reports device loss.

**Immediate containment:**
```bash
# From any other machine with admin access to a shared deployment:
clearmemory auth revoke --id <all-tokens-for-lost-device>
```
For single-user deployments where the server ran on the lost device, the at-rest encryption protects stored data — the attacker would need the master passphrase to decrypt.

**Assessment:** Review the audit log for any access between the time of loss and token revocation:
```bash
clearmemory audit export --from <loss_timestamp> --to <revocation_timestamp> --format json
```

**Recovery:**
1. Restore from most recent backup on a new device: `clearmemory restore <backup.cmb>`
2. Rotate all API tokens: `clearmemory auth rotate`
3. Rotate encryption key: `clearmemory auth rotate-key`
4. Review memories for any credentials that may have been exposed, run secret scan: `clearmemory security scan --remediate`

### Incident Type 2: Unauthorized Stream Access

**Detection:** Audit log anomaly alert, or manual review showing a user accessed streams outside their normal pattern.

**Immediate containment:**
```bash
# Revoke the suspicious token
clearmemory auth revoke --id <suspicious_token_id>

# If the stream contains confidential data, place it on legal hold to preserve evidence
clearmemory hold --stream <affected_stream> --reason "Security investigation: unauthorized access"
```

**Assessment:**
```bash
# Pull all access records for the affected stream
clearmemory audit export --stream <affected_stream> --from <incident_start> --format json

# Check if any confidential memories were expanded (full content accessed)
# Look for operation="expand" entries on confidential-classified memories
```

**Recovery:**
1. If data was exfiltrated to Tier 3 cloud APIs, notify the cloud provider and initiate breach protocol
2. Rotate affected stream's permissions: update visibility, remove unauthorized users
3. Document the incident in the audit log via a compliance event

### Incident Type 3: Poisoned Model Detected

**Detection:** Retrieval quality regression detected in benchmarks, or anomalous embedding outputs flagged by monitoring.

**Immediate containment:**
```bash
# Stop the server
clearmemory serve --stop

# Verify model integrity
clearmemory models verify --verbose
```

**Assessment:** If checksums don't match the manifest, the model was tampered with. If checksums match but quality degraded, the model may have been poisoned upstream before your manifest was created.

**Recovery:**
```bash
# Re-download from a known-good source (your internal mirror, not Hugging Face directly)
clearmemory models download --force --source internal

# Re-run retrieval benchmarks to verify quality restored
cargo bench --bench retrieval_regression

# If the index was built with the poisoned model, re-embed the entire corpus
clearmemory reindex --model bge-m3
```

### Incident Type 4: Secret Exposure in Stored Memories

**Detection:** Secret scanning finds credentials in stored memories (scheduled scan or alert from `clearmemory security scan`).

**Immediate response:**
1. Rotate the exposed credentials in their source system immediately (AWS console, GitHub settings, database admin, etc.)
2. Run targeted remediation:
```bash
clearmemory security scan --remediate --stream <affected_stream>
```
3. This redacts the secrets in stored memories retroactively

**Assessment:** Review whether the affected memories were ever sent to Tier 3 cloud APIs:
```bash
clearmemory audit export --filter "cloud_api_call=true" --from <memory_creation_date> --format json
```
If yes, the credentials were potentially exposed to the cloud provider.

### Incident Type 5: Audit Log Integrity Breach

**Detection:** `clearmemory audit verify` reports a broken chain or checkpoint mismatch.

**Immediate response:**
```bash
# Identify the break point
clearmemory audit verify --verbose
# This reports exactly which entry has a hash mismatch

# Cross-reference with external checkpoints
# Check syslog, Splunk, or Datadog for the checkpoint hashes emitted at the corresponding timestamps
```

**Assessment:** If the chain is broken at a specific point, entries after that point may have been tampered with. Entries before the break point (verified by earlier external checkpoints) are trustworthy.

**Recovery:** The audit log cannot be repaired — the break is permanent evidence of tampering. Document the incident, preserve the corrupted log as evidence, and start a new chain. The old log remains alongside the new one for forensic analysis.

### Post-Incident Documentation
Every incident should result in:
1. An entry in the audit log with `compliance_event = true` and the incident details
2. A timeline of detection → containment → recovery
3. Root cause analysis
4. Remediation steps taken
5. Policy changes to prevent recurrence

---

## Non-Goals (v1)

- No cloud-hosted backend or SaaS offering — Clear Memory is local-first software
- No account system or user authentication beyond API tokens — organizational identity is handled by the consuming application (ClearPathAI)
- No built-in UI — Clear Memory is a Rust binary with CLI, MCP, and HTTP interfaces
- No training or fine-tuning of models — inference only with pre-trained models
- No real-time streaming ingest (e.g., live Slack monitoring) — import is batch-based
- No per-stream encryption with separate keys (v2 — v1 encrypts everything with one master key)
