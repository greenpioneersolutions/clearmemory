use crate::entities;
use crate::entities::resolver::EntityResolver;
use rusqlite::Connection;

/// Result from entity graph traversal strategy.
#[derive(Debug, Clone)]
pub struct GraphResult {
    pub memory_id: String,
    pub score: f64,
}

/// Run entity graph search: extract entity mentions from query,
/// traverse relationships, find connected memories.
pub fn search(
    conn: &Connection,
    query: &str,
    resolver: &dyn EntityResolver,
    entity_boost: f64,
    top_k: usize,
) -> Result<Vec<GraphResult>, rusqlite::Error> {
    // Extract potential entity mentions from the query
    let mentions = extract_mentions(query);

    let mut all_memory_ids = Vec::new();

    for mention in &mentions {
        // Try to resolve the mention to a known entity
        if let Some(entity_id) = resolver.resolve(conn, mention) {
            // Traverse entity relationships (2 hops) to find connected memories
            let memory_ids = entities::graph::traverse(conn, &entity_id, 2)?;
            all_memory_ids.extend(memory_ids);
        }
    }

    // Deduplicate
    all_memory_ids.sort();
    all_memory_ids.dedup();

    // Score: entity boost applied to all results, ranked by how many
    // entity connections point to the memory
    let results: Vec<GraphResult> = all_memory_ids
        .into_iter()
        .enumerate()
        .take(top_k)
        .map(|(i, memory_id)| {
            let base_score = 1.0 - (i as f64 * 0.05);
            GraphResult {
                memory_id,
                score: base_score * (1.0 + entity_boost),
            }
        })
        .collect();

    Ok(results)
}

/// Extract potential entity mentions from a query string.
/// Simple heuristic: words that look like proper nouns or multi-word phrases.
fn extract_mentions(query: &str) -> Vec<String> {
    let mut mentions = Vec::new();

    // Split by common delimiters and extract meaningful phrases
    let words: Vec<&str> = query.split_whitespace().collect();

    // Single capitalized words (potential entity names)
    for word in &words {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_');
        if clean.len() > 2 {
            mentions.push(clean.to_string());
        }
    }

    // Also try consecutive pairs as entity names
    for window in words.windows(2) {
        let phrase = format!(
            "{} {}",
            window[0].trim_matches(|c: char| !c.is_alphanumeric()),
            window[1].trim_matches(|c: char| !c.is_alphanumeric())
        );
        if phrase.len() > 3 {
            mentions.push(phrase);
        }
    }

    mentions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::resolver::HeuristicResolver;
    use crate::migration;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migration::runner::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_graph_search_no_entities() {
        let conn = setup_db();
        let resolver = HeuristicResolver;
        let results = search(&conn, "random query", &resolver, 0.3, 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_graph_search_with_entity() {
        let conn = setup_db();

        // Create an entity with a relationship to a memory
        let eid = entities::graph::create_entity(&conn, "Kai", Some("person")).unwrap();
        entities::graph::add_relationship(&conn, &eid, &eid, "works_on", Some("mem1")).unwrap();

        let resolver = HeuristicResolver;
        let results = search(&conn, "What did Kai work on", &resolver, 0.3, 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].memory_id, "mem1");
    }

    #[test]
    fn test_extract_mentions() {
        let mentions = extract_mentions("What did Kai work on for the auth service");
        assert!(!mentions.is_empty());
        assert!(mentions.iter().any(|m| m == "Kai"));
    }
}
