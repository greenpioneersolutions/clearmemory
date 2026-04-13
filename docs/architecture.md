# Clear Memory — Architecture

## Overview

Clear Memory is a Rust-native memory engine that sits between users and their AI coding tools, intercepting prompts to inject relevant historical context before they reach the model. It stores every conversation verbatim, retrieves relevant fragments via four parallel search strategies, and assembles a token-budget-aware context payload — reducing AI costs while preserving institutional knowledge.

This document covers the system architecture in depth. For security specifics, see `security.md`. For the full project constitution, see `CLAUDE.md`.

---

## System Architecture

```
                          ┌─────────────┐
                          │  User Input  │
                          └──────┬──────┘
                                 │
                                 ▼
               ┌─────────────────────────────────┐
               │        CLIENT LAYER              │
               │                                  │
               │  ClearPathAI (Electron GUI)      │
               │    — or —                        │
               │  Claude Code / Copilot (MCP)     │
               │    — or —                        │
               │  CLI (direct)                    │
               │    — or —                        │
               │  HTTP API (custom integration)   │
               └────────────────┬────────────────┘
                                │
                    ┌───────────┴───────────┐
                    │   INTERFACE LAYER     │
                    │                       │
                    │  ┌─────────────────┐  │
                    │  │   MCP Server    │  │  ← 9 tools, token-authenticated
                    │  │   (port 9700)   │  │
                    │  └────────┬────────┘  │
                    │           │           │
                    │  ┌────────┴────────┐  │
                    │  │   HTTP/JSON API │  │  ← OpenAPI 3.1, versioned /v1/*
                    │  │   (port 8080)   │  │
                    │  └────────┬────────┘  │
                    │           │           │
                    │  ┌────────┴────────┐  │
                    │  │   Auth + Rate   │  │  ← Token validation, scope check,
                    │  │   Limiter       │  │     per-client rate limiting
                    │  └────────┬────────┘  │
                    └───────────┼───────────┘
                                │
                    ┌───────────┴───────────┐
                    │   CORE ENGINE         │
                    │                       │
                    │  ┌─────────────────┐  │
                    │  │  Write Path     │  │
                    │  │  (serialized)   │  │
                    │  │                 │  │
                    │  │  retain ──┐     │  │
                    │  │  import ──┤     │  │
                    │  │  forget ──┤     │  │
                    │  │  purge ───┘     │  │
                    │  │      │         │  │
                    │  │      ▼         │  │
                    │  │  Write Queue   │  │  ← tokio mpsc, serialized writes
                    │  │      │         │  │
                    │  │      ▼         │  │
                    │  │  ┌─────────┐   │  │
                    │  │  │ Secret  │   │  │  ← Scan before storage
                    │  │  │ Scanner │   │  │
                    │  │  └────┬────┘   │  │
                    │  │       │        │  │
                    │  │       ▼        │  │
                    │  │  ┌─────────┐   │  │
                    │  │  │Encryptor│   │  │  ← AES-256-GCM before disk
                    │  │  └────┬────┘   │  │
                    │  │       │        │  │
                    │  │       ▼        │  │
                    │  │  Storage Layer │  │
                    │  └─────────────────┘  │
                    │                       │
                    │  ┌─────────────────┐  │
                    │  │  Read Path      │  │
                    │  │  (concurrent)   │  │
                    │  │                 │  │
                    │  │  recall ────┐   │  │
                    │  │  expand ────┤   │  │
                    │  │  reflect ───┤   │  │
                    │  │  status ────┘   │  │
                    │  │      │         │  │
                    │  │      ▼         │  │
                    │  │  Retrieval     │  │
                    │  │  Pipeline      │  │
                    │  │  (4-strategy)  │  │
                    │  │      │         │  │
                    │  │      ▼         │  │
                    │  │  Curator       │  │  ← Tier 2+ only
                    │  │  (Qwen3-0.6B) │  │
                    │  │      │         │  │
                    │  │      ▼         │  │
                    │  │  Context       │  │
                    │  │  Compiler      │  │  ← Token budget enforcement
                    │  └─────────────────┘  │
                    │                       │
                    └───────────────────────┘
                                │
                    ┌───────────┴───────────┐
                    │   STORAGE LAYER       │
                    │                       │
                    │  ┌─────────────────┐  │
                    │  │ SQLite          │  │  ← SQLCipher encrypted
                    │  │ (structured)    │  │     Facts, entities, tags,
                    │  │                 │  │     streams, audit log,
                    │  │                 │  │     retention metadata
                    │  └─────────────────┘  │
                    │                       │
                    │  ┌─────────────────┐  │
                    │  │ LanceDB         │  │  ← Application-level encrypted
                    │  │ (vectors)       │  │     Dense + sparse embeddings
                    │  │                 │  │     Metadata-filtered search
                    │  └─────────────────┘  │
                    │                       │
                    │  ┌─────────────────┐  │
                    │  │ Verbatim Files  │  │  ← AES-256-GCM per file
                    │  │ (transcripts)   │  │     SHA-256 integrity checks
                    │  └─────────────────┘  │
                    │                       │
                    └───────────────────────┘
                                │
                    ┌───────────┴───────────┐
                    │   ML INFERENCE LAYER  │
                    │                       │
                    │  BGE-M3 (embedding)   │  ← Always loaded, ONNX Runtime
                    │  BGE-Reranker-Base    │  ← Always loaded, ONNX Runtime
                    │  Qwen3-0.6B (curator) │  ← Tier 2+, candle, resident
                    │  Qwen3-4B (reflect)   │  ← Tier 2+, candle, on-demand
                    │                       │
                    └───────────────────────┘
```

