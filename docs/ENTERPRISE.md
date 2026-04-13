# Clear Memory — Enterprise Guide

## The Problem We Solve

Your engineering teams have conversations with AI every day. Architecture decisions, debugging sessions, code reviews, project planning — all happening in conversations with Copilot, Claude Code, ChatGPT, and Cursor. When those sessions end, the reasoning disappears.

Six months of daily AI use generates roughly 19.5 million tokens of institutional knowledge. Every "why did we decide X?" gets answered from scratch. Every onboarding engineer starts from zero. Every context switch costs 30 minutes of re-explanation.

Meanwhile, AI token costs are growing 40% quarter over quarter because every prompt stuffs the full context window with information the model has already seen — or doesn't need.

Clear Memory solves both problems simultaneously. It stores every conversation verbatim, makes it searchable in milliseconds, and injects only the relevant context into future prompts. Your teams stop repeating themselves. Your AI bills go down.

---

## Why Enterprise Teams Choose Clear Memory

### Your data never leaves your machines

Clear Memory runs entirely on developer laptops and internal servers. In its default configuration (Tier 1), it makes zero external network calls after the initial one-time model download. Every conversation, every decision, every piece of context stays on hardware you control.

This isn't a policy promise — it's an architectural guarantee. There is no cloud backend to configure, no SaaS account to create, no data pipeline to audit. The binary runs, the data stays local, and the audit log proves it.

For organizations that need enhanced AI features (Tier 2), a local language model runs on the same machine. Still zero external calls. For teams that want maximum quality (Tier 3), cloud API connections are available — but the system enforces classification-based filtering: content marked as PII or confidential never reaches a cloud API, regardless of user intent. The code prevents it, not policy.

### Token cost reduction you can measure

Clear Memory's context compiler assembles the minimum viable context for each prompt within a configurable token budget. Instead of loading the entire conversation history or project context into every prompt, it loads identity context (~50 tokens), a project working set (~500 tokens), and targeted memory fragments — only when relevant.

Early modeling suggests 60-80% reduction in input tokens per interaction. Under token-based pricing models, this translates directly to measurable savings per team, per project, per month. ClearPathAI's analytics dashboard tracks tokens saved alongside retrieval metrics, giving engineering leadership a clear view of ROI.

### Institutional knowledge that survives attrition

When an engineer leaves, their AI conversations leave with them. The "why" behind months of architecture decisions, the debugging techniques that worked, the vendor evaluations that informed procurement — all gone.

Clear Memory stores these conversations as searchable institutional knowledge tagged to teams, projects, and knowledge domains. A new engineer joining a project can query "what were the auth migration tradeoffs?" and get the actual discussion — not a summary someone remembered to write down.

The tag taxonomy (Teams, Repos, Projects, Domains) maps to how your organization already thinks. Streams — scoped views across tag intersections — let a VP of Engineering ask "show me all security decisions across all projects this quarter" and get a real answer.

### Security your CISO will approve

Clear Memory was designed with the understanding that AI conversation data is sensitive by default. Developers paste API keys, discuss vulnerabilities, reference customer data, and share internal architecture details in AI sessions. The security model addresses this reality head-on.

**Encryption at rest:** All stored data is encrypted — SQLite via SQLCipher (AES-256-CBC), transcript files and vectors via AES-256-GCM. Backup files are encrypted. A lost or stolen device yields encrypted data that requires the master passphrase to decrypt.

**Secret scanning:** Before any conversation is stored, it's scanned against detection patterns for AWS keys, GitHub tokens, database connection strings, private keys, and other credentials. The system can warn (flag and classify as confidential), redact (replace with markers), or block (reject storage entirely). This prevents Clear Memory from becoming a long-term credential store.

**Classification-enforced cloud filtering:** Every memory carries a data classification (public, internal, confidential, or pii). Content classified as confidential or pii is never sent to cloud APIs — even in Tier 3. This enforcement happens in code at the content pipeline level, not through user compliance with a policy.

**Tamper-evident audit logging:** Every operation (who searched what, who accessed which memory, who modified what) is logged with chained cryptographic hashes and external checkpoint anchors. The log is append-only — it cannot be modified even by administrators. Compliance teams can export the log for external review.

**Incident response playbooks:** Documented procedures with exact CLI commands for five incident types: device theft, unauthorized access, model poisoning, credential exposure, and audit log breach. Your security team has a runbook before the product is deployed.

### Architecture that survives your architecture review

Clear Memory is built in Rust and compiles to a single native binary with no runtime dependencies. No Python interpreter to manage, no Docker containers to provision, no servers to maintain. IT pushes the binary through standard software distribution (SCCM, Jamf, Intune) and it works.

The storage layer uses SQLite (the most tested database engine in the world) for structured data and LanceDB (a columnar vector database) for semantic search. Both are embedded — no database server to operate. The entire data directory is a single portable folder that can be backed up, restored, or migrated by copying files.

