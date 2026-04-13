use rusqlite::Connection;

/// The current schema version expected by this binary.
pub const CURRENT_VERSION: i64 = 1;

/// Get the current schema version from the database.
/// Returns 0 if the schema_version table doesn't exist yet.
pub fn get_current_version(conn: &Connection) -> Result<i64, rusqlite::Error> {
    // Check if schema_version table exists
    let table_exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='schema_version'",
        [],
        |row| row.get(0),
    )?;

    if !table_exists {
        return Ok(0);
    }

    let version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )?;

    Ok(version)
}

/// Check if the database schema is compatible with this binary version.
/// Returns an error message if the schema is too new (prevents downgrade corruption).
pub fn check_compatibility(conn: &Connection) -> Result<(), String> {
    let db_version =
        get_current_version(conn).map_err(|e| format!("failed to read schema version: {e}"))?;

    if db_version > CURRENT_VERSION {
        return Err(format!(
            "Database schema version ({db_version}) is newer than this binary expects ({CURRENT_VERSION}). \
             Upgrade clearmemory to a newer version or restore from backup."
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_database_returns_version_zero() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(get_current_version(&conn).unwrap(), 0);
    }

    #[test]
    fn test_version_after_table_creation() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL, description TEXT);
             INSERT INTO schema_version VALUES (1, '2026-01-01', 'initial');",
        ).unwrap();
        assert_eq!(get_current_version(&conn).unwrap(), 1);
    }

    #[test]
    fn test_compatibility_check_newer_db() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL, description TEXT);
             INSERT INTO schema_version VALUES (999, '2026-01-01', 'future');",
        ).unwrap();
        assert!(check_compatibility(&conn).is_err());
    }

    #[test]
    fn test_compatibility_check_current() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL, description TEXT);
             INSERT INTO schema_version VALUES (1, '2026-01-01', 'initial');",
        ).unwrap();
        assert!(check_compatibility(&conn).is_ok());
    }
}
