# Clear Memory

**Store everything. Send only what matters. Pay for less.**

Clear Memory is a high-performance AI memory engine built in Rust. It stores every AI conversation verbatim, retrieves relevant context in milliseconds, and injects optimized context into LLM prompts — reducing token costs while preserving institutional knowledge.

Every architecture decision, debugging session, and project conversation your team has with AI disappears when the session ends. Clear Memory fixes that. It keeps every word, makes it searchable, and gives your AI the right context at the right time — without bloating the prompt.

---

## Why Clear Memory

**Your AI forgets everything.** Six months of daily AI use generates millions of tokens of decisions, reasoning, and context. All gone when the session ends.

**Existing solutions are lossy.** Other memory systems use an LLM to decide what to remember. They extract "user prefers Postgres" and throw away the conversation where you explained *why*. Clear Memory stores the actual words and lets retrieval find what matters.

**Token costs are exploding.** Under token-based pricing, every bloated context window burns money. Clear Memory's context compiler sends only the relevant fragments — targeting 60-80% token reduction per interaction.

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

**Store everything.** When a session ends, the full transcript is stored verbatim — no summarization, no extraction. Raw text with good embeddings achieves 96.6% recall on the LongMemEval benchmark, outperforming systems that use LLMs to decide what to keep.

**Search with four strategies.** Every query runs four retrieval strategies in parallel: semantic similarity, keyword matching, temporal proximity, and entity graph traversal. Results are merged and reranked. No single strategy covers all query types — running all four catches what any one would miss.

**Inject only what matters.** The context compiler assembles a token-budget-aware payload before your prompt reaches the model. Identity context (~50 tokens) and project working set (~500 tokens) are always loaded. Relevant memories are added on demand. The budget is configurable — you control how many tokens memory uses.

**Curate intelligently (Tier 2+).** A lightweight local model parses retrieval results and extracts only the relevant portions before injection. A session about three topics? The curator pulls only the paragraph that answers your question.

---

## Three Deployment Tiers

| | Tier 1: Offline | Tier 2: Local LLM | Tier 3: Cloud |
|---|---|---|---|
| **External calls** | Zero | Zero | Cloud APIs |
| **Data leaves machine** | Never | Never | Query context only |
| **Accuracy** | ~96% | ~99% | 99%+ |
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

## Enterprise Security

Clear Memory is built for enterprise environments where AI conversation data is sensitive by default.

**Encrypted at rest.** SQLite via SQLCipher (AES-256-CBC). Verbatim files and vectors via AES-256-GCM. Backups encrypted. A stolen device yields encrypted blobs, not readable transcripts.

**Authenticated access.** API tokens with scoped permissions (read, read-write, admin, purge). Tokens expire after configurable TTL (default 90 days). Rate limiting on all endpoints.

**Secret scanning.** Detects API keys, tokens, passwords, and connection strings before storage. Modes: warn, redact, or block.

**Classification-aware.** Four-level data classification (public, internal, confidential, pii). PII and confidential data never leaves the machine, even in Tier 3.

**Tamper-evident audit log.** Every operation logged with chained hashes and external checkpoint anchors. Append-only — cannot be modified even by admins.

**Compliance ready.** GDPR right-to-delete via purge command. Legal hold freezes streams for litigation. Compliance reporting with CSV export for auditors.

**Incident response playbooks.** Documented procedures for: device theft, unauthorized access, model poisoning, credential exposure, and audit log breach.

See `security.md` for full details.

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

## System Requirements

| | Tier 1 | Tier 2 |
|---|---|---|
| **RAM** | 1.2 GB | 2.4-4.9 GB |
| **Disk (binary + models)** | ~700 MB | ~4.5 GB |
| **CPU** | Any modern 4+ core | Same (GPU optional, improves reflect) |
| **OS** | macOS, Linux, Windows | Same |
| **Network** | Required for first model download only | Same |

---

## Documentation

| Document | Contents |
|----------|----------|
| `CLAUDE.md` | Full project constitution — architecture, schema, conventions, everything |
| `architecture.md` | System architecture diagrams and data flow |
| `security.md` | Complete security model, threat mitigations, compliance |
| `ENTERPRISE.md` | Enterprise value proposition, adoption guide, ROI framework |
| `docs/runbook.md` | Operations procedures: backup, restore, migration, troubleshooting |
| `docs/integration_guide.md` | MCP and HTTP integration for consuming applications |
| `docs/clear_format_spec.md` | Clear Format (.clear) file specification |

---

## Built With

- **Rust** — single binary, no runtime dependencies
- **SQLite + SQLCipher** — encrypted structured storage
- **LanceDB** — embedded vector search
- **BGE-M3** — dense + sparse embeddings (via fastembed-rs)
- **BGE-Reranker-Base** — cross-encoder reranking
- **Qwen3-0.6B / Qwen3-4B** — local LLM inference (via candle)
- **OpenTelemetry** — observability and metrics

---

## Contributing

See `CONTRIBUTING.md` for guidelines. All contributions require signing off on the Developer Certificate of Origin (DCO).

---

## License

[License TBD — see CLAUDE.md for licensing strategy considerations]

---

*Clear Memory is part of the ClearPathAI ecosystem. Built by engineers who got tired of repeating themselves to AI.*