---

## Retrieval Pipeline (Detail)

The retrieval pipeline is the core of Clear Memory. Every `recall` operation triggers this flow:

```
Query: "why did we switch to GraphQL?"
    │
    ├──▶ Stream resolution
    │    Determine active stream from context or explicit filter
    │    Check stream visibility permissions for requesting user
    │
    ├──▶ 4-Strategy Parallel Search (all execute concurrently via tokio::join!)
    │
    │    ┌─── Strategy 1: Semantic Similarity ──────────────────────┐
    │    │ Embed query with BGE-M3 dense encoder                    │
    │    │ Search LanceDB for top-K nearest vectors                 │
    │    │ Filter by stream tags (metadata filter on LanceDB)       │
    │    │ Return: [(memory_id, distance), ...]                     │
    │    └──────────────────────────────────────────────────────────┘
    │
    │    ┌─── Strategy 2: Keyword Matching ─────────────────────────┐
    │    │ Encode query with BGE-M3 sparse encoder                  │
    │    │ Search LanceDB sparse index for term overlap              │
    │    │ Catches exact terms: "GraphQL", "Clerk", error messages  │
    │    │ Return: [(memory_id, score), ...]                        │
    │    └──────────────────────────────────────────────────────────┘
    │
    │    ┌─── Strategy 3: Temporal Proximity ───────────────────────┐
    │    │ Parse time references from query ("last month", dates)   │
    │    │ Score all memories by temporal distance to reference      │
    │    │ Apply up to 40% distance reduction for proximate matches │
    │    │ Return: [(memory_id, temporal_score), ...]               │
    │    └──────────────────────────────────────────────────────────┘
    │
    │    ┌─── Strategy 4: Entity Graph Traversal ───────────────────┐
    │    │ Extract entity mentions from query                       │
    │    │ Look up entities in SQLite entity table                   │
    │    │ Traverse relationships: entity → works_on → project →    │
    │    │   related memories                                       │
    │    │ Apply 30% entity boost to connected memories             │
    │    │ Return: [(memory_id, entity_score), ...]                 │
    │    └──────────────────────────────────────────────────────────┘
    │
    ├──▶ Merge (Reciprocal Rank Fusion)
    │    Combine results from all 4 strategies
    │    RRF formula: score = Σ(1 / (k + rank_i)) for each strategy
    │    Deduplicate memory IDs, sum fused scores
    │    Sort by fused score descending
    │    Take top-K candidates (configurable, default 10)
    │
    ├──▶ Rerank (BGE-Reranker-Base)
    │    Cross-encoder scores each (query, memory_summary) pair
    │    Reorder candidates by relevance score
    │    This catches semantically-similar but non-answering results
    │
    ├──▶ Classification Check
    │    If Tier 3 (cloud): verify no PII/confidential in results
    │    If blocked, exclude from pipeline or fall back to local
    │    Trace classification through all derived content
    │
    ├──▶ Curator (Tier 2+ only)
    │    Qwen3-0.6B receives: query + top-K memory summaries
    │    Identifies which portions of each memory are relevant
    │    Strips irrelevant context from multi-topic sessions
    │    Returns only targeted excerpts
    │
    ├──▶ Context Compiler
    │    Assemble final payload within token budget:
    │    L0 (identity, ~50 tokens) — always
    │    L1 (working set, ~200-500 tokens) — always
    │    L2 (recall results) — fill remaining budget
    │    L3 (cross-stream deep search) — if budget remains
    │    Deduplicate against known CLI context (CLAUDE.md, files)
    │    Stop when budget exhausted
    │
    └──▶ Return
         Summary mode: return memory summaries (for progressive loading)
         Expand mode: return full verbatim content for specific ID
         Context mode: return assembled L0+L1+L2+L3 payload
```

