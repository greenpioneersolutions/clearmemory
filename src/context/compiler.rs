use crate::context::dedup::ContextDedup;
use crate::context::layers::{L0Context, L1Context, L2Memory, L3Memory};

/// Compiled context ready for injection into LLM prompt.
#[derive(Debug, Clone)]
pub struct CompiledContext {
    pub text: String,
    pub tokens_used: usize,
    pub tokens_budget: usize,
    pub l0_tokens: usize,
    pub l1_tokens: usize,
    pub l2_count: usize,
    pub l3_count: usize,
}

/// Assembles the final context payload within a token budget.
///
/// Fills in priority order: L0 → L1 → L2 → L3.
/// Highest-priority memories fill first; marginal memories are cut.
pub struct ContextCompiler {
    budget: usize,
    dedup: ContextDedup,
}

impl ContextCompiler {
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            dedup: ContextDedup::new(),
        }
    }

    pub fn with_dedup(budget: usize, dedup: ContextDedup) -> Self {
        Self { budget, dedup }
    }

    /// Compile context from all layers into a single payload.
    pub fn compile(
        &self,
        l0: &L0Context,
        l1: &L1Context,
        l2: &[L2Memory],
        l3: &[L3Memory],
    ) -> CompiledContext {
        let mut output = String::new();
        let mut remaining = self.budget;
        let mut l2_count = 0;
        let mut l3_count = 0;

        // L0: Always loaded
        let l0_text = l0.render();
        let l0_tokens = estimate_tokens(&l0_text);
        if l0_tokens <= remaining {
            output.push_str(&l0_text);
            output.push('\n');
            remaining -= l0_tokens;
        }
        let l0_used = l0_tokens.min(self.budget);

        // L1: Always loaded
        let l1_text = l1.render();
        let l1_tokens = estimate_tokens(&l1_text);
        if l1_tokens <= remaining && !l1_text.is_empty() {
            output.push_str(&l1_text);
            output.push('\n');
            remaining -= l1_tokens;
        }
        let l1_used = l1_tokens.min(remaining + l1_tokens);

        // L2: Recall results, highest score first
        for mem in l2 {
            if remaining == 0 {
                break;
            }
            if self.dedup.is_duplicate(&mem.summary) {
                continue;
            }
            let tokens = estimate_tokens(&mem.summary);
            if tokens <= remaining {
                output.push_str(&format!("- {}\n", mem.summary));
                remaining -= tokens;
                l2_count += 1;
            }
        }

        // L3: Deep search results
        for mem in l3 {
            if remaining == 0 {
                break;
            }
            if self.dedup.is_duplicate(&mem.summary) {
                continue;
            }
            let tokens = estimate_tokens(&mem.summary);
            if tokens <= remaining {
                let prefix = if let Some(ref stream) = mem.source_stream {
                    format!("[{stream}] ")
                } else {
                    String::new()
                };
                output.push_str(&format!("- {prefix}{}\n", mem.summary));
                remaining -= tokens;
                l3_count += 1;
            }
        }

        let tokens_used = self.budget - remaining;

        CompiledContext {
            text: output,
            tokens_used,
            tokens_budget: self.budget,
            l0_tokens: l0_used,
            l1_tokens: l1_used,
            l2_count,
            l3_count,
        }
    }
}

/// Rough token estimate: ~4 characters per token (conservative for English text).
fn estimate_tokens(text: &str) -> usize {
    (text.len() / 4).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tier;

    fn test_l0() -> L0Context {
        L0Context {
            tier: Tier::Offline,
            active_stream: Some("test".to_string()),
            user_id: None,
        }
    }

    fn test_l1() -> L1Context {
        L1Context {
            recent_facts: vec!["Auth uses Clerk".to_string()],
            stream_description: Some("Test project".to_string()),
        }
    }

    #[test]
    fn test_compile_basic() {
        let compiler = ContextCompiler::new(4096);
        let l2 = vec![
            L2Memory {
                memory_id: "m1".into(),
                summary: "We decided to use Clerk".into(),
                score: 0.9,
            },
            L2Memory {
                memory_id: "m2".into(),
                summary: "Auth migration complete".into(),
                score: 0.7,
            },
        ];

        let result = compiler.compile(&test_l0(), &test_l1(), &l2, &[]);

        assert!(result.tokens_used > 0);
        assert!(result.tokens_used <= result.tokens_budget);
        assert_eq!(result.l2_count, 2);
        assert!(result.text.contains("Clerk"));
    }

    #[test]
    fn test_compile_respects_budget() {
        let compiler = ContextCompiler::new(20); // Very tight budget
        let l2 = vec![L2Memory {
            memory_id: "m1".into(),
            summary: "A very long memory that should be too big to fit in a 20-token budget".into(),
            score: 0.9,
        }];

        let result = compiler.compile(&test_l0(), &test_l1(), &l2, &[]);
        assert!(result.tokens_used <= 20);
    }

    #[test]
    fn test_compile_dedup() {
        let mut dedup = ContextDedup::new();
        dedup.register_known_content("Already known context");

        let compiler = ContextCompiler::with_dedup(4096, dedup);
        let l2 = vec![
            L2Memory {
                memory_id: "m1".into(),
                summary: "Already known context".into(),
                score: 0.9,
            },
            L2Memory {
                memory_id: "m2".into(),
                summary: "New information".into(),
                score: 0.7,
            },
        ];

        let result = compiler.compile(&test_l0(), &test_l1(), &l2, &[]);
        assert_eq!(result.l2_count, 1); // Deduped one
        assert!(!result.text.contains("Already known"));
        assert!(result.text.contains("New information"));
    }

    #[test]
    fn test_compile_with_l3() {
        let compiler = ContextCompiler::new(4096);
        let l3 = vec![L3Memory {
            memory_id: "m1".into(),
            summary: "Cross-project insight".into(),
            score: 0.5,
            source_stream: Some("other-project".to_string()),
        }];

        let result = compiler.compile(&test_l0(), &test_l1(), &[], &l3);
        assert_eq!(result.l3_count, 1);
        assert!(result.text.contains("[other-project]"));
    }
}
