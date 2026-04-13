//! Database integrity checking.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Report from an integrity check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    pub issues: Vec<String>,
}

impl IntegrityReport {
    /// Returns true if no issues were found.
    pub fn is_ok(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Run SQLite `PRAGMA integrity_check` and report any issues.
pub fn check_integrity(conn: &Connection) -> Result<IntegrityReport> {
    let mut stmt = conn
        .prepare("PRAGMA integrity_check")
        .context("failed to prepare integrity check")?;

    let results: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .context("failed to run integrity check")?
        .collect::<Result<Vec<_>, _>>()
        .context("failed to read integrity check results")?;

    let issues: Vec<String> = results.into_iter().filter(|r| r != "ok").collect();

    Ok(IntegrityReport { issues })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::runner::run_migrations;

    #[test]
    fn test_check_integrity_healthy_db() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let report = check_integrity(&conn).unwrap();
        assert!(report.is_ok());
        assert!(report.issues.is_empty());
    }

    #[test]
    fn test_integrity_report_is_ok() {
        let report = IntegrityReport { issues: vec![] };
        assert!(report.is_ok());

        let report = IntegrityReport {
            issues: vec!["corruption found".into()],
        };
        assert!(!report.is_ok());
    }
}
