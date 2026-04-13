use crate::Classification;

/// Tracks classification levels through the content pipeline.
/// Derived content inherits the highest classification of its sources.
#[derive(Debug, Clone)]
pub struct ClassificationTrace {
    pub source_classifications: Vec<Classification>,
}

impl ClassificationTrace {
    pub fn new() -> Self {
        Self {
            source_classifications: Vec::new(),
        }
    }

    pub fn add_source(&mut self, classification: Classification) {
        self.source_classifications.push(classification);
    }

    /// Get the effective classification (highest of all sources).
    pub fn effective_classification(&self) -> Classification {
        self.source_classifications
            .iter()
            .copied()
            .max()
            .unwrap_or(Classification::Internal)
    }
}

impl Default for ClassificationTrace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_inherits_highest() {
        let mut trace = ClassificationTrace::new();
        trace.add_source(Classification::Public);
        trace.add_source(Classification::Confidential);
        trace.add_source(Classification::Internal);

        assert_eq!(
            trace.effective_classification(),
            Classification::Confidential
        );
    }

    #[test]
    fn test_empty_trace_defaults_internal() {
        let trace = ClassificationTrace::new();
        assert_eq!(trace.effective_classification(), Classification::Internal);
    }
}
