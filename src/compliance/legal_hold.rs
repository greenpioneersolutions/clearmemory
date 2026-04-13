//! Legal hold management — freeze streams for litigation or compliance.

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A legal hold on a stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalHold {
    pub id: String,
    pub stream_id: String,
    pub reason: String,
    pub held_by: String,
    pub held_at: String,
    pub released_at: Option<String>,
    pub released_by: Option<String>,
}

/// Create a legal hold on a stream. Returns the hold ID.
pub fn create_hold(
    conn: &Connection,
    stream_id: &str,
    reason: &str,
    held_by: &str,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let held_at = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO legal_holds (id, stream_id, reason, held_by, held_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, stream_id, reason, held_by, held_at],
    )
    .context("failed to create legal hold")?;

    Ok(id)
}

/// Release a legal hold.
pub fn release_hold(conn: &Connection, hold_id: &str, released_by: &str) -> Result<()> {
    let released_at = Utc::now().to_rfc3339();

    let updated = conn
        .execute(
            "UPDATE legal_holds SET released_at = ?1, released_by = ?2 WHERE id = ?3 AND released_at IS NULL",
            rusqlite::params![released_at, released_by, hold_id],
        )
        .context("failed to release legal hold")?;

    if updated == 0 {
        anyhow::bail!("hold not found or already released: {hold_id}");
    }

    Ok(())
}

/// Check if a stream has an active (unreleased) legal hold.
pub fn is_held(conn: &Connection, stream_id: &str) -> Result<bool> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM legal_holds WHERE stream_id = ?1 AND released_at IS NULL",
            rusqlite::params![stream_id],
            |row| row.get(0),
        )
        .context("failed to check legal hold status")?;

    Ok(count > 0)
}

/// List all legal holds (both active and released).
pub fn list_holds(conn: &Connection) -> Result<Vec<LegalHold>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, stream_id, reason, held_by, held_at, released_at, released_by FROM legal_holds ORDER BY held_at DESC",
        )
        .context("failed to prepare legal holds query")?;

    let holds = stmt
        .query_map([], |row| {
            Ok(LegalHold {
                id: row.get(0)?,
                stream_id: row.get(1)?,
                reason: row.get(2)?,
                held_by: row.get(3)?,
                held_at: row.get(4)?,
                released_at: row.get(5)?,
                released_by: row.get(6)?,
            })
        })
        .context("failed to query legal holds")?
        .collect::<Result<Vec<_>, _>>()
        .context("failed to read legal hold rows")?;

    Ok(holds)
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

    fn insert_stream(conn: &Connection, id: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO streams (id, name, owner_id, created_at) VALUES (?1, ?1, 'admin', ?2)",
            rusqlite::params![id, now],
        )
        .unwrap();
    }

    #[test]
    fn test_create_and_check_hold() {
        let conn = setup_db();
        insert_stream(&conn, "stream-1");
        let hold_id = create_hold(&conn, "stream-1", "Litigation case #123", "admin").unwrap();
        assert!(!hold_id.is_empty());
        assert!(is_held(&conn, "stream-1").unwrap());
        assert!(!is_held(&conn, "stream-2").unwrap());
    }

    #[test]
    fn test_release_hold() {
        let conn = setup_db();
        insert_stream(&conn, "stream-1");
        let hold_id = create_hold(&conn, "stream-1", "Litigation case #123", "admin").unwrap();

        assert!(is_held(&conn, "stream-1").unwrap());
        release_hold(&conn, &hold_id, "admin").unwrap();
        assert!(!is_held(&conn, "stream-1").unwrap());
    }

    #[test]
    fn test_release_nonexistent_hold_fails() {
        let conn = setup_db();
        let result = release_hold(&conn, "nonexistent", "admin");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_holds() {
        let conn = setup_db();
        insert_stream(&conn, "stream-1");
        insert_stream(&conn, "stream-2");
        create_hold(&conn, "stream-1", "Case A", "admin").unwrap();
        create_hold(&conn, "stream-2", "Case B", "legal").unwrap();

        let holds = list_holds(&conn).unwrap();
        assert_eq!(holds.len(), 2);
    }

    #[test]
    fn test_multiple_holds_on_same_stream() {
        let conn = setup_db();
        insert_stream(&conn, "stream-1");
        create_hold(&conn, "stream-1", "Case A", "admin").unwrap();
        create_hold(&conn, "stream-1", "Case B", "legal").unwrap();

        assert!(is_held(&conn, "stream-1").unwrap());
        let holds = list_holds(&conn).unwrap();
        assert_eq!(holds.len(), 2);
    }
}
