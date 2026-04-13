//! Time-based retention policy.
//!
//! Memories older than the threshold that have not been accessed recently
//! are flagged for archival. The staleness clock resets on every access.

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Find memories older than `threshold_days` that have not been accessed recently.
///
/// A memory is considered stale if:
/// - It was created more than `threshold_days` ago, AND
/// - Its `last_accessed_at` is either NULL or also older than `threshold_days` ago
/// - It is not already archived
///
/// Returns a list of memory IDs.
pub fn find_stale_memories(conn: &Connection, threshold_days: i64) -> Result<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT id FROM memories
             WHERE archived = 0
               AND created_at < datetime('now', ?1)
               AND (last_accessed_at IS NULL OR last_accessed_at < datetime('now', ?1))
             ORDER BY created_at ASC",
        )
        .context("failed to prepare stale memories query")?;

    let threshold_modifier = format!("-{threshold_days} days");

    let ids = stmt
        .query_map([&threshold_modifier], |row| row.get::<_, String>(0))
        .context("failed to query stale memories")?
        .collect::<Result<Vec<_>, _>>()
        .context("failed to read stale memory IDs")?;

    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::runner::run_migrations;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_find_stale_memories_empty_db() {
        let conn = setup_db();
        let stale = find_stale_memories(&conn, 90).unwrap();
        assert!(stale.is_empty());
    }

    #[test]
    fn test_find_stale_memories_excludes_recent() {
        let conn = setup_db();
        // Insert a memory with current timestamp — should not be stale
        conn.execute(
            "INSERT INTO memories (id, content_hash, source_format, created_at) VALUES ('m1', 'h1', 'test', datetime('now'))",
            [],
        )
        .unwrap();

        let stale = find_stale_memories(&conn, 90).unwrap();
        assert!(stale.is_empty());
    }

    #[test]
    fn test_find_stale_memories_includes_old() {
        let conn = setup_db();
        // Insert a memory from 100 days ago
        conn.execute(
            "INSERT INTO memories (id, content_hash, source_format, created_at) VALUES ('m1', 'h1', 'test', datetime('now', '-100 days'))",
            [],
        )
        .unwrap();

        let stale = find_stale_memories(&conn, 90).unwrap();
        assert_eq!(stale, vec!["m1"]);
    }

    #[test]
    fn test_find_stale_excludes_recently_accessed() {
        let conn = setup_db();
        // Insert an old memory that was recently accessed
        conn.execute(
            "INSERT INTO memories (id, content_hash, source_format, created_at, last_accessed_at) VALUES ('m1', 'h1', 'test', datetime('now', '-100 days'), datetime('now', '-1 day'))",
            [],
        )
        .unwrap();

        let stale = find_stale_memories(&conn, 90).unwrap();
        assert!(stale.is_empty());
    }

    #[test]
    fn test_find_stale_excludes_archived() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO memories (id, content_hash, source_format, created_at, archived) VALUES ('m1', 'h1', 'test', datetime('now', '-100 days'), 1)",
            [],
        )
        .unwrap();

        let stale = find_stale_memories(&conn, 90).unwrap();
        assert!(stale.is_empty());
    }
}
