use crate::entities::{aliases, graph};
use rusqlite::Connection;

/// Trait for entity resolution — maps mentions in text to entity IDs.
pub trait EntityResolver: Send + Sync {
    fn resolve(&self, conn: &Connection, mention: &str) -> Option<String>;
}

/// Tier 1 heuristic resolver: exact string, case-insensitive, alias lookup.
pub struct HeuristicResolver;

impl EntityResolver for HeuristicResolver {
    fn resolve(&self, conn: &Connection, mention: &str) -> Option<String> {
        // Try canonical name first
        if let Ok(Some(entity)) = graph::find_entity(conn, mention) {
            return Some(entity.id);
        }

        // Try alias lookup
        if let Ok(Some(entity_id)) = aliases::find_by_alias(conn, mention) {
            return Some(entity_id);
        }

        None
    }
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

    #[test]
    fn test_heuristic_resolve_by_name() {
        let conn = setup_db();
        let id = graph::create_entity(&conn, "Auth Service", Some("service")).unwrap();

        let resolver = HeuristicResolver;
        let result = resolver.resolve(&conn, "Auth Service");
        assert_eq!(result, Some(id));
    }

    #[test]
    fn test_heuristic_resolve_by_alias() {
        let conn = setup_db();
        let id = graph::create_entity(&conn, "Auth Service", Some("service")).unwrap();
        aliases::add_alias(&conn, "login-service", &id).unwrap();

        let resolver = HeuristicResolver;
        let result = resolver.resolve(&conn, "login-service");
        assert_eq!(result, Some(id));
    }

    #[test]
    fn test_heuristic_resolve_unknown() {
        let conn = setup_db();
        let resolver = HeuristicResolver;
        assert!(resolver.resolve(&conn, "unknown thing").is_none());
    }
}
