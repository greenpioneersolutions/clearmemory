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
            if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(m) = parse_file(&entry.path()) {
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
    let data: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| ImportError::ParseError(e.to_string()))?;

    let messages = data.as_array().or_else(|| data["messages"].as_array());
    let Some(msgs) = messages else {
        return Ok(vec![RawMemory {
            content,
            summary: Some("Claude Code session".into()),
            source_format: "claude_code".into(),
            date: None,
            author: None,
            tags: Vec::new(),
            metadata: serde_json::Value::Null,
        }]);
    };

    let mut conv = String::new();
    for msg in msgs {
        let role = msg["role"].as_str().unwrap_or("unknown");
        let text = msg["content"]
            .as_str()
            .or_else(|| msg["text"].as_str())
            .unwrap_or("");
        if !text.is_empty() {
            conv.push_str(&format!("[{role}]: {text}\n\n"));
        }
    }
    if conv.is_empty() {
        return Ok(Vec::new());
    }

    let summary = conv.lines().next().map(|l| {
        if l.len() > 200 {
            l[..200].to_string()
        } else {
            l.to_string()
        }
    });
    Ok(vec![RawMemory {
        content: conv,
        summary,
        source_format: "claude_code".into(),
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
    fn test_parse_claude() {
        let json =
            r#"[{"role":"user","content":"Fix auth"},{"role":"assistant","content":"Done."}]"#;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("s.json"), json).unwrap();
        assert_eq!(parse(dir.path()).unwrap().len(), 1);
    }
}
