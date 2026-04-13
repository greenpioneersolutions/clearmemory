//! Compliance reporting — generate reports for auditors.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A compliance report summarizing the state of the memory corpus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    pub memory_count: i64,
    pub classification_counts: HashMap<String, i64>,
    pub pii_count: i64,
    pub active_holds_count: i64,
}

/// Generate a compliance report from the database.
pub fn generate_report(conn: &Connection) -> Result<ComplianceReport> {
    let memory_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
        .context("failed to count memories")?;

    let mut classification_counts = HashMap::new();
    let mut stmt = conn
        .prepare("SELECT classification, COUNT(*) FROM memories GROUP BY classification")
        .context("failed to prepare classification query")?;
    let rows = stmt
        .query_map([], |row| {
            let class: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((class, count))
        })
        .context("failed to query classification counts")?;
    for row in rows {
        let (class, count) = row.context("failed to read classification row")?;
        classification_counts.insert(class, count);
    }

    let pii_count = *classification_counts.get("pii").unwrap_or(&0);

    let active_holds_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM legal_holds WHERE released_at IS NULL",
            [],
            |row| row.get(0),
        )
        .context("failed to count active holds")?;

    Ok(ComplianceReport {
        memory_count,
        classification_counts,
        pii_count,
        active_holds_count,
    })
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

    fn insert_memory(conn: &Connection, id: &str, classification: &str) {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO memories (id, content_hash, source_format, classification, created_at) VALUES (?1, ?2, 'test', ?3, ?4)",
            rusqlite::params![id, format!("hash-{id}"), classification, now],
        )
        .unwrap();
    }

    #[test]
    fn test_generate_report_empty_db() {
        let conn = setup_db();
        let report = generate_report(&conn).unwrap();
        assert_eq!(report.memory_count, 0);
        assert_eq!(report.pii_count, 0);
        assert_eq!(report.active_holds_count, 0);
        assert!(report.classification_counts.is_empty());
    }

    #[test]
    fn test_generate_report_with_data() {
        let conn = setup_db();
        insert_memory(&conn, "m1", "internal");
        insert_memory(&conn, "m2", "internal");
        insert_memory(&conn, "m3", "confidential");
        insert_memory(&conn, "m4", "pii");

        // Create an active hold
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO streams (id, name, owner_id, created_at) VALUES ('s1', 'Test', 'admin', ?1)",
            rusqlite::params![now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO legal_holds (id, stream_id, reason, held_by, held_at) VALUES ('h1', 's1', 'Case', 'legal', ?1)",
            rusqlite::params![now],
        )
        .unwrap();

        let report = generate_report(&conn).unwrap();
        assert_eq!(report.memory_count, 4);
        assert_eq!(report.classification_counts.get("internal"), Some(&2));
        assert_eq!(report.classification_counts.get("confidential"), Some(&1));
        assert_eq!(report.pii_count, 1);
        assert_eq!(report.active_holds_count, 1);
    }
}
