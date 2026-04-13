use crate::facts::temporal::Fact;
use chrono::Utc;
use uuid::Uuid;

/// Extract facts from text content (Tier 1: rule-based).
///
/// Conservative — better to miss facts than to extract incorrect ones.
/// The system works fine with zero extracted facts (it still has verbatim
/// storage + semantic search). Facts are a progressive enhancement.
pub fn extract_facts(content: &str, memory_id: &str) -> Vec<Fact> {
    let mut facts = Vec::new();
    let now = Utc::now().to_rfc3339();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Pattern: "X uses Y" / "X switched to Y" / "X migrated to Y"
        if let Some(fact) = try_extract_uses_pattern(trimmed, memory_id, &now) {
            facts.push(fact);
        }

        // Pattern: "decided to X" / "we decided X"
        if let Some(fact) = try_extract_decision_pattern(trimmed, memory_id, &now) {
            facts.push(fact);
        }
    }

    facts
}

fn try_extract_uses_pattern(line: &str, memory_id: &str, now: &str) -> Option<Fact> {
    let lower = line.to_lowercase();

    let patterns = [
        (" uses ", "uses"),
        (" switched to ", "switched_to"),
        (" migrated to ", "migrated_to"),
        (" replaced by ", "replaced_by"),
    ];

    for (pattern, predicate) in &patterns {
        if let Some(pos) = lower.find(pattern) {
            let subject = line[..pos].trim();
            let object = line[pos + pattern.len()..].trim().trim_end_matches('.');

            if !subject.is_empty()
                && !object.is_empty()
                && subject.len() < 100
                && object.len() < 100
            {
                return Some(Fact {
                    id: Uuid::new_v4().to_string(),
                    memory_id: memory_id.to_string(),
                    subject: subject.to_string(),
                    predicate: predicate.to_string(),
                    object: object.to_string(),
                    valid_from: Some(now.to_string()),
                    valid_until: None,
                    ingested_at: now.to_string(),
                    invalidated_at: None,
                    confidence: 0.7,
                });
            }
        }
    }

    None
}

fn try_extract_decision_pattern(line: &str, memory_id: &str, now: &str) -> Option<Fact> {
    let lower = line.to_lowercase();

    let prefixes = ["decided to ", "we decided to ", "team decided to "];

    for prefix in &prefixes {
        if let Some(pos) = lower.find(prefix) {
            let decision = line[pos + prefix.len()..].trim().trim_end_matches('.');
            if !decision.is_empty() && decision.len() < 200 {
                let subject = if lower.starts_with("we ") || lower.starts_with("team ") {
                    "team"
                } else {
                    line[..pos].trim()
                };

                return Some(Fact {
                    id: Uuid::new_v4().to_string(),
                    memory_id: memory_id.to_string(),
                    subject: subject.to_string(),
                    predicate: "decided".to_string(),
                    object: decision.to_string(),
                    valid_from: Some(now.to_string()),
                    valid_until: None,
                    ingested_at: now.to_string(),
                    invalidated_at: None,
                    confidence: 0.6,
                });
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_uses_pattern() {
        let facts = extract_facts("Auth service uses Clerk for authentication", "mem1");
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].subject, "Auth service");
        assert_eq!(facts[0].predicate, "uses");
        assert_eq!(facts[0].object, "Clerk for authentication");
    }

    #[test]
    fn test_extract_decision_pattern() {
        let facts = extract_facts("We decided to migrate from Auth0 to Clerk", "mem1");
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].subject, "team");
        assert_eq!(facts[0].predicate, "decided");
    }

    #[test]
    fn test_no_false_positives() {
        let facts = extract_facts("This is just a normal sentence about nothing.", "mem1");
        assert!(facts.is_empty());
    }

    #[test]
    fn test_multiple_facts() {
        let content = "Auth service uses Clerk.\nWe decided to deprecate the old system.";
        let facts = extract_facts(content, "mem1");
        assert_eq!(facts.len(), 2);
    }
}
