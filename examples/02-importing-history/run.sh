#!/bin/bash
set -e

# ============================================================================
# Clear Memory — Example 2: Importing Team History
# ============================================================================
# Import memories from Claude Code sessions, CSV standup notes, and Slack
# exports. Then search across all sources with unified retrieval.
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
export CLEARMEMORY_DATA_DIR=$(mktemp -d)
trap 'rm -rf "$CLEARMEMORY_DATA_DIR"' EXIT

echo "================================================"
echo "  Clear Memory — Example 2: Importing History"
echo "================================================"
echo ""

if ! command -v clearmemory &> /dev/null; then
    echo "ERROR: 'clearmemory' not found. Run: cargo build --release"
    exit 1
fi

# --------------------------------------------------------------------------
echo "── Step 1: Initialize ───────────────────────────────────────────────"
echo ""
clearmemory init --tier offline
echo ""

# --------------------------------------------------------------------------
echo "── Step 2: Import a Claude Code session ─────────────────────────────"
echo ""
echo "This is a real conversation about JWT middleware debugging."
echo ""
echo "$ clearmemory import data/claude-session.json --format claude_code"
clearmemory import "$SCRIPT_DIR/data/claude-session.json" --format claude_code
echo ""

# --------------------------------------------------------------------------
echo "── Step 3: Import CSV standup notes ─────────────────────────────────"
echo ""
echo "Two weeks of daily standup entries from the team."
echo ""
echo "$ clearmemory import data/standup-notes.csv --format clear --mapping auto"
clearmemory import "$SCRIPT_DIR/data/standup-notes.csv" --format clear --mapping auto
echo ""

# --------------------------------------------------------------------------
echo "── Step 4: Import Slack export ──────────────────────────────────────"
echo ""
echo "Engineering channel discussion about the deployment plan."
echo ""
echo "$ clearmemory import data/slack-export/ --format slack"
clearmemory import "$SCRIPT_DIR/data/slack-export/" --format slack
echo ""

# --------------------------------------------------------------------------
echo "── Step 5: Check corpus status ──────────────────────────────────────"
echo ""
echo "$ clearmemory status"
clearmemory status
echo ""

# --------------------------------------------------------------------------
echo "── Step 6: Create a stream ──────────────────────────────────────────"
echo ""
echo "Streams are scoped views across tag intersections."
echo ""
echo "$ clearmemory streams create \"Platform Team\" --tag team:platform"
clearmemory streams create "Platform Team" --tag team:platform
echo ""

# --------------------------------------------------------------------------
echo "── Step 7: Search across all sources ────────────────────────────────"
echo ""
echo "Now the magic: search once, find results from Claude, CSV, and Slack."
echo ""
echo "$ clearmemory recall \"authentication migration\""
clearmemory recall "authentication migration"
echo ""

# --------------------------------------------------------------------------
echo "── Step 8: Find the Claude Code session ─────────────────────────────"
echo ""
echo "$ clearmemory recall \"JWT token validation\""
clearmemory recall "JWT token validation"
echo ""

# --------------------------------------------------------------------------
echo "── Step 9: Temporal query ───────────────────────────────────────────"
echo ""
echo "$ clearmemory recall \"what happened last week\""
clearmemory recall "what happened last week"

# --------------------------------------------------------------------------
echo ""
echo "================================================"
echo "  Done! Three data sources, one searchable corpus."
echo "================================================"
echo ""
echo "What you learned:"
echo "  - Import from Claude Code sessions (JSON)"
echo "  - Import from CSV (standup notes, spreadsheets)"
echo "  - Import from Slack exports (channel archives)"
echo "  - All sources merge into one unified, searchable index"
echo "  - Streams scope your searches to relevant subsets"
echo ""
echo "Next: Try Example 3 (Claude Code sessions with secret scanning)"
