use crate::audit::logger::{query_entries, AuditEntry};
use rusqlite::Connection;

/// Export format for audit log.
pub enum ExportFormat {
    Json,
    Csv,
}

/// Export audit log entries to a string in the given format.
pub fn export_audit_log(
    conn: &Connection,
    from: Option<&str>,
    to: Option<&str>,
    format: ExportFormat,
) -> Result<String, String> {
    let entries = query_entries(conn, from, to, None, None, 100_000)
        .map_err(|e| format!("query error: {e}"))?;

    match format {
        ExportFormat::Json => export_json(&entries),
        ExportFormat::Csv => export_csv(&entries),
    }
}

fn export_json(entries: &[AuditEntry]) -> Result<String, String> {
    serde_json::to_string_pretty(entries).map_err(|e| format!("json error: {e}"))
}

fn export_csv(entries: &[AuditEntry]) -> Result<String, String> {
    let mut out = String::from(
        "id,timestamp,user_id,operation,memory_id,stream_id,classification,compliance_event,hash\n",
    );
    for e in entries {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            e.id,
            e.timestamp,
            e.user_id.as_deref().unwrap_or(""),
            e.operation,
            e.memory_id.as_deref().unwrap_or(""),
            e.stream_id.as_deref().unwrap_or(""),
            e.classification.as_deref().unwrap_or(""),
            e.compliance_event,
            e.hash,
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::logger::{AuditLogger, AuditParams};
    use crate::migration;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migration::runner::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_export_json() {
        let conn = setup_db();
        let logger = AuditLogger::new(&conn);
        logger
            .log(
                &conn,
                &AuditParams {
                    user_id: Some("user1"),
                    operation: "retain",
                    memory_id: None,
                    stream_id: None,
                    details: None,
                    classification: None,
                    compliance_event: false,
                    anomaly_flag: false,
                },
            )
            .unwrap();

        let json = export_audit_log(&conn, None, None, ExportFormat::Json).unwrap();
        assert!(json.contains("retain"));
        assert!(json.contains("user1"));
    }

    #[test]
    fn test_export_csv() {
        let conn = setup_db();
        let logger = AuditLogger::new(&conn);
        logger
            .log(
                &conn,
                &AuditParams {
                    user_id: None,
                    operation: "recall",
                    memory_id: None,
                    stream_id: None,
                    details: None,
                    classification: None,
                    compliance_event: false,
                    anomaly_flag: false,
                },
            )
            .unwrap();

        let csv = export_audit_log(&conn, None, None, ExportFormat::Csv).unwrap();
        assert!(csv.starts_with("id,timestamp"));
        assert!(csv.contains("recall"));
    }
}
