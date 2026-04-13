use crate::{Classification, ConfigError, Tier};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Root configuration loaded from `~/.clearmemory/config.toml`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub models: ModelsConfig,
    pub cloud: CloudConfig,
    pub retrieval: RetrievalConfig,
    pub retention: RetentionConfig,
    pub server: ServerConfig,
    pub encryption: EncryptionConfig,
    pub auth: AuthConfig,
    pub security: SecurityConfig,
    pub compliance: ComplianceConfig,
    pub observability: ObservabilityConfig,
    pub backup: BackupConfig,
    pub migrations: MigrationsConfig,
    pub concurrency: ConcurrencyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub tier: Tier,
    pub default_stream: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            tier: Tier::Offline,
            default_stream: "default".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelsConfig {
    pub embedding: String,
    pub curator: String,
    pub reflect: String,
    pub reranker: String,
    pub model_path: String,
    pub verify_checksums: bool,
    pub auto_download: bool,
    pub reflect_resident: bool,
    pub curator_resident: bool,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            embedding: "bge-m3".to_string(),
            curator: "qwen3-0.6b".to_string(),
            reflect: "qwen3-4b".to_string(),
            reranker: "bge-reranker-base".to_string(),
            model_path: String::new(),
            verify_checksums: true,
            auto_download: true,
            reflect_resident: false,
            curator_resident: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CloudConfig {
    pub api_provider: String,
    pub api_key_env: String,
    pub curator_model: String,
    pub reflect_model: String,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            api_provider: "anthropic".to_string(),
            api_key_env: "ANTHROPIC_API_KEY".to_string(),
            curator_model: "claude-haiku-4-5-20251001".to_string(),
            reflect_model: "claude-sonnet-4-6".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RetrievalConfig {
    pub top_k: usize,
    pub temporal_boost: f64,
    pub entity_boost: f64,
    pub token_budget: usize,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            top_k: 10,
            temporal_boost: 0.4,
            entity_boost: 0.3,
            token_budget: 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RetentionConfig {
    pub time_threshold_days: u64,
    pub size_threshold_gb: f64,
    pub performance_threshold_ms: u64,
    pub auto_archive: bool,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            time_threshold_days: 90,
            size_threshold_gb: 2.0,
            performance_threshold_ms: 200,
            auto_archive: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub mcp_enabled: bool,
    pub http_enabled: bool,
    pub http_port: u16,
    pub mcp_port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            mcp_enabled: true,
            http_enabled: true,
            http_port: 8080,
            mcp_port: 9700,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EncryptionConfig {
    pub enabled: bool,
    pub cipher: String,
    pub sqlite_cipher: String,
    pub kdf: String,
    pub kdf_memory_mb: u32,
    pub kdf_iterations: u32,
    pub passphrase_env_var: String,
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cipher: "aes-256-gcm".to_string(),
            sqlite_cipher: "aes-256-cbc".to_string(),
            kdf: "argon2id".to_string(),
            kdf_memory_mb: 64,
            kdf_iterations: 3,
            passphrase_env_var: "CLEARMEMORY_PASSPHRASE".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub require_token: bool,
    pub default_token_ttl_days: u64,
    pub tokens: Vec<TokenConfig>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            require_token: true,
            default_token_ttl_days: 90,
            tokens: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConfig {
    pub id: String,
    pub token_hash: String,
    pub scope: String,
    pub created_at: String,
    pub expires_at: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    pub bind_address: String,
    pub tls_cert_path: String,
    pub tls_key_path: String,
    pub tls_client_ca_path: String,
    pub cloud_eligible_classifications: Vec<String>,
    pub max_import_size_mb: u64,
    pub max_memory_size_mb: u64,
    pub secret_scanning: SecretScanningConfig,
    pub rate_limiting: RateLimitingConfig,
    pub insider_detection: InsiderDetectionConfig,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            tls_cert_path: String::new(),
            tls_key_path: String::new(),
            tls_client_ca_path: String::new(),
            cloud_eligible_classifications: vec!["public".to_string(), "internal".to_string()],
            max_import_size_mb: 500,
            max_memory_size_mb: 10,
            secret_scanning: SecretScanningConfig::default(),
            rate_limiting: RateLimitingConfig::default(),
            insider_detection: InsiderDetectionConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecretScanningConfig {
    pub enabled: bool,
    pub mode: String,
    pub custom_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

impl Default for SecretScanningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: "warn".to_string(),
            custom_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RateLimitingConfig {
    pub enabled: bool,
    pub read_rpm: u32,
    pub write_rpm: u32,
    pub reflect_rpm: u32,
    pub auth_rpm: u32,
    pub purge_rph: u32,
    pub max_request_body_mb: u64,
}

impl Default for RateLimitingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            read_rpm: 1000,
            write_rpm: 100,
            reflect_rpm: 10,
            auth_rpm: 10,
            purge_rph: 5,
            max_request_body_mb: 50,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InsiderDetectionConfig {
    pub enabled: bool,
    pub anomaly_threshold_stddev: f64,
    pub require_justification_for_confidential: bool,
    pub alert_on_anomaly: bool,
}

impl Default for InsiderDetectionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            anomaly_threshold_stddev: 3.0,
            require_justification_for_confidential: false,
            alert_on_anomaly: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ComplianceConfig {
    pub default_classification: Classification,
    pub pii_detection_enabled: bool,
    pub require_classification_on_retain: bool,
    pub legal_hold_enabled: bool,
    pub purge_requires_two_person: bool,
    pub purge_request_ttl_hours: u64,
}

impl Default for ComplianceConfig {
    fn default() -> Self {
        Self {
            default_classification: Classification::Internal,
            pii_detection_enabled: false,
            require_classification_on_retain: false,
            legal_hold_enabled: true,
            purge_requires_two_person: false,
            purge_request_ttl_hours: 72,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ObservabilityConfig {
    pub otel_enabled: bool,
    pub otel_endpoint: String,
    pub otel_service_name: String,
    pub metrics_log_interval_secs: u64,
    pub health_endpoint_enabled: bool,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            otel_enabled: false,
            otel_endpoint: String::new(),
            otel_service_name: "clearmemory".to_string(),
            metrics_log_interval_secs: 60,
            health_endpoint_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupConfig {
    pub auto_backup_enabled: bool,
    pub auto_backup_interval_hours: u64,
    pub backup_directory: String,
    pub backup_retention_count: u32,
    pub encrypt_backups: bool,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            auto_backup_enabled: false,
            auto_backup_interval_hours: 24,
            backup_directory: "~/.clearmemory/backups".to_string(),
            backup_retention_count: 7,
            encrypt_backups: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MigrationsConfig {
    pub auto_migrate: bool,
    pub backup_before_migrate: bool,
}

impl Default for MigrationsConfig {
    fn default() -> Self {
        Self {
            auto_migrate: true,
            backup_before_migrate: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConcurrencyConfig {
    pub read_pool_size: usize,
    pub write_queue_depth: usize,
    pub compaction_interval_secs: u64,
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            read_pool_size: 4,
            write_queue_depth: 1000,
            compaction_interval_secs: 300,
        }
    }
}

impl Config {
    /// Return the default Clear Memory data directory (`~/.clearmemory/`).
    pub fn data_dir() -> Result<PathBuf, ConfigError> {
        let home = dirs::home_dir().ok_or(ConfigError::DirNotFound)?;
        Ok(home.join(".clearmemory"))
    }

    /// Return the path to the config file (`~/.clearmemory/config.toml`).
    pub fn config_path() -> Result<PathBuf, ConfigError> {
        Ok(Self::data_dir()?.join("config.toml"))
    }

    /// Load config from `~/.clearmemory/config.toml`, falling back to defaults.
    ///
    /// Environment variable overrides:
    /// - `CLEARMEMORY_TIER` overrides `general.tier`
    /// - `CLEARMEMORY_PASSPHRASE` is read by the encryption module at runtime
    pub fn load() -> Result<Arc<Self>, ConfigError> {
        let path = Self::config_path()?;

        let mut config = if path.exists() {
            let contents = std::fs::read_to_string(&path)?;
            toml::from_str::<Config>(&contents)?
        } else {
            Config::default()
        };

        // Environment variable overrides
        if let Ok(tier_str) = std::env::var("CLEARMEMORY_TIER") {
            config.general.tier = match tier_str.as_str() {
                "offline" => Tier::Offline,
                "local_llm" => Tier::LocalLlm,
                "cloud" => Tier::Cloud,
                other => return Err(ConfigError::InvalidTier(other.to_string())),
            };
        }

        Ok(Arc::new(config))
    }

    /// Create the data directory structure if it doesn't exist.
    pub fn ensure_directories() -> Result<PathBuf, ConfigError> {
        let data_dir = Self::data_dir()?;

        let dirs = [
            data_dir.as_path(),
            &data_dir.join("verbatim"),
            &data_dir.join("archive").join("verbatim"),
            &data_dir.join("vectors"),
            &data_dir.join("models"),
            &data_dir.join("mental_models"),
            &data_dir.join("backups"),
        ];

        for dir in &dirs {
            if !dir.exists() {
                std::fs::create_dir_all(dir)?;
            }
        }

        Ok(data_dir)
    }

    /// Write a default config file to `~/.clearmemory/config.toml`.
    pub fn write_default(data_dir: &Path) -> Result<(), ConfigError> {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config)
            .map_err(|e| ConfigError::ParseError(toml::de::Error::custom(e.to_string())))?;
        std::fs::write(data_dir.join("config.toml"), toml_str)?;
        Ok(())
    }

    /// Return the SQLite database path.
    pub fn db_path(&self) -> Result<PathBuf, ConfigError> {
        Ok(Self::data_dir()?.join("clearmemory.db"))
    }

    /// Return the verbatim storage directory path.
    pub fn verbatim_dir(&self) -> Result<PathBuf, ConfigError> {
        Ok(Self::data_dir()?.join("verbatim"))
    }

    /// Return the archive verbatim directory path.
    pub fn archive_verbatim_dir(&self) -> Result<PathBuf, ConfigError> {
        Ok(Self::data_dir()?.join("archive").join("verbatim"))
    }

    /// Return the LanceDB vectors directory path.
    pub fn vectors_dir(&self) -> Result<PathBuf, ConfigError> {
        Ok(Self::data_dir()?.join("vectors"))
    }

    /// Return the models directory path.
    pub fn models_dir(&self) -> Result<PathBuf, ConfigError> {
        if self.models.model_path.is_empty() {
            Ok(Self::data_dir()?.join("models"))
        } else {
            Ok(PathBuf::from(&self.models.model_path))
        }
    }

    /// Return the mental models directory path.
    pub fn mental_models_dir(&self) -> Result<PathBuf, ConfigError> {
        Ok(Self::data_dir()?.join("mental_models"))
    }
}

// Custom deserialization error helper for toml Serializer errors
trait TomlDeErrorHelper {
    fn custom(msg: String) -> Self;
}

impl TomlDeErrorHelper for toml::de::Error {
    fn custom(msg: String) -> Self {
        // This creates a parse error with a custom message
        toml::from_str::<Config>(&format!("__invalid__ = {msg}")).unwrap_err()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.general.tier, Tier::Offline);
        assert_eq!(config.general.default_stream, "default");
        assert_eq!(config.retrieval.top_k, 10);
        assert_eq!(config.retrieval.token_budget, 4096);
        assert!(config.encryption.enabled);
        assert_eq!(config.server.http_port, 8080);
        assert_eq!(config.server.mcp_port, 9700);
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml_str = r#"
[general]
tier = "local_llm"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.tier, Tier::LocalLlm);
        // Everything else should be defaults
        assert_eq!(config.retrieval.top_k, 10);
    }

    #[test]
    fn test_parse_full_config() {
        let toml_str = r#"
[general]
tier = "cloud"
default_stream = "my-project"

[models]
embedding = "bge-small-en"

[retrieval]
top_k = 20
temporal_boost = 0.5
entity_boost = 0.4
token_budget = 8192

[retention]
time_threshold_days = 180
size_threshold_gb = 5.0

[server]
http_port = 9090
mcp_port = 9800

[encryption]
enabled = false
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.tier, Tier::Cloud);
        assert_eq!(config.general.default_stream, "my-project");
        assert_eq!(config.models.embedding, "bge-small-en");
        assert_eq!(config.retrieval.top_k, 20);
        assert_eq!(config.retrieval.token_budget, 8192);
        assert_eq!(config.retention.time_threshold_days, 180);
        assert!(!config.encryption.enabled);
    }

    #[test]
    fn test_tier_serialization() {
        assert_eq!(
            serde_json::to_string(&Tier::Offline).unwrap(),
            "\"offline\""
        );
        assert_eq!(
            serde_json::to_string(&Tier::LocalLlm).unwrap(),
            "\"local_llm\""
        );
        assert_eq!(serde_json::to_string(&Tier::Cloud).unwrap(), "\"cloud\"");
    }

    #[test]
    fn test_classification_ordering() {
        assert!(Classification::Public < Classification::Internal);
        assert!(Classification::Internal < Classification::Confidential);
        assert!(Classification::Confidential < Classification::Pii);
    }
}
