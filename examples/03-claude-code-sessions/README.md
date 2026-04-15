# Example 3: Claude Code Sessions

**Interface:** CLI
**Time to run:** ~30 seconds

## What You'll Learn

- How Claude Code conversations become **persistent, searchable knowledge**
- Cross-session search finds connections between different coding sessions
- **Secret scanning** detects leaked credentials before they become a liability
- **Backup** preserves your entire corpus as a single portable file

## Run It

```bash
cd examples/03-claude-code-sessions
./run.sh
```

## What Happens

1. Imports a 12-turn Claude Code session about designing a rate limiter
2. Imports an 8-turn session debugging a production memory leak
3. Searches for "rate limiting approach" — finds the architecture decision
4. Searches for "memory leak root cause" — finds the debugging session
5. Cross-session search: "connection pool" finds results from both sessions
6. Runs secret scanning — detects a leaked AWS key in the bugfix session
7. Creates a backup of the entire corpus

## The Secret Scanning Demo

The bugfix session deliberately contains a fake AWS access key (`AKIAIOSFODNN7EXAMPLE`). This simulates a real scenario where a developer pastes credentials into a Claude Code conversation. Clear Memory's secret scanner catches it before it becomes a long-term credential store.

## Sample Data

- `data/feature-session.json` — Building a token-bucket rate limiter with tests and hot-reloadable config
- `data/bugfix-session.json` — Debugging a connection pool memory leak with circuit breaker fix
