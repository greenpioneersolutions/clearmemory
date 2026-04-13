//! Embedding model reindex support.
//!
//! When the embedding model changes, the entire corpus must be re-embedded.
//! This module provides pausable/resumable reindex state tracking via a
//! JSON state file on disk.

use serde::{Deserialize, Serialize};
use std::path::Path;

const REINDEX_STATE_FILE: &str = "reindex_state.json";

/// Persistent state for a reindex operation (serialized to disk).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReindexState {
    /// Total number of memories to reindex.
    pub total_memories: u64,
    /// Number of memories processed so far.
    pub processed: u64,
    /// ID of the last memory that was processed (for resume).
    pub last_memory_id: Option<String>,
    /// ISO 8601 timestamp when reindexing started.
    pub started_at: String,
    /// Name of the target embedding model.
    pub model_name: String,
}

/// Computed progress information for display.
#[derive(Debug, Clone)]
pub struct ReindexProgress {
    /// Completion percentage (0.0 to 100.0).
    pub percentage: f64,
    /// Number of memories still to process.
    pub memories_remaining: u64,
    /// Estimated seconds remaining based on elapsed time and throughput.
    pub estimated_seconds: Option<u64>,
}

/// Save the current reindex state to a JSON file in `data_dir`.
pub fn save_reindex_state(data_dir: &Path, state: &ReindexState) -> Result<(), std::io::Error> {
    let path = data_dir.join(REINDEX_STATE_FILE);
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

/// Load a previously saved reindex state. Returns `None` if no state file exists.
pub fn load_reindex_state(data_dir: &Path) -> Result<Option<ReindexState>, std::io::Error> {
    let path = data_dir.join(REINDEX_STATE_FILE);

    if !path.exists() {
        return Ok(None);
    }

    let json = std::fs::read_to_string(path)?;
    let state: ReindexState = serde_json::from_str(&json)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    Ok(Some(state))
}

/// Remove the reindex state file (called on completion or cancellation).
pub fn clear_reindex_state(data_dir: &Path) -> Result<(), std::io::Error> {
    let path = data_dir.join(REINDEX_STATE_FILE);

    if path.exists() {
        std::fs::remove_file(path)?;
    }

    Ok(())
}

/// Calculate progress from the current reindex state.
pub fn calculate_progress(state: &ReindexState) -> ReindexProgress {
    let percentage = if state.total_memories == 0 {
        100.0
    } else {
        (state.processed as f64 / state.total_memories as f64) * 100.0
    };

    let memories_remaining = state.total_memories.saturating_sub(state.processed);

    // Estimate remaining time based on elapsed time and throughput
    let estimated_seconds = if state.processed > 0 {
        if let Ok(started) = chrono::DateTime::parse_from_rfc3339(&state.started_at) {
            let elapsed = chrono::Utc::now()
                .signed_duration_since(started)
                .num_seconds();
            if elapsed > 0 {
                let rate = state.processed as f64 / elapsed as f64;
                if rate > 0.0 {
                    Some((memories_remaining as f64 / rate) as u64)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    ReindexProgress {
        percentage,
        memories_remaining,
        estimated_seconds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    fn make_state() -> ReindexState {
        ReindexState {
            total_memories: 1000,
            processed: 250,
            last_memory_id: Some("mem-abc-123".to_string()),
            started_at: Utc::now().to_rfc3339(),
            model_name: "bge-m3".to_string(),
        }
    }

    #[test]
    fn test_save_and_load_state() {
        let dir = TempDir::new().unwrap();
        let state = make_state();

        save_reindex_state(dir.path(), &state).unwrap();
        let loaded = load_reindex_state(dir.path()).unwrap().unwrap();

        assert_eq!(loaded.total_memories, 1000);
        assert_eq!(loaded.processed, 250);
        assert_eq!(loaded.last_memory_id, Some("mem-abc-123".to_string()));
        assert_eq!(loaded.model_name, "bge-m3");
    }

    #[test]
    fn test_load_state_returns_none_when_missing() {
        let dir = TempDir::new().unwrap();
        let result = load_reindex_state(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_clear_state_removes_file() {
        let dir = TempDir::new().unwrap();
        let state = make_state();
        save_reindex_state(dir.path(), &state).unwrap();

        assert!(dir.path().join("reindex_state.json").exists());
        clear_reindex_state(dir.path()).unwrap();
        assert!(!dir.path().join("reindex_state.json").exists());
    }

    #[test]
    fn test_clear_state_no_error_when_missing() {
        let dir = TempDir::new().unwrap();
        // Should not error if file doesn't exist
        clear_reindex_state(dir.path()).unwrap();
    }

    #[test]
    fn test_calculate_progress_basic() {
        let state = ReindexState {
            total_memories: 1000,
            processed: 500,
            last_memory_id: None,
            started_at: Utc::now().to_rfc3339(),
            model_name: "bge-m3".to_string(),
        };

        let progress = calculate_progress(&state);
        assert!((progress.percentage - 50.0).abs() < f64::EPSILON);
        assert_eq!(progress.memories_remaining, 500);
    }

    #[test]
    fn test_calculate_progress_zero_total() {
        let state = ReindexState {
            total_memories: 0,
            processed: 0,
            last_memory_id: None,
            started_at: Utc::now().to_rfc3339(),
            model_name: "bge-m3".to_string(),
        };

        let progress = calculate_progress(&state);
        assert!((progress.percentage - 100.0).abs() < f64::EPSILON);
        assert_eq!(progress.memories_remaining, 0);
    }
}
