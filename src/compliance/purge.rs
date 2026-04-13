//! Hard purge operations for GDPR/CCPA right-to-delete compliance.

use crate::ComplianceError;
use anyhow::{Context, Result};
use rusqlite::Connection;

/// Check whether a memory's stream is under legal hold.
///
/// Returns `Ok(())` if purge is allowed, or `Err(ComplianceError::LegalHoldActive)` if held.
pub fn check_legal_hold(conn: &Connection, memory_id: &str) -> Result<()> {
    let result: Option<(String, String)> = conn
        .query_row(
            "SELECT lh.stream_id, lh.reason FROM legal_holds lh
             INNER JOIN memories m ON m.stream_id = lh.stream_id
             WHERE m.id = ?1 AND lh.released_at IS NULL
             LIMIT 1",
            rusqlite::params![memory_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .context("failed to check legal hold for memory")?;

    if let Some((stream_id, reason)) = result {
        return Err(ComplianceError::LegalHoldActive { stream_id, reason }.into());
    }

    Ok(())
}

/// Permanently delete a memory and all associated data.
///
/// Removes records from: memories, facts, memory_tags, entity_relationships.
/// The caller must check legal holds before calling this function.
pub fn purge_memory(conn: &Connection, memory_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM entity_relationships WHERE memory_id = ?1",
        rusqlite::params![memory_id],
    )
    .context("failed to purge entity_relationships")?;

    conn.execute(
        "DELETE FROM facts WHERE memory_id = ?1",
        rusqlite::params![memory_id],
    )
    .context("failed to purge facts")?;

    conn.execute(
        "DELETE FROM memory_tags WHERE memory_id = ?1",
        rusqlite::params![memory_id],
    )
    .context("failed to purge memory_tags")?;

    conn.execute(
        "DELETE FROM memories WHERE id = ?1",
        rusqlite::params![memory_id],
    )
    .context("failed to purge memory")?;

    Ok(())
}

/// Extension trait to make `query_row` return `Option` instead of erroring on no rows.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::runner::run_migrations;
    use chrono::Utc;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    fn insert_memory(conn: &Connection, id: &str, stream_id: Option<&str>) {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO memories (id, content_hash, source_format, created_at, stream_id) VALUES (?1, ?2, 'test', ?3, ?4)",
            rusqlite::params![id, format!("hash-{id}"), now, stream_id],
        )
        .unwrap();
    }

    fn insert_fact(conn: &Connection, fact_id: &str, memory_id: &str) {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO facts (id, memory_id, subject, predicate, object, ingested_at) VALUES (?1, ?2, 'subj', 'pred', 'obj', ?3)",
            rusqlite::params![fact_id, memory_id, now],
        )
        .unwrap();
    }

    fn insert_tag(conn: &Connection, memory_id: &str) {
        conn.execute(
            "INSERT INTO memory_tags (memory_id, tag_type, tag_value) VALUES (?1, 'team', 'platform')",
            rusqlite::params![memory_id],
        )
        .unwrap();
    }

    #[test]
    fn test_purge_memory_deletes_all_related_data() {
        let conn = setup_db();
        insert_memory(&conn, "mem-1", None);
        insert_fact(&conn, "fact-1", "mem-1");
        insert_tag(&conn, "mem-1");

        purge_memory(&conn, "mem-1").unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE id = 'mem-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);

        let fact_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM facts WHERE memory_id = 'mem-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fact_count, 0);

        let tag_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_tags WHERE memory_id = 'mem-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tag_count, 0);
    }

    #[test]
    fn test_purge_nonexistent_memory_succeeds() {
        let conn = setup_db();
        // Purging a non-existent memory should not error (DELETE affects 0 rows)
        purge_memory(&conn, "nonexistent").unwrap();
    }

    #[test]
    fn test_check_legal_hold_blocks_purge() {
        let conn = setup_db();

        // Create a stream and hold
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO streams (id, name, owner_id, created_at) VALUES ('s1', 'Test', 'admin', ?1)",
            rusqlite::params![now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO legal_holds (id, stream_id, reason, held_by, held_at) VALUES ('h1', 's1', 'Litigation', 'legal', ?1)",
            rusqlite::params![now],
        )
        .unwrap();

        insert_memory(&conn, "mem-1", Some("s1"));

        let result = check_legal_hold(&conn, "mem-1");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("legal hold"));
    }

    #[test]
    fn test_check_legal_hold_allows_when_no_hold() {
        let conn = setup_db();
        insert_memory(&conn, "mem-1", None);
        check_legal_hold(&conn, "mem-1").unwrap();
    }
}
