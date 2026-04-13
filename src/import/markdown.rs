use crate::import::RawMemory;
use crate::ImportError;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<RawMemory>, ImportError> {
    if path.is_dir() {
        let mut memories = Vec::new();
        for entry in std::fs::read_dir(path)
            .map_err(|e| ImportError::ParseError(e.to_string()))?
            .flatten()
        {
            let p = entry.path();
            if matches!(p.extension().and_then(|e| e.to_str()), Some("md" | "txt")) {
                memories.extend(parse_file(&p)?);
            }
        }
        Ok(memories)
    } else {
        parse_file(path)
    }
}

fn parse_file(path: &Path) -> Result<Vec<RawMemory>, ImportError> {
    let content =
        std::fs::read_to_string(path).map_err(|e| ImportError::ParseError(e.to_string()))?;
    Ok(split_by_headings(&content)
        .into_iter()
        .filter(|s| s.trim().len() > 10)
        .map(|section| {
            let summary = section.lines().find(|l| !l.trim().is_empty()).map(|l| {
                let c = l.trim_start_matches('#').trim();
                if c.len() > 200 {
                    c[..200].to_string()
                } else {
                    c.to_string()
                }
            });
            RawMemory {
                content: section,
                summary,
                source_format: "markdown".into(),
                date: None,
                author: None,
                tags: Vec::new(),
                metadata: serde_json::Value::Null,
            }
        })
        .collect())
}

fn split_by_headings(content: &str) -> Vec<String> {
    let mut sections = Vec::new();
    let mut current = String::new();
    for line in content.lines() {
        if (line.starts_with("# ") || line.starts_with("## ")) && !current.trim().is_empty() {
            sections.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        sections.push(current);
    }
    sections
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_markdown() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("n.md"),
            "# Auth\nWe chose Clerk over Auth0.\n\n# DB\nMoved to PostgreSQL.",
        )
        .unwrap();
        let m = parse(dir.path()).unwrap();
        assert_eq!(m.len(), 2);
    }
}
