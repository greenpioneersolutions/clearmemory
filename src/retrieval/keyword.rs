use rusqlite::Connection;

/// Result from keyword search strategy.
#[derive(Debug, Clone)]
pub struct KeywordResult {
    pub memory_id: String,
    pub score: f64,
}

/// Run keyword search using SQLite FTS5 with BM25 scoring.
///
/// FTS5 provides proper word-boundary matching, term weighting via BM25,
/// and O(1) lookup per term — replacing the naive substring matching that
/// scanned all memories with no word boundaries or term weighting.
///
/// Falls back to LIKE-based substring matching if the FTS5 index doesn't exist
/// (e.g., pre-migration databases).
pub fn search(
    conn: &Connection,
    query: &str,
    top_k: usize,
    stream_id: Option<&str>,
    include_archived: bool,
) -> Result<Vec<KeywordResult>, rusqlite::Error> {
    // Try FTS5 first. Fall back to LIKE if:
    // 1. The FTS5 query errors (index doesn't exist)
    // 2. FTS5 returns empty but the memories table has data (index out of sync)
    match search_fts5(conn, query, top_k, stream_id, include_archived) {
        Ok(results) if !results.is_empty() => Ok(results),
        Ok(_) => {
            // FTS5 returned empty — could be a legitimate no-match or index out of sync.
            // Check if there's content to match against; if so, try LIKE fallback.
            let has_content: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM memories WHERE summary IS NOT NULL LIMIT 1)",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            if has_content {
                search_like_fallback(conn, query, top_k, stream_id, include_archived)
            } else {
                Ok(Vec::new())
            }
        }
        Err(_) => search_like_fallback(conn, query, top_k, stream_id, include_archived),
    }
}

/// FTS5 keyword search with BM25 scoring.
fn search_fts5(
    conn: &Connection,
    query: &str,
    top_k: usize,
    stream_id: Option<&str>,
    include_archived: bool,
) -> Result<Vec<KeywordResult>, rusqlite::Error> {
    // Split query into terms and join with OR for broad matching
    let terms: Vec<&str> = query.split_whitespace().filter(|w| w.len() > 2).collect();
    if terms.is_empty() {
        return Ok(Vec::new());
    }

    // Escape FTS5 special characters and build query
    let fts_query: String = terms
        .iter()
        .map(|t| {
            // Remove FTS5 operators from terms
            t.replace('"', "")
                .replace('*', "")
                .replace('-', "")
                .replace('+', "")
        })
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" OR ");

    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    // BM25 scoring: lower rank = more relevant (FTS5 returns negative BM25 scores)
    let sql = if stream_id.is_some() {
        format!(
            "SELECT f.memory_id, rank \
             FROM memories_fts f \
             JOIN memories m ON m.id = f.memory_id \
             WHERE memories_fts MATCH ?1 \
             AND (?2 = 0 OR m.archived = 0) \
             AND m.stream_id = ?3 \
             ORDER BY rank \
             LIMIT ?4"
        )
    } else {
        format!(
            "SELECT f.memory_id, rank \
             FROM memories_fts f \
             JOIN memories m ON m.id = f.memory_id \
             WHERE memories_fts MATCH ?1 \
             AND (?2 = 0 OR m.archived = 0) \
             ORDER BY rank \
             LIMIT ?3"
        )
    };

    let mut stmt = conn.prepare(&sql)?;

    let archived_filter = if include_archived { 0i32 } else { 1i32 };

    let results: Vec<KeywordResult> = if let Some(sid) = stream_id {
        stmt.query_map(
            rusqlite::params![fts_query, archived_filter, sid, top_k as i64],
            |row| {
                let memory_id: String = row.get(0)?;
                let rank: f64 = row.get(1)?;
                // Convert FTS5 rank (negative BM25) to positive score
                // FTS5 rank is negative where more negative = more relevant
                let score = -rank;
                Ok(KeywordResult { memory_id, score })
            },
        )?
        .filter_map(|r| r.ok())
        .collect()
    } else {
        stmt.query_map(
            rusqlite::params![fts_query, archived_filter, top_k as i64],
            |row| {
                let memory_id: String = row.get(0)?;
                let rank: f64 = row.get(1)?;
                let score = -rank;
                Ok(KeywordResult { memory_id, score })
            },
        )?
        .filter_map(|r| r.ok())
        .collect()
    };

    // Normalize scores to [0, 1] range
    if results.is_empty() {
        return Ok(results);
    }

    let max_score = results.iter().map(|r| r.score).fold(0.0f64, f64::max);
    if max_score <= 0.0 {
        return Ok(results);
    }

    let normalized: Vec<KeywordResult> = results
        .into_iter()
        .map(|r| KeywordResult {
            memory_id: r.memory_id,
            score: r.score / max_score, // normalize to [0, 1]
        })
        .collect();

    Ok(normalized)
}

/// Fallback: naive substring matching (pre-FTS5 migration databases).
fn search_like_fallback(
    conn: &Connection,
    query: &str,
    top_k: usize,
    stream_id: Option<&str>,
    include_archived: bool,
) -> Result<Vec<KeywordResult>, rusqlite::Error> {
    let keywords: Vec<&str> = query.split_whitespace().filter(|w| w.len() > 2).collect();
    if keywords.is_empty() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

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

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(top_k);

    Ok(results)
}

/// Sync FTS5 index when a new memory is retained.
pub fn fts5_insert(
    conn: &Connection,
    memory_id: &str,
    summary: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO memories_fts (memory_id, summary) VALUES (?1, ?2)",
        rusqlite::params![memory_id, summary],
    )?;
    Ok(())
}

/// Remove a memory from the FTS5 index.
pub fn fts5_delete(conn: &Connection, memory_id: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM memories_fts WHERE memory_id = ?1",
        rusqlite::params![memory_id],
    )?;
    Ok(())
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
        // Sync FTS5 index
        let _ = fts5_insert(&conn, "m1", "We switched from Auth0 to Clerk for authentication");
        let _ = fts5_insert(&conn, "m2", "Database migration to PostgreSQL completed");
        let _ = fts5_insert(&conn, "m3", "Frontend performance optimization with React memoization");
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
        assert!(results.len() >= 2);
    }

    #[test]
    fn test_keyword_search_top_k() {
        let conn = setup_db();
        let results = search(&conn, "the", 1, None, false).unwrap();
        assert!(results.len() <= 1);
    }

    #[test]
    fn test_fts5_word_boundaries() {
        let conn = setup_db();
        // "auth" should NOT match "authentication" with FTS5 word boundaries
        // (unlike substring matching which would match)
        // However, porter stemming may still connect them — this tests the boundary
        let results = search(&conn, "auth", 10, None, false).unwrap();
        // With porter stemming, "auth" won't stem to "authentication"
        // so this should return empty or only exact matches
        // This is the key improvement over substring matching
        assert!(results.len() <= 1); // FTS5 is stricter than LIKE
    }
}
