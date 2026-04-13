use crate::storage::lance::LanceStorage;
use crate::StorageError;

/// Result from semantic search strategy.
#[derive(Debug, Clone)]
pub struct SemanticResult {
    pub memory_id: String,
    pub score: f64,
}

/// Run semantic similarity search: embed query -> cosine similarity in LanceDB -> top-K.
pub async fn search(
    lance: &LanceStorage,
    query_embedding: &[f32],
    top_k: usize,
    stream_id: Option<&str>,
    include_archived: bool,
) -> Result<Vec<SemanticResult>, StorageError> {
    let results = lance
        .search(query_embedding, top_k, stream_id, include_archived)
        .await?;

    Ok(results
        .into_iter()
        .map(|r| SemanticResult {
            memory_id: r.memory_id,
            score: r.score,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_semantic_search_empty() {
        let dir = tempfile::tempdir().unwrap();
        let lance = LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();
        let query = vec![0.0f32; 384];
        let results = search(&lance, &query, 10, None, false).await.unwrap();
        assert!(results.is_empty());
    }
}
