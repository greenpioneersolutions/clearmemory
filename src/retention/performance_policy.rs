//! Performance-based retention policy.
//!
//! Monitors p95 retrieval latency and flags degradation when it exceeds
//! the configured threshold. Records baselines for trend analysis.

use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

/// Measure the latest recorded p95 latency from the performance_baselines table.
///
/// Returns the most recent p95_recall_ms value, or `Ok(0.0)` if no baselines exist.
pub fn measure_p95_latency(conn: &Connection) -> Result<f64, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT p95_recall_ms FROM performance_baselines ORDER BY measured_at DESC LIMIT 1",
    )?;

    let result = stmt.query_row([], |row| row.get::<_, f64>(0));

    match result {
        Ok(val) => Ok(val),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0.0),
        Err(e) => Err(e),
    }
}

/// Record a new performance baseline measurement.
pub fn record_baseline(
    conn: &Connection,
    p95_ms: f64,
    corpus_size: i64,
    memory_count: i64,
) -> Result<(), rusqlite::Error> {
    let id = Uuid::new_v4().to_string();
    let measured_at = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO performance_baselines (id, measured_at, p95_recall_ms, corpus_size_bytes, memory_count) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, measured_at, p95_ms, corpus_size, memory_count],
    )?;

    Ok(())
}

/// Check whether the latest p95 latency exceeds the given threshold.
///
/// Returns `Some(current_p95)` if degraded, `None` if within threshold or
/// no baselines exist.
pub fn check_degradation(
    conn: &Connection,
    threshold_ms: f64,
) -> Result<Option<f64>, rusqlite::Error> {
    let p95 = measure_p95_latency(conn)?;

    if p95 > 0.0 && p95 > threshold_ms {
        Ok(Some(p95))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::runner::run_migrations;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_measure_p95_no_baselines() {
        let conn = setup_db();
        let p95 = measure_p95_latency(&conn).unwrap();
        assert!((p95 - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_record_and_measure_baseline() {
        let conn = setup_db();
        record_baseline(&conn, 150.0, 1_000_000, 500).unwrap();

        let p95 = measure_p95_latency(&conn).unwrap();
        assert!((p95 - 150.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_measure_returns_latest() {
        let conn = setup_db();
        // Insert two baselines; the second (most recent by measured_at) should be returned.
        // Because both use Utc::now() with nanosecond precision, the second insert
        // will have a later timestamp.
        record_baseline(&conn, 100.0, 500_000, 200).unwrap();
        record_baseline(&conn, 250.0, 1_500_000, 800).unwrap();

        let p95 = measure_p95_latency(&conn).unwrap();
        assert!((p95 - 250.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_check_degradation_none_when_below_threshold() {
        let conn = setup_db();
        record_baseline(&conn, 100.0, 500_000, 200).unwrap();

        let result = check_degradation(&conn, 200.0).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_check_degradation_some_when_above_threshold() {
        let conn = setup_db();
        record_baseline(&conn, 300.0, 2_000_000, 1000).unwrap();

        let result = check_degradation(&conn, 200.0).unwrap();
        assert_eq!(result, Some(300.0));
    }

    #[test]
    fn test_check_degradation_none_when_no_baselines() {
        let conn = setup_db();
        let result = check_degradation(&conn, 200.0).unwrap();
        assert!(result.is_none());
    }
}
