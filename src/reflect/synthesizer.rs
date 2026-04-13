//! Reflect engine for synthesizing across multiple memories.
//!
//! The actual Qwen3-4B candle inference is deferred. This module provides the
//! trait interface and a stub implementation that reports Tier 2+ is required.

use anyhow::Result;

/// Trait for reflect engines that synthesize coherent narratives from multiple memories.
pub trait ReflectEngine: Send + Sync {
    /// Synthesize a coherent response from multiple memory contents.
    fn synthesize(&self, query: &str, memories: &[String]) -> Result<String>;
}

/// Stub reflect engine for Tier 1 deployments where no local LLM is available.
#[derive(Debug, Default)]
pub struct StubReflectEngine;

impl ReflectEngine for StubReflectEngine {
    fn synthesize(&self, _query: &str, _memories: &[String]) -> Result<String> {
        Ok("Reflect requires Tier 2 or higher".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub_reflect_returns_tier_message() {
        let engine = StubReflectEngine;
        let result = engine
            .synthesize(
                "summarize auth migration",
                &["memory 1".into(), "memory 2".into()],
            )
            .unwrap();
        assert_eq!(result, "Reflect requires Tier 2 or higher");
    }

    #[test]
    fn test_stub_reflect_handles_empty_memories() {
        let engine = StubReflectEngine;
        let result = engine.synthesize("anything", &[]).unwrap();
        assert_eq!(result, "Reflect requires Tier 2 or higher");
    }
}
