use crate::Classification;

/// Check if a classification level is eligible for cloud API calls.
pub fn is_cloud_eligible(classification: Classification, eligible: &[String]) -> bool {
    let class_str = classification.to_string();
    eligible.iter().any(|e| e == &class_str)
}

/// Determine the highest classification from a set of source classifications.
pub fn highest_classification(classifications: &[Classification]) -> Classification {
    classifications
        .iter()
        .copied()
        .max()
        .unwrap_or(Classification::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cloud_eligibility() {
        let eligible = vec!["public".to_string(), "internal".to_string()];

        assert!(is_cloud_eligible(Classification::Public, &eligible));
        assert!(is_cloud_eligible(Classification::Internal, &eligible));
        assert!(!is_cloud_eligible(Classification::Confidential, &eligible));
        assert!(!is_cloud_eligible(Classification::Pii, &eligible));
    }

    #[test]
    fn test_highest_classification() {
        assert_eq!(
            highest_classification(&[Classification::Public, Classification::Confidential]),
            Classification::Confidential
        );
        assert_eq!(
            highest_classification(&[Classification::Internal]),
            Classification::Internal
        );
        assert_eq!(highest_classification(&[]), Classification::Internal);
    }
}
