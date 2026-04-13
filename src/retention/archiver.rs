//! Memory archiver — marks memories as archived in the database.
//! Enforces legal hold checks before archiving.

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Archive a memory by setting `archived = 1` in the database.
///
/// Checks legal hold status first — if the memory's stream has an active hold,
/// the archive operation is rejected. Archived memories are excluded from normal
/// search results but remain queryable with `--include-archive`.
pub fn archive_memory(conn: &Connection, memory_id: &str) -> Result<()> {
    // Check if the memory's stream has a legal hold
    let stream_id: Option<String> = conn
        .query_row(
            "SELECT stream_id FROM memories WHERE id = ?1",
            rusqlite::params![memory_id],
            |row| row.get(0),
        )
        .context("memory not found")?;

    if let Some(ref sid) = stream_id {
        if crate::compliance::legal_hold::is_held(conn, sid)? {
            anyhow::bail!("cannot archive: stream '{sid}' is under legal hold");
        }
    }

    let updated = conn
        .execute(
            "UPDATE memories SET archived = 1 WHERE id = ?1 AND archived = 0",
            rusqlite::params![memory_id],
        )
        .context("failed to archive memory")?;

    if updated == 0 {
        anyhow::bail!("memory not found or already archived: {memory_id}");
    }

    // Log retention event
    let _ = conn.execute(
        "INSERT INTO retention_events (id, timestamp, trigger_type, memories_archived, details) \
         VALUES (?1, datetime('now'), 'archive', 1, ?2)",
        rusqlite::params![uuid::Uuid::new_v4().to_string(), memory_id],
    );

    Ok(())
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
    fn test_archive_memory() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO memories (id, content_hash, source_format, created_at) VALUES ('m1', 'h1', 'test', datetime('now'))",
            [],
        )
        .unwrap();

        archive_memory(&conn, "m1").unwrap();

        let archived: i64 = conn
            .query_row("SELECT archived FROM memories WHERE id = 'm1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(archived, 1);
    }

    #[test]
    fn test_archive_blocked_by_legal_hold() {
        let conn = setup_db();

        // Create a stream and put a hold on it
        conn.execute(
            "INSERT INTO streams (id, name, owner_id, created_at) VALUES ('s1', 'test', 'user1', datetime('now'))",
            [],
        )
        .unwrap();
        crate::compliance::legal_hold::create_hold(&conn, "s1", "litigation", "admin").unwrap();

        // Create a memory in the held stream
        conn.execute(
            "INSERT INTO memories (id, content_hash, source_format, created_at, stream_id) VALUES ('m1', 'h1', 'test', datetime('now'), 's1')",
            [],
        )
        .unwrap();

        // Archive should be blocked
        let result = archive_memory(&conn, "m1");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("legal hold"));
    }

    #[test]
    fn test_archive_nonexistent_memory_fails() {
        let conn = setup_db();
        let result = archive_memory(&conn, "nonexistent");
        assert!(result.is_err());
    }
}
