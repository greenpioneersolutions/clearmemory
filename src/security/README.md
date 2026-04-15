# security/ -- Authentication, Encryption, Rate Limiting, and Threat Mitigation

## Role in the Architecture

The `security` module is the largest module in the codebase and implements the full security stack for Clear Memory. It spans authentication and token management, at-rest encryption, transport security, secret detection and redaction, rate limiting, data classification tracing, cloud content filtering, and insider threat detection. These components work together to address the threat model described in CLAUDE.md.

Security is not gated by tier -- all tiers get the same security primitives. The key architectural decisions are:

- **At-rest encryption is a v1 feature** (not deferred to v2). SQLite is encrypted via SQLCipher (AES-256-CBC), verbatim files via AES-256-GCM, and LanceDB via application-level encryption. All keyed from a single master passphrase via Argon2id key derivation.
- **API token authentication** protects all MCP/HTTP endpoints with scoped tokens (read, read-write, admin, purge).
- **Secret scanning** runs on the retain path to prevent Clear Memory from becoming a long-term credential store.
- **Classification tracing** ensures that confidential/PII content never leaks to Tier 3 cloud APIs, even through derived content (curator output, reflect input).

## File-by-File Description

### `mod.rs`

Module root. Re-exports all nine submodules:

- `pub mod auth;`
- `pub mod classification_tracer;`
- `pub mod cloud_filter;`
- `pub mod encryption;`
- `pub mod insider_detection;`
- `pub mod rate_limiter;`
- `pub mod redactor;`
- `pub mod secret_scanner;`
- `pub mod tls;`

### `auth.rs`

API token authentication with scoped permissions and expiration.

**Key types:**

- **`Scope` (enum)** -- Token permission levels: `Read`, `ReadWrite`, `Admin`, `Purge`. Ordered from least to most privileged (except `Purge` which is a separate, dedicated scope).
  - `parse(s: &str) -> Option<Self>` -- Parses from string ("read", "read-write", "admin", "purge")
  - `as_str(&self) -> &'static str` -- Converts to string
  - `satisfies(&self, required: Scope) -> bool` -- Checks if this scope satisfies a requirement. Admin satisfies Read and ReadWrite. Purge only satisfies Purge (separation of duties). Read satisfies only Read.

- **`ValidatedToken`** -- Result of successful token validation. Fields:
  - `id: String` -- Token identifier (e.g., "primary", "readonly")
  - `scope: Scope` -- The token's permission level
  - `label: Option<String>` -- Optional descriptive label

- **`TokenValidator`** -- Stateful validator that checks incoming tokens against stored hashes.
  - `from_config(tokens: &[TokenConfig]) -> Self` -- Creates a validator from config file token entries. Parses expiration dates and scopes.
  - `permissive() -> Self` -- Creates a validator with no tokens configured; all requests pass as Admin scope (development mode).
  - `validate(&self, bearer_token: &str) -> Result<ValidatedToken, AuthError>` -- Validates a bearer token. If no tokens are configured, allows everything (dev mode). Otherwise, hashes the token with SHA-256 and looks it up. Checks expiration. Returns `AuthError::InvalidToken` or `AuthError::TokenExpired` on failure.
  - `check_scope(token: &ValidatedToken, required: Scope) -> Result<(), AuthError>` -- Static method that checks if a validated token has sufficient scope. Returns `AuthError::InsufficientScope` with details on failure.

**Key functions:**

- **`generate_token() -> (String, String)`** -- Generates a new 256-bit API token using `rand::thread_rng()`. Returns `(raw_token, sha256_hash)`. Raw tokens are prefixed with `cmk_` followed by 64 hex characters. The hash is stored in config; the raw token is shown to the user once.
- **`hash_token(token: &str) -> String`** -- SHA-256 hashes a token for storage. Returns format `"sha256:<hex>"`.

**Error types used:** `AuthError` (defined in `src/lib.rs`) with variants `InvalidToken`, `TokenExpired`, `InsufficientScope { required, actual }`.

