//! Index rebuild — re-embed all memories from SQLite + verbatim files.
//!
//! When the LanceDB integration is fully connected, this module will:
//! 1. Read all active memories from SQLite
//! 2. Load each memory's verbatim content from disk
//! 3. Re-embed with the configured embedding model (BGE-M3 or bge-small)
//! 4. Write new vectors to a fresh LanceDB collection
//! 5. Atomically swap the old index for the new one
//!
//! The operation is pausable/resumable: progress is tracked in SQLite so
//! an interrupted reindex can continue from where it left off.

use anyhow::Result;

/// Rebuild the LanceDB vector index from SQLite metadata and verbatim files.
///
/// This is a placeholder stub. The full implementation requires the LanceDB
/// and fastembed crate integrations to be wired up.
pub fn rebuild_index() -> Result<()> {
    tracing::info!("rebuild_index: stub — LanceDB reindex not yet implemented");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rebuild_index_stub_succeeds() {
        // The stub should return Ok without error
        rebuild_index().unwrap();
    }
}
