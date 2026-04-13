use crate::audit::chain::compute_chain_hash;
use chrono::Utc;
use rusqlite::{params, Connection};
use std::sync::Mutex;
use tracing::instrument;
use uuid::Uuid;

/// An audit log entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: String,
    pub user_id: Option<String>,
    pub operation: String,
    pub memory_id: Option<String>,
    pub stream_id: Option<String>,
    pub details: Option<String>,
    pub classification: Option<String>,
    pub compliance_event: bool,
    pub anomaly_flag: bool,
    pub hash: String,
    pub previous_hash: String,
}

/// Parameters for logging an audit entry.
pub struct AuditParams<'a> {
    pub user_id: Option<&'a str>,
    pub operation: &'a str,
    pub memory_id: Option<&'a str>,
    pub stream_id: Option<&'a str>,
    pub details: Option<&'a str>,
    pub classification: Option<&'a str>,
    pub compliance_event: bool,
    pub anomaly_flag: bool,
}

/// Append-only audit logger with chained hash integrity.
pub struct AuditLogger {
    last_hash: Mutex<String>,
}

const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

impl AuditLogger {
    /// Create a new audit logger. Reads the last hash from the database.
    pub fn new(conn: &Connection) -> Self {
        let last_hash = Self::read_last_hash(conn).unwrap_or_else(|| GENESIS_HASH.to_string());
        Self {
            last_hash: Mutex::new(last_hash),
        }
    }

    /// Create a logger with genesis hash (for testing or fresh databases).
    pub fn new_genesis() -> Self {
        Self {
            last_hash: Mutex::new(GENESIS_HASH.to_string()),
        }
    }

    /// Log an operation to the audit log.
    #[instrument(skip(self, conn, p))]
    pub fn log(
        &self,
        conn: &Connection,
        p: &AuditParams<'_>,
    ) -> Result<AuditEntry, rusqlite::Error> {
        let id = Uuid::new_v4().to_string();
        let timestamp = Utc::now().to_rfc3339();

        let previous_hash = {
            let guard = self.last_hash.lock().unwrap();
            guard.clone()
        };

        let content = format!(
            "{id}|{timestamp}|{}|{}|{}|{}|{}|{}|{}",
            p.user_id.unwrap_or(""),
            p.operation,
            p.memory_id.unwrap_or(""),
            p.stream_id.unwrap_or(""),
            p.details.unwrap_or(""),
            p.classification.unwrap_or(""),
            p.compliance_event,
        );
        let hash = compute_chain_hash(&previous_hash, &content);

        conn.execute(
            "INSERT INTO audit_log (id, timestamp, user_id, operation, memory_id, stream_id, \
             details, classification, compliance_event, anomaly_flag, hash, previous_hash) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id,
                timestamp,
                p.user_id,
                p.operation,
                p.memory_id,
                p.stream_id,
                p.details,
                p.classification,
                p.compliance_event as i32,
                p.anomaly_flag as i32,
                hash,
                previous_hash,
            ],
        )?;

        {
            let mut guard = self.last_hash.lock().unwrap();
            *guard = hash.clone();
        }

        Ok(AuditEntry {
            id,
            timestamp,
            user_id: p.user_id.map(String::from),
            operation: p.operation.to_string(),
            memory_id: p.memory_id.map(String::from),
            stream_id: p.stream_id.map(String::from),
            details: p.details.map(String::from),
            classification: p.classification.map(String::from),
            compliance_event: p.compliance_event,
            anomaly_flag: p.anomaly_flag,
            hash,
            previous_hash,
        })
    }

    fn read_last_hash(conn: &Connection) -> Option<String> {
        conn.query_row(
            "SELECT hash FROM audit_log ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok()
    }
}

/// Query audit log entries with optional filters.
pub fn query_entries(
    conn: &Connection,
    from: Option<&str>,
    to: Option<&str>,
    operation: Option<&str>,
    stream_id: Option<&str>,
    limit: usize,
) -> Result<Vec<AuditEntry>, rusqlite::Error> {
    let mut sql = String::from(
        "SELECT id, timestamp, user_id, operation, memory_id, stream_id, details, \
         classification, compliance_event, anomaly_flag, hash, previous_hash FROM audit_log WHERE 1=1",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(f) = from {
        sql.push_str(&format!(" AND timestamp >= ?{param_idx}"));
        param_values.push(Box::new(f.to_string()));
        param_idx += 1;
    }
    if let Some(t) = to {
        sql.push_str(&format!(" AND timestamp <= ?{param_idx}"));
        param_values.push(Box::new(t.to_string()));
        param_idx += 1;
    }
    if let Some(op) = operation {
        sql.push_str(&format!(" AND operation = ?{param_idx}"));
        param_values.push(Box::new(op.to_string()));
        param_idx += 1;
    }
    if let Some(s) = stream_id {
        sql.push_str(&format!(" AND stream_id = ?{param_idx}"));
        param_values.push(Box::new(s.to_string()));
        let _ = param_idx + 1;
    }

    sql.push_str(&format!(" ORDER BY rowid ASC LIMIT {limit}"));

    let p: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|v| v.as_ref()).collect();

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(p.as_slice(), |row| {
        let ce: i32 = row.get(8)?;
        let af: i32 = row.get(9)?;
        Ok(AuditEntry {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            user_id: row.get(2)?,
            operation: row.get(3)?,
            memory_id: row.get(4)?,
            stream_id: row.get(5)?,
            details: row.get(6)?,
            classification: row.get(7)?,
            compliance_event: ce != 0,
            anomaly_flag: af != 0,
            hash: row.get(10)?,
            previous_hash: row.get(11)?,
        })
    })?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migration::runner::run_migrations(&conn).unwrap();
        conn
    }

    fn simple_params<'a>(operation: &'a str) -> AuditParams<'a> {
        AuditParams {
            user_id: None,
            operation,
            memory_id: None,
            stream_id: None,
            details: None,
            classification: None,
            compliance_event: false,
            anomaly_flag: false,
        }
    }

    #[test]
    fn test_log_entry() {
        let conn = setup_db();
        let logger = AuditLogger::new(&conn);

        let entry = logger
            .log(
                &conn,
                &AuditParams {
                    user_id: Some("user1"),
                    operation: "retain",
                    memory_id: Some("mem1"),
                    stream_id: None,
                    details: None,
                    classification: None,
                    compliance_event: false,
                    anomaly_flag: false,
                },
            )
            .unwrap();

        assert!(!entry.id.is_empty());
        assert_eq!(entry.operation, "retain");
        assert_eq!(entry.previous_hash, GENESIS_HASH);
        assert_ne!(entry.hash, GENESIS_HASH);
    }

    #[test]
    fn test_chain_integrity() {
        let conn = setup_db();
        let logger = AuditLogger::new(&conn);

        let e1 = logger.log(&conn, &simple_params("retain")).unwrap();
        let e2 = logger.log(&conn, &simple_params("recall")).unwrap();

        assert_eq!(e2.previous_hash, e1.hash);
    }

    #[test]
    fn test_query_entries() {
        let conn = setup_db();
        let logger = AuditLogger::new(&conn);

        logger.log(&conn, &simple_params("retain")).unwrap();
        logger.log(&conn, &simple_params("recall")).unwrap();
        logger.log(&conn, &simple_params("retain")).unwrap();

        let all = query_entries(&conn, None, None, None, None, 100).unwrap();
        assert_eq!(all.len(), 3);

        let retains = query_entries(&conn, None, None, Some("retain"), None, 100).unwrap();
        assert_eq!(retains.len(), 2);
    }
}
