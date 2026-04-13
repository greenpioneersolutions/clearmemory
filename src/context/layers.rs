use crate::Tier;

/// L0: Identity context — who is the user, what CLI is active, current project.
#[derive(Debug, Clone)]
pub struct L0Context {
    pub tier: Tier,
    pub active_stream: Option<String>,
    pub user_id: Option<String>,
}

impl L0Context {
    pub fn render(&self) -> String {
        let mut parts = vec![format!("Tier: {}", self.tier)];
        if let Some(ref stream) = self.active_stream {
            parts.push(format!("Active stream: {stream}"));
        }
        if let Some(ref user) = self.user_id {
            parts.push(format!("User: {user}"));
        }
        parts.join(" | ")
    }

    pub fn estimate_tokens(&self) -> usize {
        // Rough estimate: ~4 chars per token
        self.render().len() / 4 + 1
    }
}

/// L1: Working set — active stream context, recent decisions, current project state.
#[derive(Debug, Clone)]
pub struct L1Context {
    pub recent_facts: Vec<String>,
    pub stream_description: Option<String>,
}

impl L1Context {
    pub fn render(&self) -> String {
        let mut parts = Vec::new();
        if let Some(ref desc) = self.stream_description {
            parts.push(format!("Stream: {desc}"));
        }
        for fact in &self.recent_facts {
            parts.push(fact.clone());
        }
        parts.join("\n")
    }

    pub fn estimate_tokens(&self) -> usize {
        self.render().len() / 4 + 1
    }
}

/// L2: Recall — relevant memories from semantic search within active stream.
#[derive(Debug, Clone)]
pub struct L2Memory {
    pub memory_id: String,
    pub summary: String,
    pub score: f64,
}

/// L3: Deep search — cross-stream, cross-project retrieval.
#[derive(Debug, Clone)]
pub struct L3Memory {
    pub memory_id: String,
    pub summary: String,
    pub score: f64,
    pub source_stream: Option<String>,
}

/// Determine which layers to activate based on the query context.
pub fn should_activate_l2(query: &str) -> bool {
    // L2 activates when there's a meaningful query (not just status checks)
    !query.is_empty() && query.len() > 5
}

pub fn should_activate_l3(_query: &str, l2_results_count: usize, l2_max_score: f64) -> bool {
    // L3 activates when L2 results are insufficient
    l2_results_count < 3 || l2_max_score < 0.3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l0_render() {
        let ctx = L0Context {
            tier: Tier::Offline,
            active_stream: Some("my-project".to_string()),
            user_id: Some("user1".to_string()),
        };
        let rendered = ctx.render();
        assert!(rendered.contains("offline"));
        assert!(rendered.contains("my-project"));
    }

    #[test]
    fn test_l1_render() {
        let ctx = L1Context {
            recent_facts: vec![
                "Auth uses Clerk".to_string(),
                "DB is PostgreSQL".to_string(),
            ],
            stream_description: Some("Platform team auth work".to_string()),
        };
        let rendered = ctx.render();
        assert!(rendered.contains("Clerk"));
        assert!(rendered.contains("PostgreSQL"));
    }

    #[test]
    fn test_should_activate_l2() {
        assert!(should_activate_l2("why did we switch auth"));
        assert!(!should_activate_l2(""));
        assert!(!should_activate_l2("hi"));
    }

    #[test]
    fn test_should_activate_l3() {
        assert!(should_activate_l3("query", 1, 0.1));
        assert!(!should_activate_l3("query", 10, 0.8));
    }
}
