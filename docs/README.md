# docs/ — Clear Memory Documentation

Documentation for the Clear Memory engine, covering architecture, security, enterprise deployment, and benchmarks.

---

## Available Documents

| File | Audience | Contents |
|------|----------|----------|
| `architecture.md` | Developers, integrators | System architecture — storage layer, retrieval pipeline, context compiler, data flow diagrams. Start here for understanding how the engine works. |
| `security.md` | Security teams, auditors | Threat model (14 threats with mitigations), encryption (SQLCipher + AES-256-GCM), API auth, rate limiting, secret scanning, transport security, compliance controls. |
| `ENTERPRISE.md` | Enterprise decision-makers | Value proposition, deployment models, ROI framework, adoption guide, capacity planning. Non-technical. |
| `benchmarks.md` | Developers, evaluators | Retrieval quality benchmarks (LongMemEval methodology), per-strategy precision, scale testing up to 10K memories, latency measurements. |

## Planned Documents

The following are referenced in `CLAUDE.md` and will be created as the project matures:

- `runbook.md` — Operations procedures: setup, backup/restore, migration, troubleshooting
- `integration_guide.md` — MCP and HTTP API integration for consuming applications
- `clear_format_spec.md` — Clear Format (.clear) file specification for enterprise data import
- `mcp_tools_schema.json` — JSON Schema definitions for all 9 MCP tools
- `adr/` — Architecture Decision Records (verbatim-over-extraction, Rust-over-Python, BGE-M3 selection, LanceDB-over-SQLite-VSS, tiered deployment, streams model, encryption, secret scanning)

## Reading Order

For a new contributor: `architecture.md` first, then `security.md`, then `CLAUDE.md` (root) for the full project constitution.
