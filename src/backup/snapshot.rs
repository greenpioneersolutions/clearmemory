//! Backup snapshot creation.
//!
//! Full implementation will use SQLite Online Backup API + LanceDB snapshot +
//! verbatim file hardlinks. This placeholder creates a metadata JSON file.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use std::path::Path;

/// Metadata written into a backup file.
#[derive(Debug, Serialize)]
struct BackupMetadata {
    version: &'static str,
    created_at: String,
    db_path: String,
    verbatim_dir: String,
}

/// Create a backup of the Clear Memory data.
///
/// Currently creates a metadata JSON file at `output_path`. The full
/// implementation will produce a `.cmb` archive containing the SQLite
/// database (via Online Backup API), LanceDB snapshot, and verbatim files.
pub fn create_backup(db_path: &Path, verbatim_dir: &Path, output_path: &Path) -> Result<()> {
    let metadata = BackupMetadata {
        version: "0.1.0",
        created_at: Utc::now().to_rfc3339(),
        db_path: db_path.display().to_string(),
        verbatim_dir: verbatim_dir.display().to_string(),
    };

    let json =
        serde_json::to_string_pretty(&metadata).context("failed to serialize backup metadata")?;

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).context("failed to create backup directory")?;
    }

    std::fs::write(output_path, json).context("failed to write backup file")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_backup_writes_metadata() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("clearmemory.db");
        let verbatim_dir = dir.path().join("verbatim");
        let output_path = dir.path().join("backup.cmb");

        create_backup(&db_path, &verbatim_dir, &output_path).unwrap();

        assert!(output_path.exists());
        let content = std::fs::read_to_string(&output_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["version"], "0.1.0");
    }
}
