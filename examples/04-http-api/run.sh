#!/bin/bash
set -e

# ============================================================================
# Clear Memory — Example 4: HTTP API
# ============================================================================
# Start the HTTP server and interact with every endpoint using curl.
# This demonstrates how to integrate Clear Memory into web apps and scripts.
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
export CLEARMEMORY_DATA_DIR=$(mktemp -d)
SERVER_PID=""
PORT=18080

cleanup() {
    if [ -n "$SERVER_PID" ]; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    rm -rf "$CLEARMEMORY_DATA_DIR"
}
trap cleanup EXIT

echo "================================================"
echo "  Clear Memory — Example 4: HTTP API"
echo "================================================"
echo ""

if ! command -v clearmemory &> /dev/null; then
    echo "ERROR: 'clearmemory' not found. Run: cargo build --release"
    exit 1
fi

if ! command -v jq &> /dev/null; then
    echo "WARNING: 'jq' not found. JSON output won't be pretty-printed."
    echo "Install: brew install jq (macOS) or apt install jq (Linux)"
    JQ="cat"
else
    JQ="jq ."
fi

# --------------------------------------------------------------------------
echo "── Step 1: Initialize and seed data ─────────────────────────────────"
echo ""
clearmemory init --tier offline
clearmemory import "$SCRIPT_DIR/data/sprint-retro.clear" --format clear
echo ""
echo "Imported 8 sprint retrospective memories."

# --------------------------------------------------------------------------
echo ""
echo "── Step 2: Start HTTP server ────────────────────────────────────────"
echo ""
echo "$ clearmemory serve --http --port $PORT &"
clearmemory serve --http --port $PORT &
SERVER_PID=$!
echo "Server PID: $SERVER_PID"
echo ""

# Wait for server to be ready
echo "Waiting for server..."
for i in $(seq 1 30); do
    if curl -sf "http://localhost:$PORT/health" > /dev/null 2>&1; then
        echo "Server is ready!"
        break
    fi
    sleep 0.5
done
echo ""

# --------------------------------------------------------------------------
echo "── Step 3: Health check ─────────────────────────────────────────────"
echo ""
echo "$ curl http://localhost:$PORT/health"
curl -s "http://localhost:$PORT/health" | $JQ
echo ""

# --------------------------------------------------------------------------
echo "── Step 4: Store a memory via HTTP ──────────────────────────────────"
echo ""
echo '$ curl -X POST http://localhost:'$PORT'/v1/retain -H "Content-Type: application/json" -d ...'
RETAIN_RESPONSE=$(curl -s -X POST "http://localhost:$PORT/v1/retain" \
    -H "Content-Type: application/json" \
    -d '{
        "content": "Decided to adopt OpenTelemetry for distributed tracing. It replaces our custom tracing middleware and integrates with Grafana Tempo for trace storage. Migration estimated at 2 sprints.",
        "tags": ["team:platform", "domain:infrastructure/monitoring"]
    }')
echo "$RETAIN_RESPONSE" | $JQ
echo ""

# --------------------------------------------------------------------------
echo "── Step 5: Get corpus status ────────────────────────────────────────"
echo ""
echo "$ curl http://localhost:$PORT/v1/status"
curl -s "http://localhost:$PORT/v1/status" | $JQ
echo ""

# --------------------------------------------------------------------------
echo "── Step 6: Search via HTTP ──────────────────────────────────────────"
echo ""
echo '$ curl -X POST http://localhost:'$PORT'/v1/recall -d {"query": "deployment strategy"}'
RECALL_RESPONSE=$(curl -s -X POST "http://localhost:$PORT/v1/recall" \
    -H "Content-Type: application/json" \
    -d '{"query": "deployment strategy"}')
echo "$RECALL_RESPONSE" | $JQ
echo ""

# Extract first memory_id for expand
MEMORY_ID=$(echo "$RECALL_RESPONSE" | jq -r '.results[0].memory_id // empty' 2>/dev/null || true)

# --------------------------------------------------------------------------
if [ -n "$MEMORY_ID" ]; then
    echo "── Step 7: Expand a memory (full content) ───────────────────────────"
    echo ""
    echo "$ curl http://localhost:$PORT/v1/expand/$MEMORY_ID"
    curl -s "http://localhost:$PORT/v1/expand/$MEMORY_ID" | $JQ
    echo ""
fi

# --------------------------------------------------------------------------
echo "── Step 8: List streams ─────────────────────────────────────────────"
echo ""
echo "$ curl http://localhost:$PORT/v1/streams"
curl -s "http://localhost:$PORT/v1/streams" | $JQ
echo ""

# --------------------------------------------------------------------------
echo "── Step 9: List tags ────────────────────────────────────────────────"
echo ""
echo "$ curl http://localhost:$PORT/v1/tags"
curl -s "http://localhost:$PORT/v1/tags" | $JQ
echo ""

# --------------------------------------------------------------------------
if [ -n "$MEMORY_ID" ]; then
    echo "── Step 10: Forget a memory ─────────────────────────────────────────"
    echo ""
    echo '$ curl -X POST http://localhost:'$PORT'/v1/forget -d {"memory_id": "'$MEMORY_ID'"}'
    curl -s -X POST "http://localhost:$PORT/v1/forget" \
        -H "Content-Type: application/json" \
        -d "{\"memory_id\": \"$MEMORY_ID\"}" | $JQ
    echo ""
fi

# --------------------------------------------------------------------------
echo ""
echo "Stopping server (PID $SERVER_PID)..."
kill "$SERVER_PID" 2>/dev/null || true
wait "$SERVER_PID" 2>/dev/null || true
SERVER_PID=""

echo ""
echo "================================================"
echo "  Done! You've used every HTTP API endpoint."
echo "================================================"
echo ""
echo "Endpoints covered:"
echo "  GET  /health       — health check"
echo "  POST /v1/retain    — store a memory"
echo "  GET  /v1/status    — corpus overview"
echo "  POST /v1/recall    — search memories"
echo "  GET  /v1/expand/:id — full verbatim content"
echo "  GET  /v1/streams   — list streams"
echo "  GET  /v1/tags      — list tags"
echo "  POST /v1/forget    — invalidate a memory"
echo ""
echo "Next: Try Example 5 (MCP integration)"
