use sha2::{Digest, Sha256};

/// Compute a chained hash: SHA-256(previous_hash + content).
pub fn compute_chain_hash(previous_hash: &str, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(previous_hash.as_bytes());
    hasher.update(b"|");
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

/// Verify the integrity of a chain of audit entries.
/// Returns Ok(()) if the chain is valid, or an error with the ID of the first broken entry.
pub fn verify_chain(
    entries: &[(String, String, String)], // (id, hash, previous_hash)
) -> Result<(), String> {
    if entries.is_empty() {
        return Ok(());
    }

    for window in entries.windows(2) {
        let (_, current_hash, _) = &window[0];
        let (next_id, _, next_previous) = &window[1];

        if next_previous != current_hash {
            return Err(format!(
                "chain broken at entry {next_id}: expected previous_hash={current_hash}, got {next_previous}"
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_hash_deterministic() {
        let h1 = compute_chain_hash("prev", "content");
        let h2 = compute_chain_hash("prev", "content");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_chain_hash_differs_with_different_inputs() {
        let h1 = compute_chain_hash("prev", "content1");
        let h2 = compute_chain_hash("prev", "content2");
        assert_ne!(h1, h2);

        let h3 = compute_chain_hash("prev1", "content");
        let h4 = compute_chain_hash("prev2", "content");
        assert_ne!(h3, h4);
    }

    #[test]
    fn test_verify_valid_chain() {
        let entries = vec![
            ("1".to_string(), "hash_a".to_string(), "genesis".to_string()),
            ("2".to_string(), "hash_b".to_string(), "hash_a".to_string()),
            ("3".to_string(), "hash_c".to_string(), "hash_b".to_string()),
        ];
        assert!(verify_chain(&entries).is_ok());
    }

    #[test]
    fn test_verify_broken_chain() {
        let entries = vec![
            ("1".to_string(), "hash_a".to_string(), "genesis".to_string()),
            ("2".to_string(), "hash_b".to_string(), "WRONG".to_string()),
        ];
        assert!(verify_chain(&entries).is_err());
    }

    #[test]
    fn test_verify_empty_chain() {
        assert!(verify_chain(&[]).is_ok());
    }
}
