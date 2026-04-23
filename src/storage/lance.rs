use crate::StorageError;
use arrow_array::{Array, ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use lancedb::query::{ExecutableQuery, QueryBase};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

/// A vector search result from LanceDB.
#[derive(Debug, Clone)]
pub struct VectorSearchResult {
    pub memory_id: String,
    pub score: f64,
}

/// The number of dimensions for the default embedding model (bge-small-en-v1.5).
/// BGE-M3 uses 1024; we use 384 for benchmarks with bge-small-en.
pub const DEFAULT_VECTOR_DIM: i32 = 384;

/// LanceDB vector storage for semantic search.
///
/// Stores dense vectors for memory content and supports approximate nearest
/// neighbor search with optional stream filtering.
#[derive(Clone)]
pub struct LanceStorage {
    vectors_dir: PathBuf,
    db: lancedb::Connection,
    vector_dim: i32,
}

impl LanceStorage {
    /// Open or create the LanceDB storage at the given directory.
    pub async fn open(vectors_dir: PathBuf) -> Result<Self, StorageError> {
        Self::open_with_dim(vectors_dir, DEFAULT_VECTOR_DIM).await
    }

    /// Open or create the LanceDB storage with a specific vector dimension.
    pub async fn open_with_dim(
        vectors_dir: PathBuf,
        vector_dim: i32,
    ) -> Result<Self, StorageError> {
        if !vectors_dir.exists() {
            std::fs::create_dir_all(&vectors_dir).map_err(StorageError::VerbatimIo)?;
        }

        let db = lancedb::connect(vectors_dir.to_str().unwrap_or("."))
            .execute()
            .await
            .map_err(|e| {
                StorageError::VectorStorage(format!("failed to connect to LanceDB: {e}"))
            })?;

        info!(path = %vectors_dir.display(), dim = vector_dim, "lance storage opened");

        Ok(Self {
            vectors_dir,
            db,
            vector_dim,
        })
    }

    /// Build the Arrow schema for the memories table.
    fn schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("memory_id", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    self.vector_dim,
                ),
                false,
            ),
            Field::new("stream_id", DataType::Utf8, true),
        ]))
    }

    /// Create a RecordBatch from a single memory's data.
    fn make_batch(
        &self,
        memory_id: &str,
        dense_vector: &[f32],
        stream_id: Option<&str>,
    ) -> Result<RecordBatch, StorageError> {
        let memory_ids = Arc::new(StringArray::from(vec![memory_id])) as ArrayRef;

        let float_arr = Arc::new(Float32Array::from(dense_vector.to_vec())) as ArrayRef;
        let field = Arc::new(Field::new("item", DataType::Float32, true));
        let vectors = Arc::new(FixedSizeListArray::new(
            field,
            self.vector_dim,
            float_arr,
            None,
        )) as ArrayRef;

        let stream_ids = Arc::new(StringArray::from(vec![stream_id])) as ArrayRef;

        RecordBatch::try_new(self.schema(), vec![memory_ids, vectors, stream_ids])
            .map_err(|e| StorageError::VectorStorage(format!("failed to create record batch: {e}")))
    }

    /// Insert a vector for a memory. Creates the table on first insert.
    pub async fn insert(
        &self,
        memory_id: &str,
        dense_vector: &[f32],
        stream_id: Option<&str>,
    ) -> Result<(), StorageError> {
        if dense_vector.is_empty() {
            return Ok(());
        }

        if dense_vector.len() != self.vector_dim as usize {
            return Err(StorageError::VectorStorage(format!(
                "vector dimension mismatch: expected {}, got {}",
                self.vector_dim,
                dense_vector.len()
            )));
        }

        let batch = self.make_batch(memory_id, dense_vector, stream_id)?;

        // Try to open existing table; if it doesn't exist, create it
        match self.db.open_table("memories").execute().await {
            Ok(table) => {
                table.add(vec![batch]).execute().await.map_err(|e| {
                    StorageError::VectorStorage(format!("failed to add to table: {e}"))
                })?;
            }
            Err(_) => {
                self.db
                    .create_table("memories", vec![batch])
                    .execute()
                    .await
                    .map_err(|e| {
                        StorageError::VectorStorage(format!("failed to create table: {e}"))
                    })?;
                info!("created LanceDB memories table");
            }
        }

        Ok(())
    }

    /// Search for similar vectors. Returns results sorted by similarity (highest first).
    pub async fn search(
        &self,
        query_vector: &[f32],
        top_k: usize,
        stream_id: Option<&str>,
        _include_archived: bool,
    ) -> Result<Vec<VectorSearchResult>, StorageError> {
        if query_vector.is_empty() {
            return Ok(Vec::new());
        }

        let table = match self.db.open_table("memories").execute().await {
            Ok(t) => t,
            Err(_) => {
                // Table doesn't exist yet -- no memories stored
                return Ok(Vec::new());
            }
        };

        let query_vec: Vec<f32> = query_vector.to_vec();

        let vector_query = table
            .vector_search(query_vec)
            .map_err(|e| StorageError::VectorStorage(format!("search setup failed: {e}")))?;

        // Use cosine distance — BGE models are trained with cosine similarity objective.
        // Cosine distance range: [0, 2] where 0 = identical vectors.
        let mut vector_query = vector_query
            .distance_type(lancedb::DistanceType::Cosine)
            .limit(top_k);

        if let Some(sid) = stream_id {
            vector_query = vector_query.only_if(format!("stream_id = '{sid}'"));
        }

        let results = vector_query
            .execute()
            .await
            .map_err(|e| StorageError::VectorStorage(format!("search execution failed: {e}")))?;

        // Collect results from the stream
        use futures::TryStreamExt;
        let batches: Vec<RecordBatch> = results
            .try_collect()
            .await
            .map_err(|e| StorageError::VectorStorage(format!("failed to collect results: {e}")))?;

        let mut search_results = Vec::new();

        for batch in &batches {
            let id_col = batch
                .column_by_name("memory_id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            let distance_col = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

            if let (Some(ids), Some(distances)) = (id_col, distance_col) {
                for i in 0..ids.len() {
                    if ids.is_null(i) {
                        continue;
                    }
                    let memory_id = ids.value(i).to_string();
                    let distance = distances.value(i) as f64;
                    // Cosine distance [0, 2] → similarity [0, 1]
                    let score = 1.0 - (distance / 2.0);
                    search_results.push(VectorSearchResult { memory_id, score });
                }
            }
        }

        // Sort by score descending (highest similarity first)
        search_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(search_results)
    }

    /// Delete vectors for a memory.
    pub async fn delete(&self, memory_id: &str) -> Result<(), StorageError> {
        let table = match self.db.open_table("memories").execute().await {
            Ok(t) => t,
            Err(_) => return Ok(()), // Table doesn't exist, nothing to delete
        };

        table
            .delete(&format!("memory_id = '{memory_id}'"))
            .await
            .map_err(|e| StorageError::VectorStorage(format!("delete failed: {e}")))?;

        Ok(())
    }

    /// Get the number of vectors stored.
    pub async fn vector_count(&self) -> Result<usize, StorageError> {
        let table = match self.db.open_table("memories").execute().await {
            Ok(t) => t,
            Err(_) => return Ok(0),
        };

        let count = table
            .count_rows(None)
            .await
            .map_err(|e| StorageError::VectorStorage(format!("count failed: {e}")))?;

        Ok(count)
    }

    /// Return the storage directory path.
    pub fn path(&self) -> &PathBuf {
        &self.vectors_dir
    }

    /// Return the vector dimension.
    pub fn vector_dim(&self) -> i32 {
        self.vector_dim
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lance_opens() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();
        assert_eq!(storage.vector_count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_lance_search_empty() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();
        let results = storage.search(&[0.0; 384], 10, None, false).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_lance_insert_and_count() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();

        let vector: Vec<f32> = (0..384).map(|i| i as f32 / 384.0).collect();
        storage
            .insert("mem1", &vector, Some("stream1"))
            .await
            .unwrap();

        assert_eq!(storage.vector_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_lance_insert_and_search() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();

        // Insert two vectors that are different
        let v1: Vec<f32> = (0..384).map(|i| (i as f32).sin()).collect();
        let v2: Vec<f32> = (0..384).map(|i| (i as f32).cos()).collect();

        storage.insert("mem1", &v1, None).await.unwrap();
        storage.insert("mem2", &v2, None).await.unwrap();

        // Search with v1 -- should find mem1 as most similar
        let results = storage.search(&v1, 10, None, false).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].memory_id, "mem1");
    }

    #[tokio::test]
    async fn test_lance_delete() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();

        let vector: Vec<f32> = (0..384).map(|i| i as f32 / 384.0).collect();
        storage.insert("mem1", &vector, None).await.unwrap();
        assert_eq!(storage.vector_count().await.unwrap(), 1);

        storage.delete("mem1").await.unwrap();
        assert_eq!(storage.vector_count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_lance_empty_vector_noop() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();

        // Empty vector should be a no-op
        storage.insert("mem1", &[], None).await.unwrap();
        assert_eq!(storage.vector_count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_lance_dimension_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();

        // Wrong dimension should error
        let wrong_dim: Vec<f32> = vec![1.0; 128];
        let result = storage.insert("mem1", &wrong_dim, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lance_stream_filter() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LanceStorage::open(dir.path().join("vectors"))
            .await
            .unwrap();

        let v1: Vec<f32> = (0..384).map(|i| (i as f32).sin()).collect();
        let v2: Vec<f32> = (0..384).map(|i| (i as f32).cos()).collect();

        storage.insert("mem1", &v1, Some("stream-a")).await.unwrap();
        storage.insert("mem2", &v2, Some("stream-b")).await.unwrap();

        // Search with stream filter -- should only find mem1
        let results = storage
            .search(&v1, 10, Some("stream-a"), false)
            .await
            .unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().all(|r| r.memory_id == "mem1"));
    }
}
