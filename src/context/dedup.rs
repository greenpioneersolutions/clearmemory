use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// Tracks content hashes of known context sources for deduplication.
pub struct ContextDedup {
    known_hashes: HashSet<String>,
}

impl ContextDedup {
    pub fn new() -> Self {
        Self {
            known_hashes: HashSet::new(),
        }
    }

    /// Register a known context source (e.g., CLAUDE.md contents, file passed via --add-dir).
    pub fn register_known_content(&mut self, content: &str) {
        let hash = hash_content(content);
        self.known_hashes.insert(hash);

        // Also register paragraph-level hashes for partial overlap detection
        for paragraph in content.split("\n\n") {
            let trimmed = paragraph.trim();
            if trimmed.len() > 50 {
                self.known_hashes.insert(hash_content(trimmed));
            }
        }
    }

    /// Check if a memory's content is already in the known context.
    pub fn is_duplicate(&self, content: &str) -> bool {
        let hash = hash_content(content);
        self.known_hashes.contains(&hash)
    }

    /// Get the number of registered content hashes.
    pub fn known_count(&self) -> usize {
        self.known_hashes.len()
    }
}

impl Default for ContextDedup {
    fn default() -> Self {
        Self::new()
    }
}

fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_exact_match() {
        let mut dedup = ContextDedup::new();
        dedup.register_known_content("This is already in the CLAUDE.md file");
        assert!(dedup.is_duplicate("This is already in the CLAUDE.md file"));
        assert!(!dedup.is_duplicate("This is a new memory"));
    }

    #[test]
    fn test_dedup_paragraph_level() {
        let mut dedup = ContextDedup::new();
        let content = "Short paragraph.\n\nThis is a longer paragraph that should be registered as a separate hash for dedup purposes.";
        dedup.register_known_content(content);

        // The long paragraph should be registered
        assert!(dedup.is_duplicate("This is a longer paragraph that should be registered as a separate hash for dedup purposes."));
    }

    #[test]
    fn test_dedup_empty() {
        let dedup = ContextDedup::new();
        assert!(!dedup.is_duplicate("anything"));
        assert_eq!(dedup.known_count(), 0);
    }
}
