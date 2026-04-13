use rusqlite::Connection;

/// Result from keyword search strategy.
#[derive(Debug, Clone)]
pub struct KeywordResult {
    pub memory_id: String,
    pub score: f64,
}

/// Run keyword search using SQLite LIKE matching as a fallback strategy.
///
/// The spec calls for BGE-M3 sparse vectors, but if LanceDB Rust bindings
/// lack native sparse vector support, this SQLite-based keyword search
/// serves as a battle-tested fallback. It matches against memory summaries.
pub fn search(
    conn: &Connection,
    query: &str,
    top_k: usize,
    stream_id: Option<&str>,
    include_archived: bool,
) -> Result<Vec<KeywordResult>, rusqlite::Error> {
    // Split query into keywords for matching
    let keywords: Vec<&str> = query.split_whitespace().filter(|w| w.len() > 2).collect();

    if keywords.is_empty() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    // Build query with LIKE conditions for each keyword
    let mut sql = String::from("SELECT id, summary FROM memories WHERE summary IS NOT NULL");
    if !include_archived {
        sql.push_str(" AND archived = 0");
    }
    if stream_id.is_some() {
        sql.push_str(" AND stream_id = ?1");
    }

    let mut stmt = conn.prepare(&sql)?;
    let stream_owned = stream_id.map(String::from);
    let param_vec: Vec<Box<dyn rusqlite::types::ToSql>> = if let Some(ref s) = stream_owned {
        vec![Box::new(s.clone())]
    } else {
        vec![]
    };
    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        param_vec.iter().map(|p| p.as_ref()).collect();

    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let id: String = row.get(0)?;
        let summary: String = row.get(1)?;
        Ok((id, summary))
    })?;

    for row in rows.flatten() {
        let (id, summary) = row;
        let lower_summary = summary.to_lowercase();
        let mut match_count = 0;
        for kw in &keywords {
            if lower_summary.contains(&kw.to_lowercase()) {
                match_count += 1;
            }
        }
        if match_count > 0 {
            let score = match_count as f64 / keywords.len() as f64;
            results.push(KeywordResult {
                memory_id: id,
                score,
            });
        }
    }

    // Sort by score descending and take top_k
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(top_k);

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migration::runner::run_migrations(&conn).unwrap();
        // Insert test memories
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) \
             VALUES ('m1', 'h1', 'We switched from Auth0 to Clerk for authentication', 'clear', '2026-01-01')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) \
             VALUES ('m2', 'h2', 'Database migration to PostgreSQL completed', 'clear', '2026-02-01')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memories (id, content_hash, summary, source_format, created_at) \
             VALUES ('m3', 'h3', 'Frontend performance optimization with React memoization', 'clear', '2026-03-01')",
            [],
        ).unwrap();
        conn
    }

    #[test]
    fn test_keyword_search_finds_match() {
        let conn = setup_db();
        let results = search(&conn, "authentication Clerk", 10, None, false).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].memory_id, "m1");
    }

    #[test]
    fn test_keyword_search_no_match() {
        let conn = setup_db();
        let results = search(&conn, "GraphQL schema", 10, None, false).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_keyword_search_multiple_matches() {
        let conn = setup_db();
        let results = search(&conn, "migration authentication", 10, None, false).unwrap();
        // Both m1 (authentication) and m2 (migration) should match
        assert!(results.len() >= 2);
    }

    #[test]
    fn test_keyword_search_top_k() {
        let conn = setup_db();
        let results = search(&conn, "the", 1, None, false).unwrap();
        assert!(results.len() <= 1);
    }
}
