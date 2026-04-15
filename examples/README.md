# Clear Memory — Examples

Five runnable examples that walk you through Clear Memory's capabilities across every integration path: CLI, HTTP API, and MCP.

## Prerequisites

```bash
# Build the binary
cargo build --release

# Add to PATH
export PATH="$PWD/target/release:$PATH"

# Verify
clearmemory --help
```

Optional: `jq` for pretty-printing JSON in the HTTP example (`brew install jq` or `apt install jq`).

## The Examples

| # | Example | Interface | What You'll Learn | Time |
|---|---------|-----------|-------------------|------|
| 1 | [Getting Started](01-getting-started/) | CLI | Core retain/recall/expand loop, tags, status | ~30s |
| 2 | [Importing History](02-importing-history/) | CLI | Import from Claude Code, CSV, Slack — unified search | ~30s |
| 3 | [Claude Code Sessions](03-claude-code-sessions/) | CLI | Capture sessions, secret scanning, backup | ~30s |
| 4 | [HTTP API](04-http-api/) | HTTP/curl | Full REST API with request/response examples | ~30s |
| 5 | [MCP Integration](05-mcp-integration/) | MCP | JSON-RPC protocol — how AI tools talk to Clear Memory | ~20s |

## Run Them

```bash
# Run in order for the full experience
cd examples/01-getting-started && ./run.sh && cd ..
cd 02-importing-history && ./run.sh && cd ..
cd 03-claude-code-sessions && ./run.sh && cd ..
cd 04-http-api && ./run.sh && cd ..
cd 05-mcp-integration && ./run.sh && cd ..

# Or jump to whichever interface you care about
cd examples/04-http-api && ./run.sh
```

Each example uses a temporary data directory and cleans up after itself — nothing is written to your `~/.clearmemory/`.

## The Story

All five examples follow the fictional **Meridian** platform team — Sarah Chen, Kai Rivera, and Priya Sharma — through a sprint. The data is realistic but synthetic:

- Auth migration from Auth0 to Clerk
- PostgreSQL migration and connection pool debugging
- Frontend performance optimization
- CI/CD pipeline and monitoring stack decisions
- Sprint retrospectives and deployment planning

This gives you a feel for how Clear Memory works with real engineering conversations.

## What's Next

After running the examples:

- **Import your own data:** `clearmemory import ~/.claude/ --format claude_code`
- **Connect to Claude Code:** `claude mcp add clearmemory -- clearmemory serve`
- **Read the benchmarks:** [docs/benchmarks.md](../docs/benchmarks.md)
- **Full documentation:** [CLAUDE.md](../CLAUDE.md)
