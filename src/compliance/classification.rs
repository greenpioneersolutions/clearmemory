//! Content classification based on secret scanning results.

use crate::security::secret_scanner::SecretScanner;
use crate::Classification;

/// Classify content based on whether it contains secrets.
///
/// If the scanner detects secrets, the content is classified as `Confidential`.
/// Otherwise, it defaults to `Internal`.
pub fn classify_content(content: &str, scanner: &SecretScanner) -> Classification {
    if scanner.has_secrets(content) {
        Classification::Confidential
    } else {
        Classification::Internal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_normal_content_as_internal() {
        let scanner = SecretScanner::new();
        let classification =
            classify_content("Regular discussion about architecture decisions", &scanner);
        assert_eq!(classification, Classification::Internal);
    }

    #[test]
    fn test_classify_content_with_secrets_as_confidential() {
        let scanner = SecretScanner::new();
        let classification = classify_content(
            "Connect using postgres://admin:secret@db.host/prod",
            &scanner,
        );
        assert_eq!(classification, Classification::Confidential);
    }

    #[test]
    fn test_classify_content_with_aws_key() {
        let scanner = SecretScanner::new();
        let classification = classify_content("AWS key is AKIAIOSFODNN7EXAMPLE", &scanner);
        assert_eq!(classification, Classification::Confidential);
    }
}
