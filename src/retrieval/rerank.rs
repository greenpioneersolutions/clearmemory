use crate::retrieval::merge::MergedResult;

/// A reranked result with cross-encoder score.
#[derive(Debug, Clone)]
pub struct RerankedResult {
    pub memory_id: String,
    pub rerank_score: f64,
    pub original_fused_score: f64,
}

/// Trait for reranking results with a cross-encoder model.
pub trait Reranker: Send + Sync {
    fn rerank(
        &self,
        query: &str,
        candidates: &[(String, String)], // (memory_id, summary_text)
    ) -> Result<Vec<(String, f64)>, String>;
}

/// Placeholder reranker that preserves the existing fusion ranking.
/// Used when the BGE-Reranker-Base model is not available (Tier 1 without model).
pub struct PassthroughReranker;

impl Reranker for PassthroughReranker {
    fn rerank(
        &self,
        _query: &str,
        candidates: &[(String, String)],
    ) -> Result<Vec<(String, f64)>, String> {
        Ok(candidates
            .iter()
            .enumerate()
            .map(|(i, (id, _))| {
                (id.clone(), 1.0 - i as f64 * 0.01) // Preserve order with decreasing score
            })
            .collect())
    }
}

/// Real cross-encoder reranker using BGE-Reranker-Base via fastembed.
///
/// Scores each (query, document) pair independently using a cross-encoder architecture.
/// This catches semantically-similar but non-answering results that bi-encoder search misses.
/// Model: BAAI/bge-reranker-base (~400MB, downloaded on first use).
pub struct FastembedReranker {
    model: std::sync::Mutex<fastembed::TextRerank>,
}

// fastembed's TextRerank uses ONNX Runtime which is thread-safe for inference.
unsafe impl Send for FastembedReranker {}
unsafe impl Sync for FastembedReranker {}

impl FastembedReranker {
    /// Load the BGE-Reranker-Base model.
    pub fn new() -> Result<Self, String> {
        use fastembed::{RerankInitOptions, RerankerModel, TextRerank};

        let model = TextRerank::try_new(
            RerankInitOptions::new(RerankerModel::BGERerankerBase)
                .with_show_download_progress(true),
        )
        .map_err(|e| format!("failed to load reranker model: {e}"))?;

        Ok(Self {
            model: std::sync::Mutex::new(model),
        })
    }
}

impl Reranker for FastembedReranker {
    fn rerank(
        &self,
        query: &str,
        candidates: &[(String, String)],
    ) -> Result<Vec<(String, f64)>, String> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let documents: Vec<&str> = candidates.iter().map(|(_, text)| text.as_str()).collect();

        let mut model = self
            .model
            .lock()
            .map_err(|e| format!("reranker lock poisoned: {e}"))?;

        let results = model
            .rerank(query, &documents, false, None)
            .map_err(|e| format!("rerank failed: {e}"))?;

        // Map fastembed results back to memory IDs with scores
        let mut scored: Vec<(String, f64)> = results
            .iter()
            .map(|r| {
                let memory_id = candidates[r.index].0.clone();
                (memory_id, r.score as f64)
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(scored)
    }
}

/// Rerank merged results using a cross-encoder.
pub fn rerank_results(
    reranker: &dyn Reranker,
    query: &str,
    merged: &[MergedResult],
    summaries: &std::collections::HashMap<String, String>, // memory_id -> summary
    top_k: usize,
) -> Result<Vec<RerankedResult>, String> {
    // Build candidate list: (memory_id, summary_text)
    let candidates: Vec<(String, String)> = merged
        .iter()
        .filter_map(|m| {
            let summary = summaries.get(&m.memory_id)?.clone();
            Some((m.memory_id.clone(), summary))
        })
        .collect();

    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let reranked = reranker.rerank(query, &candidates)?;

    let mut results: Vec<RerankedResult> = reranked
        .into_iter()
        .filter_map(|(memory_id, rerank_score)| {
            let original = merged.iter().find(|m| m.memory_id == memory_id)?;
            Some(RerankedResult {
                memory_id,
                rerank_score,
                original_fused_score: original.fused_score,
            })
        })
        .collect();

    // Sort by rerank score descending
    results.sort_by(|a, b| {
        b.rerank_score
            .partial_cmp(&a.rerank_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(top_k);

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retrieval::merge::{MergedResult, Strategy};
    use std::collections::HashMap;

    #[test]
    fn test_passthrough_reranker() {
        let reranker = PassthroughReranker;
        let candidates = vec![
            ("m1".to_string(), "first memory".to_string()),
            ("m2".to_string(), "second memory".to_string()),
        ];
        let result = reranker.rerank("query", &candidates).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "m1");
        assert!(result[0].1 > result[1].1);
    }

    #[test]
    fn test_rerank_results() {
        let reranker = PassthroughReranker;
        let merged = vec![
            MergedResult {
                memory_id: "m1".to_string(),
                fused_score: 0.8,
                contributing_strategies: vec![Strategy::Semantic],
            },
            MergedResult {
                memory_id: "m2".to_string(),
                fused_score: 0.6,
                contributing_strategies: vec![Strategy::Keyword],
            },
        ];
        let mut summaries = HashMap::new();
        summaries.insert("m1".to_string(), "first".to_string());
        summaries.insert("m2".to_string(), "second".to_string());

        let results = rerank_results(&reranker, "query", &merged, &summaries, 10).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_rerank_empty() {
        let reranker = PassthroughReranker;
        let results = rerank_results(&reranker, "query", &[], &HashMap::new(), 10).unwrap();
        assert!(results.is_empty());
    }
}
