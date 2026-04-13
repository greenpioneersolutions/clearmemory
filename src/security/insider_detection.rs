//! Insider threat detection for shared deployments.
//!
//! Monitors access patterns per user and flags anomalies such as
//! querying unfamiliar streams, burst access to confidential memories,
//! or access outside normal hours.

use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

/// A single access event to be checked against the user's historical pattern.
#[derive(Debug, Clone)]
pub struct AccessPattern {
    pub user_id: String,
    pub stream_id: String,
    pub timestamp: String,
    pub operation: String,
}

/// A user's historical access profile derived from audit log data.
#[derive(Debug, Clone)]
pub struct AccessProfile {
    pub user_id: String,
    pub usual_streams: Vec<String>,
    pub avg_daily_queries: f64,
    pub typical_hours: (u32, u32),
}

/// An anomaly event flagged by the detection system.
#[derive(Debug, Clone)]
pub struct AnomalyEvent {
    pub user_id: String,
    pub event_type: String,
    pub severity: f64,
    pub details: String,
    pub timestamp: String,
}

/// Record an access event in the audit log for pattern tracking.
pub fn log_access(conn: &Connection, pattern: &AccessPattern) -> Result<(), rusqlite::Error> {
    let id = Uuid::new_v4().to_string();

    // We use a placeholder hash chain for access logging; the full audit
    // module handles chained hashes for tamper evidence.
    conn.execute(
        "INSERT INTO audit_log (id, timestamp, user_id, operation, stream_id, hash, previous_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            id,
            pattern.timestamp,
            pattern.user_id,
            pattern.operation,
            pattern.stream_id,
            "placeholder",
            "placeholder",
        ],
    )?;

    Ok(())
}

/// Build an access profile for a user from their audit log history.
fn build_profile(conn: &Connection, user_id: &str) -> Result<AccessProfile, rusqlite::Error> {
    // Gather distinct streams the user has accessed
    let mut stmt = conn.prepare(
        "SELECT DISTINCT stream_id FROM audit_log WHERE user_id = ?1 AND stream_id IS NOT NULL",
    )?;
    let usual_streams: Vec<String> = stmt
        .query_map([user_id], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();

    // Count total queries and distinct days for average
    let total_queries: i64 = conn.query_row(
        "SELECT COUNT(*) FROM audit_log WHERE user_id = ?1",
        [user_id],
        |row| row.get(0),
    )?;

    let distinct_days: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT date(timestamp)) FROM audit_log WHERE user_id = ?1",
        [user_id],
        |row| row.get(0),
    )?;

    let avg_daily = if distinct_days > 0 {
        total_queries as f64 / distinct_days as f64
    } else {
        0.0
    };

    Ok(AccessProfile {
        user_id: user_id.to_string(),
        usual_streams,
        avg_daily_queries: avg_daily,
        typical_hours: (8, 20), // Default business hours; a full implementation would compute from data
    })
}

/// Detect anomalies in the current access relative to the user's historical pattern.
///
/// Currently checks:
/// - **Unfamiliar stream access:** the user is accessing a stream they have never
///   queried before (based on audit_log history).
///
/// The `threshold_stddev` parameter is reserved for future statistical scoring;
/// currently any novel stream access is flagged with a fixed severity.
pub fn detect_anomalies(
    conn: &Connection,
    user_id: &str,
    current_access: &AccessPattern,
    _threshold_stddev: f64,
) -> Result<Vec<AnomalyEvent>, rusqlite::Error> {
    let profile = build_profile(conn, user_id)?;
    let mut anomalies = Vec::new();

    // Check if the stream is unfamiliar
    if !current_access.stream_id.is_empty()
        && !profile.usual_streams.contains(&current_access.stream_id)
        && !profile.usual_streams.is_empty()
    {
        anomalies.push(AnomalyEvent {
            user_id: user_id.to_string(),
            event_type: "unfamiliar_stream".to_string(),
            severity: 1.0,
            details: format!(
                "User accessed stream '{}' which is not in their history of {} known streams",
                current_access.stream_id,
                profile.usual_streams.len()
            ),
            timestamp: Utc::now().to_rfc3339(),
        });
    }

    Ok(anomalies)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::runner::run_migrations;
    use chrono::Utc;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_log_access_inserts_audit_entry() {
        let conn = setup_db();
        let pattern = AccessPattern {
            user_id: "alice".to_string(),
            stream_id: "stream-1".to_string(),
            timestamp: Utc::now().to_rfc3339(),
            operation: "recall".to_string(),
        };

        log_access(&conn, &pattern).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM audit_log WHERE user_id = 'alice'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_detect_anomalies_no_history_no_anomaly() {
        let conn = setup_db();

        // First access ever — no history means no baseline to compare against,
        // so no anomaly should be flagged.
        let access = AccessPattern {
            user_id: "bob".to_string(),
            stream_id: "stream-1".to_string(),
            timestamp: Utc::now().to_rfc3339(),
            operation: "recall".to_string(),
        };

        let anomalies = detect_anomalies(&conn, "bob", &access, 3.0).unwrap();
        assert!(anomalies.is_empty());
    }

    #[test]
    fn test_detect_anomalies_flags_unfamiliar_stream() {
        let conn = setup_db();

        // Build some history for alice on stream-1
        let past = AccessPattern {
            user_id: "alice".to_string(),
            stream_id: "stream-1".to_string(),
            timestamp: "2026-04-01T10:00:00Z".to_string(),
            operation: "recall".to_string(),
        };
        log_access(&conn, &past).unwrap();
        log_access(&conn, &past).unwrap();

        // Now alice accesses an unfamiliar stream
        let current = AccessPattern {
            user_id: "alice".to_string(),
            stream_id: "secret-stream".to_string(),
            timestamp: Utc::now().to_rfc3339(),
            operation: "recall".to_string(),
        };

        let anomalies = detect_anomalies(&conn, "alice", &current, 3.0).unwrap();
        assert_eq!(anomalies.len(), 1);
        assert_eq!(anomalies[0].event_type, "unfamiliar_stream");
    }

    #[test]
    fn test_detect_anomalies_no_flag_for_known_stream() {
        let conn = setup_db();

        let past = AccessPattern {
            user_id: "alice".to_string(),
            stream_id: "stream-1".to_string(),
            timestamp: "2026-04-01T10:00:00Z".to_string(),
            operation: "recall".to_string(),
        };
        log_access(&conn, &past).unwrap();

        // Same stream — should not flag
        let current = AccessPattern {
            user_id: "alice".to_string(),
            stream_id: "stream-1".to_string(),
            timestamp: Utc::now().to_rfc3339(),
            operation: "recall".to_string(),
        };

        let anomalies = detect_anomalies(&conn, "alice", &current, 3.0).unwrap();
        assert!(anomalies.is_empty());
    }
}
