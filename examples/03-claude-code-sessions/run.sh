#!/bin/bash
set -e

# ============================================================================
# Clear Memory — Example 3: Claude Code Sessions
# ============================================================================
# Import real Claude Code sessions, search the decisions made during coding,
# detect leaked secrets, and create a backup.
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
export CLEARMEMORY_DATA_DIR=$(mktemp -d)
BACKUP_FILE="/tmp/clearmemory-example-backup-$$.cmb"
trap 'rm -rf "$CLEARMEMORY_DATA_DIR" "$BACKUP_FILE"' EXIT

echo "========================================================"
echo "  Clear Memory — Example 3: Claude Code Sessions"
echo "========================================================"
echo ""

if ! command -v clearmemory &> /dev/null; then
    echo "ERROR: 'clearmemory' not found. Run: cargo build --release"
    exit 1
fi

# --------------------------------------------------------------------------
echo "── Step 1: Initialize ───────────────────────────────────────────────"
clearmemory init --tier offline
echo ""

# --------------------------------------------------------------------------
echo "── Step 2: Import a feature development session ─────────────────────"
echo ""
echo "This is a 12-turn Claude Code session about building rate limiting."
echo ""
echo "$ clearmemory import data/feature-session.json --format claude_code"
clearmemory import "$SCRIPT_DIR/data/feature-session.json" --format claude_code
echo ""

# --------------------------------------------------------------------------
echo "── Step 3: Import a bugfix session ──────────────────────────────────"
echo ""
echo "An 8-turn session debugging a production memory leak."
echo "(This one contains a deliberately planted AWS key for the secret scan demo.)"
echo ""
echo "$ clearmemory import data/bugfix-session.json --format claude_code"
clearmemory import "$SCRIPT_DIR/data/bugfix-session.json" --format claude_code
echo ""

# --------------------------------------------------------------------------
echo "── Step 4: Search for the rate limiting decision ────────────────────"
echo ""
echo "$ clearmemory recall \"rate limiting approach\""
clearmemory recall "rate limiting approach"
echo ""

# --------------------------------------------------------------------------
echo "── Step 5: Search for the production bug ────────────────────────────"
echo ""
echo "$ clearmemory recall \"memory leak root cause\""
clearmemory recall "memory leak root cause"
echo ""

# --------------------------------------------------------------------------
echo "── Step 6: Cross-session search ─────────────────────────────────────"
echo ""
echo "Search spans both sessions — finds connections between them."
echo ""
echo "$ clearmemory recall \"connection pool\""
clearmemory recall "connection pool"
echo ""

# --------------------------------------------------------------------------
echo "── Step 7: Scan for secrets ─────────────────────────────────────────"
echo ""
echo "The bugfix session contains a leaked AWS key. Let's find it."
echo ""
echo "$ clearmemory security scan"
clearmemory security scan
echo ""

# --------------------------------------------------------------------------
echo "── Step 8: Create a backup ──────────────────────────────────────────"
echo ""
echo "$ clearmemory backup $BACKUP_FILE --no-encrypt"
clearmemory backup "$BACKUP_FILE" --no-encrypt
echo ""
echo "Backup created at: $BACKUP_FILE"
ls -lh "$BACKUP_FILE" 2>/dev/null || true

# --------------------------------------------------------------------------
echo ""
echo "========================================================"
echo "  Done! Two Claude Code sessions are now searchable."
echo "========================================================"
echo ""
echo "What you learned:"
echo "  - Claude Code sessions become persistent, searchable knowledge"
echo "  - Search spans multiple sessions — find decisions across contexts"
echo "  - Secret scanning detects leaked credentials (AWS keys, tokens, etc.)"
echo "  - Backups preserve your entire corpus as a single encrypted file"
echo ""
echo "Next: Try Example 4 (HTTP API with curl)"
