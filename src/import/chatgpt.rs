use crate::import::RawMemory;
use crate::ImportError;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<RawMemory>, ImportError> {
    let content =
        std::fs::read_to_string(path).map_err(|e| ImportError::ParseError(e.to_string()))?;
    let data: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| ImportError::ParseError(e.to_string()))?;
    let conversations = data
        .as_array()
        .ok_or_else(|| ImportError::ParseError("expected array".into()))?;
    let mut memories = Vec::new();

    for conv in conversations {
        let title = conv["title"].as_str().unwrap_or("Untitled");
        let date = conv["create_time"]
            .as_f64()
            .and_then(|t| chrono::DateTime::from_timestamp(t as i64, 0))
            .map(|dt| dt.to_rfc3339());
        let mut messages = Vec::new();
        if let Some(mapping) = conv["mapping"].as_object() {
            for (_k, node) in mapping {
                if let Some(msg) = node["message"].as_object() {
                    let role = msg
                        .get("author")
                        .and_then(|a| a["role"].as_str())
                        .unwrap_or("unknown");
                    if let Some(parts) = msg.get("content").and_then(|c| c["parts"].as_array()) {
                        for part in parts {
                            if let Some(t) = part.as_str() {
                                if !t.is_empty() {
                                    messages.push(format!("[{role}]: {t}"));
                                }
                            }
                        }
                    }
                }
            }
        }
        if !messages.is_empty() {
            memories.push(RawMemory {
                content: messages.join("\n\n"),
                summary: Some(format!("ChatGPT: {title}")),
                source_format: "chatgpt".into(),
                date,
                author: None,
                tags: Vec::new(),
                metadata: serde_json::json!({"title": title}),
            });
        }
    }
    Ok(memories)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_chatgpt() {
        let json = r#"[{"title":"Auth","create_time":1700000000,"mapping":{"n1":{"message":{"author":{"role":"user"},"content":{"parts":["Use Clerk?"]}}}}}]"#;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.json");
        std::fs::write(&p, json).unwrap();
        let m = parse(&p).unwrap();
        assert_eq!(m.len(), 1);
    }
}
