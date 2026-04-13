use crate::config::TokenConfig;
use crate::AuthError;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Token scope levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    Read,
    ReadWrite,
    Admin,
    Purge,
}

impl Scope {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "read" => Some(Self::Read),
            "read-write" => Some(Self::ReadWrite),
            "admin" => Some(Self::Admin),
            "purge" => Some(Self::Purge),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::ReadWrite => "read-write",
            Self::Admin => "admin",
            Self::Purge => "purge",
        }
    }

    /// Check if this scope satisfies a required scope.
    /// Admin satisfies read and read-write. Purge only satisfies purge.
    pub fn satisfies(&self, required: Scope) -> bool {
        match required {
            Scope::Read => matches!(self, Scope::Read | Scope::ReadWrite | Scope::Admin),
            Scope::ReadWrite => matches!(self, Scope::ReadWrite | Scope::Admin),
            Scope::Admin => matches!(self, Scope::Admin),
            Scope::Purge => matches!(self, Scope::Purge),
        }
    }
}

/// Validated token information.
#[derive(Debug, Clone)]
pub struct ValidatedToken {
    pub id: String,
    pub scope: Scope,
    pub label: Option<String>,
}

/// Token validator that checks incoming tokens against stored hashes.
pub struct TokenValidator {
    tokens: HashMap<String, StoredToken>,
}

#[derive(Clone)]
struct StoredToken {
    id: String,
    scope: Scope,
    expires_at: Option<DateTime<Utc>>,
    label: Option<String>,
}

impl TokenValidator {
    /// Create a validator from config token entries.
    pub fn from_config(tokens: &[TokenConfig]) -> Self {
        let mut map = HashMap::new();

        for t in tokens {
            let scope = Scope::parse(&t.scope).unwrap_or(Scope::Read);
            let expires_at = DateTime::parse_from_rfc3339(&t.expires_at)
                .ok()
                .map(|dt| dt.with_timezone(&Utc));

            map.insert(
                t.token_hash.clone(),
                StoredToken {
                    id: t.id.clone(),
                    scope,
                    expires_at,
                    label: t.label.clone(),
                },
            );
        }

        Self { tokens: map }
    }

    /// Create a validator with no tokens (all requests pass as admin for development).
    pub fn permissive() -> Self {
        Self {
            tokens: HashMap::new(),
        }
    }

    /// Validate a bearer token. Returns token info or an error.
    pub fn validate(&self, bearer_token: &str) -> Result<ValidatedToken, AuthError> {
        // If no tokens configured, allow everything (dev mode)
        if self.tokens.is_empty() {
            return Ok(ValidatedToken {
                id: "dev".to_string(),
                scope: Scope::Admin,
                label: Some("development".to_string()),
            });
        }

        let token_hash = hash_token(bearer_token);

        let stored = self
            .tokens
            .get(&token_hash)
            .ok_or(AuthError::InvalidToken)?;

        // Check expiration
        if let Some(expires_at) = stored.expires_at {
            if Utc::now() > expires_at {
                return Err(AuthError::TokenExpired);
            }
        }

        Ok(ValidatedToken {
            id: stored.id.clone(),
            scope: stored.scope,
            label: stored.label.clone(),
        })
    }

    /// Check if a validated token has the required scope.
    pub fn check_scope(token: &ValidatedToken, required: Scope) -> Result<(), AuthError> {
        if token.scope.satisfies(required) {
            Ok(())
        } else {
            Err(AuthError::InsufficientScope {
                required: required.as_str().to_string(),
                actual: token.scope.as_str().to_string(),
            })
        }
    }
}

/// Generate a new 256-bit API token. Returns (raw_token, sha256_hash).
pub fn generate_token() -> (String, String) {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let raw = format!("cmk_{}", hex_encode(&bytes));
    let hash = hash_token(&raw);
    (raw, hash)
}

/// Hash a token with SHA-256 for storage.
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let hash = hasher.finalize();
    format!("sha256:{}", hex_encode(&hash))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_satisfies() {
        assert!(Scope::Admin.satisfies(Scope::Read));
        assert!(Scope::Admin.satisfies(Scope::ReadWrite));
        assert!(Scope::Admin.satisfies(Scope::Admin));
        assert!(!Scope::Admin.satisfies(Scope::Purge));

        assert!(Scope::ReadWrite.satisfies(Scope::Read));
        assert!(Scope::ReadWrite.satisfies(Scope::ReadWrite));
        assert!(!Scope::ReadWrite.satisfies(Scope::Admin));

        assert!(Scope::Read.satisfies(Scope::Read));
        assert!(!Scope::Read.satisfies(Scope::ReadWrite));

        assert!(Scope::Purge.satisfies(Scope::Purge));
        assert!(!Scope::Purge.satisfies(Scope::Read));
    }

    #[test]
    fn test_generate_token() {
        let (raw, hash) = generate_token();
        assert!(raw.starts_with("cmk_"));
        assert!(hash.starts_with("sha256:"));
        assert_eq!(raw.len(), 4 + 64); // "cmk_" + 32 bytes hex
    }

    #[test]
    fn test_hash_token_deterministic() {
        let h1 = hash_token("test-token");
        let h2 = hash_token("test-token");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_permissive_validator() {
        let v = TokenValidator::permissive();
        let result = v.validate("anything");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().scope, Scope::Admin);
    }

    #[test]
    fn test_validator_with_tokens() {
        let (raw, hash) = generate_token();
        let config = vec![TokenConfig {
            id: "test".to_string(),
            token_hash: hash,
            scope: "read-write".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            expires_at: "2027-01-01T00:00:00Z".to_string(),
            label: Some("test token".to_string()),
        }];

        let v = TokenValidator::from_config(&config);
        let result = v.validate(&raw).unwrap();
        assert_eq!(result.scope, Scope::ReadWrite);
        assert_eq!(result.id, "test");
    }

    #[test]
    fn test_validator_rejects_unknown_token() {
        let (_, hash) = generate_token();
        let config = vec![TokenConfig {
            id: "test".to_string(),
            token_hash: hash,
            scope: "read".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            expires_at: "2027-01-01T00:00:00Z".to_string(),
            label: None,
        }];

        let v = TokenValidator::from_config(&config);
        assert!(v.validate("wrong-token").is_err());
    }

    #[test]
    fn test_validator_rejects_expired_token() {
        let (raw, hash) = generate_token();
        let config = vec![TokenConfig {
            id: "test".to_string(),
            token_hash: hash,
            scope: "read".to_string(),
            created_at: "2020-01-01T00:00:00Z".to_string(),
            expires_at: "2020-06-01T00:00:00Z".to_string(), // expired
            label: None,
        }];

        let v = TokenValidator::from_config(&config);
        let result = v.validate(&raw);
        assert!(matches!(result, Err(AuthError::TokenExpired)));
    }
}
