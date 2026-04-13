use crate::config::EncryptionConfig;
use crate::EncryptionError;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::Argon2;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Trait for encryption/decryption operations.
/// Allows swapping between real encryption and no-op for testing or disabled mode.
pub trait EncryptionProvider: Send + Sync {
    /// Encrypt plaintext bytes. Returns ciphertext with nonce prepended.
    fn encrypt_bytes(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError>;

    /// Decrypt ciphertext bytes. Expects nonce prepended to ciphertext.
    fn decrypt_bytes(&self, ciphertext: &[u8]) -> Result<Vec<u8>, EncryptionError>;

    /// Return the hex-encoded key for SQLCipher PRAGMA.
    fn sqlite_key_hex(&self) -> Result<String, EncryptionError>;

    /// Whether encryption is actually enabled.
    fn is_enabled(&self) -> bool;
}

/// AES-256-GCM encryption backed by Argon2id key derivation.
pub struct Argon2GcmProvider {
    /// The derived 256-bit encryption key
    key: [u8; 32],
}

/// Nonce size for AES-256-GCM (96 bits / 12 bytes)
const NONCE_SIZE: usize = 12;

impl Argon2GcmProvider {
    /// Create a new provider by deriving a key from the given passphrase.
    pub fn from_passphrase(
        passphrase: &str,
        config: &EncryptionConfig,
    ) -> Result<Self, EncryptionError> {
        let key = derive_key(passphrase, config)?;
        Ok(Self { key })
    }

    /// Create a provider from a pre-derived key (for testing or key rotation).
    pub fn from_key(key: [u8; 32]) -> Self {
        Self { key }
    }
}

impl EncryptionProvider for Argon2GcmProvider {
    fn encrypt_bytes(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| EncryptionError::EncryptFailed(e.to_string()))?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| EncryptionError::EncryptFailed(e.to_string()))?;

        // Prepend nonce to ciphertext: [nonce (12 bytes)][ciphertext + tag]
        let mut result = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    fn decrypt_bytes(&self, ciphertext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        if ciphertext.len() < NONCE_SIZE {
            return Err(EncryptionError::DecryptFailed(
                "ciphertext too short to contain nonce".to_string(),
            ));
        }

        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| EncryptionError::DecryptFailed(e.to_string()))?;

        let nonce = Nonce::from_slice(&ciphertext[..NONCE_SIZE]);
        let plaintext = cipher
            .decrypt(nonce, &ciphertext[NONCE_SIZE..])
            .map_err(|e| EncryptionError::DecryptFailed(e.to_string()))?;

        Ok(plaintext)
    }

    fn sqlite_key_hex(&self) -> Result<String, EncryptionError> {
        Ok(format!("x'{}'", hex_encode(&self.key)))
    }

    fn is_enabled(&self) -> bool {
        true
    }
}

/// No-op encryption provider for when encryption is disabled.
pub struct NoopProvider;

impl EncryptionProvider for NoopProvider {
    fn encrypt_bytes(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        Ok(plaintext.to_vec())
    }

    fn decrypt_bytes(&self, ciphertext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        Ok(ciphertext.to_vec())
    }

    fn sqlite_key_hex(&self) -> Result<String, EncryptionError> {
        Ok(String::new())
    }

    fn is_enabled(&self) -> bool {
        false
    }
}

/// Derive a 256-bit key from a passphrase using Argon2id.
fn derive_key(passphrase: &str, config: &EncryptionConfig) -> Result<[u8; 32], EncryptionError> {
    // Deterministic salt from passphrase hash. In Clear Memory's single-user model,
    // this ensures the same passphrase always produces the same key.
    let mut salt_hasher = Sha256::new();
    salt_hasher.update(b"clearmemory-salt-v1:");
    salt_hasher.update(passphrase.as_bytes());
    let salt_hash = salt_hasher.finalize();
    let salt = &salt_hash[..16]; // 128-bit salt

    let params = argon2::Params::new(
        config.kdf_memory_mb * 1024, // memory in KiB
        config.kdf_iterations,
        1, // parallelism
        Some(32),
    )
    .map_err(|e| EncryptionError::KeyDerivationFailed(e.to_string()))?;

    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| EncryptionError::KeyDerivationFailed(e.to_string()))?;

    Ok(key)
}

