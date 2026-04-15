# Clear Memory

**Store everything. Send only what matters. Pay for less.**

Clear Memory is a high-performance AI memory engine built in Rust. It stores every AI conversation verbatim, retrieves relevant context in milliseconds, and injects optimized context into LLM prompts — reducing token costs while preserving institutional knowledge.

Every architecture decision, debugging session, and project conversation your team has with AI disappears when the session ends. Clear Memory fixes that. It keeps every word, makes it searchable, and gives your AI the right context at the right time — without bloating the prompt.

---

## Why Clear Memory

**Your AI forgets everything.** Six months of daily AI use generates millions of tokens of decisions, reasoning, and context. All gone when the session ends.

**Existing solutions are lossy.** Other memory systems use an LLM to decide what to remember. They extract "user prefers Postgres" and throw away the conversation where you explained *why*. Clear Memory stores the actual words and lets retrieval find what matters.

**Token costs are exploding.** Under token-based pricing, every bloated context window burns money. Clear Memory's context compiler sends only the relevant fragments within a configurable token budget — injecting targeted memory instead of full conversation history.

**Your data should stay yours.** Clear Memory runs entirely on your machine. No cloud, no subscriptions, no data leaving your network. You choose your security posture.

---

## Quick Start

```bash
# Install
cargo install clearmemory

# Initialize (downloads embedding model, ~600MB)
clearmemory init

# Import your existing conversations
clearmemory import ~/.claude/ --format claude_code
clearmemory import ~/chatgpt-export/ --format chatgpt

# Search your memory
clearmemory recall "why did we switch to GraphQL"

# Connect to Claude Code via MCP
claude mcp add clearmemory -- clearmemory serve

# Or run as a standalone server
clearmemory serve --both    # MCP (9700) + HTTP (8080)
```

Your AI now remembers everything. Ask it about decisions from three months ago and it just knows.

---

## How It Works

**Store everything.** When a session ends, the full transcript is stored verbatim alongside structured facts extracted at ingestion time. The original is never summarized or replaced. Our benchmarks measure **93.3% Recall@10** on scale tests (stable from 500 to 10,000 memories) and **76.8% Recall@10** on hard LongMemEval-style queries including multi-hop reasoning and abstraction. See [benchmarks.md](docs/benchmarks.md) for full methodology and results.

**Search with four strategies.** Every query runs four retrieval strategies in parallel: semantic similarity, keyword matching, temporal proximity, and entity graph traversal. Results are merged and reranked. No single strategy covers all query types — running all four catches what any one would miss.

**Inject only what matters.** The context compiler assembles a token-budget-aware payload before your prompt reaches the model. Identity context (~50 tokens) and project working set (~500 tokens) are always loaded. Relevant memories are added on demand. The budget is configurable — you control how many tokens memory uses.

**Curate intelligently (Tier 2+).** A lightweight local model parses retrieval results and extracts only the relevant portions before injection. A session about three topics? The curator pulls only the paragraph that answers your question.

---

## Structured Fact Extraction

At ingestion time, Clear Memory extracts subject-predicate-object facts with temporal validity from each memory and indexes them in SQLite. The verbatim transcript is always preserved alongside the structured facts for audibility and full-context retrieval.

```
Stored memory: "We decided to migrate from Auth0 to Clerk"
    │
    ├── Verbatim file: original text preserved, encrypted, never modified
    │
    └── Extracted fact:
        subject: "auth provider"  predicate: "is"  object: "Clerk"
        valid_from: 2026-03-15    valid_until: NULL (current)
```

When a new fact contradicts an existing one, the old fact is automatically invalidated — not deleted. This enables:

- **Knowledge update queries:** "What is our current auth provider?" returns Clerk, not Auth0.
- **Historical queries:** "What was our auth provider in January?" returns Auth0 with full context.
- **Conflict detection:** Contradictory facts are surfaced, not silently overwritten.

---

## How Clear Memory Works With Your Existing Tools

Clear Memory's value depends on what you're integrating with. Two scenarios:

### Scenario 1: You're building your own GenAI app

If you're invoking an LLM directly (via API) with no existing context or memory management, Clear Memory's context compiler is the primary value. It assembles only the relevant memory fragments within a configurable token budget before your prompt reaches the model. Instead of stuffing the full conversation history into every call, you send identity (~50 tokens) + working set (~500 tokens) + targeted recall results. The token savings are real and measurable.

