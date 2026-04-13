use crate::security::secret_scanner::{SecretMatch, SecretScanner};

/// Redact detected secrets in content, replacing them with `[REDACTED:<type>]`.
pub fn redact(content: &str, matches: &[SecretMatch]) -> String {
    if matches.is_empty() {
        return content.to_string();
    }

    // Sort matches by start position (reverse) to replace from end to start
    let mut sorted: Vec<&SecretMatch> = matches.iter().collect();
    sorted.sort_by(|a, b| b.start.cmp(&a.start));

    let mut result = content.to_string();
    for m in sorted {
        let replacement = format!("[REDACTED:{}]", m.pattern_name);
        result.replace_range(m.start..m.end, &replacement);
    }

    result
}

/// Scan and redact secrets in content. Returns (redacted_content, secrets_found).
pub fn scan_and_redact(scanner: &SecretScanner, content: &str) -> (String, Vec<SecretMatch>) {
    let matches = scanner.scan(content);
    let redacted = redact(content, &matches);
    (redacted, matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_aws_key() {
        let scanner = SecretScanner::new();
        let content = "Use key AKIAIOSFODNN7EXAMPLE for access";
        let (redacted, matches) = scan_and_redact(&scanner, content);

        assert!(!matches.is_empty());
        assert!(redacted.contains("[REDACTED:aws_key]"));
        assert!(!redacted.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn test_redact_no_secrets() {
        let scanner = SecretScanner::new();
        let content = "No secrets here, just normal text.";
        let (redacted, matches) = scan_and_redact(&scanner, content);

        assert!(matches.is_empty());
        assert_eq!(redacted, content);
    }

    #[test]
    fn test_redact_preserves_surrounding_text() {
        let scanner = SecretScanner::new();
        let content = "before postgres://user:pass@host/db after";
        let (redacted, _) = scan_and_redact(&scanner, content);

        assert!(redacted.starts_with("before "));
        assert!(redacted.ends_with(" after"));
    }
}
