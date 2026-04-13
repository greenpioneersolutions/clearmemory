//! TLS configuration for shared deployments.
//!
//! Supports server TLS (cert + key) and mutual TLS (client CA) for
//! zero-trust environments.

use std::path::Path;

/// TLS configuration for the MCP/HTTP server.
#[derive(Debug, Clone, Default)]
pub struct TlsConfig {
    /// Path to the PEM-encoded server certificate.
    pub cert_path: String,
    /// Path to the PEM-encoded server private key.
    pub key_path: String,
    /// Optional path to a PEM-encoded CA certificate for mutual TLS.
    /// When set, clients must present a certificate signed by this CA.
    pub client_ca_path: String,
}

/// Check whether TLS is configured (both cert and key paths are non-empty).
pub fn is_tls_configured(config: &TlsConfig) -> bool {
    !config.cert_path.is_empty() && !config.key_path.is_empty()
}

/// Validate that the configured TLS files exist on disk.
///
/// Returns `Ok(())` if all specified paths exist, or an error describing
/// which file is missing.
pub fn validate_tls_config(config: &TlsConfig) -> Result<(), String> {
    if !is_tls_configured(config) {
        return Err("TLS not configured: cert_path and key_path must both be set".to_string());
    }

    if !Path::new(&config.cert_path).exists() {
        return Err(format!(
            "TLS certificate file not found: {}",
            config.cert_path
        ));
    }

    if !Path::new(&config.key_path).exists() {
        return Err(format!("TLS key file not found: {}", config.key_path));
    }

    if !config.client_ca_path.is_empty() && !Path::new(&config.client_ca_path).exists() {
        return Err(format!(
            "TLS client CA file not found: {}",
            config.client_ca_path
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_is_tls_configured_false_when_empty() {
        let config = TlsConfig::default();
        assert!(!is_tls_configured(&config));
    }

    #[test]
    fn test_is_tls_configured_false_when_partial() {
        let config = TlsConfig {
            cert_path: "/some/cert.pem".to_string(),
            key_path: String::new(),
            client_ca_path: String::new(),
        };
        assert!(!is_tls_configured(&config));
    }

    #[test]
    fn test_is_tls_configured_true_when_both_set() {
        let config = TlsConfig {
            cert_path: "/some/cert.pem".to_string(),
            key_path: "/some/key.pem".to_string(),
            client_ca_path: String::new(),
        };
        assert!(is_tls_configured(&config));
    }

    #[test]
    fn test_validate_tls_config_not_configured() {
        let config = TlsConfig::default();
        let result = validate_tls_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not configured"));
    }

    #[test]
    fn test_validate_tls_config_missing_cert() {
        let config = TlsConfig {
            cert_path: "/nonexistent/cert.pem".to_string(),
            key_path: "/nonexistent/key.pem".to_string(),
            client_ca_path: String::new(),
        };
        let result = validate_tls_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("certificate file not found"));
    }

    #[test]
    fn test_validate_tls_config_valid_files() {
        let dir = TempDir::new().unwrap();
        let cert = dir.path().join("cert.pem");
        let key = dir.path().join("key.pem");
        std::fs::write(&cert, "cert-data").unwrap();
        std::fs::write(&key, "key-data").unwrap();

        let config = TlsConfig {
            cert_path: cert.display().to_string(),
            key_path: key.display().to_string(),
            client_ca_path: String::new(),
        };
        assert!(validate_tls_config(&config).is_ok());
    }

    #[test]
    fn test_validate_tls_config_missing_client_ca() {
        let dir = TempDir::new().unwrap();
        let cert = dir.path().join("cert.pem");
        let key = dir.path().join("key.pem");
        std::fs::write(&cert, "cert-data").unwrap();
        std::fs::write(&key, "key-data").unwrap();

        let config = TlsConfig {
            cert_path: cert.display().to_string(),
            key_path: key.display().to_string(),
            client_ca_path: "/nonexistent/ca.pem".to_string(),
        };
        let result = validate_tls_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("client CA file not found"));
    }
}
