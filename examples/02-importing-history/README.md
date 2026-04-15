# Example 2: Importing Team History

**Interface:** CLI
**Time to run:** ~30 seconds

## What You'll Learn

- Import from **3 different formats**: Claude Code sessions, CSV, Slack exports
- All sources merge into one searchable corpus
- **Streams** scope searches to relevant subsets
- Temporal queries ("what happened last week") work across sources

## Run It

```bash
cd examples/02-importing-history
./run.sh
```

## What Happens

1. Imports a Claude Code session about JWT middleware debugging
2. Imports 10 CSV standup notes spanning 2 weeks
3. Imports a Slack channel export about deployment planning
4. Creates a "Platform Team" stream
5. Searches across all sources — finds results from Claude, CSV, and Slack in one query

## Sample Data

- `data/claude-session.json` — 8-turn Claude Code conversation about JWT auth fixes and rate limiting
- `data/standup-notes.csv` — 10 daily standup entries from 3 team members
- `data/slack-export/engineering/2026-03-15.json` — Slack discussion about the March 20 deployment plan
