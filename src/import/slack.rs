use crate::import::RawMemory;
use crate::ImportError;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<RawMemory>, ImportError> {
    if !path.is_dir() {
        return Err(ImportError::ParseError(
            "Slack import requires a directory".into(),
        ));
    }
    let mut memories = Vec::new();
    for entry in std::fs::read_dir(path)
        .map_err(|e| ImportError::ParseError(e.to_string()))?
        .flatten()
    {
        let ch_path = entry.path();
        if ch_path.is_dir() {
            let ch_name = ch_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            if let Ok(m) = parse_channel(&ch_path, ch_name) {
                memories.extend(m);
            }
        }
    }
    Ok(memories)
}

fn parse_channel(dir: &Path, channel: &str) -> Result<Vec<RawMemory>, ImportError> {
    let mut memories = Vec::new();
    for entry in std::fs::read_dir(dir)
        .map_err(|e| ImportError::ParseError(e.to_string()))?
        .flatten()
    {
        if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(msgs) = serde_json::from_str::<Vec<serde_json::Value>>(&content) else {
            continue;
        };

        let mut conv = String::new();
        let mut date = None;
        for msg in &msgs {
            let user = msg["user"].as_str().unwrap_or("unknown");
            let text = msg["text"].as_str().unwrap_or("");
            if date.is_none() {
                date = msg["ts"]
                    .as_str()
                    .and_then(|t| t.split('.').next())
                    .and_then(|s| s.parse::<i64>().ok())
                    .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
                    .map(|dt| dt.to_rfc3339());
            }
            if !text.is_empty() {
                conv.push_str(&format!("[{user}]: {text}\n"));
            }
        }
        if !conv.is_empty() {
            memories.push(RawMemory {
                content: conv,
                summary: Some(format!("Slack #{channel}")),
                source_format: "slack".into(),
                date,
                author: None,
                tags: Vec::new(),
                metadata: serde_json::json!({"channel": channel}),
            });
        }
    }
    Ok(memories)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_slack() {
        let dir = tempfile::tempdir().unwrap();
        let ch = dir.path().join("eng");
        std::fs::create_dir(&ch).unwrap();
        std::fs::write(
            ch.join("2026-01-01.json"),
            r#"[{"user":"U1","text":"Migrate DB","ts":"1700000000.000"}]"#,
        )
        .unwrap();
        let m = parse(dir.path()).unwrap();
        assert_eq!(m.len(), 1);
        assert!(m[0].content.contains("Migrate"));
    }
}
