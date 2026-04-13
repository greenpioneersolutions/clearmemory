use rusqlite::{params, Connection};

/// Check if a user can read from a stream.
pub fn can_read(
    conn: &Connection,
    user_id: &str,
    stream_id: &str,
) -> Result<bool, rusqlite::Error> {
    let (owner_id, visibility): (String, String) = conn.query_row(
        "SELECT owner_id, visibility FROM streams WHERE id = ?1",
        params![stream_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    Ok(match visibility.as_str() {
        "org" => true,
        "team" => {
            // Owner can always read, plus check if user has write access (implies read)
            owner_id == user_id || has_write_access(conn, user_id, stream_id)?
        }
        _ => owner_id == user_id, // private
    })
}

/// Check if a user can write to a stream.
pub fn can_write(
    conn: &Connection,
    user_id: &str,
    stream_id: &str,
) -> Result<bool, rusqlite::Error> {
    let owner_id: String = conn.query_row(
        "SELECT owner_id FROM streams WHERE id = ?1",
        params![stream_id],
        |row| row.get(0),
    )?;

    if owner_id == user_id {
        return Ok(true);
    }

    has_write_access(conn, user_id, stream_id)
}

fn has_write_access(
    conn: &Connection,
    user_id: &str,
    stream_id: &str,
) -> Result<bool, rusqlite::Error> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM stream_writers WHERE stream_id = ?1 AND user_id = ?2",
        params![stream_id, user_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration;
    use crate::streams::manager;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migration::runner::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_owner_can_read_and_write_private() {
        let conn = setup_db();
        let id = manager::create_stream(&conn, "private-stream", None, "owner1", "private", &[])
            .unwrap();

        assert!(can_read(&conn, "owner1", &id).unwrap());
        assert!(can_write(&conn, "owner1", &id).unwrap());
        assert!(!can_read(&conn, "other", &id).unwrap());
        assert!(!can_write(&conn, "other", &id).unwrap());
    }

    #[test]
    fn test_org_visibility_anyone_reads() {
        let conn = setup_db();
        let id =
            manager::create_stream(&conn, "public-stream", None, "owner1", "org", &[]).unwrap();

        assert!(can_read(&conn, "anyone", &id).unwrap());
        assert!(!can_write(&conn, "anyone", &id).unwrap());
    }

    #[test]
    fn test_write_access_grants_read() {
        let conn = setup_db();
        let id = manager::create_stream(&conn, "team-stream", None, "owner1", "team", &[]).unwrap();
        manager::grant_write_access(&conn, &id, "writer1").unwrap();

        assert!(can_read(&conn, "writer1", &id).unwrap());
        assert!(can_write(&conn, "writer1", &id).unwrap());
    }
}
