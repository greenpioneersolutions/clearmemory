# Example 5: MCP Integration

**Interface:** MCP (Model Context Protocol)
**Time to run:** ~20 seconds

## What You'll Learn

- What **MCP** is and how AI tools use it to talk to Clear Memory
- The **JSON-RPC 2.0** wire protocol that runs under the hood
- How to configure Clear Memory as an MCP server for Claude Code and Cursor
- All **9 MCP tools** and what they do

## Run It

```bash
cd examples/05-mcp-integration
./run.sh
```

## What Happens

1. Seeds the database with 6 architecture decisions
2. Shows the JSON-RPC request format for each MCP tool
3. Demonstrates `clearmemory_status` — corpus health check
4. Demonstrates `clearmemory_recall` — search for "what messaging system"
5. Demonstrates `clearmemory_retain` — store a new decision about gRPC
6. Demonstrates `clearmemory_streams` and `clearmemory_tags`

## Setting Up MCP for Real

```bash
# Claude Code — one command
claude mcp add clearmemory -- clearmemory serve

# Now Claude Code automatically has access to all 9 tools.
# It can recall your past decisions, store new ones, and search
# across your entire conversation history.
```

## The 9 MCP Tools

| Tool | Purpose |
|------|---------|
| `clearmemory_recall` | Search memories with semantic + keyword + temporal + entity graph |
| `clearmemory_expand` | Get full verbatim content for a specific memory |
| `clearmemory_retain` | Store a new memory with optional tags |
| `clearmemory_reflect` | Synthesize across memories (Tier 2+) |
| `clearmemory_status` | Corpus overview and health metrics |
| `clearmemory_import` | Bulk import from files |
| `clearmemory_forget` | Invalidate a memory (temporal marking) |
| `clearmemory_streams` | List, create, switch streams |
| `clearmemory_tags` | Manage team/repo/project/domain tags |