/// Create the appropriate encryption provider based on config.
pub fn create_provider(
    config: &EncryptionConfig,
) -> Result<Arc<dyn EncryptionProvider>, EncryptionError> {
    if !config.enabled {
        return Ok(Arc::new(NoopProvider));
    }

    let passphrase = std::env::var(&config.passphrase_env_var)
        .map_err(|_| EncryptionError::PassphraseRequired)?;

    let provider = Argon2GcmProvider::from_passphrase(&passphrase, config)?;
    Ok(Arc::new(provider))
}

/// Create a provider from an explicit passphrase (for init and interactive use).
pub fn create_provider_with_passphrase(
    passphrase: &str,
    config: &EncryptionConfig,
) -> Result<Arc<dyn EncryptionProvider>, EncryptionError> {
    if !config.enabled {
        return Ok(Arc::new(NoopProvider));
    }

    let provider = Argon2GcmProvider::from_passphrase(passphrase, config)?;
    Ok(Arc::new(provider))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> EncryptionConfig {
        EncryptionConfig {
            enabled: true,
            kdf_memory_mb: 4, // Low memory for fast tests
            kdf_iterations: 1,
            ..EncryptionConfig::default()
        }
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let provider =
            Argon2GcmProvider::from_passphrase("test-passphrase", &test_config()).unwrap();
        let plaintext = b"Hello, Clear Memory!";

        let ciphertext = provider.encrypt_bytes(plaintext).unwrap();
        assert_ne!(&ciphertext[..], &plaintext[..]);
        assert!(ciphertext.len() > plaintext.len());

        let decrypted = provider.decrypt_bytes(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_passphrases_produce_different_keys() {
        let config = test_config();
        let p1 = Argon2GcmProvider::from_passphrase("passphrase-1", &config).unwrap();
        let p2 = Argon2GcmProvider::from_passphrase("passphrase-2", &config).unwrap();
        assert_ne!(p1.key, p2.key);
    }

    #[test]
    fn test_same_passphrase_produces_same_key() {
        let config = test_config();
        let p1 = Argon2GcmProvider::from_passphrase("same", &config).unwrap();
        let p2 = Argon2GcmProvider::from_passphrase("same", &config).unwrap();
        assert_eq!(p1.key, p2.key);
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let config = test_config();
        let p1 = Argon2GcmProvider::from_passphrase("correct", &config).unwrap();
        let p2 = Argon2GcmProvider::from_passphrase("wrong", &config).unwrap();

        let ct = p1.encrypt_bytes(b"secret data").unwrap();
        assert!(p2.decrypt_bytes(&ct).is_err());
    }

    #[test]
    fn test_decrypt_truncated_fails() {
        let provider = Argon2GcmProvider::from_passphrase("test", &test_config()).unwrap();
        assert!(provider.decrypt_bytes(&[0u8; 5]).is_err());
    }

    #[test]
    fn test_noop_provider() {
        let provider = NoopProvider;
        let data = b"plaintext data";

        let encrypted = provider.encrypt_bytes(data).unwrap();
        assert_eq!(encrypted, data);

        let decrypted = provider.decrypt_bytes(data).unwrap();
        assert_eq!(decrypted, data);

        assert!(!provider.is_enabled());
    }

    #[test]
    fn test_sqlite_key_hex_format() {
        let provider = Argon2GcmProvider::from_passphrase("test", &test_config()).unwrap();
        let hex_key = provider.sqlite_key_hex().unwrap();
        assert!(hex_key.starts_with("x'"));
        assert!(hex_key.ends_with('\''));
        assert_eq!(hex_key.len(), 67); // "x'" + 64 hex chars + "'"
    }

    #[test]
    fn test_encrypt_empty_data() {
        let provider = Argon2GcmProvider::from_passphrase("test", &test_config()).unwrap();
        let ct = provider.encrypt_bytes(b"").unwrap();
        let pt = provider.decrypt_bytes(&ct).unwrap();
        assert!(pt.is_empty());
    }

    #[test]
    fn test_encrypt_large_data() {
        let provider = Argon2GcmProvider::from_passphrase("test", &test_config()).unwrap();
        let large = vec![0xABu8; 1_000_000];
        let ct = provider.encrypt_bytes(&large).unwrap();
        let pt = provider.decrypt_bytes(&ct).unwrap();
        assert_eq!(pt, large);
    }
}
