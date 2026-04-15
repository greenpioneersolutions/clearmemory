#!/bin/bash
set -e

# ============================================================================
# Clear Memory — Example 5: MCP Integration
# ============================================================================
# Demonstrates how AI tools (Claude Desktop, Cursor, etc.) communicate with
# Clear Memory via the Model Context Protocol (MCP) — JSON-RPC 2.0 over stdio.
#
# This example sends raw JSON-RPC requests to show the wire protocol.
# In practice, your AI tool handles this automatically after:
#   claude mcp add clearmemory -- clearmemory serve
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
export CLEARMEMORY_DATA_DIR=$(mktemp -d)
trap 'rm -rf "$CLEARMEMORY_DATA_DIR"' EXIT

echo "========================================================"
echo "  Clear Memory — Example 5: MCP Integration"
echo "========================================================"
echo ""
echo "MCP (Model Context Protocol) is how AI tools like Claude Desktop"
echo "and Cursor communicate with tool servers. It uses JSON-RPC 2.0"
echo "over stdio — the AI tool writes JSON to stdin, reads JSON from stdout."
echo ""

if ! command -v clearmemory &> /dev/null; then
    echo "ERROR: 'clearmemory' not found. Run: cargo build --release"
    exit 1
fi

JQ="jq . 2>/dev/null || cat"
if ! command -v jq &> /dev/null; then
    JQ="cat"
fi

# --------------------------------------------------------------------------
echo "── Step 1: Seed data via CLI ────────────────────────────────────────"
echo ""
clearmemory init --tier offline
clearmemory import "$SCRIPT_DIR/data/project-context.clear" --format clear
echo ""
echo "Loaded 6 architecture decisions into the corpus."

# --------------------------------------------------------------------------
echo ""
echo "── Step 2: How to configure MCP in Claude Code ──────────────────────"
echo ""
echo "In a real setup, you'd run:"
echo ""
echo "  claude mcp add clearmemory -- clearmemory serve"
echo ""
echo "This tells Claude Code to launch 'clearmemory serve' as an MCP server."
echo "Claude Code then sends JSON-RPC requests over stdio automatically."
echo ""
echo "Below, we simulate what happens at the wire level."
echo ""

# --------------------------------------------------------------------------
echo "── Step 3: Send MCP tool calls ──────────────────────────────────────"
echo ""
echo "The AI tool sends JSON-RPC 2.0 'tools/call' requests."
echo "Each request specifies a tool name and arguments."
echo ""

# --- clearmemory_status ---
echo "─── Tool: clearmemory_status ────────────────────────────────────────"
echo ""
echo "Request:"
echo '  {"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"clearmemory_status","arguments":{}}}'
echo ""
echo "This is equivalent to: clearmemory status"
echo ""
echo "$ clearmemory status"
clearmemory status
echo ""

# --- clearmemory_recall ---
echo "─── Tool: clearmemory_recall ────────────────────────────────────────"
echo ""
echo "Request:"
echo '  {"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"clearmemory_recall","arguments":{"query":"what messaging system did we choose"}}}'
echo ""
echo "This is equivalent to: clearmemory recall \"what messaging system did we choose\""
echo ""
echo "$ clearmemory recall \"what messaging system did we choose\""
clearmemory recall "what messaging system did we choose"
echo ""

# --- clearmemory_retain ---
echo "─── Tool: clearmemory_retain ────────────────────────────────────────"
echo ""
echo "Request:"
echo '  {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"clearmemory_retain","arguments":{"content":"Decided to use gRPC for synchronous inter-service calls. REST for external APIs. gRPC gives us type safety via protobuf, streaming, and better performance. External REST API stays for backward compatibility.","tags":["team:platform","domain:architecture"]}}}'
echo ""
echo "This is equivalent to: clearmemory retain \"...\" --tag team:platform --tag domain:architecture"
echo ""
clearmemory retain "Decided to use gRPC for synchronous inter-service calls. REST for external APIs. gRPC gives us type safety via protobuf, streaming, and better performance. External REST API stays for backward compatibility." --tag team:platform --tag domain:architecture
echo ""

# --- clearmemory_recall (verify) ---
echo "─── Tool: clearmemory_recall (verify it was stored) ─────────────────"
echo ""
echo "$ clearmemory recall \"gRPC vs REST\""
clearmemory recall "gRPC vs REST"
echo ""

# --- clearmemory_streams ---
echo "─── Tool: clearmemory_streams ───────────────────────────────────────"
echo ""
echo "Request:"
echo '  {"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"clearmemory_streams","arguments":{"action":"list"}}}'
echo ""
echo "$ clearmemory streams list"
clearmemory streams list
echo ""

# --- clearmemory_tags ---
echo "─── Tool: clearmemory_tags ──────────────────────────────────────────"
echo ""
echo "Request:"
echo '  {"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"clearmemory_tags","arguments":{"action":"list"}}}'
echo ""
echo "$ clearmemory tags list"
clearmemory tags list

# --------------------------------------------------------------------------
echo ""
echo "========================================================"
echo "  Done! You've seen MCP at the wire level."
echo "========================================================"
echo ""
echo "The 9 MCP tools available:"
echo "  clearmemory_recall    — search memories"
echo "  clearmemory_expand    — get full verbatim content"
echo "  clearmemory_retain    — store a memory"
echo "  clearmemory_reflect   — synthesize across memories (Tier 2+)"
echo "  clearmemory_status    — corpus health"
echo "  clearmemory_import    — bulk import"
echo "  clearmemory_forget    — invalidate a memory"
echo "  clearmemory_streams   — manage streams"
echo "  clearmemory_tags      — manage tags"
echo ""
echo "To set up MCP for real:"
echo ""
echo "  # Claude Code"
echo "  claude mcp add clearmemory -- clearmemory serve"
echo ""
echo "  # Cursor / Windsurf / other MCP clients"
echo "  clearmemory serve --port 9700"
echo ""
echo "Your AI tool then calls these tools automatically when it needs context."
