pub mod chatgpt;
pub mod claude_code;
pub mod clear_format;
pub mod converter;
pub mod copilot;
pub mod markdown;
pub mod slack;

use crate::ImportError;
use std::path::Path;

/// A raw memory parsed from an import source.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RawMemory {
    pub content: String,
    pub summary: Option<String>,
    pub source_format: String,
    pub date: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<(String, String)>,
    pub metadata: serde_json::Value,
}

/// Supported import formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    ClaudeCode,
    Copilot,
    ChatGpt,
    Slack,
    Markdown,
    ClearFormat,
    Auto,
}

impl ImportFormat {
    pub fn parse_name(s: &str) -> Result<Self, ImportError> {
        match s {
            "claude_code" => Ok(Self::ClaudeCode),
            "copilot" => Ok(Self::Copilot),
            "chatgpt" => Ok(Self::ChatGpt),
            "slack" => Ok(Self::Slack),
            "markdown" => Ok(Self::Markdown),
            "clear" => Ok(Self::ClearFormat),
            "auto" => Ok(Self::Auto),
            other => Err(ImportError::UnsupportedFormat(other.to_string())),
        }
    }
}

/// Auto-detect the format of a file or directory.
pub fn detect_format(path: &Path) -> Result<ImportFormat, ImportError> {
    if !path.exists() {
        return Err(ImportError::FileNotFound(path.display().to_string()));
    }

    // Check extension first
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext {
            "clear" => return Ok(ImportFormat::ClearFormat),
            "md" | "txt" => return Ok(ImportFormat::Markdown),
            "csv" | "xlsx" => return Ok(ImportFormat::ClearFormat), // Needs conversion first
            _ => {}
        }
    }

    // For JSON files, inspect structure
    if path.extension().and_then(|e| e.to_str()) == Some("json") {
        if let Ok(content) = std::fs::read_to_string(path) {
            if content.contains("clear_format_version") {
                return Ok(ImportFormat::ClearFormat);
            }
            if content.contains("\"conversations\"") || content.contains("\"mapping\"") {
                return Ok(ImportFormat::ChatGpt);
            }
        }
    }

    // For directories, check structure
    if path.is_dir() {
        // Check for Slack export structure (channels as subdirectories with JSON)
        let has_json_subdirs = std::fs::read_dir(path)
            .map(|entries| {
                entries.flatten().any(|e| {
                    e.path().is_dir()
                        && std::fs::read_dir(e.path())
                            .map(|inner| {
                                inner.flatten().any(|f| {
                                    f.path().extension().and_then(|e| e.to_str()) == Some("json")
                                })
                            })
                            .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        if has_json_subdirs {
            return Ok(ImportFormat::Slack);
        }

        // Default: treat directory of files as markdown
        return Ok(ImportFormat::Markdown);
    }

    Err(ImportError::DetectionFailed(path.display().to_string()))
}

/// Parse a file or directory into raw memories using the specified format.
pub fn parse(path: &Path, format: ImportFormat) -> Result<Vec<RawMemory>, ImportError> {
    let format = if format == ImportFormat::Auto {
        detect_format(path)?
    } else {
        format
    };

    match format {
        ImportFormat::ClearFormat => clear_format::parse(path),
        ImportFormat::Markdown => markdown::parse(path),
        ImportFormat::ChatGpt => chatgpt::parse(path),
        ImportFormat::ClaudeCode => claude_code::parse(path),
        ImportFormat::Copilot => copilot::parse(path),
        ImportFormat::Slack => slack::parse(path),
        ImportFormat::Auto => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_from_str() {
        assert_eq!(
            ImportFormat::parse_name("clear").unwrap(),
            ImportFormat::ClearFormat
        );
        assert_eq!(
            ImportFormat::parse_name("auto").unwrap(),
            ImportFormat::Auto
        );
        assert!(ImportFormat::parse_name("invalid").is_err());
    }
}
