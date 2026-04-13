use crate::config::Config;
use crate::curator::qwen::{CuratorModel, NoopCurator};
use crate::entities::resolver::HeuristicResolver;
use crate::retrieval::rerank::PassthroughReranker;
use crate::security::encryption::{self, EncryptionProvider};
use crate::storage::embeddings::EmbeddingManager;
use crate::storage::lance::LanceStorage;
use crate::storage::sqlite::{RetainParams, SqliteStorage};
use crate::storage::verbatim::VerbatimStorage;
use crate::{Classification, Tier};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, instrument, warn};

/// The core Clear Memory engine — coordinates all subsystems.
pub struct Engine {
    pub config: Arc<Config>,
    pub sqlite: SqliteStorage,
    pub verbatim: VerbatimStorage,
    pub lance: LanceStorage,
    pub encryption: Arc<dyn EncryptionProvider>,
    pub embeddings: Option<Arc<EmbeddingManager>>,
    start_time: Instant,
    data_dir: PathBuf,
}

/// Result of a recall (search) operation.
#[derive(Debug, serde::Serialize)]
pub struct RecallResponse {
    pub results: Vec<RecallHit>,
    pub total_candidates: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct RecallHit {
    pub memory_id: String,
    pub summary: Option<String>,
    pub score: f64,
    pub created_at: String,
}

/// Result of a retain (store) operation.
#[derive(Debug, serde::Serialize)]
pub struct RetainResponse {
    pub memory_id: String,
    pub content_hash: String,
}

/// Result of an expand (full content) operation.
#[derive(Debug, serde::Serialize)]
pub struct ExpandResponse {
    pub memory_id: String,
    pub content: String,
    pub source_format: String,
    pub created_at: String,
}

/// Result of a status query.
#[derive(Debug, serde::Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub tier: String,
    pub memory_count: i64,
    pub vector_count: usize,
    pub uptime_secs: u64,
}

impl Engine {
    /// Initialize the engine from config.
    pub async fn init(config: Arc<Config>) -> Result<Self, anyhow::Error> {
        let data_dir = Config::ensure_directories()?;

        // Set up encryption
        let enc = if config.encryption.enabled {
            match encryption::create_provider(&config.encryption) {
                Ok(p) => p,
                Err(_) => {
                    info!("encryption passphrase not set, running without encryption");
                    Arc::new(encryption::NoopProvider) as Arc<dyn EncryptionProvider>
                }
            }
        } else {
            Arc::new(encryption::NoopProvider) as Arc<dyn EncryptionProvider>
        };

        let sqlite = SqliteStorage::open(
            &data_dir.join("clearmemory.db"),
            enc.clone(),
            config.concurrency.write_queue_depth,
        )
        .await?;

        let verbatim = VerbatimStorage::new(
            data_dir.join("verbatim"),
            data_dir.join("archive").join("verbatim"),
            enc.clone(),
        );

        // Load embedding model (graceful degradation — warn and continue without if unavailable)
        let embedding_model_name = config.models.embedding.clone();
        let embeddings = match tokio::task::spawn_blocking(move || {
            EmbeddingManager::new(&embedding_model_name)
        })
        .await
        {
            Ok(Ok(mgr)) => {
                let dim = mgr.dimensions() as i32;
                info!(model = %config.models.embedding, dim, "embedding model loaded");
                Some(Arc::new(mgr))
            }
            Ok(Err(e)) => {
                warn!(error = %e, "failed to load embedding model, continuing without semantic search");
                None
            }
            Err(e) => {
                warn!(error = %e, "embedding model task panicked, continuing without semantic search");
                None
            }
        };

        let vector_dim = embeddings
            .as_ref()
            .map(|e| e.dimensions() as i32)
            .unwrap_or(crate::storage::lance::DEFAULT_VECTOR_DIM);

        let lance = LanceStorage::open_with_dim(data_dir.join("vectors"), vector_dim).await?;

        info!(tier = %config.general.tier, "engine initialized");

        Ok(Self {
            config,
            sqlite,
            verbatim,
            lance,
            encryption: enc,
            embeddings,
            start_time: Instant::now(),
            data_dir,
        })
    }