---

## Write Path (Detail)

All write operations are serialized through a single async writer task to prevent SQLite/LanceDB inconsistency.

```
retain("We decided to use Clerk for auth", tags={team: platform, project: q1-migration})
    │
    ├──▶ Authentication + Authorization
    │    Validate API token, check scope (read-write or admin)
    │    Verify write access to target stream
    │
    ├──▶ Rate Limit Check
    │    100 writes/min per client (configurable)
    │    If exceeded: 429 response with Retry-After
    │
    ├──▶ Secret Scanning
    │    Run content against 9+ pattern categories
    │    Mode: warn (flag + classify as confidential)
    │         redact (replace secrets with [REDACTED])
    │         block (reject the retain)
    │
    ├──▶ Classification
    │    Apply data classification (default: internal)
    │    Override if secrets detected (auto-classify as confidential)
    │
    ├──▶ Write Queue (tokio mpsc channel)
    │    Enqueue write operation
    │    Interactive writes from users take priority over bulk imports
    │
    ├──▶ Writer Task (single async task, processes queue sequentially)
    │
    │    ┌─── 1. Generate embedding ─────────────────────────────────┐
    │    │ BGE-M3 dense + sparse encoding of content                 │
    │    └───────────────────────────────────────────────────────────┘
    │
    │    ┌─── 2. Encrypt content ────────────────────────────────────┐
    │    │ AES-256-GCM encrypt verbatim content                      │
    │    │ Write encrypted file to ~/.clearmemory/verbatim/           │
    │    │ Store SHA-256 hash of plaintext for integrity verification │
    │    └───────────────────────────────────────────────────────────┘
    │
    │    ┌─── 3. SQLite transaction (IMMEDIATE) ─────────────────────┐
    │    │ INSERT into memories table                                 │
    │    │ INSERT into memory_tags (one row per tag)                  │
    │    │ INSERT into facts (if fact extraction enabled)             │
    │    │ UPDATE entities + entity_aliases (if entity detected)      │
    │    │ INSERT into audit_log                                      │
    │    └───────────────────────────────────────────────────────────┘
    │
    │    ┌─── 4. LanceDB append ─────────────────────────────────────┐
    │    │ Append dense vector to memories collection                 │
    │    │ Append sparse vector to sparse collection                  │
    │    │ Include metadata: memory_id, stream_id, tags, timestamp    │
    │    └───────────────────────────────────────────────────────────┘
    │
    │    ┌─── 5. Entity Resolution (Tier 2+) ────────────────────────┐
    │    │ Tier 1: heuristic matching (exact, case-insensitive)       │
    │    │ Tier 2: Qwen3-0.6B resolves aliases                       │
    │    │ "the auth service" == "our OAuth system" == "login svc"    │
    │    │ Update entity_aliases table                                │
    │    └───────────────────────────────────────────────────────────┘
    │
    └──▶ Response
         Return memory_id to caller
         Emit metrics: retain.latency_ms, retain.size_bytes
```

