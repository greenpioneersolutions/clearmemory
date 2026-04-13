use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

/// A stream: a scoped view across tag intersections.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Stream {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: String,
    pub visibility: String,
    pub created_at: String,
}

/// Create a new stream with tag filters.
pub fn create_stream(
    conn: &Connection,
    name: &str,
    description: Option<&str>,
    owner_id: &str,
    visibility: &str,
    tags: &[(String, String)],
) -> Result<String, rusqlite::Error> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO streams (id, name, description, owner_id, visibility, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, name, description, owner_id, visibility, now],
    )?;

    for (tag_type, tag_value) in tags {
        conn.execute(
            "INSERT INTO stream_tags (stream_id, tag_type, tag_value) VALUES (?1, ?2, ?3)",
            params![id, tag_type, tag_value],
        )?;
    }

    Ok(id)
}

/// List all streams.
pub fn list_streams(conn: &Connection) -> Result<Vec<Stream>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, owner_id, visibility, created_at FROM streams ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Stream {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            owner_id: row.get(3)?,
            visibility: row.get(4)?,
            created_at: row.get(5)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Get a stream by ID or name.
pub fn get_stream(conn: &Connection, id_or_name: &str) -> Result<Option<Stream>, rusqlite::Error> {
    let result = conn.query_row(
        "SELECT id, name, description, owner_id, visibility, created_at FROM streams \
         WHERE id = ?1 OR name = ?1",
        params![id_or_name],
        |row| {
            Ok(Stream {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                owner_id: row.get(3)?,
                visibility: row.get(4)?,
                created_at: row.get(5)?,
            })
        },
    );

    match result {
        Ok(s) => Ok(Some(s)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Grant write access to a user on a stream.
pub fn grant_write_access(
    conn: &Connection,
    stream_id: &str,
    user_id: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT OR IGNORE INTO stream_writers (stream_id, user_id) VALUES (?1, ?2)",
        params![stream_id, user_id],
    )?;
    Ok(())
}

/// Get the tag filters for a stream.
pub fn get_stream_tags(
    conn: &Connection,
    stream_id: &str,
) -> Result<Vec<(String, String)>, rusqlite::Error> {
    let mut stmt =
        conn.prepare("SELECT tag_type, tag_value FROM stream_tags WHERE stream_id = ?1")?;
    let rows = stmt.query_map(params![stream_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
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

    #[test]
    fn test_create_and_list_streams() {
        let conn = setup_db();

        let id = create_stream(
            &conn,
            "Platform Auth",
            Some("Platform team auth stream"),
            "user1",
            "team",
            &[
                ("team".into(), "platform".into()),
                ("domain".into(), "security/auth".into()),
            ],
        )
        .unwrap();

        assert!(!id.is_empty());

        let streams = list_streams(&conn).unwrap();
        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].name, "Platform Auth");
    }

    #[test]
    fn test_get_stream_by_name() {
        let conn = setup_db();
        create_stream(&conn, "test-stream", None, "user1", "private", &[]).unwrap();

        let stream = get_stream(&conn, "test-stream").unwrap();
        assert!(stream.is_some());
        assert_eq!(stream.unwrap().name, "test-stream");
    }

    #[test]
    fn test_get_nonexistent_stream() {
        let conn = setup_db();
        let stream = get_stream(&conn, "nonexistent").unwrap();
        assert!(stream.is_none());
    }

    #[test]
    fn test_stream_tags() {
        let conn = setup_db();
        let id = create_stream(
            &conn,
            "tagged",
            None,
            "user1",
            "private",
            &[("team".into(), "frontend".into())],
        )
        .unwrap();

        let tags = get_stream_tags(&conn, &id).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0], ("team".into(), "frontend".into()));
    }
}
