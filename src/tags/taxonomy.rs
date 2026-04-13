use rusqlite::{params, Connection};

/// Valid tag types.
pub const TAG_TYPES: &[&str] = &["team", "repo", "project", "domain"];

/// A tag attached to a memory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Tag {
    pub tag_type: String,
    pub tag_value: String,
}

/// Validate that a tag type is one of the four supported types.
pub fn validate_tag_type(tag_type: &str) -> Result<(), String> {
    if TAG_TYPES.contains(&tag_type) {
        Ok(())
    } else {
        Err(format!(
            "invalid tag type '{tag_type}': must be one of {}",
            TAG_TYPES.join(", ")
        ))
    }
}

/// Parse a tag string like "team:platform" into (type, value).
pub fn parse_tag(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, ':').collect();
    if parts.len() != 2 || parts[1].is_empty() {
        return Err(format!("invalid tag format '{s}': expected 'type:value'"));
    }
    validate_tag_type(parts[0])?;
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// List all tags, optionally filtered by type.
pub fn list_tags(conn: &Connection, tag_type: Option<&str>) -> Result<Vec<Tag>, rusqlite::Error> {
    let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(tt) = tag_type {
        (
            "SELECT DISTINCT tag_type, tag_value FROM memory_tags WHERE tag_type = ?1 ORDER BY tag_value".to_string(),
            vec![Box::new(tt.to_string())],
        )
    } else {
        (
            "SELECT DISTINCT tag_type, tag_value FROM memory_tags ORDER BY tag_type, tag_value"
                .to_string(),
            vec![],
        )
    };

    let params_ref: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_ref.as_slice(), |row| {
        Ok(Tag {
            tag_type: row.get(0)?,
            tag_value: row.get(1)?,
        })
    })?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Add a tag to a memory.
pub fn add_tag(
    conn: &Connection,
    memory_id: &str,
    tag_type: &str,
    tag_value: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT OR IGNORE INTO memory_tags (memory_id, tag_type, tag_value) VALUES (?1, ?2, ?3)",
        params![memory_id, tag_type, tag_value],
    )?;
    Ok(())
}

/// Remove a tag from a memory.
pub fn remove_tag(
    conn: &Connection,
    memory_id: &str,
    tag_type: &str,
    tag_value: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM memory_tags WHERE memory_id = ?1 AND tag_type = ?2 AND tag_value = ?3",
        params![memory_id, tag_type, tag_value],
    )?;
    Ok(())
}

/// Rename a tag value across all memories.
pub fn rename_tag(
    conn: &Connection,
    tag_type: &str,
    old_value: &str,
    new_value: &str,
) -> Result<usize, rusqlite::Error> {
    let count = conn.execute(
        "UPDATE memory_tags SET tag_value = ?1 WHERE tag_type = ?2 AND tag_value = ?3",
        params![new_value, tag_type, old_value],
    )?;
    Ok(count)
}

/// Get all tags for a memory.
pub fn get_memory_tags(conn: &Connection, memory_id: &str) -> Result<Vec<Tag>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT tag_type, tag_value FROM memory_tags WHERE memory_id = ?1 ORDER BY tag_type, tag_value"
    )?;
    let rows = stmt.query_map(params![memory_id], |row| {
        Ok(Tag {
            tag_type: row.get(0)?,
            tag_value: row.get(1)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tag() {
        let (t, v) = parse_tag("team:platform").unwrap();
        assert_eq!(t, "team");
        assert_eq!(v, "platform");
    }

    #[test]
    fn test_parse_domain_tag() {
        let (t, v) = parse_tag("domain:security/auth").unwrap();
        assert_eq!(t, "domain");
        assert_eq!(v, "security/auth");
    }

    #[test]
    fn test_parse_invalid_tag() {
        assert!(parse_tag("invalid").is_err());
        assert!(parse_tag("badtype:value").is_err());
        assert!(parse_tag("team:").is_err());
    }

    #[test]
    fn test_validate_tag_type() {
        assert!(validate_tag_type("team").is_ok());
        assert!(validate_tag_type("repo").is_ok());
        assert!(validate_tag_type("project").is_ok());
        assert!(validate_tag_type("domain").is_ok());
        assert!(validate_tag_type("invalid").is_err());
    }
}
