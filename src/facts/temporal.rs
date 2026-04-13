use rusqlite::{params, Connection};

/// A temporal fact: a subject-predicate-object triple with time bounds.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Fact {
    pub id: String,
    pub memory_id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    pub ingested_at: String,
    pub invalidated_at: Option<String>,
    pub confidence: f64,
}

/// Get all currently valid facts for a subject.
pub fn current_facts(conn: &Connection, subject: &str) -> Result<Vec<Fact>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, memory_id, subject, predicate, object, valid_from, valid_until, \
         ingested_at, invalidated_at, confidence FROM facts \
         WHERE LOWER(subject) = LOWER(?1) AND valid_until IS NULL AND invalidated_at IS NULL \
         ORDER BY ingested_at DESC",
    )?;
    let rows = stmt.query_map(params![subject], map_fact_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Get facts that were valid at a specific point in time.
pub fn facts_at(
    conn: &Connection,
    subject: &str,
    timestamp: &str,
) -> Result<Vec<Fact>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, memory_id, subject, predicate, object, valid_from, valid_until, \
         ingested_at, invalidated_at, confidence FROM facts \
         WHERE LOWER(subject) = LOWER(?1) \
         AND (valid_from IS NULL OR valid_from <= ?2) \
         AND (valid_until IS NULL OR valid_until > ?2) \
         ORDER BY ingested_at DESC",
    )?;
    let rows = stmt.query_map(params![subject, timestamp], map_fact_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Get the full history of facts for a subject.
pub fn fact_history(conn: &Connection, subject: &str) -> Result<Vec<Fact>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, memory_id, subject, predicate, object, valid_from, valid_until, \
         ingested_at, invalidated_at, confidence FROM facts \
         WHERE LOWER(subject) = LOWER(?1) ORDER BY ingested_at ASC",
    )?;
    let rows = stmt.query_map(params![subject], map_fact_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Insert a new fact.
pub fn insert_fact(conn: &Connection, fact: &Fact) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO facts (id, memory_id, subject, predicate, object, valid_from, valid_until, \
         ingested_at, invalidated_at, confidence) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            fact.id,
            fact.memory_id,
            fact.subject,
            fact.predicate,
            fact.object,
            fact.valid_from,
            fact.valid_until,
            fact.ingested_at,
            fact.invalidated_at,
            fact.confidence,
        ],
    )?;
    Ok(())
}

fn map_fact_row(row: &rusqlite::Row<'_>) -> Result<Fact, rusqlite::Error> {
    Ok(Fact {
        id: row.get(0)?,
        memory_id: row.get(1)?,
        subject: row.get(2)?,
        predicate: row.get(3)?,
        object: row.get(4)?,
        valid_from: row.get(5)?,
        valid_until: row.get(6)?,
        ingested_at: row.get(7)?,
        invalidated_at: row.get(8)?,
        confidence: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration;
    use chrono::Utc;
    use uuid::Uuid;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migration::runner::run_migrations(&conn).unwrap();
        // Insert a test memory
        conn.execute(
            "INSERT INTO memories (id, content_hash, source_format, created_at) \
             VALUES ('mem1', 'hash1', 'clear', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    fn make_fact(subject: &str, predicate: &str, object: &str) -> Fact {
        Fact {
            id: Uuid::new_v4().to_string(),
            memory_id: "mem1".to_string(),
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            valid_from: Some("2026-01-01T00:00:00Z".to_string()),
            valid_until: None,
            ingested_at: Utc::now().to_rfc3339(),
            invalidated_at: None,
            confidence: 1.0,
        }
    }

    #[test]
    fn test_insert_and_query_current_facts() {
        let conn = setup_db();
        let fact = make_fact("auth-service", "uses", "Clerk");
        insert_fact(&conn, &fact).unwrap();

        let facts = current_facts(&conn, "auth-service").unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].object, "Clerk");
    }

    #[test]
    fn test_invalidated_facts_excluded() {
        let conn = setup_db();
        let mut fact = make_fact("auth-service", "uses", "Auth0");
        fact.valid_until = Some("2026-03-01T00:00:00Z".to_string());
        insert_fact(&conn, &fact).unwrap();

        let current = current_facts(&conn, "auth-service").unwrap();
        assert_eq!(current.len(), 0); // Superseded fact excluded

        let history = fact_history(&conn, "auth-service").unwrap();
        assert_eq!(history.len(), 1); // But visible in history
    }

    #[test]
    fn test_facts_at_timestamp() {
        let conn = setup_db();

        let mut old_fact = make_fact("team", "lead", "Alice");
        old_fact.valid_from = Some("2025-01-01T00:00:00Z".to_string());
        old_fact.valid_until = Some("2026-06-01T00:00:00Z".to_string());
        insert_fact(&conn, &old_fact).unwrap();

        let mut new_fact = make_fact("team", "lead", "Bob");
        new_fact.valid_from = Some("2026-06-01T00:00:00Z".to_string());
        insert_fact(&conn, &new_fact).unwrap();

        let at_may = facts_at(&conn, "team", "2026-05-01T00:00:00Z").unwrap();
        assert_eq!(at_may.len(), 1);
        assert_eq!(at_may[0].object, "Alice");

        let at_july = facts_at(&conn, "team", "2026-07-01T00:00:00Z").unwrap();
        assert_eq!(at_july.len(), 1);
        assert_eq!(at_july[0].object, "Bob");
    }
}