### Scenario 2: You're using tools that already manage context

Tools like Claude Code (with CLAUDE.md), GitHub Copilot (with workspace context), and Cursor already have their own context management. They already decide what to include in prompts. In this scenario, **Clear Memory's primary value is not token saving — it's cross-session and cross-team institutional memory.**

What these tools can't do:
- Remember decisions from three months ago across hundreds of sessions
- Search across your entire team's AI conversation history
- Answer "why did we decide X?" when the session where you discussed it is long gone
- Carry context across different tools (a decision made in Claude Code, recalled in Copilot)

The context compiler still helps by deduplicating against what the CLI has already loaded (CLAUDE.md contents, workspace files) to avoid double-injecting the same information. But the core ROI is institutional knowledge retention and cross-session retrieval, not token reduction.

---

## Three Deployment Tiers

| | Tier 1: Offline | Tier 2: Local LLM | Tier 3: Cloud |
|---|---|---|---|
| **External calls** | Zero | Zero | Cloud APIs |
| **Data leaves machine** | Never | Never | Query context only |
| **Accuracy** | 76.8% measured ([benchmarks](docs/benchmarks.md)) | Higher (planned) | Highest (planned) |
| **RAM** | ~1.2GB | ~2.4-4.9GB | ~2.4-4.9GB |
| **Features** | Storage, retrieval, context compiler | + Curator, reflect, entity resolution | + Cloud-quality synthesis |
| **Use case** | Air-gapped, regulated | GPU-equipped teams | Cloud-connected teams |

All three tiers share the same binary, storage engine, and retrieval pipeline. Upgrade without migrating data.

---

## Organize with Tags & Streams

Tag memories with four dimensions that match how your org already thinks:

| Tag | Examples |
|-----|---------|
| **Team** | `platform`, `frontend`, `security` |
| **Repo** | `auth-service`, `api-gateway` |
| **Project** | `q1-migration`, `soc2-audit` |
| **Domain** | `security/auth`, `infrastructure/ci-cd` |

**Streams** are scoped views across tag intersections. Create a stream like "Platform Team + auth-service + Security domain" and search only within that intersection. The system also checks related streams for adjacent results.

Tags are optional. The system works with zero tags out of the box.

---

## MCP Tools

Connect Clear Memory to any MCP-compatible tool (Claude Code, Copilot, Cursor, Windsurf):

```bash
claude mcp add clearmemory -- clearmemory serve
```

**9 tools available:**

| Tool | What it does |
|------|-------------|
| `clearmemory_recall` | Search with stream/tag filters. Returns summaries. |
| `clearmemory_expand` | Get full verbatim content for a specific memory. |
| `clearmemory_reflect` | Synthesize across memories into a coherent narrative. (Tier 2+) |
| `clearmemory_retain` | Store a memory with optional tags. |
| `clearmemory_import` | Bulk import from files (7 formats supported). |
| `clearmemory_forget` | Invalidate a memory (temporal marking, not deletion). |
| `clearmemory_streams` | List, create, switch, or describe streams. |
| `clearmemory_tags` | Manage team/repo/project/domain tags. |
| `clearmemory_status` | Corpus overview, health, performance metrics. |

---

## HTTP API

Clear Memory exposes a full REST API alongside MCP. Every operation available via MCP is also available over HTTP:

```bash
clearmemory serve --http --port 8080    # HTTP only
clearmemory serve --both                # MCP (9700) + HTTP (8080)
```

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/v1/recall` | POST | Search with stream/tag filters |
| `/v1/expand/:id` | GET | Full verbatim content for a memory |
| `/v1/retain` | POST | Store a memory with optional tags |
| `/v1/forget` | POST | Invalidate a memory |
| `/v1/status` | GET | Corpus overview, health |
| `/v1/streams` | GET/POST | List or create streams |
| `/v1/tags` | GET/POST | Manage tags |
| `/health` | GET | Health check (K8s probe compatible) |

All endpoints require `Authorization: Bearer <token>` header.

---

## Integration Without MCP

Not every environment supports MCP. Clear Memory's HTTP API and CLI are equally valid integration paths.

### Example 1: Git post-commit hook

Auto-save session context on every commit:

```bash
#!/bin/bash
# .git/hooks/post-commit
REPO=$(basename "$(git rev-parse --show-toplevel)")
BRANCH=$(git rev-parse --abbrev-ref HEAD)
SESSION_LOG="$HOME/.claude/last_session.txt"

