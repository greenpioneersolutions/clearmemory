use regex::RegexSet;

/// A detected secret in content.
#[derive(Debug, Clone)]
pub struct SecretMatch {
    pub pattern_name: String,
    pub start: usize,
    pub end: usize,
}

/// Secret scanner using compiled regex patterns.
pub struct SecretScanner {
    patterns: RegexSet,
    pattern_names: Vec<String>,
    individual_patterns: Vec<regex::Regex>,
}

impl SecretScanner {
    /// Create a scanner with the built-in detection patterns.
    pub fn new() -> Self {
        let (names, regexes): (Vec<String>, Vec<String>) = built_in_patterns().into_iter().unzip();

        let patterns = RegexSet::new(&regexes).expect("built-in regex patterns should be valid");
        let individual_patterns: Vec<regex::Regex> = regexes
            .iter()
            .map(|r| regex::Regex::new(r).expect("built-in regex should be valid"))
            .collect();

        Self {
            patterns,
            pattern_names: names,
            individual_patterns,
        }
    }

    /// Scan content for secrets. Returns all matches.
    pub fn scan(&self, content: &str) -> Vec<SecretMatch> {
        let matches: Vec<usize> = self.patterns.matches(content).into_iter().collect();
        let mut results = Vec::new();

        for idx in matches {
            for m in self.individual_patterns[idx].find_iter(content) {
                results.push(SecretMatch {
                    pattern_name: self.pattern_names[idx].clone(),
                    start: m.start(),
                    end: m.end(),
                });
            }
        }

        results
    }

    /// Check if content contains any secrets.
    pub fn has_secrets(&self, content: &str) -> bool {
        self.patterns.is_match(content)
    }
}

impl Default for SecretScanner {
    fn default() -> Self {
        Self::new()
    }
}

fn built_in_patterns() -> Vec<(String, String)> {
    vec![
        ("aws_key".into(), r"(?:AKIA[0-9A-Z]{16})".into()),
        ("aws_secret".into(), r"(?i)aws_secret_access_key\s*[=:]\s*\S+".into()),
        ("github_token".into(), r"(?:ghp_[a-zA-Z0-9]{36}|gho_[a-zA-Z0-9]{36}|ghs_[a-zA-Z0-9]{36}|github_pat_[a-zA-Z0-9_]{22,})".into()),
        ("generic_api_key".into(), r"(?i)(?:api_key|apikey|x-api-key)\s*[=:]\s*\S+".into()),
        ("database_url".into(), r"(?:postgres|mysql|mongodb|redis)://\S+".into()),
        ("private_key".into(), r"-----BEGIN (?:RSA |OPENSSH )?PRIVATE KEY-----".into()),
        ("jwt_token".into(), r"eyJ[a-zA-Z0-9_-]{10,}\.eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]+".into()),
        ("generic_password".into(), r"(?i)(?:password|passwd|secret)\s*[=:]\s*\S{8,}".into()),
        ("anthropic_key".into(), r"sk-ant-[a-zA-Z0-9_-]{20,}".into()),
        ("openai_key".into(), r"sk-(?:proj-)?[a-zA-Z0-9]{40,}".into()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_aws_key() {
        let scanner = SecretScanner::new();
        let matches = scanner.scan("Found key AKIAIOSFODNN7EXAMPLE in config");
        assert!(!matches.is_empty());
        assert_eq!(matches[0].pattern_name, "aws_key");
    }

    #[test]
    fn test_detect_github_token() {
        let scanner = SecretScanner::new();
        assert!(scanner.has_secrets("token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij"));
    }

    #[test]
    fn test_detect_database_url() {
        let scanner = SecretScanner::new();
        let matches = scanner.scan("DATABASE_URL=postgres://user:pass@host/db");
        assert!(!matches.is_empty());
        assert_eq!(matches[0].pattern_name, "database_url");
    }

    #[test]
    fn test_detect_private_key() {
        let scanner = SecretScanner::new();
        assert!(scanner.has_secrets("-----BEGIN RSA PRIVATE KEY-----\nMIIE..."));
    }

    #[test]
    fn test_no_false_positive_on_normal_text() {
        let scanner = SecretScanner::new();
        let matches = scanner.scan("This is normal text about authentication and security.");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_detect_anthropic_key() {
        let scanner = SecretScanner::new();
        assert!(scanner.has_secrets("sk-ant-api03-abcdefghijklmnopqrstuvwx"));
    }

    #[test]
    fn test_multiple_secrets() {
        let scanner = SecretScanner::new();
        let content = "key=AKIAIOSFODNN7EXAMPLE and url=postgres://user:pass@host/db";
        let matches = scanner.scan(content);
        assert!(matches.len() >= 2);
    }
}
