#!/bin/bash
set -e

# ============================================================================
# Clear Memory — Example 1: Getting Started
# ============================================================================
# This example walks you through the core workflow:
#   retain → recall → expand → tags → status
#
# Prerequisites: `clearmemory` binary on PATH (cargo build --release)
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
export CLEARMEMORY_DATA_DIR=$(mktemp -d)
trap 'rm -rf "$CLEARMEMORY_DATA_DIR"' EXIT

echo "============================================"
echo "  Clear Memory — Example 1: Getting Started"
echo "============================================"
echo ""
echo "Using temp data dir: $CLEARMEMORY_DATA_DIR"
echo ""

# Check binary exists
if ! command -v clearmemory &> /dev/null; then
    echo "ERROR: 'clearmemory' not found on PATH."
    echo "Build it first: cargo build --release"
    echo "Then add to PATH: export PATH=\"\$PWD/target/release:\$PATH\""
    exit 1
fi

# --------------------------------------------------------------------------
echo ""
echo "── Step 1: Initialize Clear Memory ──────────────────────────────────"
echo ""
echo "$ clearmemory init --tier offline"
clearmemory init --tier offline
echo ""
echo "Clear Memory is now initialized in offline mode (Tier 1)."
echo "All data stays on your machine. No external calls."

# --------------------------------------------------------------------------
echo ""
echo "── Step 2: Import seed memories ─────────────────────────────────────"
echo ""
echo "We have 5 team decisions in a .clear file (JSON format)."
echo ""
echo "$ clearmemory import $SCRIPT_DIR/data/first-memories.clear --format clear"
clearmemory import "$SCRIPT_DIR/data/first-memories.clear" --format clear
echo ""
echo "5 memories imported — auth migration, CI/CD, monitoring, frontend, incident."

# --------------------------------------------------------------------------
echo ""
echo "── Step 3: Store a new memory via CLI ───────────────────────────────"
echo ""
echo "$ clearmemory retain \"We chose PostgreSQL over MySQL...\" --tag team:platform --tag domain:infrastructure"
clearmemory retain "We chose PostgreSQL over MySQL for ACID compliance, JSON support with jsonb columns, and better performance on complex queries. MySQL was considered but its JSON support is less mature and we need strong transactional guarantees for our payment pipeline." --tag team:platform --tag domain:infrastructure
echo ""
echo "Memory stored. Now we have 6 memories total."

# --------------------------------------------------------------------------
echo ""
echo "── Step 4: Search — semantic query ──────────────────────────────────"
echo ""
echo "Let's ask: 'what database did we choose and why?'"
echo "Note: this is a semantic search, not just keyword matching."
echo ""
echo "$ clearmemory recall \"what database did we choose and why\""
clearmemory recall "what database did we choose and why"

# --------------------------------------------------------------------------
echo ""
echo "── Step 5: Search — different query ─────────────────────────────────"
echo ""
echo "Now searching for 'authentication' — finds the Auth0 → Clerk decision."
echo ""
echo "$ clearmemory recall \"authentication\""
clearmemory recall "authentication"

# --------------------------------------------------------------------------
echo ""
echo "── Step 6: List all tags ────────────────────────────────────────────"
echo ""
echo "$ clearmemory tags list"
clearmemory tags list

# --------------------------------------------------------------------------
echo ""
echo "── Step 7: Corpus status ────────────────────────────────────────────"
echo ""
echo "$ clearmemory status"
clearmemory status

# --------------------------------------------------------------------------
echo ""
echo "============================================"
echo "  Done! You've completed the core workflow."
echo "============================================"
echo ""
echo "What you learned:"
echo "  - clearmemory init    → initialize the engine"
echo "  - clearmemory import  → bulk load from .clear files"
echo "  - clearmemory retain  → store individual memories with tags"
echo "  - clearmemory recall  → semantic search across all memories"
echo "  - clearmemory tags    → organize with team/repo/project/domain"
echo "  - clearmemory status  → check corpus health"
echo ""
echo "Next: Try Example 2 (importing from Claude Code, CSV, and Slack)"
