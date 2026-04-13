use crate::migration::versioning;
use chrono::Utc;
use rusqlite::Connection;
use tracing::{info, warn};
use uuid::Uuid;

/// Run all pending migrations against the database.
pub fn run_migrations(conn: &Connection) -> Result<(), String> {
    // Check compatibility first — refuse if DB is newer than binary
    versioning::check_compatibility(conn)?;

    let current_version = versioning::get_current_version(conn)
        .map_err(|e| format!("failed to read schema version: {e}"))?;

    let target_version = versioning::CURRENT_VERSION;

    if current_version >= target_version {
        info!(current_version, "schema is up to date");
        return Ok(());
    }

    info!(current_version, target_version, "running migrations");

    // Apply each migration in sequence
    for version in (current_version + 1)..=target_version {
        apply_migration(conn, current_version, version)?;
    }

    Ok(())
}

/// Apply a single migration version.
fn apply_migration(conn: &Connection, from_version: i64, to_version: i64) -> Result<(), String> {
    let migration_id = Uuid::new_v4().to_string();
    let started_at = Utc::now().to_rfc3339();

    // Log the migration attempt (if migration_log table exists)
    let _ = log_migration_start(conn, &migration_id, from_version, to_version, &started_at);

    let sql = get_migration_sql(to_version)?;

    // Run migration in a transaction
    match conn.execute_batch(&sql) {
        Ok(()) => {
            let completed_at = Utc::now().to_rfc3339();
            let _ = log_migration_complete(conn, &migration_id, &completed_at, "success", None);
            info!(version = to_version, "migration applied successfully");
            Ok(())
        }
        Err(e) => {
            let error_msg = e.to_string();
            let _ = log_migration_complete(
                conn,
                &migration_id,
                &Utc::now().to_rfc3339(),
                "failed",
                Some(&error_msg),
            );
            warn!(version = to_version, error = %e, "migration failed");
            Err(format!("migration to version {to_version} failed: {e}"))
        }
    }
}

/// Get the SQL content for a specific migration version.
fn get_migration_sql(version: i64) -> Result<String, String> {
    match version {
        1 => Ok(include_str!("../../migrations/001_initial_schema.sql").to_string()),
        _ => Err(format!("unknown migration version: {version}")),
    }
}

fn log_migration_start(
    conn: &Connection,
    id: &str,
    from_version: i64,
    to_version: i64,
    started_at: &str,
) -> Result<(), rusqlite::Error> {
    // migration_log table may not exist yet for the first migration
    conn.execute(
        "INSERT OR IGNORE INTO migration_log (id, from_version, to_version, started_at, status) VALUES (?1, ?2, ?3, ?4, 'in_progress')",
        rusqlite::params![id, from_version, to_version, started_at],
    )?;
    Ok(())
}

fn log_migration_complete(
    conn: &Connection,
    id: &str,
    completed_at: &str,
    status: &str,
    error_message: Option<&str>,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE migration_log SET completed_at = ?1, status = ?2, error_message = ?3 WHERE id = ?4",
        rusqlite::params![completed_at, status, error_message, id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_initial_migration() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        // Verify the schema was created
        let version = versioning::get_current_version(&conn).unwrap();
        assert_eq!(version, 1);

        // Verify key tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"memories".to_string()));
        assert!(tables.contains(&"facts".to_string()));
        assert!(tables.contains(&"entities".to_string()));
        assert!(tables.contains(&"audit_log".to_string()));
        assert!(tables.contains(&"streams".to_string()));
        assert!(tables.contains(&"legal_holds".to_string()));
    }

    #[test]
    fn test_idempotent_migration() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        // Running again should be a no-op
        run_migrations(&conn).unwrap();

        let version = versioning::get_current_version(&conn).unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn test_migration_sql_exists() {
        assert!(get_migration_sql(1).is_ok());
        assert!(get_migration_sql(999).is_err());
    }
}
