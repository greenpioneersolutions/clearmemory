use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::Mutex;
use tracing::{info, warn};

/// Manages embedding model loading and inference.
///
/// Uses fastembed-rs with ONNX Runtime backend. Default model is BGE-Small-EN-v1.5
/// (384 dimensions, ~50MB) for fast benchmarking. Production deployments should
/// use BGE-M3 (1024 dimensions) for multilingual support and higher quality.
pub struct EmbeddingManager {
    model: Mutex<TextEmbedding>,
    dimensions: usize,
    model_name: String,
}

// Safety: TextEmbedding uses ONNX Runtime which is thread-safe for inference.
// The Mutex ensures only one inference runs at a time.
unsafe impl Send for EmbeddingManager {}
unsafe impl Sync for EmbeddingManager {}

impl EmbeddingManager {
    /// Load an embedding model by name.
    ///
    /// Supported models:
    /// - "bge-small-en" -> BGESmallENV15 (384-dim, ~50MB, English-only, fast)
    /// - "bge-base-en" -> BGEBaseENV15 (768-dim, ~130MB, English-only)
    /// - "bge-large-en" -> BGELargeENV15 (1024-dim, ~335MB, English-only, high quality)
    /// - "bge-m3" -> BGEM3 (1024-dim, ~600MB, 100+ languages, production default)
    ///
    /// The model is downloaded on first use and cached locally.
    pub fn new(model_name: &str) -> Result<Self, anyhow::Error> {
        let (embedding_model, dimensions) = match model_name {
            "bge-small-en" | "bge-small-en-v1.5" => (EmbeddingModel::BGESmallENV15, 384),
            "bge-base-en" | "bge-base-en-v1.5" => (EmbeddingModel::BGEBaseENV15, 768),
            "bge-large-en" | "bge-large-en-v1.5" => (EmbeddingModel::BGELargeENV15, 1024),
            "bge-m3" => (EmbeddingModel::BGEM3, 1024),
            _ => {
                warn!(
                    model = model_name,
                    "unknown model, falling back to bge-small-en-v1.5"
                );
                (EmbeddingModel::BGESmallENV15, 384)
            }
        };

        info!(
            model = model_name,
            dim = dimensions,
            "loading embedding model"
        );

        let model = TextEmbedding::try_new(
            InitOptions::new(embedding_model).with_show_download_progress(true),
        )?;

        info!(model = model_name, "embedding model loaded");

        Ok(Self {
            model: Mutex::new(model),
            dimensions,
            model_name: model_name.to_string(),
        })
    }

    /// Embed multiple documents in a batch.
    ///
    /// Returns one vector per input document, each of length `dimensions()`.
    pub fn embed_documents(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, anyhow::Error> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let docs: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
        let mut model = self
            .model
            .lock()
            .map_err(|e| anyhow::anyhow!("model lock poisoned: {e}"))?;
        let embeddings = model.embed(docs, None)?;
        Ok(embeddings)
    }

    /// Embed a single query string.
    pub fn embed_query(&self, text: &str) -> Result<Vec<f32>, anyhow::Error> {
        let mut model = self
            .model
            .lock()
            .map_err(|e| anyhow::anyhow!("model lock poisoned: {e}"))?;
        let embeddings = model.embed(vec![text.to_string()], None)?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("embedding returned no results"))
    }

    /// Return the vector dimension for this model.
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Return the model name.
    pub fn model_name(&self) -> &str {
        &self.model_name
    }
}

/// Sparse embedding output: a list of (token_index, weight) pairs.
#[derive(Debug, Clone)]
pub struct SparseEmbedding {
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}

/// Manages sparse embedding generation via BGE-M3's SPLADE output.
///
/// Sparse embeddings capture exact term importance — complementary to dense
/// vectors for keyword-heavy queries like error codes, config values, proper nouns.
pub struct SparseEmbeddingManager {
    model: Mutex<fastembed::SparseTextEmbedding>,
}

unsafe impl Send for SparseEmbeddingManager {}
unsafe impl Sync for SparseEmbeddingManager {}

