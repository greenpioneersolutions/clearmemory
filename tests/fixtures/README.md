# tests/fixtures/ — Test Fixture Data

Synthetic test data used by the benchmark and test suites. All data is fabricated — no real user conversations, API keys, or proprietary content.

---

## Fixture Files

| File | Format | Contents | Used By |
|------|--------|----------|---------|
| `sample_claude_code_session.json` | Claude Code transcript JSON | 4-exchange synthetic conversation about JWT authentication | Import parser tests, benchmark corpus building |
| `sample_copilot_session.log` | Copilot CLI session log | Synthetic database connection pool discussion | Import parser tests |
| `sample_chatgpt_export.json` | ChatGPT export JSON (OpenAI format) | Synthetic auth provider migration discussion | Import parser tests |
| `sample_slack_export/engineering/2026-03-15.json` | Slack workspace export | Synthetic engineering channel messages with user IDs (U001, U002) | Import parser tests |
| `sample.clear` | Clear Format (.clear) JSON | 3 memories with full tag metadata (team, repo, project, domain) | Clear Format parser and validator tests |
| `sample.csv` | CSV | 3 rows with fictional employee names (Sarah Chen, Kai Rivera, Priya Sharma) and decisions | CSV-to-Clear conversion tests |
| `corrupt_fixtures/corrupt.json` | Intentionally malformed JSON | Invalid JSON for testing error handling and recovery | Adversarial/recovery tests |

## Adding New Fixtures

When adding test fixtures:
- Use synthetic data only — never paste real conversations or credentials
- If a fixture needs to contain credential-like strings for secret scanning tests, use obviously fake values (e.g., `AKIAIOSFODNN7EXAMPLE`)
- Match the exact format of the real source (ChatGPT export structure, Slack export directory layout, etc.) so parsers are tested against realistic structure
