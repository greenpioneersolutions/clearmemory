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
            if matches!(p.extension().and_then(|e| e.to_str()), Some("log" | "json")) {
                if let Ok(m) = parse_file(&p) {
                    memories.extend(m);
                }
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
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }

    // Try JSON
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
        if let Some(msgs) = data.as_array().or_else(|| data["messages"].as_array()) {
            let mut conv = String::new();
            for msg in msgs {
                let role = msg["role"].as_str().unwrap_or("unknown");
                let text = msg["content"].as_str().unwrap_or("");
                if !text.is_empty() {
                    conv.push_str(&format!("[{role}]: {text}\n\n"));
                }
            }
            if !conv.is_empty() {
                let summary = conv.lines().next().map(|l| {
                    if l.len() > 200 {
                        l[..200].to_string()
                    } else {
                        l.to_string()
                    }
                });
                return Ok(vec![RawMemory {
                    content: conv,
                    summary,
                    source_format: "copilot".into(),
                    date: None,
                    author: None,
                    tags: Vec::new(),
                    metadata: serde_json::Value::Null,
                }]);
            }
        }
    }

    let summary = content.lines().next().map(|l| {
        if l.len() > 200 {
            l[..200].to_string()
        } else {
            l.to_string()
        }
    });
    Ok(vec![RawMemory {
        content,
        summary,
        source_format: "copilot".into(),
        date: None,
        author: None,
        tags: Vec::new(),
        metadata: serde_json::Value::Null,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_copilot_log() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("s.log"), "User: Fix DB\nCopilot: Done.").unwrap();
        assert_eq!(parse(dir.path()).unwrap().len(), 1);
    }
}
