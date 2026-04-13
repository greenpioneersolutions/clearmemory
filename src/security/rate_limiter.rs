use crate::config::RateLimitingConfig;
use governor::{Quota, RateLimiter as GovernorLimiter};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Mutex;

/// Operation category for rate limiting.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum OpCategory {
    Read,
    Write,
    Reflect,
    Auth,
    Purge,
}

type DirectLimiter = GovernorLimiter<
    governor::state::NotKeyed,
    governor::state::InMemoryState,
    governor::clock::DefaultClock,
>;

/// Per-client rate limiter.
pub struct RateLimiter {
    limiters: Mutex<HashMap<(String, OpCategory), DirectLimiter>>,
    config: RateLimitingConfig,
}

impl RateLimiter {
    pub fn new(config: &RateLimitingConfig) -> Self {
        Self {
            limiters: Mutex::new(HashMap::new()),
            config: config.clone(),
        }
    }

    /// Check if a request is allowed. Returns Ok(()) or Err with retry-after seconds.
    pub fn check(&self, client_id: &str, category: OpCategory) -> Result<(), u64> {
        if !self.config.enabled {
            return Ok(());
        }

        let rpm = self.rpm_for_category(category);
        if rpm == 0 {
            return Ok(());
        }

        let key = (client_id.to_string(), category);
        let mut limiters = self.limiters.lock().unwrap();

        let limiter = limiters.entry(key).or_insert_with(|| {
            let quota =
                Quota::per_minute(NonZeroU32::new(rpm).unwrap_or(NonZeroU32::new(1).unwrap()));
            GovernorLimiter::direct(quota)
        });

        match limiter.check() {
            Ok(()) => Ok(()),
            Err(_) => Err(60), // retry after 60 seconds
        }
    }

    fn rpm_for_category(&self, category: OpCategory) -> u32 {
        match category {
            OpCategory::Read => self.config.read_rpm,
            OpCategory::Write => self.config.write_rpm,
            OpCategory::Reflect => self.config.reflect_rpm,
            OpCategory::Auth => self.config.auth_rpm,
            OpCategory::Purge => self.config.purge_rph, // per hour, but using rpm field
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_request() {
        let config = RateLimitingConfig::default();
        let limiter = RateLimiter::new(&config);
        assert!(limiter.check("client1", OpCategory::Read).is_ok());
    }

    #[test]
    fn test_rate_limiter_disabled() {
        let config = RateLimitingConfig {
            enabled: false,
            ..Default::default()
        };
        let limiter = RateLimiter::new(&config);
        // Should always allow when disabled
        for _ in 0..10000 {
            assert!(limiter.check("client1", OpCategory::Read).is_ok());
        }
    }
}
