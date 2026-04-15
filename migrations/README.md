# migrations/ — SQLite Schema Migrations

SQL migration files for the Clear Memory SQLite database. Applied automatically on startup by the migration runner (`src/migration/runner.rs`).

---

## Current Migrations

| File | Version | Description |
|------|---------|-------------|
| `001_initial_schema.sql` | 1 | Creates all v1 tables: memories, memory_tags, facts, entities, entity_aliases, entity_relationships, streams, stream_tags, stream_writers, audit_log, retention_events, performance_baselines, legal_holds, schema_version, migration_log. Also creates 16 performance indices. |

## How Migrations Work

1. On startup, `src/migration/versioning.rs` reads the current `schema_version` from the database.
2. `src/migration/runner.rs` finds all migration files with version numbers higher than the current version.
3. Each migration runs inside a transaction — if it fails, the transaction rolls back and the migration is recorded as failed in `migration_log`.
4. After all migrations succeed, the `schema_version` table is updated.

## Adding a New Migration

1. Create a file named `NNN_description.sql` (e.g., `002_add_profiles_table.sql`).
2. Number must be strictly sequential — no gaps.
3. Write idempotent SQL where possible (`CREATE TABLE IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`).
4. The migration runner applies files in numeric order.
5. Test with: `cargo test --test migration_tests` (planned).

## Configuration

```toml
[migrations]
auto_migrate = true              # apply on startup (default true)
backup_before_migrate = true     # create backup before migrating (default true)
```

## Important Notes

- Migrations are **forward-only**. There are no down migrations. If a migration needs to be reversed, create a new forward migration that undoes the changes.
- The `migration_log` table records every migration attempt (success, failure, rollback) for audit purposes.
- Before any migration, the engine creates an automatic backup if `backup_before_migrate = true`.
