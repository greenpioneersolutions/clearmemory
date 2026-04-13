//! Size-based retention policy.
//!
//! When the corpus exceeds the configured threshold, identify oldest and
//! least-accessed memories for archival.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

/// Calculate the total size of files in the verbatim directory (in bytes).
pub fn calculate_corpus_size(data_dir: &Path) -> Result<u64> {
    let verbatim_dir = data_dir.join("verbatim");

    if !verbatim_dir.exists() {
        return Ok(0);
    }

    let mut total: u64 = 0;
    let entries = std::fs::read_dir(&verbatim_dir).context("failed to read verbatim directory")?;

    for entry in entries {
        let entry = entry.context("failed to read directory entry")?;
        let metadata = entry.metadata().context("failed to read file metadata")?;
        if metadata.is_file() {
            total += metadata.len();
        }
    }

    Ok(total)
}

/// Find archival candidates: oldest, least-accessed active memories.
///
/// Returns up to `limit` memory IDs sorted by staleness (oldest first,
/// then by lowest access count).
pub fn find_archival_candidates(conn: &Connection, limit: u32) -> Result<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT id FROM memories
             WHERE archived = 0
             ORDER BY access_count ASC, created_at ASC
             LIMIT ?1",
        )
        .context("failed to prepare archival candidates query")?;

    let ids = stmt
        .query_map([limit], |row| row.get::<_, String>(0))
        .context("failed to query archival candidates")?
        .collect::<Result<Vec<_>, _>>()
        .context("failed to read archival candidate IDs")?;

    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::runner::run_migrations;
    use tempfile::TempDir;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_calculate_corpus_size_empty() {
        let dir = TempDir::new().unwrap();
        let size = calculate_corpus_size(dir.path()).unwrap();
        assert_eq!(size, 0);
    }

    #[test]
    fn test_calculate_corpus_size_with_files() {
        let dir = TempDir::new().unwrap();
        let verbatim = dir.path().join("verbatim");
        std::fs::create_dir(&verbatim).unwrap();
        std::fs::write(verbatim.join("a.txt"), "hello").unwrap();
        std::fs::write(verbatim.join("b.txt"), "world!!").unwrap();

        let size = calculate_corpus_size(dir.path()).unwrap();
        assert_eq!(size, 12); // 5 + 7
    }

    #[test]
    fn test_find_archival_candidates_empty() {
        let conn = setup_db();
        let candidates = find_archival_candidates(&conn, 10).unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_find_archival_candidates_respects_limit() {
        let conn = setup_db();
        for i in 0..5 {
            conn.execute(
                "INSERT INTO memories (id, content_hash, source_format, created_at, access_count) VALUES (?1, ?2, 'test', datetime('now', ?3), 0)",
                rusqlite::params![format!("m{i}"), format!("h{i}"), format!("-{} days", i * 10)],
            )
            .unwrap();
        }

        let candidates = find_archival_candidates(&conn, 3).unwrap();
        assert_eq!(candidates.len(), 3);
    }

    #[test]
    fn test_find_archival_candidates_excludes_archived() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO memories (id, content_hash, source_format, created_at, archived) VALUES ('m1', 'h1', 'test', datetime('now', '-100 days'), 1)",
            [],
        )
        .unwrap();

        let candidates = find_archival_candidates(&conn, 10).unwrap();
        assert!(candidates.is_empty());
    }
}
