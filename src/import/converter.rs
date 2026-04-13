use crate::ImportError;
use std::path::Path;

pub fn csv_to_clear(input: &Path, mapping: &str) -> Result<String, ImportError> {
    let content =
        std::fs::read_to_string(input).map_err(|e| ImportError::ParseError(e.to_string()))?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(content.as_bytes());
    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| ImportError::ParseError(e.to_string()))?
        .iter()
        .map(String::from)
        .collect();
    let col_map = if mapping == "auto" {
        auto_map(&headers)
    } else {
        parse_mapping(mapping, &headers)?
    };

    let mut memories = Vec::new();
    for result in reader.records() {
        let record = result.map_err(|e| ImportError::ParseError(e.to_string()))?;
        let date = col_map.date.and_then(|i| record.get(i).map(String::from));
        let author = col_map.author.and_then(|i| record.get(i).map(String::from));
        let text = col_map
            .content
            .and_then(|i| record.get(i).map(String::from))
            .unwrap_or_default();
        if !text.is_empty() {
            memories.push(serde_json::json!({"date": date, "author": author, "content": text, "tags": {}, "metadata": {}}));
        }
    }

    serde_json::to_string_pretty(&serde_json::json!({
        "clear_format_version": "1.0", "source": "csv-import",
        "exported_at": chrono::Utc::now().to_rfc3339(), "memories": memories
    }))
    .map_err(|e| ImportError::ParseError(e.to_string()))
}

struct ColMap {
    date: Option<usize>,
    author: Option<usize>,
    content: Option<usize>,
}

fn auto_map(headers: &[String]) -> ColMap {
    let mut m = ColMap {
        date: None,
        author: None,
        content: None,
    };
    for (i, h) in headers.iter().enumerate() {
        let l = h.to_lowercase();
        if l.contains("date") || l.contains("time") {
            m.date = Some(i);
        } else if l.contains("author") || l.contains("user") || l.contains("name") {
            m.author = Some(i);
        } else if l.contains("content")
            || l.contains("text")
            || l.contains("note")
            || l.contains("message")
        {
            m.content = Some(i);
        }
    }
    if m.content.is_none() && !headers.is_empty() {
        m.content = Some(headers.len() - 1);
    }
    m
}

fn parse_mapping(mapping: &str, headers: &[String]) -> Result<ColMap, ImportError> {
    let mut m = ColMap {
        date: None,
        author: None,
        content: None,
    };
    for pair in mapping.split(',') {
        let p: Vec<&str> = pair.split('=').collect();
        if p.len() != 2 {
            continue;
        }
        let idx = headers
            .iter()
            .position(|h| h == p[1].trim())
            .ok_or_else(|| ImportError::ParseError(format!("column '{}' not found", p[1])))?;
        match p[0].trim() {
            "date" => m.date = Some(idx),
            "author" => m.author = Some(idx),
            "content" | "notes" => m.content = Some(idx),
            _ => {}
        }
    }
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_csv_to_clear() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("d.csv");
        std::fs::write(&p, "date,author,content\n2026-03-15,Sarah,Used Clerk").unwrap();
        let r = csv_to_clear(&p, "auto").unwrap();
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(v["memories"].as_array().unwrap().len(), 1);
    }
}
