# Example 1: Getting Started

**Interface:** CLI
**Time to run:** ~30 seconds (includes model download on first run)

## What You'll Learn

- The core **retain → recall → expand** workflow
- How to import memories from `.clear` files
- How tags (team, repo, project, domain) organize your knowledge
- That search is **semantic**, not just keyword matching

## Prerequisites

```bash
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

## Run It

```bash
cd examples/01-getting-started
./run.sh
```

## What Happens

1. Initializes Clear Memory in offline mode (Tier 1 — zero external calls)
2. Imports 5 team decisions from a `.clear` file
3. Stores a new memory about PostgreSQL via CLI
4. Searches for "what database did we choose" — finds the PostgreSQL decision via semantic search
5. Searches for "authentication" — finds the Auth0 → Clerk migration decision
6. Lists all tags and shows corpus status

## Sample Data

`data/first-memories.clear` contains 5 realistic engineering decisions from the Meridian platform team:

- Auth migration: Auth0 → Clerk
- CI/CD: Jenkins → GitHub Actions
- Monitoring: Prometheus + Grafana
- Frontend state: Zustand over Redux
- Incident: connection pool exhaustion postmortem