    /// Store a new memory (the full write path from the architecture doc).
    ///
    /// Flow: secret scanning → classification → encrypt → store verbatim →
    ///       SQLite (memories + tags) → LanceDB append → entity resolution →
    ///       fact extraction → audit log
    #[instrument(skip(self, content))]
    pub async fn retain(
        &self,
        content: &str,
        tags: Vec<(String, String)>,
        classification: Option<Classification>,
        stream_id: Option<String>,
    ) -> Result<RetainResponse, anyhow::Error> {
        // 1. Secret scanning (before anything is stored)
        let scanner = crate::security::secret_scanner::SecretScanner::new();
        let mut effective_classification =
            classification.unwrap_or(self.config.compliance.default_classification);

        let store_content = match self.config.security.secret_scanning.mode.as_str() {
            "block" => {
                if scanner.has_secrets(content) {
                    anyhow::bail!(
                        "Memory contains detected secrets. Remove credentials and retry."
                    );
                }
                content.to_string()
            }
            "redact" => {
                let (redacted, matches) =
                    crate::security::redactor::scan_and_redact(&scanner, content);
                if !matches.is_empty() {
                    effective_classification =
                        effective_classification.max(Classification::Confidential);
                }
                redacted
            }
            _ => {
                // "warn" mode
                if scanner.has_secrets(content) {
                    effective_classification =
                        effective_classification.max(Classification::Confidential);
                }
                content.to_string()
            }
        };

        // 2. Encrypt and store verbatim content
        let content_hash = self.verbatim.store(store_content.as_bytes()).await?;

        // 3. Generate summary (first line, truncated)
        let summary = store_content.lines().next().map(|l| {
            if l.len() > 200 {
                l[..200].to_string()
            } else {
                l.to_string()
            }
        });

        // 4. Store in SQLite (memories + tags, via write queue)
        let memory_id = self
            .sqlite
            .retain(RetainParams {
                content_hash: content_hash.clone(),
                summary,
                source_format: "clear".to_string(),
                classification: effective_classification,
                owner_id: None,
                stream_id: stream_id.clone(),
                tags,
            })
            .await?;

        // 5. LanceDB vector append — generate embedding and insert
        if let Some(ref emb) = self.embeddings {
            let emb_clone = emb.clone();
            let content_for_embed = store_content.clone();
            match tokio::task::spawn_blocking(move || emb_clone.embed_query(&content_for_embed))
                .await
            {
                Ok(Ok(vector)) => {
                    if let Err(e) = self
                        .lance
                        .insert(&memory_id, &vector, stream_id.as_deref())
                        .await
                    {
                        warn!(error = %e, "failed to insert vector into LanceDB");
                    }
                }
                Ok(Err(e)) => {
                    warn!(error = %e, "failed to generate embedding for memory");
                }
                Err(e) => {
                    warn!(error = %e, "embedding task panicked");
                }
            }
        }

        // 6. Entity resolution (Tier 1: heuristic)
        let data_dir = self.data_dir.clone();
        let enc = self.encryption.clone();
        let memory_id_clone = memory_id.clone();
        let content_clone = store_content.clone();
        drop(tokio::task::spawn_blocking(move || {
            let conn = match rusqlite::Connection::open(data_dir.join("clearmemory.db")) {
                Ok(c) => c,
                Err(_) => return,
            };
            if enc.is_enabled() {
                if let Ok(key) = enc.sqlite_key_hex() {
                    if !key.is_empty() {
                        let _ = conn.execute_batch(&format!("PRAGMA key = '{key}';"));
                    }
                }
            }

            // Extract and store facts
            let facts = crate::facts::extractor::extract_facts(&content_clone, &memory_id_clone);
            for fact in &facts {
                let _ = crate::facts::temporal::insert_fact(&conn, fact);
                // Check for conflicts and resolve them
                let conflicts =
                    crate::facts::conflict::detect_conflicts(&conn, fact).unwrap_or_default();
                if !conflicts.is_empty() {
                    let _ = crate::facts::conflict::resolve_conflicts(&conn, &conflicts);
                }
            }

            // Audit log entry
            let logger = crate::audit::logger::AuditLogger::new(&conn);
            let _ = logger.log(
                &conn,
                &crate::audit::logger::AuditParams {
                    user_id: None,
                    operation: "retain",
                    memory_id: Some(&memory_id_clone),
                    stream_id: None,
                    details: None,
                    classification: None,
                    compliance_event: false,
                    anomaly_flag: false,
                },
            );
        }));

        Ok(RetainResponse {
            memory_id,
            content_hash,
        })
    }

