use std::collections::HashMap;

/// A scored result from any retrieval strategy.
#[derive(Debug, Clone)]
pub struct ScoredResult {
    pub memory_id: String,
    pub score: f64,
    pub strategy: Strategy,
}

/// Which retrieval strategy produced this result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Strategy {
    Semantic,
    Keyword,
    Temporal,
    EntityGraph,
}

/// Merged result after reciprocal rank fusion.
#[derive(Debug, Clone)]
pub struct MergedResult {
    pub memory_id: String,
    pub fused_score: f64,
    pub contributing_strategies: Vec<Strategy>,
}

/// Per-strategy weight multipliers for weighted RRF.
/// Higher weight = more influence on final ranking.
#[derive(Debug, Clone)]
pub struct StrategyWeights {
    pub semantic: f64,
    pub keyword: f64,
    pub temporal: f64,
    pub entity_graph: f64,
}

impl Default for StrategyWeights {
    fn default() -> Self {
        Self {
            semantic: 1.5,    // semantic search is the primary signal
            keyword: 1.0,     // keyword matching is reliable for exact terms
            temporal: 0.8,    // temporal is supplementary
            entity_graph: 0.8, // entity graph is supplementary
        }
    }
}

impl StrategyWeights {
    /// Equal weights (original RRF behavior).
    pub fn equal() -> Self {
        Self {
            semantic: 1.0,
            keyword: 1.0,
            temporal: 1.0,
            entity_graph: 1.0,
        }
    }

    fn weight_for(&self, strategy: &Strategy) -> f64 {
        match strategy {
            Strategy::Semantic => self.semantic,
            Strategy::Keyword => self.keyword,
            Strategy::Temporal => self.temporal,
            Strategy::EntityGraph => self.entity_graph,
        }
    }
}

/// Merge results from multiple strategies using Weighted Reciprocal Rank Fusion.
///
/// Weighted RRF formula: score(d) = sum over strategies of w_i / (k + rank_i(d))
/// where k = 60 is the standard constant and w_i is the per-strategy weight.
///
/// With equal weights (all 1.0), this reduces to standard RRF.
pub fn reciprocal_rank_fusion(
    strategy_results: Vec<Vec<ScoredResult>>,
    k: f64,
) -> Vec<MergedResult> {
    weighted_reciprocal_rank_fusion(strategy_results, k, &StrategyWeights::equal())
}

/// Weighted RRF with configurable per-strategy weights.
pub fn weighted_reciprocal_rank_fusion(
    strategy_results: Vec<Vec<ScoredResult>>,
    k: f64,
    weights: &StrategyWeights,
) -> Vec<MergedResult> {
    let mut scores: HashMap<String, (f64, Vec<Strategy>)> = HashMap::new();

    for results in &strategy_results {
        for (rank, result) in results.iter().enumerate() {
            let weight = weights.weight_for(&result.strategy);
            let rrf_score = weight / (k + rank as f64 + 1.0);
            let entry = scores
                .entry(result.memory_id.clone())
                .or_insert((0.0, Vec::new()));
            entry.0 += rrf_score;
            if !entry.1.contains(&result.strategy) {
                entry.1.push(result.strategy);
            }
        }
    }

    let mut merged: Vec<MergedResult> = scores
        .into_iter()
        .map(|(memory_id, (fused_score, strategies))| MergedResult {
            memory_id,
            fused_score,
            contributing_strategies: strategies,
        })
        .collect();

    // Sort by fused score descending
    merged.sort_by(|a, b| {
        b.fused_score
            .partial_cmp(&a.fused_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_results(strategy: Strategy, ids: &[&str]) -> Vec<ScoredResult> {
        ids.iter()
            .enumerate()
            .map(|(i, id)| ScoredResult {
                memory_id: id.to_string(),
                score: 1.0 - i as f64 * 0.1,
                strategy,
            })
            .collect()
    }

    #[test]
    fn test_rrf_single_strategy() {
        let results = vec![make_results(Strategy::Semantic, &["m1", "m2", "m3"])];
        let merged = reciprocal_rank_fusion(results, 60.0);

        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].memory_id, "m1"); // Highest rank = highest score
        assert!(merged[0].fused_score > merged[1].fused_score);
    }

    #[test]
    fn test_rrf_multi_strategy_boost() {
        let semantic = make_results(Strategy::Semantic, &["m1", "m2", "m3"]);
        let keyword = make_results(Strategy::Keyword, &["m2", "m4", "m1"]);
        let results = vec![semantic, keyword];

        let merged = reciprocal_rank_fusion(results, 60.0);

        // m1 and m2 appear in both strategies, should have higher fused scores
        let m1 = merged.iter().find(|r| r.memory_id == "m1").unwrap();
        let m3 = merged.iter().find(|r| r.memory_id == "m3").unwrap();
        assert!(m1.fused_score > m3.fused_score);

        // m2 appears as rank 1 in both, should be top
        let m2 = merged.iter().find(|r| r.memory_id == "m2").unwrap();
        assert_eq!(m2.contributing_strategies.len(), 2);
    }

    #[test]
    fn test_rrf_empty_input() {
        let merged = reciprocal_rank_fusion(Vec::new(), 60.0);
        assert!(merged.is_empty());
    }

    #[test]
    fn test_rrf_deduplicates() {
        let s1 = make_results(Strategy::Semantic, &["m1", "m1", "m2"]);
        let merged = reciprocal_rank_fusion(vec![s1], 60.0);
        // m1 appears twice in same strategy, both ranks contribute
        let m1 = merged.iter().find(|r| r.memory_id == "m1").unwrap();
        let m2 = merged.iter().find(|r| r.memory_id == "m2").unwrap();
        assert!(m1.fused_score > m2.fused_score);
    }
}
