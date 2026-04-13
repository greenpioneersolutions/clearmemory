//! Health check endpoint logic.

use crate::Tier;
use serde::{Deserialize, Serialize};

/// Overall health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Health check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: Status,
    pub uptime_secs: u64,
    pub memory_count: u64,
    pub tier: Tier,
}

/// Perform a basic health check.
///
/// Returns a healthy status with the provided metrics. The full implementation
/// will check SQLite accessibility, LanceDB consistency, model status, and
/// port availability.
pub fn check_health() -> HealthStatus {
    HealthStatus {
        status: Status::Healthy,
        uptime_secs: 0,
        memory_count: 0,
        tier: Tier::Offline,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_health_returns_healthy() {
        let health = check_health();
        assert_eq!(health.status, Status::Healthy);
        assert_eq!(health.tier, Tier::Offline);
    }

    #[test]
    fn test_health_status_serialization() {
        let health = HealthStatus {
            status: Status::Degraded,
            uptime_secs: 3600,
            memory_count: 42,
            tier: Tier::LocalLlm,
        };
        let json = serde_json::to_string(&health).unwrap();
        assert!(json.contains("\"degraded\""));
        assert!(json.contains("\"local_llm\""));
    }
}