    /// Search for memories (the full read path from the architecture doc).
    ///
    /// Flow: stream permission check → 4-strategy parallel search (tokio::join!) →
    ///       RRF merge → rerank → classification gate (Tier 3) → curator (Tier 2+) →
    ///       context compiler assembly
    #[instrument(skip(self))]
    pub async fn recall(
        &self,
        query: &str,
        stream_id: Option<String>,
        include_archived: bool,
    ) -> Result<RecallResponse, anyhow::Error> {
        // Stream permission check
        if let Some(ref sid) = stream_id {
            let data_dir = self.data_dir.clone();
            let enc = self.encryption.clone();
            let sid_clone = sid.clone();
            let permitted = tokio::task::spawn_blocking(move || {
                let conn = rusqlite::Connection::open(data_dir.join("clearmemory.db"))?;
                if enc.is_enabled() {
                    if let Ok(key) = enc.sqlite_key_hex() {
                        if !key.is_empty() {
                            conn.execute_batch(&format!("PRAGMA key = '{key}';"))?;
                        }
                    }
                }
                // Default user "local" for single-user deployments
                crate::streams::security::can_read(&conn, "local", &sid_clone)
            })
            .await??;
            if !permitted {
                anyhow::bail!("access denied to stream {sid}");
            }
        }

        // Collect summaries for reranking
        let memories = self
            .sqlite
            .search_memories(stream_id.as_deref(), include_archived, 100)
            .await?;

        let summaries: HashMap<String, String> = memories
            .iter()
            .filter_map(|m| m.summary.as_ref().map(|s| (m.id.clone(), s.clone())))
            .collect();

        // Generate query embedding if model is available
        let query_embedding: Option<Vec<f32>> = if let Some(ref emb) = self.embeddings {
            let emb_clone = emb.clone();
            let q = query.to_string();
            match tokio::task::spawn_blocking(move || emb_clone.embed_query(&q)).await {
                Ok(Ok(vec)) => Some(vec),
                Ok(Err(e)) => {
                    warn!(error = %e, "failed to embed query, running without semantic search");
                    None
                }
                Err(e) => {
                    warn!(error = %e, "query embedding task panicked");
                    None
                }
            }
        } else {
            None
        };

        // Run 4-strategy parallel retrieval
        let data_dir = self.data_dir.clone();
        let enc = self.encryption.clone();
        let lance = self.lance.clone();
        let retrieval_config = crate::retrieval::RecallConfig {
            top_k: self.config.retrieval.top_k,
            temporal_boost: self.config.retrieval.temporal_boost,
            entity_boost: self.config.retrieval.entity_boost,
            include_archived,
            stream_id: stream_id.clone(),
        };
        let query_owned = query.to_string();
        let summaries_clone = summaries.clone();

        let result = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(data_dir.join("clearmemory.db"))?;
            if enc.is_enabled() {
                if let Ok(key) = enc.sqlite_key_hex() {
                    if !key.is_empty() {
                        conn.execute_batch(&format!("PRAGMA key = '{key}';"))?;
                    }
                }
            }

            let resolver = HeuristicResolver;
            let reranker = PassthroughReranker;

            let rt = tokio::runtime::Handle::current();
            rt.block_on(crate::retrieval::recall(
                &query_owned,
                &conn,
                &lance,
                query_embedding.as_deref(),
                &resolver,
                &reranker,
                &summaries_clone,
                &retrieval_config,
            ))
            .map_err(|e| anyhow::anyhow!(e))
        })
        .await
        .map_err(|e| anyhow::anyhow!("recall task failed: {e}"))??;

        // Classification gate (Tier 3): filter out PII/confidential from cloud-bound results
        let filtered_results = if self.config.general.tier == Tier::Cloud {
            let eligible = &self.config.security.cloud_eligible_classifications;
            result
                .results
                .into_iter()
                .filter(|r| {
                    let mem = memories.iter().find(|m| m.id == r.memory_id);
                    match mem {
                        Some(m) => crate::security::cloud_filter::is_cloud_eligible(
                            m.classification,
                            eligible,
                        ),
                        None => true,
                    }
                })
                .collect()
        } else {
            result.results
        };

        // Curator filtering (Tier 2+)
        let curated_results = if self.config.general.tier != Tier::Offline {
            let curator = NoopCurator; // Will be replaced with Qwen3-0.6B when candle is integrated
            let excerpts: Vec<crate::curator::qwen::MemoryExcerpt> = filtered_results
                .iter()
                .filter_map(|r| {
                    let mem = memories.iter().find(|m| m.id == r.memory_id)?;
                    Some(crate::curator::qwen::MemoryExcerpt {
                        memory_id: r.memory_id.clone(),
                        content: mem.summary.clone().unwrap_or_default(),
                        relevance_score: r.rerank_score,
                    })
                })
                .collect();

            match curator.curate(query, &excerpts) {
                Ok(curated) => {
                    info!(
                        original = excerpts.len(),
                        curated = curated.len(),
                        "curator filtered results"
                    );
                    filtered_results // Pass through for now (NoopCurator preserves all)
                }
                Err(e) => {
                    warn!(error = %e, "curator failed, using unfiltered results");
                    filtered_results
                }
            }
        } else {
            filtered_results
        };

        // Audit the recall operation
        let data_dir2 = self.data_dir.clone();
        let enc2 = self.encryption.clone();
        let query_for_audit = query.to_string();
        let result_count = curated_results.len();
        drop(tokio::task::spawn_blocking(move || {
            if let Ok(conn) = rusqlite::Connection::open(data_dir2.join("clearmemory.db")) {
                if enc2.is_enabled() {
                    if let Ok(key) = enc2.sqlite_key_hex() {
                        if !key.is_empty() {
                            let _ = conn.execute_batch(&format!("PRAGMA key = '{key}';"));
                        }
                    }
                }
                let logger = crate::audit::logger::AuditLogger::new(&conn);
                let details = format!("query={}, results={result_count}", query_for_audit);
                let _ = logger.log(
                    &conn,
                    &crate::audit::logger::AuditParams {
                        user_id: None,
                        operation: "recall",
                        memory_id: None,
                        stream_id: None,
                        details: Some(&details),
                        classification: None,
                        compliance_event: false,
                        anomaly_flag: false,
                    },
                );
            }
        }));

        // Build response
        let hits: Vec<RecallHit> = curated_results
            .iter()
            .filter_map(|r| {
                let mem = memories.iter().find(|m| m.id == r.memory_id)?;
                Some(RecallHit {
                    memory_id: r.memory_id.clone(),
                    summary: mem.summary.clone(),
                    score: r.rerank_score,
                    created_at: mem.created_at.clone(),
                })
            })
            .collect();

        Ok(RecallResponse {
            total_candidates: result.total_candidates,
            results: hits,
        })
    }

    /// Get full verbatim content for a memory.
    #[instrument(skip(self))]
    pub async fn expand(&self, memory_id: &str) -> Result<ExpandResponse, anyhow::Error> {
        let memory = self.sqlite.get_memory(memory_id).await?;

        let content_bytes = self.verbatim.read(&memory.content_hash).await?;
        let content = String::from_utf8(content_bytes)
            .map_err(|e| anyhow::anyhow!("invalid UTF-8 in verbatim content: {e}"))?;

        // Update access time (resets retention staleness clock)
        let _ = self.sqlite.update_access_time(memory_id).await;

        // Audit the expand
        let data_dir = self.data_dir.clone();
        let enc = self.encryption.clone();
        let mid = memory_id.to_string();
        drop(tokio::task::spawn_blocking(move || {
            if let Ok(conn) = rusqlite::Connection::open(data_dir.join("clearmemory.db")) {
                if enc.is_enabled() {
                    if let Ok(key) = enc.sqlite_key_hex() {
                        if !key.is_empty() {
                            let _ = conn.execute_batch(&format!("PRAGMA key = '{key}';"));
                        }
                    }
                }
                let logger = crate::audit::logger::AuditLogger::new(&conn);
                let _ = logger.log(
                    &conn,
                    &crate::audit::logger::AuditParams {
                        user_id: None,
                        operation: "expand",
                        memory_id: Some(&mid),
                        stream_id: None,
                        details: None,
                        classification: None,
                        compliance_event: false,
                        anomaly_flag: false,
                    },
                );
            }
        }));

        Ok(ExpandResponse {
            memory_id: memory.id,
            content,
            source_format: memory.source_format,
            created_at: memory.created_at,
        })
    }

    /// Invalidate a memory (temporal marking, not deletion).
    #[instrument(skip(self))]
    pub async fn forget(
        &self,
        memory_id: &str,
        reason: Option<String>,
    ) -> Result<(), anyhow::Error> {
        self.sqlite.forget(memory_id.to_string(), reason).await?;
        Ok(())
    }

    /// Get corpus status.
    pub async fn status(&self) -> Result<StatusResponse, anyhow::Error> {
        let memory_count = self.sqlite.memory_count().await?;
        let vector_count = self.lance.vector_count().await?;

        Ok(StatusResponse {
            status: "healthy".to_string(),
            tier: self.config.general.tier.to_string(),
            memory_count,
            vector_count,
            uptime_secs: self.start_time.elapsed().as_secs(),
        })
    }

    /// Get the data directory path.
    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_engine_components() {
        let mut config = Config::default();
        config.encryption.enabled = false;
        let config = Arc::new(config);

        let sqlite = SqliteStorage::open_in_memory().await.unwrap();
        let count = sqlite.memory_count().await.unwrap();
        assert_eq!(count, 0);
    }
}
