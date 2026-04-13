use crate::migration;
use crate::security::encryption::EncryptionProvider;
use crate::{Classification, StorageError};
use chrono::Utc;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, instrument};
use uuid::Uuid;

/// A stored memory record.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Memory {
    pub id: String,
    pub content_hash: String,
    pub summary: Option<String>,
    pub source_format: String,
    pub classification: Classification,
    pub created_at: String,
    pub last_accessed_at: Option<String>,
    pub access_count: i64,
    pub archived: bool,
    pub owner_id: Option<String>,
    pub stream_id: Option<String>,
}

/// Parameters for creating a new memory.
pub struct RetainParams {
    pub content_hash: String,
    pub summary: Option<String>,
    pub source_format: String,
    pub classification: Classification,
    pub owner_id: Option<String>,
    pub stream_id: Option<String>,
    pub tags: Vec<(String, String)>, // (tag_type, tag_value)
}

/// A write operation dispatched to the write queue.
#[allow(dead_code)]
enum WriteOp {
    Retain {
        params: RetainParams,
        reply: oneshot::Sender<Result<String, StorageError>>,
    },
    Forget {
        memory_id: String,
        reason: Option<String>,
        reply: oneshot::Sender<Result<(), StorageError>>,
    },
    UpdateAccessTime {
        memory_id: String,
        reply: oneshot::Sender<Result<(), StorageError>>,
    },
    Archive {
        memory_id: String,
        reply: oneshot::Sender<Result<(), StorageError>>,
    },
    RawSql {
        sql: String,
        reply: oneshot::Sender<Result<(), StorageError>>,
    },
}

/// SQLite storage layer with write queue for serialized writes and concurrent reads.
pub struct SqliteStorage {
    /// Read-only connection pool (bypasses write queue)
    read_conn: Arc<tokio::sync::Mutex<Connection>>,
    /// Write queue sender
    write_tx: mpsc::Sender<WriteOp>,
}

impl SqliteStorage {
    /// Open or create the database, run migrations, and start the writer task.
    pub async fn open(
        db_path: &Path,
        encryption: Arc<dyn EncryptionProvider>,
        queue_depth: usize,
    ) -> Result<Self, StorageError> {
        let db_path = db_path.to_path_buf();
        let encryption_clone = encryption.clone();

        // Open the write connection in a blocking task
        let write_conn = tokio::task::spawn_blocking({
            let db_path = db_path.clone();
            let encryption = encryption_clone.clone();
            move || open_connection(&db_path, &*encryption)
        })
        .await
        .map_err(|e| {
            StorageError::Sqlite(rusqlite::Error::InvalidParameterName(e.to_string()))
        })??;

        // Run migrations
        migration::runner::run_migrations(&write_conn)
            .map_err(|e| StorageError::Sqlite(rusqlite::Error::InvalidParameterName(e)))?;

        // Open read connection
        let read_conn = tokio::task::spawn_blocking({
            let db_path = db_path.clone();
            move || open_connection(&db_path, &*encryption_clone)
        })
        .await
        .map_err(|e| {
            StorageError::Sqlite(rusqlite::Error::InvalidParameterName(e.to_string()))
        })??;

        let read_conn = Arc::new(tokio::sync::Mutex::new(read_conn));

        // Start write queue
        let (write_tx, write_rx) = mpsc::channel::<WriteOp>(queue_depth);
        tokio::spawn(writer_task(write_conn, write_rx));

        info!(path = %db_path.display(), "sqlite storage opened");

        Ok(Self {
            read_conn,
            write_tx,
        })
    }

    /// Open an in-memory database for testing.
    pub async fn open_in_memory() -> Result<Self, StorageError> {
        let _encryption = Arc::new(crate::security::encryption::NoopProvider);
        let write_conn = Connection::open_in_memory()?;
        enable_wal(&write_conn)?;

        migration::runner::run_migrations(&write_conn)
            .map_err(|e| StorageError::Sqlite(rusqlite::Error::InvalidParameterName(e)))?;

        let read_conn = Connection::open_in_memory()?;
        // For in-memory, both connections share state only if using shared cache.
        // For testing, we'll use the write connection for reads too.
        // We re-run migrations on read conn since it's a separate in-memory DB.
        migration::runner::run_migrations(&read_conn)
            .map_err(|e| StorageError::Sqlite(rusqlite::Error::InvalidParameterName(e)))?;

        let read_conn = Arc::new(tokio::sync::Mutex::new(read_conn));
        let (write_tx, write_rx) = mpsc::channel::<WriteOp>(100);
        tokio::spawn(writer_task(write_conn, write_rx));

        Ok(Self {
            read_conn,
            write_tx,
        })
    }

