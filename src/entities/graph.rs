use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

/// An entity node in the entity graph.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Entity {
    pub id: String,
    pub canonical_name: String,
    pub entity_type: Option<String>,
    pub first_seen: String,
    pub last_seen: String,
}

/// A relationship edge between two entities.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EntityRelationship {
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relationship: String,
    pub memory_id: Option<String>,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
}

/// Create a new entity.
pub fn create_entity(
    conn: &Connection,
    canonical_name: &str,
    entity_type: Option<&str>,
) -> Result<String, rusqlite::Error> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO entities (id, canonical_name, entity_type, first_seen, last_seen) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, canonical_name, entity_type, now, now],
    )?;

    Ok(id)
}

/// Find an entity by canonical name (case-insensitive).
pub fn find_entity(conn: &Connection, name: &str) -> Result<Option<Entity>, rusqlite::Error> {
    let result = conn.query_row(
        "SELECT id, canonical_name, entity_type, first_seen, last_seen FROM entities \
         WHERE LOWER(canonical_name) = LOWER(?1)",
        params![name],
        map_entity_row,
    );

    match result {
        Ok(e) => Ok(Some(e)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Get an entity by ID.
pub fn get_entity(conn: &Connection, entity_id: &str) -> Result<Option<Entity>, rusqlite::Error> {
    let result = conn.query_row(
        "SELECT id, canonical_name, entity_type, first_seen, last_seen FROM entities WHERE id = ?1",
        params![entity_id],
        map_entity_row,
    );

    match result {
        Ok(e) => Ok(Some(e)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Add a relationship between two entities.
pub fn add_relationship(
    conn: &Connection,
    source_id: &str,
    target_id: &str,
    relationship: &str,
    memory_id: Option<&str>,
) -> Result<(), rusqlite::Error> {
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT OR REPLACE INTO entity_relationships \
         (source_entity_id, target_entity_id, relationship, memory_id, valid_from) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![source_id, target_id, relationship, memory_id, now],
    )?;

    Ok(())
}

/// Traverse the entity graph: find connected entities (1-2 hops) and their memories.
pub fn traverse(
    conn: &Connection,
    entity_id: &str,
    max_hops: usize,
) -> Result<Vec<String>, rusqlite::Error> {
    let mut visited = std::collections::HashSet::new();
    let mut memory_ids = Vec::new();
    let mut frontier = vec![entity_id.to_string()];

    for _ in 0..max_hops {
        let mut next_frontier = Vec::new();

        for eid in &frontier {
            if !visited.insert(eid.clone()) {
                continue;
            }

            // Get outgoing relationships
            let mut stmt = conn.prepare(
                "SELECT target_entity_id, memory_id FROM entity_relationships \
                 WHERE source_entity_id = ?1 AND valid_until IS NULL",
            )?;
            let rows = stmt.query_map(params![eid], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?;

            for row in rows.flatten() {
                let (target_id, mem_id) = row;
                next_frontier.push(target_id);
                if let Some(mid) = mem_id {
                    memory_ids.push(mid);
                }
            }

            // Get incoming relationships
            let mut stmt = conn.prepare(
                "SELECT source_entity_id, memory_id FROM entity_relationships \
                 WHERE target_entity_id = ?1 AND valid_until IS NULL",
            )?;
            let rows = stmt.query_map(params![eid], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?;

            for row in rows.flatten() {
                let (source_id, mem_id) = row;
                next_frontier.push(source_id);
                if let Some(mid) = mem_id {
                    memory_ids.push(mid);
                }
            }
        }

        frontier = next_frontier;
    }

    // Deduplicate
    memory_ids.sort();
    memory_ids.dedup();

    Ok(memory_ids)
}

fn map_entity_row(row: &rusqlite::Row<'_>) -> Result<Entity, rusqlite::Error> {
    Ok(Entity {
        id: row.get(0)?,
        canonical_name: row.get(1)?,
        entity_type: row.get(2)?,
        first_seen: row.get(3)?,
        last_seen: row.get(4)?,
    })
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
    fn test_create_and_find_entity() {
        let conn = setup_db();
        let id = create_entity(&conn, "Auth Service", Some("service")).unwrap();

        let found = find_entity(&conn, "auth service").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, id);
    }

    #[test]
    fn test_relationships_and_traverse() {
        let conn = setup_db();
        let e1 = create_entity(&conn, "Kai", Some("person")).unwrap();
        let e2 = create_entity(&conn, "Auth Project", Some("project")).unwrap();

        add_relationship(&conn, &e1, &e2, "works_on", Some("mem1")).unwrap();

        let memories = traverse(&conn, &e1, 2).unwrap();
        assert_eq!(memories, vec!["mem1"]);
    }

    #[test]
    fn test_find_nonexistent_entity() {
        let conn = setup_db();
        assert!(find_entity(&conn, "nope").unwrap().is_none());
    }
}