Schema versioning and migration tooling ensure upgrades never risk data loss. Embedding model changes trigger a re-indexing process that runs in the background without interrupting active queries. The system defines explicit degradation modes for every component — if the local LLM fails to load, retrieval falls back to Tier 1 behavior rather than failing entirely.

Multi-user concurrency is handled via SQLite's WAL mode (concurrent readers, serialized writers) with a write queue that guarantees ordering across the SQLite database, vector index, and transcript files. This model is proven to scale to teams of 50+ concurrent users.

### Compliance readiness out of the box

**GDPR / CCPA right-to-delete:** The `purge` command permanently removes all traces of a user's data — database records, vector embeddings, encrypted transcript files, and archived copies. A purge record is written to the audit log confirming deletion occurred. For shared deployments, purge requires two-person authorization (request + approve) to prevent accidental or malicious destruction.

**Legal hold:** Streams can be frozen with a single command, preventing modification or deletion of their contents. New memories can still be added (preservation doesn't stop ongoing work), but nothing can be removed while the hold is active. This capability is critical for litigation readiness and is typically a feature of much more expensive commercial platforms.

**Compliance reporting:** A built-in report generator produces per-classification memory counts, age distribution, per-stream breakdowns, PII exposure status, active legal holds, recent purge operations, and retention policy status — exportable as CSV for auditors or JSON for tooling.

**Data classification:** Four-level classification (public, internal, confidential, pii) applied at the memory level, with automatic escalation when secrets are detected. Classification flows through the entire content pipeline — curator outputs and reflect inputs inherit the classification of their source memories.

---

## Deployment Models

### Model 1: Individual Developer

A single developer installs Clear Memory on their laptop for personal AI memory. All data is private. No shared infrastructure needed.

**Setup time:** Under 5 minutes (install binary, run init, import existing conversations).

**Infrastructure required:** None. The binary runs locally.

**Typical use:** "I want my AI to remember my decisions across sessions."

### Model 2: Team Deployment

A team of 5-20 engineers each runs Clear Memory locally, with shared streams for project-level institutional knowledge. Each developer owns their personal memories. Shared streams are visible to the team.

**Setup time:** Under 30 minutes per developer (install binary, configure shared stream, import).

**Infrastructure required:** None for Tier 1/2. A shared network path for backup synchronization (optional).

**Typical use:** "I want my team's AI to know our project's history."

### Model 3: Shared Server Deployment

A Clear Memory instance runs as a service (Docker container or systemd service) for a department of 50-200 engineers. All queries route through the shared server. Stream visibility and access control manage multi-team isolation.

**Setup time:** 1-2 hours for initial server setup. Under 10 minutes per developer to configure MCP client.

**Infrastructure required:** A server with 8GB+ RAM (Tier 2), 50GB+ storage. Can run on a VM, container, or bare metal.

**Typical use:** "I want enterprise-wide AI memory with access control, audit logging, and compliance."

### Model 4: ClearPathAI Integration

Clear Memory runs as a sidecar process behind ClearPathAI's Electron desktop application. ClearPathAI provides the GUI for non-technical users (managers, team leads) while Clear Memory handles storage, retrieval, and context injection automatically.

**Setup time:** Zero for end users — ClearPathAI launches Clear Memory on app start.

**Infrastructure required:** None beyond ClearPathAI installation.

**Typical use:** "I want my non-technical team members to benefit from AI memory without learning command-line tools."

---

## How We Take Security Seriously

Security is not a feature we added. It's the foundation the system is built on.

| Layer | What We Do | Why It Matters |
|-------|-----------|----------------|
| **Encryption at rest** | AES-256 encryption on all stored data — database, transcripts, vectors, backups | A stolen device yields nothing without the passphrase |
| **Authentication** | Scoped API tokens with expiration on all interfaces | No anonymous access, least-privilege enforcement |
| **Secret scanning** | Automated credential detection before storage (AWS keys, tokens, passwords, connection strings) | Prevents memory system from becoming a credential store |
| **Classification** | Four-level data classification with pipeline enforcement | PII/confidential data never reaches cloud APIs, enforced in code |
| **Audit logging** | Tamper-evident chain with external checkpoints | Cryptographically verifiable record of every access |
| **Insider detection** | Access pattern monitoring with anomaly flagging | Catches unauthorized browsing of sensitive streams |
| **Two-person purge** | Destructive operations require separate request and approval | Prevents accidental or malicious data destruction |
| **Model supply chain** | Pinned revisions, self-published checksums, benchmark gate | Detects and prevents poisoned model attacks |
| **Incident response** | Five documented playbooks with exact CLI procedures | Your security team has a runbook before deployment |
| **Transport security** | Local-only by default, TLS required for network access, mutual TLS supported | Zero attack surface in default configuration |

For the complete threat model, encryption details, and incident response playbooks, see `security.md`.

---

## How We Take Architecture Seriously

| Decision | What We Chose | Why |
|----------|--------------|-----|
| **Language** | Rust | Single binary, no runtime dependencies, native concurrency, enterprise-grade distribution story |
| **Storage** | SQLite + LanceDB (embedded) | No database server to operate. Most tested DB engine + purpose-built vector search. Single portable directory. |
| **Embedding** | BGE-M3 via ONNX | Top-tier retrieval accuracy, dense + sparse from one model, 100+ languages, runs on CPU |
| **Retrieval** | 4-strategy parallel search | Semantic, keyword, temporal, and graph — each catches what the others miss. Cross-encoder reranker as final pass. |
| **Encryption** | SQLCipher + AES-256-GCM | Industry-standard, hardware-accelerated, proven in enterprise |
| **Concurrency** | WAL mode + serialized write queue | Proven model: unlimited concurrent readers, ordered writes, zero corruption risk |
| **Tiered deployment** | Three tiers, same binary | Security review once. Upgrade without migration. Each tier has its own enterprise champion. |
| **Key derivation** | Argon2id | Memory-hard, resistant to GPU/ASIC attacks. OWASP recommended. |
| **Observability** | OpenTelemetry native | Plugs into any enterprise monitoring stack without custom integration |
| **Migration** | Schema versioning + auto-apply | Upgrades never risk data loss. Binary refuses downgrade. Embedding model changes handled via background reindex. |

For the complete architecture with data flow diagrams, see `architecture.md`.

---

## Measuring Success

Clear Memory exposes metrics that prove its value. Through ClearPathAI's analytics dashboard or directly via the OpenTelemetry pipeline, engineering leadership can track:

| Metric | What It Tells You |
|--------|-------------------|
| **Tokens saved per interaction** | Direct cost reduction from context compilation |
| **Tokens saved per team per month** | Budget impact at the organizational level |
| **Retrieval latency (p50, p95, p99)** | System health and user experience |
| **Corpus size and growth rate** | Storage planning and retention policy effectiveness |
| **Memory access patterns** | Which knowledge is most valuable, which streams are most active |
| **Recall hit rate** | What percentage of sessions benefited from injected memory |
| **Retention policy triggers** | Whether the system is self-managing effectively |

---

## Getting Started

### For Individual Evaluation

```bash
cargo install clearmemory
clearmemory init
clearmemory import ~/.claude/ --format claude_code
clearmemory recall "what was that auth decision?"
```

### For Team Pilot

1. Designate a project to pilot on
2. Each team member installs Clear Memory and imports their sessions for that project
3. Create a shared stream with the project's tags
4. Use for 2 weeks, then review retrieval quality and token savings

### For Enterprise Deployment

1. Security review: share `security.md` with your CISO. Tier 1 evaluation typically takes 1-2 weeks.
2. Architecture review: share `architecture.md` with your architecture review board.
3. Pilot deployment: start with one team (5-10 engineers) on Tier 1 for 30 days.
4. Measure: track tokens saved, retrieval hit rate, and developer feedback.
5. Expand: based on pilot results, scale to additional teams. Consider Tier 2 for enhanced quality.

### For ClearPathAI Integration

Clear Memory ships as the memory layer for ClearPathAI (Slice 31). See `docs/clearpathAI_integration.md` for the full integration specification.

---

## Comparison

| | Clear Memory | Mem0 | Zep | Letta | Hindsight |
|---|---|---|---|---|---|
| **Local-only mode** | Yes (Tier 1) | No | No | Optional | Yes |
| **Encryption at rest** | Yes (v1) | Enterprise only | SOC 2 | No | No |
| **Benchmark (LongMemEval)** | Target 96%+ | 49% | 63.8% | Unpublished | 91.4% |
| **Language** | Rust | Python | Python | Python | Python/Go |
| **Runtime dependencies** | None | Python + vector DB | Python + Neo4j | Python + server | Docker + Postgres |
| **Secret scanning** | Built-in | No | No | No | No |
| **Legal hold** | Built-in | No | No | No | No |
| **Audit log (tamper-evident)** | Yes | Enterprise only | Enterprise only | No | No |
| **Token cost optimization** | Context compiler | Memory compression | N/A | N/A | N/A |
| **Cost** | Free (open source) | $19-249/mo | $25+/mo | $20-200/mo | Free (open source) |

---

## Documentation

| Document | Audience |
|----------|----------|
| `README.md` | Developers evaluating Clear Memory |
| `ENTERPRISE.md` | This document — engineering leadership, security, architecture review |
| `security.md` | CISO and security team — full threat model and controls |
| `architecture.md` | Architecture review board — system design and data flow |
| `CLAUDE.md` | Development team — project constitution with every technical detail |
| `docs/runbook.md` | Operations team — backup, restore, migration, troubleshooting |
| `docs/integration_guide.md` | Integration engineers — MCP and HTTP API guide |

---

*Clear Memory is part of the ClearPathAI ecosystem. Built for enterprises that take AI memory, security, and cost seriously.*