    /// Store a new memory. Returns the generated memory ID.
    #[instrument(skip(self, params))]
    pub async fn retain(&self, params: RetainParams) -> Result<String, StorageError> {
        let (reply_tx, reply_rx) = oneshot::channel();

        self.write_tx
            .send(WriteOp::Retain {
                params,
                reply: reply_tx,
            })
            .await
            .map_err(|_| StorageError::WriteQueueClosed)?;

        reply_rx.await.map_err(|_| StorageError::WriteQueueClosed)?
    }

    /// Mark a memory as forgotten (sets valid_until on facts, does not delete).
    #[instrument(skip(self))]
    pub async fn forget(
        &self,
        memory_id: String,
        reason: Option<String>,
    ) -> Result<(), StorageError> {
        let (reply_tx, reply_rx) = oneshot::channel();

        self.write_tx
            .send(WriteOp::Forget {
                memory_id,
                reason,
                reply: reply_tx,
            })
            .await
            .map_err(|_| StorageError::WriteQueueClosed)?;

        reply_rx.await.map_err(|_| StorageError::WriteQueueClosed)?
    }

    /// Get a memory by ID (read path — bypasses write queue).
    #[instrument(skip(self))]
    pub async fn get_memory(&self, memory_id: &str) -> Result<Memory, StorageError> {
        let id = memory_id.to_string();
        let conn = self.read_conn.lock().await;

        let memory = conn.query_row(
            "SELECT id, content_hash, summary, source_format, classification, created_at, \
             last_accessed_at, access_count, archived, owner_id, stream_id \
             FROM memories WHERE id = ?1",
            params![id],
            |row| {
                let classification_str: String = row.get(4)?;
                let archived_int: i64 = row.get(8)?;
                Ok(Memory {
                    id: row.get(0)?,
                    content_hash: row.get(1)?,
                    summary: row.get(2)?,
                    source_format: row.get(3)?,
                    classification: parse_classification(&classification_str),
                    created_at: row.get(5)?,
                    last_accessed_at: row.get(6)?,
                    access_count: row.get(7)?,
                    archived: archived_int != 0,
                    owner_id: row.get(9)?,
                    stream_id: row.get(10)?,
                })
            },
        )?;

        Ok(memory)
    }

    /// Search memories by various criteria (read path).
    pub async fn search_memories(
        &self,
        stream_id: Option<&str>,
        include_archived: bool,
        limit: usize,
    ) -> Result<Vec<Memory>, StorageError> {
        let stream = stream_id.map(String::from);
        let conn = self.read_conn.lock().await;

        let mut sql = String::from(
            "SELECT id, content_hash, summary, source_format, classification, created_at, \
             last_accessed_at, access_count, archived, owner_id, stream_id FROM memories WHERE 1=1",
        );

        if !include_archived {
            sql.push_str(" AND archived = 0");
        }
        if stream.is_some() {
            sql.push_str(" AND stream_id = ?1");
        }

        sql.push_str(&format!(" ORDER BY created_at DESC LIMIT {limit}"));

        let mut stmt = conn.prepare(&sql)?;
        let rows = if let Some(ref s) = stream {
            stmt.query_map(params![s], map_memory_row)?
        } else {
            stmt.query_map([], map_memory_row)?
        };

        let memories: Vec<Memory> = rows.filter_map(|r| r.ok()).collect();
        Ok(memories)
    }

    /// Get the total number of active (non-archived) memories.
    pub async fn memory_count(&self) -> Result<i64, StorageError> {
        let conn = self.read_conn.lock().await;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE archived = 0",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Update access time and count for a memory (used by expand).
    pub async fn update_access_time(&self, memory_id: &str) -> Result<(), StorageError> {
        let (reply_tx, reply_rx) = oneshot::channel();

        self.write_tx
            .send(WriteOp::UpdateAccessTime {
                memory_id: memory_id.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| StorageError::WriteQueueClosed)?;

        reply_rx.await.map_err(|_| StorageError::WriteQueueClosed)?
    }

    /// Execute arbitrary SQL through the write queue (for audit logging, etc.).
    pub async fn execute_write(&self, sql: String) -> Result<(), StorageError> {
        let (reply_tx, reply_rx) = oneshot::channel();

        self.write_tx
            .send(WriteOp::RawSql {
                sql,
                reply: reply_tx,
            })
            .await
            .map_err(|_| StorageError::WriteQueueClosed)?;

        reply_rx.await.map_err(|_| StorageError::WriteQueueClosed)?
    }
}

/// The writer task processes operations from the write queue sequentially.
async fn writer_task(conn: Connection, mut rx: mpsc::Receiver<WriteOp>) {
    while let Some(op) = rx.recv().await {
        match op {
            WriteOp::Retain { params, reply } => {
                let result = do_retain(&conn, params);
                let _ = reply.send(result);
            }
            WriteOp::Forget {
                memory_id,
                reason,
                reply,
            } => {
                let result = do_forget(&conn, &memory_id, reason.as_deref());
                let _ = reply.send(result);
            }
            WriteOp::UpdateAccessTime { memory_id, reply } => {
                let result = do_update_access(&conn, &memory_id);
                let _ = reply.send(result);
            }
            WriteOp::Archive { memory_id, reply } => {
                let result = do_archive(&conn, &memory_id);
                let _ = reply.send(result);
            }
            WriteOp::RawSql { sql, reply } => {
                let result = conn.execute_batch(&sql).map_err(StorageError::from);
                let _ = reply.send(result);
            }
        }
    }
    debug!("writer task shutting down");
}

fn do_retain(conn: &Connection, params: RetainParams) -> Result<String, StorageError> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO memories (id, content_hash, summary, source_format, classification, \
         created_at, owner_id, stream_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            params.content_hash,
            params.summary,
            params.source_format,
            params.classification.to_string(),
            now,
            params.owner_id,
            params.stream_id,
        ],
    )?;