### `encryption.rs`

At-rest encryption using AES-256-GCM with Argon2id key derivation.

**Key types:**

- **`EncryptionProvider` (trait)** -- Abstract encryption interface. Requires `Send + Sync`. Methods:
  - `encrypt_bytes(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError>` -- Encrypts data. Returns nonce prepended to ciphertext.
  - `decrypt_bytes(&self, ciphertext: &[u8]) -> Result<Vec<u8>, EncryptionError>` -- Decrypts data. Expects nonce prepended.
  - `sqlite_key_hex(&self) -> Result<String, EncryptionError>` -- Returns hex-encoded key for SQLCipher `PRAGMA key`.
  - `is_enabled(&self) -> bool` -- Whether encryption is active.

- **`Argon2GcmProvider`** -- Real encryption provider. Holds a derived 256-bit key.
  - `from_passphrase(passphrase: &str, config: &EncryptionConfig) -> Result<Self, EncryptionError>` -- Derives key from passphrase using Argon2id with configurable memory and iteration parameters. Uses a deterministic salt derived from the passphrase (appropriate for Clear Memory's single-user model where the same passphrase must always produce the same key).
  - `from_key(key: [u8; 32]) -> Self` -- Creates from a pre-derived key (for testing or key rotation).
  - Nonce is 96 bits (12 bytes), randomly generated per encryption operation. Ciphertext format: `[nonce (12 bytes)][ciphertext + AES-GCM tag]`.

- **`NoopProvider`** -- Pass-through provider for when encryption is disabled. `encrypt_bytes` and `decrypt_bytes` return input unchanged.

**Key functions:**

- **`create_provider(config: &EncryptionConfig) -> Result<Arc<dyn EncryptionProvider>, EncryptionError>`** -- Factory that creates the appropriate provider based on config. If encryption is disabled, returns `NoopProvider`. If enabled, reads the passphrase from the environment variable specified by `config.passphrase_env_var` and creates an `Argon2GcmProvider`.
- **`create_provider_with_passphrase(passphrase: &str, config: &EncryptionConfig) -> Result<Arc<dyn EncryptionProvider>, EncryptionError>`** -- Same as above but takes an explicit passphrase (for `clearmemory init` and interactive use).

**Error types used:** `EncryptionError` (defined in `src/lib.rs`) with variants `EncryptFailed(String)`, `DecryptFailed(String)`, `KeyDerivationFailed(String)`, `PassphraseRequired`.

### `secret_scanner.rs`

Detects credentials and secrets in content before storage.

**Key types:**

- **`SecretMatch`** -- A detected secret. Fields:
  - `pattern_name: String` -- Which pattern matched (e.g., "aws_key", "github_token")
  - `start: usize` -- Byte offset of match start
  - `end: usize` -- Byte offset of match end

- **`SecretScanner`** -- Compiled regex scanner. Uses `regex::RegexSet` for efficient multi-pattern matching plus individual `regex::Regex` patterns for extracting match positions.
  - `new() -> Self` -- Creates a scanner with all built-in patterns.
  - `scan(&self, content: &str) -> Vec<SecretMatch>` -- Scans content for all secret patterns. Returns all matches with positions.
  - `has_secrets(&self, content: &str) -> bool` -- Quick boolean check for any secrets.

**Built-in detection patterns (10 total):**
- `aws_key` -- AKIA followed by 16 alphanumeric chars
- `aws_secret` -- aws_secret_access_key assignment
- `github_token` -- ghp_, gho_, ghs_, github_pat_ prefixed tokens
- `generic_api_key` -- api_key/apikey/x-api-key assignments
- `database_url` -- postgres://, mysql://, mongodb://, redis:// URLs
- `private_key` -- PEM private key headers
- `jwt_token` -- Base64-encoded JWT with header.payload.signature structure
- `generic_password` -- password/passwd/secret assignments (8+ chars)
- `anthropic_key` -- sk-ant- prefixed keys
- `openai_key` -- sk- or sk-proj- prefixed keys (40+ chars)

### `redactor.rs`

Replaces detected secrets with `[REDACTED:<type>]` markers.

**Key functions:**

- **`redact(content: &str, matches: &[SecretMatch]) -> String`** -- Replaces all matched secret spans with `[REDACTED:<pattern_name>]`. Processes matches from end to start to preserve byte offsets during replacement.
- **`scan_and_redact(scanner: &SecretScanner, content: &str) -> (String, Vec<SecretMatch>)`** -- Convenience function that scans and redacts in one call. Returns both the redacted content and the list of matches found.

### `rate_limiter.rs`

Per-client rate limiting for MCP/HTTP endpoints using the `governor` crate.

**Key types:**

- **`OpCategory` (enum)** -- Operation categories with different rate limits: `Read`, `Write`, `Reflect`, `Auth`, `Purge`.

- **`RateLimiter`** -- Per-client rate limiter. Maintains a `HashMap` of `(client_id, OpCategory)` to governor limiters, protected by a `Mutex`.
  - `new(config: &RateLimitingConfig) -> Self` -- Creates from config.
  - `check(&self, client_id: &str, category: OpCategory) -> Result<(), u64>` -- Checks if a request is allowed. Returns `Ok(())` if allowed, `Err(retry_after_seconds)` if rate limited. When disabled, always allows. Lazily creates per-client limiters on first request.

### `cloud_filter.rs`

Filters content by classification level before sending to Tier 3 cloud APIs.

**Key functions:**

- **`is_cloud_eligible(classification: Classification, eligible: &[String]) -> bool`** -- Checks if a classification level is in the list of cloud-eligible classifications. Uses the `Classification` enum from `src/lib.rs`.
- **`highest_classification(classifications: &[Classification]) -> Classification`** -- Returns the highest (most restrictive) classification from a set. Defaults to `Internal` if the set is empty.

### `classification_tracer.rs`

Tracks classification levels through the content pipeline so derived content inherits source classifications.

**Key types:**

- **`ClassificationTrace`** -- Accumulates source classification levels as content flows through the pipeline.
  - `new() -> Self` -- Creates an empty trace.
  - `add_source(&mut self, classification: Classification)` -- Adds a source classification.
  - `effective_classification(&self) -> Classification` -- Returns the highest classification across all sources. Defaults to `Internal` if no sources added.

This ensures that if a curator model processes memories classified as `confidential`, the curator output is also treated as `confidential` and blocked from Tier 3 cloud APIs.

### `tls.rs`

TLS configuration and validation for shared deployments.

**Key types:**

- **`TlsConfig`** -- TLS configuration. Fields:
  - `cert_path: String` -- Path to PEM-encoded server certificate
  - `key_path: String` -- Path to PEM-encoded server private key
  - `client_ca_path: String` -- Optional path to CA certificate for mutual TLS (mTLS)

**Key functions:**

- **`is_tls_configured(config: &TlsConfig) -> bool`** -- Returns true if both cert and key paths are non-empty.
- **`validate_tls_config(config: &TlsConfig) -> Result<(), String>`** -- Validates that all configured file paths exist on disk. Checks cert, key, and optionally client CA.

### `insider_detection.rs`

Access anomaly detection for shared deployments.

**Key types:**

- **`AccessPattern`** -- A single access event to check. Fields: `user_id`, `stream_id`, `timestamp`, `operation`.
- **`AccessProfile`** -- A user's historical access profile. Fields: `user_id`, `usual_streams` (list of stream IDs), `avg_daily_queries`, `typical_hours` (tuple of start/end hour).
- **`AnomalyEvent`** -- A flagged anomaly. Fields: `user_id`, `event_type`, `severity` (float), `details`, `timestamp`.

**Key functions:**

- **`log_access(conn: &Connection, pattern: &AccessPattern) -> Result<(), rusqlite::Error>`** -- Records an access event in the audit log. Uses placeholder hash chain values (the full audit module handles tamper-evident chained hashes).
- **`detect_anomalies(conn: &Connection, user_id: &str, current_access: &AccessPattern, _threshold_stddev: f64) -> Result<Vec<AnomalyEvent>, rusqlite::Error>`** -- Detects anomalies by comparing current access against the user's historical profile. Currently implements one check: **unfamiliar stream access** (user is accessing a stream they have never queried before, when they do have history on other streams). The `threshold_stddev` parameter is reserved for future statistical scoring. No anomalies are flagged for users with no history (first access ever).

## Key Public Types Other Modules Depend On

- **`TokenValidator` / `ValidatedToken` / `Scope`** -- Used by MCP/HTTP request handlers to authenticate and authorize every request.
- **`EncryptionProvider` (trait)** -- Used by the storage layer (`src/storage/`) for encrypting verbatim files and LanceDB data, and for providing the SQLCipher key.
- **`SecretScanner`** -- Used by the retain/import pipeline to scan content before storage.
- **`RateLimiter` / `OpCategory`** -- Used by MCP/HTTP server middleware to enforce rate limits.
- **`ClassificationTrace`** -- Used by the retrieval and context compilation pipeline to track classification through derived content.
- **`is_cloud_eligible()` / `highest_classification()`** -- Used by the context compiler and reflect engine to gate Tier 3 cloud API calls.

## Relevant config.toml Keys

```toml
[auth]
require_token = true
default_token_ttl_days = 90
tokens = [
    { id = "primary", token_hash = "sha256:...", scope = "admin", created_at = "...", expires_at = "..." }
]

[encryption]
enabled = true
cipher = "aes-256-gcm"
sqlite_cipher = "aes-256-cbc"
kdf = "argon2id"
kdf_memory_mb = 64
kdf_iterations = 3
passphrase_env_var = "CLEARMEMORY_PASSPHRASE"

[security]
bind_address = "127.0.0.1"
tls_cert_path = ""
tls_key_path = ""
tls_client_ca_path = ""
cloud_eligible_classifications = ["public", "internal"]
max_import_size_mb = 500
max_memory_size_mb = 10

[security.secret_scanning]
enabled = true
mode = "warn"                   # "warn", "redact", "block"
custom_patterns = []
exclude_patterns = []

[security.rate_limiting]
enabled = true
read_rpm = 1000
write_rpm = 100
reflect_rpm = 10
auth_rpm = 10
purge_rph = 5
max_request_body_mb = 50

[security.insider_detection]
enabled = false
anomaly_threshold_stddev = 3.0
require_justification_for_confidential = false
alert_on_anomaly = true
```

## Deferred / Planned Functionality

- **Custom secret scanning patterns:** The `custom_patterns` and `exclude_patterns` config fields are defined but not yet wired into `SecretScanner::new()`.
- **Secret scanning modes:** The three modes (warn, redact, block) are described in the architecture but the mode selection logic is not yet integrated into the retain pipeline -- the scanner and redactor are available as building blocks.
- **`clearmemory security scan` command:** Retroactive scanning of existing memories for secrets is planned but not yet implemented as a CLI command.
- **Burst access detection:** The insider detection currently only checks for unfamiliar stream access. Burst access detection (abnormally high query volume) and off-hours access detection are planned.
- **Statistical anomaly scoring:** The `threshold_stddev` parameter is accepted but not yet used for statistical scoring. The current implementation uses a simple "have they accessed this stream before?" check.
- **Confidential access justification:** The `require_justification_for_confidential` config option is defined but the justification prompt and recording logic are not yet implemented.
- **Token rotation CLI:** The `clearmemory auth rotate` and `clearmemory auth rotate-key` commands are architecturally defined but the implementation connecting them to the auth module is at the CLI/handler level.
- **Mutual TLS integration:** The `TlsConfig` and validation are implemented, but actual TLS server setup with `axum` is handled at the server layer and is not yet fully wired.
