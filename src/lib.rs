//! Clear Memory — a high-performance, local-first AI memory engine.
//!
//! Store everything. Send only what matters. Pay for less.

pub mod audit;
pub mod backup;
pub mod compliance;
pub mod config;
pub mod context;
pub mod curator;
pub mod engine;
pub mod entities;
pub mod facts;
pub mod import;
pub mod migration;
pub mod observability;
pub mod reflect;
pub mod repair;
pub mod retention;
pub mod retrieval;
pub mod security;
pub mod server;
pub mod storage;
pub mod streams;
pub mod tags;

use thiserror::Error;

/// Top-level error type for library operations.
#[derive(Error, Debug)]
pub enum ClearMemoryError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("encryption error: {0}")]
    Encryption(#[from] EncryptionError),

    #[error("retrieval error: {0}")]
    Retrieval(#[from] RetrievalError),

    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    #[error("import error: {0}")]
    Import(#[from] ImportError),

    #[error("auth error: {0}")]
    Auth(#[from] AuthError),

    #[error("audit error: {0}")]
    Audit(#[from] AuditError),

    #[error("compliance error: {0}")]
    Compliance(#[from] ComplianceError),
}

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("verbatim file error: {0}")]
    VerbatimIo(#[from] std::io::Error),

    #[error("content hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("memory not found: {0}")]
    MemoryNotFound(String),

    #[error("write queue full")]
    WriteQueueFull,

    #[error("write queue closed")]
    WriteQueueClosed,

    #[error("vector storage error: {0}")]
    VectorStorage(String),
}

#[derive(Error, Debug)]
pub enum EncryptionError {
    #[error("encryption failed: {0}")]
    EncryptFailed(String),

    #[error("decryption failed: {0}")]
    DecryptFailed(String),

    #[error("key derivation failed: {0}")]
    KeyDerivationFailed(String),

    #[error("passphrase required but not provided")]
    PassphraseRequired,
}

#[derive(Error, Debug)]
pub enum RetrievalError {
    #[error("embedding error: {0}")]
    Embedding(String),

    #[error("search error: {0}")]
    Search(String),

    #[error("reranking error: {0}")]
    Reranking(String),
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("config file error: {0}")]
    FileError(#[from] std::io::Error),

    #[error("config parse error: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("invalid tier: {0} (expected: offline, local_llm, or cloud)")]
    InvalidTier(String),

    #[error("config directory not found")]
    DirNotFound,
}

#[derive(Error, Debug)]
pub enum ImportError {
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("parse error: {0}")]
    ParseError(String),

    #[error("file not found: {0}")]
    FileNotFound(String),

    #[error("format detection failed for: {0}")]
    DetectionFailed(String),
}

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("invalid token")]
    InvalidToken,

    #[error("token expired")]
    TokenExpired,

    #[error("insufficient scope: requires {required}, has {actual}")]
    InsufficientScope { required: String, actual: String },

    #[error("token revoked")]
    TokenRevoked,
}

#[derive(Error, Debug)]
pub enum AuditError {
    #[error("audit chain integrity violation at entry {entry_id}")]
    ChainIntegrityViolation { entry_id: String },

    #[error("audit log write failed: {0}")]
    WriteFailed(String),
}

#[derive(Error, Debug)]
pub enum ComplianceError {
    #[error("legal hold active on stream {stream_id}: {reason}")]
    LegalHoldActive { stream_id: String, reason: String },

    #[error("purge requires confirmation")]
    PurgeRequiresConfirmation,

    #[error("purge requires approval from a second purge-scope holder")]
    PurgeRequiresApproval,

    #[error("purge request expired")]
    PurgeRequestExpired,
}

/// Deployment tier for the Clear Memory engine.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Fully offline — no external calls, no LLM
    #[default]
    Offline,
    /// Offline + bundled local LLM (Qwen3-0.6B curator, Qwen3-4B reflect)
    LocalLlm,
    /// Cloud-connected — optional cloud APIs for highest quality
    Cloud,
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tier::Offline => write!(f, "offline"),
            Tier::LocalLlm => write!(f, "local_llm"),
            Tier::Cloud => write!(f, "cloud"),
        }
    }
}

/// Data classification levels for compliance.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    Public,
    #[default]
    Internal,
    Confidential,
    Pii,
}

impl std::fmt::Display for Classification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Classification::Public => write!(f, "public"),
            Classification::Internal => write!(f, "internal"),
            Classification::Confidential => write!(f, "confidential"),
            Classification::Pii => write!(f, "pii"),
        }
    }
}