if [ -f "$SESSION_LOG" ]; then
  curl -s -X POST http://localhost:8080/v1/retain \
    -H "Authorization: Bearer $CLEARMEMORY_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{
      \"content\": $(cat "$SESSION_LOG" | jq -Rs .),
      \"tags\": {\"repo\": \"$REPO\", \"project\": \"$BRANCH\"}
    }"
fi
```

### Example 2: GitHub Action — import PR discussions on merge

```yaml
# .github/workflows/save-pr-memory.yml
name: Save PR Discussion to Clear Memory
on:
  pull_request:
    types: [closed]

jobs:
  save-memory:
    if: github.event.pull_request.merged == true
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Export PR discussion as markdown
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          PR_NUM=${{ github.event.pull_request.number }}
          REPO=${{ github.repository }}
          echo "# PR #$PR_NUM: ${{ github.event.pull_request.title }}" > /tmp/pr-discussion.md
          echo "" >> /tmp/pr-discussion.md
          echo "${{ github.event.pull_request.body }}" >> /tmp/pr-discussion.md
          echo "" >> /tmp/pr-discussion.md
          gh api "repos/$REPO/pulls/$PR_NUM/comments" \
            --jq '.[] | "**\(.user.login):** \(.body)\n"' >> /tmp/pr-discussion.md

      - name: Import to Clear Memory
        env:
          CLEARMEMORY_TOKEN: ${{ secrets.CLEARMEMORY_TOKEN }}
        run: |
          clearmemory import /tmp/pr-discussion.md \
            --format markdown \
            --tag repo:${{ github.event.repository.name }} \
            --tag project:pr-${{ github.event.pull_request.number }}
```

### Example 3: Claude Code wrapper script

Capture session output and retain via HTTP API:

```bash
#!/bin/bash
# claude-with-memory.sh — wraps claude CLI with auto-save
OUTPUT=$(mktemp)
claude "$@" 2>&1 | tee "$OUTPUT"

# Retain the session transcript
curl -s -X POST http://localhost:8080/v1/retain \
  -H "Authorization: Bearer $CLEARMEMORY_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"content\": $(cat "$OUTPUT" | jq -Rs .),
    \"tags\": {\"repo\": \"$(basename $(pwd))\"}
  }" > /dev/null