impl SparseEmbeddingManager {
    /// Load the BGE-M3 sparse model.
    pub fn new() -> Result<Self, anyhow::Error> {
        use fastembed::{SparseInitOptions, SparseModel, SparseTextEmbedding};

        info!("loading sparse embedding model (BGE-M3 SPLADE)");
        let model = SparseTextEmbedding::try_new(
            SparseInitOptions::new(SparseModel::BGEM3).with_show_download_progress(true),
        )?;
        info!("sparse embedding model loaded");

        Ok(Self {
            model: Mutex::new(model),
        })
    }

    /// Generate sparse embeddings for documents.
    pub fn embed_documents(&self, texts: &[&str]) -> Result<Vec<SparseEmbedding>, anyhow::Error> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let docs: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
        let mut model = self
            .model
            .lock()
            .map_err(|e| anyhow::anyhow!("sparse model lock poisoned: {e}"))?;

        let results = model.embed(docs, None)?;

        Ok(results
            .into_iter()
            .map(|r| SparseEmbedding {
                indices: r.indices.into_iter().map(|i| i as u32).collect(),
                values: r.values,
            })
            .collect())
    }

    /// Generate sparse embedding for a single query.
    pub fn embed_query(&self, text: &str) -> Result<SparseEmbedding, anyhow::Error> {
        let results = self.embed_documents(&[text])?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("sparse embedding returned no results"))
    }

    /// Compute sparse dot-product similarity between a query and a document embedding.
    pub fn sparse_similarity(query: &SparseEmbedding, doc: &SparseEmbedding) -> f32 {
        let mut score = 0.0f32;
        let mut qi = 0;
        let mut di = 0;

        // Both index arrays are sorted — merge-join for efficiency
        while qi < query.indices.len() && di < doc.indices.len() {
            match query.indices[qi].cmp(&doc.indices[di]) {
                std::cmp::Ordering::Equal => {
                    score += query.values[qi] * doc.values[di];
                    qi += 1;
                    di += 1;
                }
                std::cmp::Ordering::Less => qi += 1,
                std::cmp::Ordering::Greater => di += 1,
            }
        }

        score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that the embedding model loads and produces correct dimensions.
    /// This test requires model download (~50MB) so is marked #[ignore].
    #[test]
    #[ignore]
    fn test_embedding_dimensions() {
        let manager = EmbeddingManager::new("bge-small-en").unwrap();
        assert_eq!(manager.dimensions(), 384);

        let vector = manager.embed_query("test document").unwrap();
        assert_eq!(vector.len(), 384);
    }

    /// Test that similar documents produce similar embeddings.
    #[test]
    #[ignore]
    fn test_embedding_similarity() {
        let manager = EmbeddingManager::new("bge-small-en").unwrap();

        let v1 = manager
            .embed_query("authentication and login security")
            .unwrap();
        let v2 = manager
            .embed_query("user login and auth mechanisms")
            .unwrap();
        let v3 = manager
            .embed_query("database performance tuning for PostgreSQL")
            .unwrap();

        let sim_related = cosine_similarity(&v1, &v2);
        let sim_unrelated = cosine_similarity(&v1, &v3);

        println!("similar topics cosine: {sim_related:.4}");
        println!("unrelated topics cosine: {sim_unrelated:.4}");

        assert!(
            sim_related > sim_unrelated,
            "related topics ({sim_related:.4}) should be more similar than unrelated ({sim_unrelated:.4})"
        );
        assert!(
            sim_related > 0.5,
            "related topics should have cosine > 0.5, got {sim_related:.4}"
        );
    }

    /// Test batch embedding produces correct number of results.
    #[test]
    #[ignore]
    fn test_batch_embedding() {
        let manager = EmbeddingManager::new("bge-small-en").unwrap();

        let texts = vec![
            "first document about auth",
            "second document about databases",
            "third document about frontend",
        ];

        let embeddings = manager.embed_documents(&texts).unwrap();
        assert_eq!(embeddings.len(), 3);
        for emb in &embeddings {
            assert_eq!(emb.len(), 384);
        }
    }

    /// Test empty input handling.
    #[test]
    #[ignore]
    fn test_empty_batch() {
        let manager = EmbeddingManager::new("bge-small-en").unwrap();
        let embeddings = manager.embed_documents(&[]).unwrap();
        assert!(embeddings.is_empty());
    }

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
        let dot: f64 = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| *x as f64 * *y as f64)
            .sum();
        let norm_a: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
        let norm_b: f64 = b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        dot / (norm_a * norm_b)
    }
}
