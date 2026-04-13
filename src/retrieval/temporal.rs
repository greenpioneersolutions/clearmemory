use chrono::{Datelike, Duration, NaiveDate, Utc};
use rusqlite::{params, Connection};

/// Result from temporal search strategy.
#[derive(Debug, Clone)]
pub struct TemporalResult {
    pub memory_id: String,
    pub score: f64,
}

/// Run temporal proximity search: detect time references in query,
/// find memories near that time range, apply temporal boost.
pub fn search(
    conn: &Connection,
    query: &str,
    top_k: usize,
    temporal_boost: f64,
    include_archived: bool,
) -> Result<Vec<TemporalResult>, rusqlite::Error> {
    let time_range = detect_time_range(query);

    let (start, end) = match time_range {
        Some(range) => range,
        None => return Ok(Vec::new()), // No time reference detected
    };

    let start_str = start.to_string();
    let end_str = end.to_string();

    let mut sql = String::from(
        "SELECT id, created_at FROM memories WHERE created_at >= ?1 AND created_at <= ?2",
    );
    if !include_archived {
        sql.push_str(" AND archived = 0");
    }
    sql.push_str(&format!(" ORDER BY created_at DESC LIMIT {top_k}"));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![start_str, end_str], |row| {
        let id: String = row.get(0)?;
        let _created_at: String = row.get(1)?;
        Ok(id)
    })?;

    let results: Vec<TemporalResult> = rows
        .flatten()
        .enumerate()
        .map(|(i, memory_id)| {
            // Score with temporal boost: closer to the target = higher score
            let base_score = 1.0 - (i as f64 * 0.05);
            let score = base_score * (1.0 + temporal_boost);
            TemporalResult {
                memory_id,
                score: score.max(0.0),
            }
        })
        .collect();

    Ok(results)
}

/// Detect a time range from natural language in the query.
/// Returns (start_date, end_date) as ISO date strings, or None if no time reference found.
fn detect_time_range(query: &str) -> Option<(String, String)> {
    let lower = query.to_lowercase();
    let today = Utc::now().date_naive();

    // "last week"
    if lower.contains("last week") {
        let start = today - Duration::days(14);
        let end = today - Duration::days(7);
        return Some((start.to_string(), end.to_string()));
    }

    // "this week"
    if lower.contains("this week") {
        let start = today - Duration::days(7);
        return Some((start.to_string(), today.to_string()));
    }

    // "last month"
    if lower.contains("last month") {
        let start = today - Duration::days(60);
        let end = today - Duration::days(30);
        return Some((start.to_string(), end.to_string()));
    }

    // "N days ago" / "N weeks ago" / "N months ago"
    if let Some(range) = parse_relative_time(&lower, &today) {
        return Some(range);
    }

    // "yesterday"
    if lower.contains("yesterday") {
        let yesterday = today - Duration::days(1);
        return Some((yesterday.to_string(), today.to_string()));
    }

    // "today"
    if lower.contains("today") {
        return Some((today.to_string(), today.to_string()));
    }

    // Month names: "in January", "in February", etc.
    if let Some(range) = parse_month_reference(&lower, &today) {
        return Some(range);
    }

    None
}

fn parse_relative_time(lower: &str, today: &NaiveDate) -> Option<(String, String)> {
    let patterns = [
        ("days ago", 1),
        ("day ago", 1),
        ("weeks ago", 7),
        ("week ago", 7),
        ("months ago", 30),
        ("month ago", 30),
    ];

    for (suffix, multiplier) in &patterns {
        if let Some(pos) = lower.find(suffix) {
            // Look for a number before the suffix
            let before = lower[..pos].trim();
            if let Some(n) = before
                .split_whitespace()
                .next_back()
                .and_then(|w| w.parse::<i64>().ok())
            {
                let days = n * multiplier;
                let start = *today - Duration::days(days + 7);
                let end = *today - Duration::days(0_i64.max(days - 7));
                return Some((start.to_string(), end.to_string()));
            }
        }
    }

    None
}

fn parse_month_reference(lower: &str, today: &NaiveDate) -> Option<(String, String)> {
    let months = [
        ("january", 1),
        ("february", 2),
        ("march", 3),
        ("april", 4),
        ("may", 5),
        ("june", 6),
        ("july", 7),
        ("august", 8),
        ("september", 9),
        ("october", 10),
        ("november", 11),
        ("december", 12),
    ];

    for (name, month) in &months {
        if lower.contains(name) {
            let year = if *month > today.month() {
                today.year() - 1 // Assume last year if month is in the future
            } else {
                today.year()
            };

            let start = NaiveDate::from_ymd_opt(year, *month, 1)?;
            let end = if *month == 12 {
                NaiveDate::from_ymd_opt(year + 1, 1, 1)?
            } else {
                NaiveDate::from_ymd_opt(year, *month + 1, 1)?
            };

            return Some((start.to_string(), end.to_string()));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_last_week() {
        let range = detect_time_range("what happened last week");
        assert!(range.is_some());
    }

    #[test]
    fn test_detect_last_month() {
        let range = detect_time_range("changes from last month");
        assert!(range.is_some());
    }

    #[test]
    fn test_detect_days_ago() {
        let range = detect_time_range("what we discussed 3 days ago");
        assert!(range.is_some());
    }

    #[test]
    fn test_detect_month_name() {
        let range = detect_time_range("decisions made in january");
        assert!(range.is_some());
    }

    #[test]
    fn test_no_time_reference() {
        let range = detect_time_range("why did we switch to GraphQL");
        assert!(range.is_none());
    }

    #[test]
    fn test_detect_yesterday() {
        let range = detect_time_range("what happened yesterday");
        assert!(range.is_some());
    }

    #[test]
    fn test_temporal_search_empty_db() {
        let conn = Connection::open_in_memory().unwrap();
        crate::migration::runner::run_migrations(&conn).unwrap();
        let results = search(&conn, "last week", 10, 0.4, false).unwrap();
        assert!(results.is_empty());
    }
}