---

## Storage Architecture

### Directory Layout

```
~/.clearmemory/
├── config.toml                 ← All configuration (encrypted passphrase not stored here)
├── clearmemory.db              ← SQLite database (SQLCipher encrypted)
├── audit_checkpoints.log       ← External audit chain checkpoint hashes
├── vectors/                    ← LanceDB directory (application-level encrypted)
│   └── memories/
│       ├── dense/              ← BGE-M3 dense vectors (1024 dimensions)
│       └── sparse/             ← BGE-M3 sparse vectors
├── verbatim/                   ← Encrypted transcript files
│   ├── a1b2c3.enc             ← AES-256-GCM encrypted, named by content hash
│   └── d4e5f6.enc
├── archive/                    ← Archived memories (same encryption)
│   └── verbatim/
├── models/                     ← ML models (downloaded on first run)
│   ├── bge-m3-onnx/
│   ├── bge-reranker-base-onnx/
│   ├── qwen3-0.6b/            ← Curator model (Tier 2+)
│   ├── qwen3-4b/              ← Reflect model (Tier 2+)
│   └── models.manifest        ← Checksums + ed25519 signature
├── mental_models/              ← Reflect-generated synthesis documents
├── backups/                    ← Scheduled backup .cmb files (encrypted)
└── migrations/                 ← Applied migration history
```

### Data Flow Between Storage Components

```
                    ┌─────────────────────┐
                    │     SQLite           │
                    │  (metadata + graph)  │
                    │                      │
                    │  memories ◄──────────┼──── memory_id links to verbatim file
                    │  facts               │
                    │  entities            │
                    │  entity_aliases      │
                    │  entity_relationships│
                    │  streams             │
                    │  memory_tags         │
                    │  audit_log           │
                    │  legal_holds         │
                    │  retention_events    │
                    │  performance_baselines│
                    │  schema_version      │
                    │  migration_log       │
                    └─────────┬───────────┘
                              │
                    memory_id │ links across stores
                              │
                    ┌─────────┴───────────┐
                    │     LanceDB          │
                    │  (vector search)     │
                    │                      │
                    │  dense vectors ◄─────┼──── embedding of memory content
                    │  sparse vectors ◄────┼──── sparse encoding for keywords
                    │  metadata: memory_id,│     
                    │    stream_id, tags,   │     
                    │    timestamp          │     
                    └─────────────────────┘
                              │
                    memory_id │ content_hash maps to file
                              │
                    ┌─────────┴───────────┐
                    │  Verbatim Files      │
                    │  (encrypted on disk) │
                    │                      │
                    │  {content_hash}.enc  │
                    │  Original transcript │
                    │  in full fidelity    │
                    └─────────────────────┘
```

### Concurrency Model

