use rusqlite::{params, Connection};

/// Add an alias for an entity.
pub fn add_alias(conn: &Connection, alias: &str, entity_id: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT OR IGNORE INTO entity_aliases (alias, entity_id) VALUES (?1, ?2)",
        params![alias, entity_id],
    )?;
    Ok(())
}

/// Find an entity ID by alias (case-insensitive).
pub fn find_by_alias(conn: &Connection, alias: &str) -> Result<Option<String>, rusqlite::Error> {
    let result = conn.query_row(
        "SELECT entity_id FROM entity_aliases WHERE LOWER(alias) = LOWER(?1) LIMIT 1",
        params![alias],
        |row| row.get(0),
    );

    match result {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Get all aliases for an entity.
pub fn get_aliases(conn: &Connection, entity_id: &str) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt =
        conn.prepare("SELECT alias FROM entity_aliases WHERE entity_id = ?1 ORDER BY alias")?;
    let rows = stmt.query_map(params![entity_id], |row| row.get(0))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Remove an alias.
pub fn remove_alias(
    conn: &Connection,
    alias: &str,
    entity_id: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM entity_aliases WHERE alias = ?1 AND entity_id = ?2",
        params![alias, entity_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migration::runner::run_migrations(&conn).unwrap();
        // Insert a test entity
        conn.execute(
            "INSERT INTO entities (id, canonical_name, entity_type, first_seen, last_seen) \
             VALUES ('e1', 'Auth Service', 'service', '2026-01-01', '2026-04-01')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_add_and_find_alias() {
        let conn = setup_db();
        add_alias(&conn, "auth-service", "e1").unwrap();
        add_alias(&conn, "the auth system", "e1").unwrap();

        let found = find_by_alias(&conn, "auth-service").unwrap();
        assert_eq!(found, Some("e1".to_string()));
    }

    #[test]
    fn test_case_insensitive_lookup() {
        let conn = setup_db();
        add_alias(&conn, "Auth-Service", "e1").unwrap();

        let found = find_by_alias(&conn, "auth-service").unwrap();
        assert_eq!(found, Some("e1".to_string()));
    }

    #[test]
    fn test_find_nonexistent_alias() {
        let conn = setup_db();
        let found = find_by_alias(&conn, "nonexistent").unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn test_get_aliases() {
        let conn = setup_db();
        add_alias(&conn, "alias-a", "e1").unwrap();
        add_alias(&conn, "alias-b", "e1").unwrap();

        let aliases = get_aliases(&conn, "e1").unwrap();
        assert_eq!(aliases.len(), 2);
    }
}
