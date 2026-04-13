use crate::import::RawMemory;
use crate::ImportError;
use std::path::Path;

#[derive(serde::Deserialize)]
struct ClearFile {
    #[allow(dead_code)]
    clear_format_version: String,
    memories: Vec<ClearMemory>,
}

#[derive(serde::Deserialize)]
struct ClearMemory {
    date: Option<String>,
    author: Option<String>,
    #[serde(default)]
    content: String,
    #[serde(default)]
    tags: ClearTags,
    #[serde(default)]
    metadata: serde_json::Value,
}

#[derive(Default, serde::Deserialize)]
struct ClearTags {
    team: Option<String>,
    repo: Option<String>,
    project: Option<String>,
    domain: Option<String>,
}

pub fn parse(path: &Path) -> Result<Vec<RawMemory>, ImportError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ImportError::ParseError(format!("read error: {e}")))?;
    let clear_file: ClearFile = serde_json::from_str(&content)
        .map_err(|e| ImportError::ParseError(format!("json error: {e}")))?;

    Ok(clear_file
        .memories
        .into_iter()
        .filter(|m| !m.content.is_empty())
        .map(|m| {
            let mut tags = Vec::new();
            if let Some(t) = &m.tags.team {
                tags.push(("team".into(), t.clone()));
            }
            if let Some(r) = &m.tags.repo {
                tags.push(("repo".into(), r.clone()));
            }
            if let Some(p) = &m.tags.project {
                tags.push(("project".into(), p.clone()));
            }
            if let Some(d) = &m.tags.domain {
                tags.push(("domain".into(), d.clone()));
            }
            let summary = m.content.lines().next().map(|l| {
                if l.len() > 200 {
                    l[..200].to_string()
                } else {
                    l.to_string()
                }
            });
            RawMemory {
                content: m.content,
                summary,
                source_format: "clear".into(),
                date: m.date,
                author: m.author,
                tags,
                metadata: m.metadata,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_clear_format() {
        let json = r#"{"clear_format_version":"1.0","memories":[{"date":"2026-03-15","author":"Sarah","content":"Decided to use Clerk.","tags":{"team":"platform"}}]}"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.clear");
        std::fs::write(&path, json).unwrap();
        let memories = parse(&path).unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].tags.len(), 1);
    }
}