rm "$OUTPUT"
```

---

## Import Formats

| Format | Flag | Source |
|--------|------|--------|
| Claude Code | `--format claude_code` | `~/.claude/` transcripts |
| Copilot CLI | `--format copilot` | Copilot session logs |
| ChatGPT | `--format chatgpt` | OpenAI export JSON |
| Slack | `--format slack` | Workspace export |
| Markdown | `--format markdown` | Any `.md` or `.txt` files |
| Clear Format | `--format clear` | `.clear` files (our enterprise JSON format) |
| Auto-detect | `--format auto` | Inspects file structure |

**The Clear Format** (`.clear`) is a JSON schema designed for enterprise integration. Non-technical users create data in CSV or Excel and convert:

```bash
clearmemory convert csv-to-clear data.csv --mapping auto
clearmemory convert excel-to-clear data.xlsx
```

---

## Feature Maturity

| Feature | Status | Notes |
|---------|--------|-------|
| Verbatim storage + structured fact extraction | Available | Full write path with encryption |
| 4-strategy parallel retrieval (semantic, keyword, temporal, entity graph) | Available | 93.3% Recall@10 measured at scale |
| Reciprocal Rank Fusion merge | Available | k=60, configurable |
| CLI (all commands) | Available | `clearmemory init/recall/retain/import/...` |
| MCP server (9 tools) | Available | JSON-RPC 2.0 over stdio |
| HTTP REST API | Available | axum, all 9 operations |
| 7 import format parsers | Available | Claude Code, Copilot, ChatGPT, Slack, Markdown, Clear Format, auto-detect |
| SQLCipher encryption at rest | Available | AES-256-CBC (SQLite), AES-256-GCM (files) |
| API token authentication + scopes | Available | read, read-write, admin, purge |
| Rate limiting | Available | Per-client, configurable per operation type |
| Secret scanning (regex-based) | Available | 9 pattern categories, warn/redact/block modes |
| Tamper-evident audit log | Available | Chained SHA-256 hashes, external checkpoints |
| Data classification (manual) | Available | 4 levels, classification pipeline tracing |
| Retention policies (time, size, performance) | Available | Archive, not delete |
| Backup / restore | Available | Encrypted `.cmb` snapshots |
| BGE-Reranker-Base cross-encoder | In Development | Implemented, not yet wired into default pipeline |
| PII pattern detection | Planned | Regex-based, auto-classify as `pii` |
| Entropy-based secret detection | Planned | Shannon entropy for high-entropy strings |
| Curator model (Qwen3-0.6B) | Planned | candle integration for Tier 2 |
| Reflect / synthesis (Qwen3-4B) | Planned | candle integration for Tier 2 |
| LLM-based entity resolution | Planned | Tier 2+ enhanced alias linking |
| LLM-based content classification | Planned | v2, topic-sensitivity classification |

## Enterprise Roadmap

The following enterprise features are designed and documented. Implementation status varies — see the maturity table above for details.

**Encrypted at rest.** SQLite via SQLCipher (AES-256-CBC). Verbatim files and vectors via AES-256-GCM. Backups encrypted. A stolen device yields encrypted blobs, not readable transcripts.

**Authenticated access.** API tokens with scoped permissions (read, read-write, admin, purge). Tokens expire after configurable TTL (default 90 days). Rate limiting on all endpoints.

**Secret scanning.** Detects API keys, tokens, passwords, and connection strings before storage. Modes: warn, redact, or block. Current detection is regex-based — see [security.md](docs/security.md) for limitations and hardening roadmap.

**Classification-aware.** Four-level data classification (public, internal, confidential, pii). PII and confidential data never leaves the machine, even in Tier 3.

**Tamper-evident audit log.** Every operation logged with chained hashes and external checkpoint anchors. Append-only — cannot be modified even by admins.

**Compliance ready.** GDPR right-to-delete via purge command. Legal hold freezes streams for litigation. Compliance reporting with CSV export for auditors.

**Incident response playbooks.** Documented procedures for: device theft, unauthorized access, model poisoning, credential exposure, and audit log breach.

See [security.md](docs/security.md) for full details and [ENTERPRISE.md](docs/ENTERPRISE.md) for the enterprise adoption guide.

---

## ClearPathAI Integration

Clear Memory is the memory engine behind [ClearPathAI](https://github.com/clearpathai/clearpathai) — an Electron desktop app that wraps GitHub Copilot CLI and Claude Code CLI into a clean GUI for non-technical users.

When integrated with ClearPathAI:
- Context is automatically injected before every prompt reaches the CLI
- Sessions are auto-saved on completion with tags from the active workspace
- The analytics dashboard shows tokens saved, corpus health, and retrieval trends
- Stream and tag management is handled through the GUI

Clear Memory works independently without ClearPathAI via CLI, MCP, or HTTP API.

---

## Configuration

```bash
# View current config
clearmemory config show

# Edit config
clearmemory config edit    # opens config.toml in $EDITOR
```

Configuration lives in `~/.clearmemory/config.toml`. Key settings:

```toml
[general]
tier = "offline"              # "offline", "local_llm", "cloud"

[retrieval]
top_k = 10                    # results per strategy before merge
token_budget = 4096           # max tokens for context injection

[retention]
time_threshold_days = 90      # archive unaccessed memories after this
size_threshold_gb = 2         # warn when corpus exceeds this
performance_threshold_ms = 200 # flag when p95 latency exceeds this

[encryption]
enabled = true                # at-rest encryption (default: on)
```

See `CLAUDE.md` for the complete configuration reference.

---

## CLI Reference

```bash
# Setup
clearmemory init                              # guided onboarding
clearmemory init --tier local_llm             # download all models

# Import
clearmemory import <path> --format auto       # auto-detect format
clearmemory import <path> --stream my-project # tag to a stream

# Search
clearmemory recall "query"                    # search everything
clearmemory recall "query" --stream <name>    # within a stream
clearmemory expand <memory_id>                # full verbatim content

# Synthesis (Tier 2+)
clearmemory reflect "summarize auth project"

# Memory management
clearmemory retain "content" --tag team:platform
clearmemory forget <memory_id>