```
                ┌─────────────────────────────────────┐
                │           Connection Pool             │
                │                                       │
   Read ────────┤  Read Conn 1 ──┐                      │
   Read ────────┤  Read Conn 2 ──┤── concurrent reads   │
   Read ────────┤  Read Conn 3 ──┤   (WAL mode)         │
   Read ────────┤  Read Conn 4 ──┘                      │
                │                                       │
   Write ───────┤  Write Queue ──▶ Writer Task ──▶      │
   Write ───────┤  (mpsc chan)     (single task)        │
   Write ───────┤                   │                   │
   Import ──────┤                   ├─▶ SQLite txn      │
                │                   ├─▶ LanceDB append  │
                │                   └─▶ Verbatim write  │
                └─────────────────────────────────────┘

  Reads: unlimited concurrency, never blocked by writes (WAL)
  Writes: serialized through queue, guaranteed ordering
  Imports: same queue as writes, lower priority than interactive writes
```

---

## Tiered Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                         TIER 1: Offline                          │
│                                                                  │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌───────────┐  │
│  │  BGE-M3    │  │  BGE-M3    │  │  Temporal   │  │  Entity   │  │
│  │  Dense     │  │  Sparse    │  │  Scoring    │  │  Graph    │  │
│  │  Search    │  │  Search    │  │             │  │  Traversal│  │
│  └─────┬──────┘  └─────┬──────┘  └──────┬─────┘  └─────┬─────┘  │
│        └────────┬──────┴─────────┬──────┘              │        │
│                 ▼                ▼                      │        │
│        ┌────────────────────────────────────────────────┘        │
│        │  Reciprocal Rank Fusion + BGE-Reranker-Base             │
│        └────────────────────────┬───────────────────────         │
│                                 ▼                                │
│                    Context Compiler (token budget)                │
│                                                                  │
│  Zero external calls. ~96% accuracy. ~1.2GB RAM.                 │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                    TIER 2: Offline + Local LLM                   │
│                                                                  │
│  Everything in Tier 1, plus:                                     │
│                                                                  │
│  ┌────────────────────┐       ┌─────────────────────────┐        │
│  │  Curator            │       │  Reflect Engine          │        │
│  │  Qwen3-0.6B         │       │  Qwen3-4B                │        │
│  │  (~1s per query)    │       │  (~5-10s per synthesis)  │        │
│  │                     │       │                          │        │
│  │  Parses retrieval   │       │  Synthesizes across      │        │
│  │  results, extracts  │       │  memories into coherent  │        │
│  │  relevant portions  │       │  mental models           │        │
│  └─────────────────────┘       └──────────────────────────┘        │
│                                                                  │
│  ┌─────────────────────┐                                         │
│  │  Entity Resolution   │                                         │
│  │  LLM-enhanced alias  │                                         │
│  │  linking              │                                         │
│  └──────────────────────┘                                         │
│                                                                  │
│  Zero external calls. ~99% accuracy. ~2.4-4.9GB RAM.             │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                    TIER 3: Cloud-Connected                        │
│                                                                  │
│  Everything in Tier 2, plus:                                     │
│                                                                  │
│  ┌──────────────────────────────────────────────────────┐        │
│  │  Cloud API Integration                                │        │
│  │                                                       │        │
│  │  Curator → Claude Haiku / GPT-5-mini                  │        │
│  │  Reflect → Claude Sonnet / GPT-5                      │        │
│  │  Entity Resolution → best available model             │        │
│  │                                                       │        │
│  │  ┌─────────────────────────────────┐                  │        │
│  │  │  Classification Gate            │                  │        │
│  │  │  PII/confidential → local only  │                  │        │
│  │  │  public/internal → cloud OK     │                  │        │
│  │  └─────────────────────────────────┘                  │        │
│  └───────────────────────────────────────────────────────┘        │
│                                                                  │
│  Cloud calls for enhanced quality. 99%+ accuracy.                │
└──────────────────────────────────────────────────────────────────┘
```

---

## Tag Taxonomy & Streams Architecture

```
                    ┌─────────────────────────────┐
                    │          Memory              │
                    │  "We decided to use Clerk"   │
                    └─────────────┬───────────────┘
                                  │
                    ┌─────────────┴───────────────┐
                    │         memory_tags           │
                    │                               │
                    │  team: platform               │
                    │  repo: auth-service            │
                    │  project: q1-migration         │
                    │  domain: security/auth         │
                    └─────────────┬───────────────┘
                                  │
              ┌───────────────────┼───────────────────┐
              │                   │                   │
              ▼                   ▼                   ▼
    ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
    │  Stream A        │ │  Stream B        │ │  Stream C        │
    │  "Platform Auth" │ │  "Q1 Migration"  │ │  "All Security"  │
    │                  │ │                  │ │                  │
    │  team: platform  │ │  project:        │ │  domain:         │
    │  domain:         │ │    q1-migration  │ │    security/*    │
    │    security/auth │ │                  │ │                  │
    │                  │ │  (all teams)     │ │  (all teams,     │
    │  visibility:     │ │                  │ │   all projects)  │
    │    team           │ │  visibility:     │ │                  │
    │  owner: kai      │ │    org           │ │  visibility:     │
    └─────────────────┘ └─────────────────┘ │    team           │
                                             └─────────────────┘

    The same memory appears in all three streams because its tags
    intersect with each stream's filter definition.

    A recall within Stream A only searches memories tagged with
    team:platform AND domain:security/auth.

    Related stream detection: when searching Stream A, the system
    also checks Stream B and C for potentially relevant results
    from adjacent tag intersections.
```

---

## Context Compiler Architecture

```
Token Budget: 4096 tokens (configurable)

┌──────────────────────────────────────────────────────────┐
│  L0: Identity (always loaded)                    ~50 tok │
│  Who is the user, what CLI is active, current project    │
├──────────────────────────────────────────────────────────┤
│  L1: Working Set (always loaded)            ~200-500 tok │
│  Active stream context, recent decisions, project state  │
│  Updated from most recent sessions in active stream      │
├──────────────────────────────────────────────────────────┤
│  L2: Recall Results (on demand)            ~1000-2000 tok│
│  Relevant memories from retrieval pipeline               │
│  Summaries first; full content via expand if needed      │
│  Curator-filtered in Tier 2+                             │
├──────────────────────────────────────────────────────────┤
│  L3: Deep Search (on demand)                ~500-1000 tok│
│  Cross-stream results when L2 insufficient               │
│  Only triggered for cross-project queries                │
├──────────────────────────────────────────────────────────┤
│  REMAINING BUDGET → passed to CLI for user's actual      │
│  prompt + model's own context                            │
└──────────────────────────────────────────────────────────┘

Priority: L0 > L1 > L2 > L3
If budget is exhausted at L2, L3 is skipped.
If L2 results are large, least-relevant memories are trimmed.

Deduplication: content already in CLI context (CLAUDE.md, files)
is detected via hashing and deprioritized.
```

---

## ML Inference Architecture

```
┌─────────────────────────────────────────────────────┐
│  fastembed-rs (ONNX Runtime backend)                │
│                                                     │
│  ┌──────────────────────┐  ┌──────────────────────┐ │
│  │  BGE-M3              │  │  BGE-Reranker-Base   │ │
│  │  ~600MB quantized    │  │  ~400MB quantized    │ │
│  │  Dense: 1024 dims    │  │  Cross-encoder       │ │
│  │  Sparse: term weights│  │  (query, doc) → score│ │
│  │  ~50ms per embed     │  │  ~20ms per pair      │ │
│  │  Always resident     │  │  Always resident     │ │
│  └──────────────────────┘  └──────────────────────┘ │
└─────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────┐
│  candle (Hugging Face Rust ML framework)            │
│                                                     │
│  ┌──────────────────────┐  ┌──────────────────────┐ │
│  │  Qwen3-0.6B          │  │  Qwen3-4B            │ │
│  │  ~1.2GB quantized    │  │  ~2.5GB quantized    │ │
│  │  Curator: parse +    │  │  Reflect: synthesis   │ │
│  │    filter results    │  │    across memories    │ │
│  │  ~1s per inference   │  │  ~5-10s per inference │ │
│  │  Always resident     │  │  Loaded on demand     │ │
│  │  (Tier 2+ only)      │  │  (Tier 2+ only)       │ │
│  └──────────────────────┘  └──────────────────────┘ │
└─────────────────────────────────────────────────────┘

Model loading strategy:
  - Tier 1: BGE-M3 + BGE-Reranker only (~1.2GB total)
  - Tier 2: + Qwen3-0.6B resident + Qwen3-4B on-demand
  - reflect_resident=false (default): 4B loads when reflect
    is called, unloads after inference completes
  - reflect_resident=true: keeps 4B in RAM for faster
    reflect at the cost of ~2.5GB permanent RAM usage
```

---

## Retention Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Three Retention Triggers (run periodically + on startup)   │
│                                                             │
│  ┌─────────────────┐ ┌─────────────────┐ ┌───────────────┐ │
│  │  Time-Based      │ │  Size-Based      │ │  Performance  │ │
│  │                  │ │                  │ │               │ │
│  │  > 90 days old   │ │  Corpus > 2GB   │ │  p95 > 200ms  │ │
│  │  AND             │ │                  │ │               │ │
│  │  not accessed    │ │  Identify oldest │ │  Identify     │ │
│  │  recently        │ │  least-accessed  │ │  heaviest     │ │
│  │                  │ │  memories        │ │  streams      │ │
│  │  Access resets   │ │                  │ │               │ │
│  │  the clock       │ │  Warn user first │ │  Recommend    │ │
│  └────────┬─────────┘ └────────┬─────────┘ └───────┬───────┘ │
│           └────────────┬───────┴───────────┬───────┘         │
│                        ▼                                     │
│              ┌─────────────────────┐                         │
│              │  Archive Candidates  │                         │
│              │  (shown to user)     │                         │
│              └──────────┬──────────┘                         │
│                         │                                    │
│              ┌──────────┴──────────┐                         │
│              │  Legal Hold Check    │                         │
│              │  Held streams are    │                         │
│              │  exempt from archive │                         │
│              └──────────┬──────────┘                         │
│                         │                                    │
│                         ▼                                    │
│              ┌─────────────────────┐                         │
│              │  Archive Operation   │                         │
│              │                      │                         │
│              │  1. Move verbatim    │                         │
│              │     to archive/      │                         │
│              │  2. Remove vectors   │                         │
│              │     from LanceDB    │                         │
│              │  3. Set archived=1   │                         │
│              │     in SQLite        │                         │
│              │  4. Log retention    │                         │
│              │     event            │                         │
│              └─────────────────────┘                         │
│                                                              │
│  Archived memories:                                          │
│  - NOT deleted, only moved                                   │
│  - Metadata stays in SQLite (queryable)                      │
│  - Excluded from normal recall                               │
│  - Included with --include-archive flag                      │
└──────────────────────────────────────────────────────────────┘
```

---

## ClearPathAI Integration Architecture

```
┌────────────────────────────────────────────────────────┐
│  ClearPathAI (Electron)                                │
│                                                        │
│  ┌──────────────────────────────────────────────────┐  │
│  │  CLIManager (adapter pattern)                     │  │
│  │                                                   │  │
│  │  ┌──────────────┐    ┌───────────────┐            │  │
│  │  │ CopilotAdapter│    │ ClaudeAdapter  │            │  │
│  │  └──────┬───────┘    └───────┬───────┘            │  │
│  │         └────────┬───────────┘                     │  │
│  │                  │                                 │  │
│  │         ┌────────┴────────┐                        │  │
│  │         │ Before sending   │                        │  │
│  │         │ prompt to CLI:   │                        │  │
│  │         │                  │                        │  │
│  │         │ 1. Call Clear    │                        │  │
│  │         │    Memory for    │◄──── Local socket ────┐│  │
│  │         │    context       │                       ││  │
│  │         │ 2. Prepend L0+L1 │                       ││  │
│  │         │    to prompt     │                       ││  │
│  │         │ 3. If relevant   │                       ││  │
│  │         │    memories,     │                       ││  │
│  │         │    append L2     │                       ││  │
│  │         │ 4. Send enriched │                       ││  │
│  │         │    prompt to CLI │                       ││  │
│  │         └────────┬────────┘                        ││  │
│  │                  │                                 ││  │
│  │                  ▼                                 ││  │
│  │         ┌────────────────┐                         ││  │
│  │         │ After session:  │                         ││  │
│  │         │                 │                         ││  │
│  │         │ 1. Send full    │                         ││  │
│  │         │    transcript   │────── retain ──────────►││  │
│  │         │    to Clear     │                        ││  │
│  │         │    Memory       │                        ││  │
│  │         │ 2. Auto-tag     │                        ││  │
│  │         │    from active  │                        ││  │
│  │         │    workspace    │                        ││  │
│  │         └─────────────────┘                        ││  │
│  └────────────────────────────────────────────────────┘│  │
│                                                        │  │
│  ┌──────────────────────────────────────────────────┐  │  │
│  │  Analytics Dashboard (Slice 19)                   │  │  │
│  │                                                   │  │  │
│  │  • Tokens saved per session/day/week/month        │  │  │
│  │  • Memory corpus health                           │  │  │
│  │  • Most-accessed memories and streams             │  │  │
│  │  • Retrieval latency trends                       │  │  │
│  └───────────────────────────────────────────────────┘  │  │
└────────────────────────────────────────────────────────┘  │
                                                            │
┌───────────────────────────────────────────────────────────┘
│
│  ┌────────────────────────────────────────────────────────┐
│  │  Clear Memory Engine (Rust sidecar)                    │
│  │                                                        │
│  │  Launched by ClearPathAI on app start:                 │
│  │  clearmemory serve --both                              │
│  │                                                        │
│  │  Communicates via local socket                         │
│  │  MCP (port 9700) + HTTP (port 8080)                    │
│  │                                                        │
│  │  Runs independently — survives ClearPathAI restart     │
│  │  Also usable standalone via CLI or other MCP clients   │
│  └────────────────────────────────────────────────────────┘
```

---

## Observability Architecture

```
┌────────────────────────────────────────────────────────┐
│  Clear Memory Engine                                   │
│                                                        │
│  ┌──────────────────────┐                              │
│  │  tracing crate        │  ← Structured logging       │
│  │  (spans + events)     │     to stdout/file           │
│  └──────────┬───────────┘                              │
│             │                                          │
│  ┌──────────┴───────────┐                              │
│  │  OpenTelemetry SDK    │  ← If OTEL endpoint set     │
│  │  (metrics + traces)   │                              │
│  └──────────┬───────────┘                              │
│             │                                          │
│  ┌──────────┴───────────┐                              │
│  │  Health Endpoint      │  ← GET /health               │
│  │  (JSON status)        │     K8s probes compatible    │
│  └──────────────────────┘                              │
└──────────────┬─────────────────────────────────────────┘
               │
    ┌──────────┴──────────────────────┐
    │                                 │
    ▼                                 ▼
┌──────────┐                  ┌──────────────┐
│ Datadog  │                  │ Grafana      │
│ Splunk   │                  │ Prometheus   │
│ etc.     │                  │ etc.         │
└──────────┘                  └──────────────┘

Metrics emitted: corpus size, recall latency (p50/p95/p99),
per-strategy latency, curator/reflect latency, tokens saved,
retention events, errors by component.

Traces: every MCP/HTTP request creates a span with child spans
for each retrieval strategy, merge, rerank, curator, and
context compilation. Trace IDs propagate to audit log.
```
