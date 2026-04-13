//! Curator model interface for filtering retrieval results before context injection.
//!
//! The actual Qwen3-0.6B candle inference is deferred until the candle dependency
//! is integrated. This module provides the trait interface and a noop implementation
//! for Tier 1 (fully offline) behavior.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// An excerpt from a retrieved memory, before curation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryExcerpt {
    pub memory_id: String,
    pub content: String,
    pub relevance_score: f64,
}

/// A curated excerpt — the portion of a memory deemed relevant to the query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuratedExcerpt {
    pub memory_id: String,
    pub content: String,
    pub relevance_score: f64,
}

/// Trait for curator models that filter and extract relevant portions of memories.
pub trait CuratorModel: Send + Sync {
    /// Given a query and a set of retrieved memory excerpts, return only the
    /// relevant portions. Implementations may trim, reorder, or filter excerpts.
    fn curate(&self, query: &str, memories: &[MemoryExcerpt]) -> Result<Vec<CuratedExcerpt>>;
}

/// Noop curator that passes through all excerpts unchanged (Tier 1 behavior).
///
/// When no LLM is available, retrieval results go directly to the context
/// compiler with their original content and scores.
#[derive(Debug, Default)]
pub struct NoopCurator;

impl CuratorModel for NoopCurator {
    fn curate(&self, _query: &str, memories: &[MemoryExcerpt]) -> Result<Vec<CuratedExcerpt>> {
        Ok(memories
            .iter()
            .map(|m| CuratedExcerpt {
                memory_id: m.memory_id.clone(),
                content: m.content.clone(),
                relevance_score: m.relevance_score,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_curator_preserves_input() {
        let curator = NoopCurator;
        let excerpts = vec![
            MemoryExcerpt {
                memory_id: "mem-1".into(),
                content: "We decided to use GraphQL".into(),
                relevance_score: 0.95,
            },
            MemoryExcerpt {
                memory_id: "mem-2".into(),
                content: "Auth migration plan discussed".into(),
                relevance_score: 0.82,
            },
        ];

        let result = curator.curate("GraphQL decision", &excerpts).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].memory_id, "mem-1");
        assert_eq!(result[0].content, "We decided to use GraphQL");
        assert!((result[0].relevance_score - 0.95).abs() < f64::EPSILON);
        assert_eq!(result[1].memory_id, "mem-2");
        assert_eq!(result[1].content, "Auth migration plan discussed");
    }

    #[test]
    fn test_noop_curator_handles_empty_input() {
        let curator = NoopCurator;
        let result = curator.curate("anything", &[]).unwrap();
        assert!(result.is_empty());
    }
}