    // Insert tags
    for (tag_type, tag_value) in &params.tags {
        conn.execute(
            "INSERT OR IGNORE INTO memory_tags (memory_id, tag_type, tag_value) VALUES (?1, ?2, ?3)",
            params![id, tag_type, tag_value],
        )?;
    }

    Ok(id)
}

fn do_forget(
    conn: &Connection,
    memory_id: &str,
    _reason: Option<&str>,
) -> Result<(), StorageError> {
    let now = Utc::now().to_rfc3339();

    // Set valid_until on all facts associated with this memory
    conn.execute(
        "UPDATE facts SET valid_until = ?1, invalidated_at = ?1 WHERE memory_id = ?2 AND valid_until IS NULL",
        params![now, memory_id],
    )?;

    Ok(())
}

fn do_update_access(conn: &Connection, memory_id: &str) -> Result<(), StorageError> {
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "UPDATE memories SET last_accessed_at = ?1, access_count = access_count + 1 WHERE id = ?2",
        params![now, memory_id],
    )?;

    Ok(())
}

fn do_archive(conn: &Connection, memory_id: &str) -> Result<(), StorageError> {
    conn.execute(
        "UPDATE memories SET archived = 1 WHERE id = ?1",
        params![memory_id],
    )?;
    Ok(())
}

/// Open a SQLite connection with appropriate settings.
fn open_connection(
    db_path: &Path,
    encryption: &dyn EncryptionProvider,
) -> Result<Connection, StorageError> {
    let conn = Connection::open(db_path)?;

    // Apply SQLCipher key if encryption is enabled
    if encryption.is_enabled() {
        if let Ok(key) = encryption.sqlite_key_hex() {
            if !key.is_empty() {
                conn.execute_batch(&format!("PRAGMA key = '{key}';"))?;
            }
        }
    }

    enable_wal(&conn)?;

    // Enable foreign keys
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    Ok(conn)
}

fn enable_wal(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    Ok(())
}

fn parse_classification(s: &str) -> Classification {
    match s {
        "public" => Classification::Public,
        "confidential" => Classification::Confidential,
        "pii" => Classification::Pii,
        _ => Classification::Internal,
    }
}

fn map_memory_row(row: &rusqlite::Row<'_>) -> Result<Memory, rusqlite::Error> {
    let classification_str: String = row.get(4)?;
    let archived_int: i64 = row.get(8)?;
    Ok(Memory {
        id: row.get(0)?,
        content_hash: row.get(1)?,
        summary: row.get(2)?,
        source_format: row.get(3)?,
        classification: parse_classification(&classification_str),
        created_at: row.get(5)?,
        last_accessed_at: row.get(6)?,
        access_count: row.get(7)?,
        archived: archived_int != 0,
        owner_id: row.get(9)?,
        stream_id: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_retain_and_get() {
        let storage = SqliteStorage::open_in_memory().await.unwrap();

        let id = storage
            .retain(RetainParams {
                content_hash: "abc123".to_string(),
                summary: Some("test memory".to_string()),
                source_format: "clear".to_string(),
                classification: Classification::Internal,
                owner_id: None,
                stream_id: None,
                tags: vec![("team".to_string(), "platform".to_string())],
            })
            .await
            .unwrap();

        assert!(!id.is_empty());

        // Note: in-memory DBs are separate for read/write connections,
        // so we can't read from the read connection what was written.
        // This tests the write path only. Integration tests use file-backed DBs.
    }

    #[tokio::test]
    async fn test_memory_count_empty() {
        let storage = SqliteStorage::open_in_memory().await.unwrap();
        let count = storage.memory_count().await.unwrap();
        assert_eq!(count, 0);
    }
}
