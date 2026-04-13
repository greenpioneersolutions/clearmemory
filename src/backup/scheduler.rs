//! Scheduled background backup management.
//!
//! Determines when a backup should run based on the configured interval,
//! and cleans up old backup files beyond the retention count.

use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

/// Configuration for scheduled backups.
#[derive(Debug, Clone)]
pub struct BackupSchedule {
    /// Hours between automatic backups.
    pub interval_hours: u64,
    /// Directory where `.cmb` backup files are stored.
    pub backup_dir: PathBuf,
    /// Maximum number of backup files to retain.
    pub retention_count: u32,
    /// Whether to encrypt backup files.
    pub encrypt: bool,
    /// ISO 8601 timestamp of the last completed backup, if any.
    pub last_backup: Option<String>,
}

/// Check whether a backup should run based on the schedule.
///
/// Returns `true` if no backup has ever run or if enough time has elapsed
/// since the last backup.
pub fn should_run_backup(schedule: &BackupSchedule) -> bool {
    let Some(ref last) = schedule.last_backup else {
        return true;
    };

    let Ok(last_dt) = DateTime::parse_from_rfc3339(last) else {
        // If the timestamp is unparseable, run a backup to be safe
        return true;
    };

    let elapsed = Utc::now().signed_duration_since(last_dt);
    let interval = chrono::Duration::hours(schedule.interval_hours as i64);

    elapsed >= interval
}

/// Remove old `.cmb` backup files beyond the retention count.
///
/// Lists all `.cmb` files in `backup_dir`, sorts by modification time
/// (newest first), and deletes files beyond `retention_count`.
///
/// Returns the number of files deleted.
pub fn cleanup_old_backups(
    backup_dir: &Path,
    retention_count: u32,
) -> Result<usize, std::io::Error> {
    if !backup_dir.exists() {
        return Ok(0);
    }

    let mut cmb_files: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();

    for entry in std::fs::read_dir(backup_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) == Some("cmb") && path.is_file() {
            let mtime = entry.metadata()?.modified()?;
            cmb_files.push((path, mtime));
        }
    }

    // Sort newest first
    cmb_files.sort_by(|a, b| b.1.cmp(&a.1));

    let mut deleted = 0;
    for (path, _) in cmb_files.iter().skip(retention_count as usize) {
        std::fs::remove_file(path)?;
        deleted += 1;
    }

    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    #[test]
    fn test_should_run_backup_no_history() {
        let schedule = BackupSchedule {
            interval_hours: 24,
            backup_dir: PathBuf::from("/tmp/backups"),
            retention_count: 7,
            encrypt: true,
            last_backup: None,
        };
        assert!(should_run_backup(&schedule));
    }

    #[test]
    fn test_should_run_backup_recent() {
        let schedule = BackupSchedule {
            interval_hours: 24,
            backup_dir: PathBuf::from("/tmp/backups"),
            retention_count: 7,
            encrypt: true,
            last_backup: Some(Utc::now().to_rfc3339()),
        };
        assert!(!should_run_backup(&schedule));
    }

    #[test]
    fn test_should_run_backup_overdue() {
        let old = Utc::now() - chrono::Duration::hours(48);
        let schedule = BackupSchedule {
            interval_hours: 24,
            backup_dir: PathBuf::from("/tmp/backups"),
            retention_count: 7,
            encrypt: true,
            last_backup: Some(old.to_rfc3339()),
        };
        assert!(should_run_backup(&schedule));
    }

    #[test]
    fn test_should_run_backup_invalid_timestamp() {
        let schedule = BackupSchedule {
            interval_hours: 24,
            backup_dir: PathBuf::from("/tmp/backups"),
            retention_count: 7,
            encrypt: true,
            last_backup: Some("not-a-date".to_string()),
        };
        assert!(should_run_backup(&schedule));
    }

    #[test]
    fn test_cleanup_old_backups_empty_dir() {
        let dir = TempDir::new().unwrap();
        let deleted = cleanup_old_backups(dir.path(), 3).unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_cleanup_old_backups_within_retention() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("backup1.cmb"), "data1").unwrap();
        std::fs::write(dir.path().join("backup2.cmb"), "data2").unwrap();

        let deleted = cleanup_old_backups(dir.path(), 5).unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_cleanup_old_backups_deletes_excess() {
        let dir = TempDir::new().unwrap();

        // Create 5 backup files
        for i in 0..5 {
            std::fs::write(
                dir.path().join(format!("backup{i}.cmb")),
                format!("data{i}"),
            )
            .unwrap();
        }

        // Also create a non-cmb file that should be ignored
        std::fs::write(dir.path().join("notes.txt"), "not a backup").unwrap();

        let deleted = cleanup_old_backups(dir.path(), 2).unwrap();
        assert_eq!(deleted, 3);

        // Verify only 2 .cmb files remain
        let remaining: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("cmb"))
            .collect();
        assert_eq!(remaining.len(), 2);
    }

    #[test]
    fn test_cleanup_nonexistent_dir() {
        let deleted = cleanup_old_backups(Path::new("/nonexistent/path"), 3).unwrap();
        assert_eq!(deleted, 0);
    }
}
