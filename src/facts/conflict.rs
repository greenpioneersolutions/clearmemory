use crate::facts::temporal::Fact;
use chrono::Utc;
use rusqlite::{params, Connection};

/// A detected conflict between two facts.
#[derive(Debug)]
pub struct Conflict {
    pub existing_fact_id: String,
    pub new_fact: Fact,
    pub reason: String,
}

/// Check for conflicts between a new fact and existing facts.
/// Same subject + same predicate + different object = conflict.
pub fn detect_conflicts(
    conn: &Connection,
    new_fact: &Fact,
) -> Result<Vec<Conflict>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, memory_id, subject, predicate, object, valid_from, valid_until, \
         ingested_at, invalidated_at, confidence FROM facts \
         WHERE LOWER(subject) = LOWER(?1) AND LOWER(predicate) = LOWER(?2) \
         AND valid_until IS NULL AND invalidated_at IS NULL",
    )?;

    let rows = stmt.query_map(params![new_fact.subject, new_fact.predicate], |row| {
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
    })?;

    let mut conflicts = Vec::new();
    for existing in rows.flatten() {
        if existing.object.to_lowercase() != new_fact.object.to_lowercase() {
            conflicts.push(Conflict {
                existing_fact_id: existing.id.clone(),
                new_fact: new_fact.clone(),
                reason: format!(
                    "{} {} '{}' conflicts with existing '{}'",
                    new_fact.subject, new_fact.predicate, new_fact.object, existing.object
                ),
            });
        }
    }

    Ok(conflicts)
}

/// Resolve conflicts by invalidating older facts (Tier 1: timestamp-based).
pub fn resolve_conflicts(
    conn: &Connection,
    conflicts: &[Conflict],
) -> Result<usize, rusqlite::Error> {
    let now = Utc::now().to_rfc3339();
    let mut resolved = 0;

    for conflict in conflicts {
        conn.execute(
            "UPDATE facts SET valid_until = ?1, invalidated_at = ?1 WHERE id = ?2",
            params![now, conflict.existing_fact_id],
        )?;
        resolved += 1;
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::temporal::{insert_fact, Fact};
    use crate::migration;
    use uuid::Uuid;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migration::runner::run_migrations(&conn).unwrap();
        conn.execute(
            "INSERT INTO memories (id, content_hash, source_format, created_at) \
             VALUES ('mem1', 'h1', 'clear', '2026-01-01')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_detect_conflict() {
        let conn = setup_db();

        let old = Fact {
            id: Uuid::new_v4().to_string(),
            memory_id: "mem1".to_string(),
            subject: "auth".to_string(),
            predicate: "uses".to_string(),
            object: "Auth0".to_string(),
            valid_from: Some("2026-01-01".to_string()),
            valid_until: None,
            ingested_at: "2026-01-01".to_string(),
            invalidated_at: None,
            confidence: 1.0,
        };
        insert_fact(&conn, &old).unwrap();

        let new = Fact {
            id: Uuid::new_v4().to_string(),
            memory_id: "mem1".to_string(),
            subject: "auth".to_string(),
            predicate: "uses".to_string(),
            object: "Clerk".to_string(),
            valid_from: Some("2026-04-01".to_string()),
            valid_until: None,
            ingested_at: "2026-04-01".to_string(),
            invalidated_at: None,
            confidence: 1.0,
        };

        let conflicts = detect_conflicts(&conn, &new).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].reason.contains("Auth0"));
    }

    #[test]
    fn test_resolve_conflicts() {
        let conn = setup_db();

        let old = Fact {
            id: Uuid::new_v4().to_string(),
            memory_id: "mem1".to_string(),
            subject: "db".to_string(),
            predicate: "uses".to_string(),
            object: "MySQL".to_string(),
            valid_from: None,
            valid_until: None,
            ingested_at: "2026-01-01".to_string(),
            invalidated_at: None,
            confidence: 1.0,
        };
        insert_fact(&conn, &old).unwrap();

        let new = Fact {
            id: Uuid::new_v4().to_string(),
            memory_id: "mem1".to_string(),
            subject: "db".to_string(),
            predicate: "uses".to_string(),
            object: "PostgreSQL".to_string(),
            valid_from: None,
            valid_until: None,
            ingested_at: "2026-04-01".to_string(),
            invalidated_at: None,
            confidence: 1.0,
        };

        let conflicts = detect_conflicts(&conn, &new).unwrap();
        let resolved = resolve_conflicts(&conn, &conflicts).unwrap();
        assert_eq!(resolved, 1);

        // Old fact should now be invalidated
        let conflicts_after = detect_conflicts(&conn, &new).unwrap();
        assert!(conflicts_after.is_empty());
    }

    #[test]
    fn test_no_conflict_same_object() {
        let conn = setup_db();

        let existing = Fact {
            id: Uuid::new_v4().to_string(),
            memory_id: "mem1".to_string(),
            subject: "cache".to_string(),
            predicate: "uses".to_string(),
            object: "Redis".to_string(),
            valid_from: None,
            valid_until: None,
            ingested_at: "2026-01-01".to_string(),
            invalidated_at: None,
            confidence: 1.0,
        };
        insert_fact(&conn, &existing).unwrap();

        let same = Fact {
            id: Uuid::new_v4().to_string(),
            memory_id: "mem1".to_string(),
            subject: "cache".to_string(),
            predicate: "uses".to_string(),
            object: "Redis".to_string(),
            valid_from: None,
            valid_until: None,
            ingested_at: "2026-04-01".to_string(),
            invalidated_at: None,
            confidence: 1.0,
        };

        let conflicts = detect_conflicts(&conn, &same).unwrap();
        assert!(conflicts.is_empty());
    }
}