# Organization
clearmemory streams list
clearmemory streams create "Platform Auth" --tag team:platform
clearmemory tags list

# Server
clearmemory serve                             # MCP server (9700)
clearmemory serve --both                      # MCP + HTTP (8080)

# Maintenance
clearmemory status                            # corpus overview
clearmemory backup ~/backups/                 # snapshot
clearmemory restore <backup.cmb>              # restore
clearmemory repair                            # integrity check

# Security
clearmemory auth status                       # token overview
clearmemory auth rotate                       # rotate primary token
clearmemory security scan                     # scan for secrets
clearmemory compliance report                 # compliance report
clearmemory audit verify                      # verify audit chain
```

---

## Try It Out

The [examples/](examples/) directory contains 5 runnable examples that walk you through Clear Memory's capabilities:

| Example | Interface | What You'll Learn |
|---------|-----------|-------------------|
| [01-getting-started](examples/01-getting-started/) | CLI | Core retain/recall/expand loop, tags, status |
| [02-importing-history](examples/02-importing-history/) | CLI | Import from Claude Code, CSV, Slack — unified search |
| [03-claude-code-sessions](examples/03-claude-code-sessions/) | CLI | Capture Claude sessions, scan for secrets, backup |
| [04-http-api](examples/04-http-api/) | HTTP/curl | Full REST API surface with request/response examples |
| [05-mcp-integration](examples/05-mcp-integration/) | MCP | JSON-RPC protocol — how AI tools talk to Clear Memory |

Each example includes sample data, a runnable script, and a detailed README. Run them in order for the full experience, or jump to whichever interface you care about.

```bash
cd examples/01-getting-started && ./run.sh
```

---

## System Requirements

| | Tier 1 | Tier 2 |
|---|---|---|
| **RAM** | 1.2 GB | 2.4-4.9 GB |
| **Disk (binary + models)** | ~700 MB | ~4.5 GB |
| **CPU** | Any modern 4+ core | Same (GPU optional, improves reflect) |
| **OS** | macOS, Linux, Windows | Same |
| **Network** | Required for first model download only | Same |

---

## Benchmarks

Retrieval quality is measured, not claimed. See **[docs/benchmarks.md](docs/benchmarks.md)** for full methodology, results, and reproduction commands.

| Benchmark | Corpus | Score | Notes |
|-----------|--------|-------|-------|
| **Official LongMemEval** (ICLR 2025) | 500 questions, same dataset as competitors | **52.8% R_any@10** (keyword-only) | Directly comparable to published results |
| Custom scale test | 500–10,000 memories, 30 queries | **93.3% R@10** | Stable across all corpus sizes |
| Custom LongMemEval-style | 128 memories, 80 queries | **76.8% R@10** | Self-authored corpus, not comparable to official |

Full pipeline results (with embeddings) will be significantly higher. See [benchmarks.md](docs/benchmarks.md) for complete methodology, per-question results, and reproduction commands.

## Documentation

| Document | Contents |
|----------|----------|
| **[docs/benchmarks.md](docs/benchmarks.md)** | **Retrieval quality benchmarks — methodology, measured results, reproduction commands** |
| [CLAUDE.md](CLAUDE.md) | Full project constitution — architecture, schema, conventions, everything |
| [docs/architecture.md](docs/architecture.md) | System architecture diagrams and data flow |
| [docs/security.md](docs/security.md) | Complete security model, threat mitigations, compliance |
| [docs/ENTERPRISE.md](docs/ENTERPRISE.md) | Enterprise value proposition, adoption guide |

---

## Built With

- **Rust** — single binary, no runtime dependencies
- **SQLite + SQLCipher** — encrypted structured storage
- **LanceDB** — embedded vector search
- **BGE-Small-EN / BGE-M3** — dense + sparse embeddings (via fastembed-rs)
- **BGE-Reranker-Base** — cross-encoder reranking
- **Qwen3-0.6B / Qwen3-4B** — local LLM inference (via candle)
- **OpenTelemetry** — observability and metrics

---

## Contributing

Contributions welcome. Please open an issue to discuss significant changes before submitting a PR.

---

## License

Apache 2.0 — see [LICENSE](LICENSE) for details.

---

*Clear Memory is part of the ClearPathAI ecosystem. Built by engineers who got tired of repeating themselves to AI.*
