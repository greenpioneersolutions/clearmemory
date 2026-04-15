# Clear Memory — Security Model

## Overview

Clear Memory is designed for enterprise environments where AI conversation data is sensitive by default. This document covers every security control, threat mitigation, and compliance capability in the system. For architecture details, see `architecture.md`. For the full project constitution, see `CLAUDE.md`.

**Security philosophy:** Defense in depth. Every layer assumes the layer above it has been compromised. Encryption at rest means a stolen device yields nothing. Authentication means a rogue process can't read memories. Classification tracing means sensitive data never leaves the machine accidentally. The audit log means every action is accountable.

---

## Threat Model

| # | Threat | Attack Vector | Severity | Mitigation | Status |
|---|--------|---------------|----------|------------|--------|
| 1 | Unauthorized memory access | Malicious MCP/HTTP client on same machine or network | High | API token authentication with scopes on all interfaces | v1 |
| 2 | Data exfiltration via device theft | Laptop stolen, directory copied | Critical | At-rest encryption: SQLCipher (SQLite), AES-256-GCM (files, LanceDB) | v1 |
| 3 | Sensitive data sent to cloud APIs | PII/confidential content reaching Tier 3 providers | High | Classification-aware filtering on entire content pipeline (raw → curator → reflect → API) | v1 |
| 4 | Credential exposure in stored memories | API keys, tokens, passwords in transcripts | High | Secret scanning on retain path with warn/redact/block modes | v1 |
| 5 | Model supply chain poisoning | Compromised model on Hugging Face | High | Pinned revisions, self-published checksums, benchmark verification gate, enterprise model mirror | v1 |
| 6 | Verbatim file tampering | Direct filesystem modification | Medium | SHA-256 checksums verified on every expand | v1 |
| 7 | Audit log tampering | Replacing or modifying log entries | Medium | Append-only with chained hashes + external checkpoint anchors | v1 |
| 8 | DoS via API flooding | Compromised client flooding queries | Medium | Per-client rate limiting on all MCP/HTTP endpoints | v1 |
| 9 | DoS via large imports | Malicious import file | Medium | Size caps per operation + rate limiting on retain/import | v1 |
| 10 | Insider threat | Legitimate user accessing unauthorized streams | Medium | Access anomaly detection, confidential access justification, separation of duties | v1 |
| 11 | Unauthorized destructive operations | Malicious purge of another user's data | High | Dedicated purge scope + two-person authorization for shared deployments | v1 |
| 12 | Permanent credential reuse | Stolen API token used indefinitely | Medium | Token expiration with configurable TTL (default 90 days) | v1 |
| 13 | Backup exfiltration | Unencrypted backup on shared storage | High | Backup encryption with AES-256-GCM using master passphrase | v1 |
| 14 | Classification bypass via derived content | Confidential excerpts laundered through curator into cloud API | High | Classification tracing through entire content pipeline | v1 |
| 15 | Direct filesystem access bypassing app | User reads SQLite directly, ignoring stream permissions | Low | Documented limitation. All data encrypted at rest. v2 adds per-stream keys. | v1 (partial) |

---

## Encryption

### At-Rest Encryption

All stored data is encrypted at rest. This is not optional in enterprise deployments.

**SQLite database:** Encrypted via SQLCipher (AES-256-CBC). The `rusqlite` crate with `bundled-sqlcipher` feature provides transparent encryption. Every read and write goes through the SQLCipher layer. The database file is unreadable without the derived key.

**Verbatim transcript files:** Each file is encrypted with AES-256-GCM before writing to disk. The authentication tag ensures both confidentiality and integrity. File names are content hashes (opaque), revealing nothing about content.

**LanceDB vector data:** Encrypted at the application level. Data is encrypted before writing to the Lance columnar format and decrypted on read. This adds approximately 5% overhead to read/write operations. Vectors and metadata are both encrypted.

**Backup files:** `.cmb` backup archives are encrypted with AES-256-GCM after compression. Restore requires the master passphrase.

### Key Management

The encryption key is derived from a master passphrase using Argon2id — a memory-hard key derivation function resistant to GPU and ASIC attacks.

**Initialization:**
- On `clearmemory init`, the user sets a master passphrase
- Alternatively, one is auto-generated (displayed once, never stored)
- The passphrase derives a 256-bit encryption key via Argon2id
- Argon2id parameters: 64MB memory, 3 iterations (configurable)

**Runtime:**
- On startup, the passphrase is provided via interactive prompt or `CLEARMEMORY_PASSPHRASE` environment variable
- The derived key is held in memory for the duration of the process
- The passphrase itself is never written to disk, never logged, never included in error messages

**Rotation:**
- `clearmemory auth rotate-key` generates a new key from a new passphrase
- All data is re-encrypted with the new key (SQLite re-keyed, files re-encrypted, backups re-encrypted)
- The old key is securely zeroed from memory after rotation

**Configuration:**
```toml
[encryption]
enabled = true
cipher = "aes-256-gcm"
sqlite_cipher = "aes-256-cbc"
kdf = "argon2id"
kdf_memory_mb = 64
kdf_iterations = 3
passphrase_env_var = "CLEARMEMORY_PASSPHRASE"
```

---

## Authentication & Authorization

### API Tokens

Every MCP and HTTP request must include a valid API token. Tokens are scoped to limit what each client can do.

**Token generation:** On `clearmemory init`, a 256-bit token is generated using a cryptographically secure random number generator. The token is displayed once to the user. Only the SHA-256 hash is stored in config.

**Token scopes:**

| Scope | Permitted Operations |
|-------|---------------------|
| `read` | recall, expand, status, streams list, tags list |
| `read-write` | Everything in read + retain, import, forget, streams create, tags manage |
| `admin` | Everything in read-write + auth management, config changes, repair, compliance reporting |
| `purge` | Dedicated destructive operations: purge, hard delete. Intentionally separate from admin. |

A single token has exactly one scope. Multiple tokens can be issued with different scopes.

**Token lifecycle:**

| Event | Behavior |
|-------|----------|
| Creation | `clearmemory auth create --scope read --ttl 30d --label "monitoring"` |
| Validation | Every request checked against stored hash. Invalid → 401 + audit log entry. |
| Expiration | Tokens have configurable TTL (default 90 days). Expired → 401 with clear message. |
| Warning | 14 days before expiry: warning in health endpoint + daily log warning. |
| Rotation | `clearmemory auth rotate` generates new token, invalidates old. |
| Revocation | `clearmemory auth revoke --id <label>` immediately invalidates a specific token. |
| Status | `clearmemory auth status` shows all tokens with scope, expiry, last used timestamp. |

### Purge Authorization (Two-Person Rule)

Purge operations are irreversible permanent deletions. They require elevated authorization:

**Single-user deployment:** Requires `purge` scope token + `--confirm` flag. The `admin` scope alone cannot purge.

**Shared deployment (when `purge_requires_two_person = true`):**
1. User A (any write scope) requests purge with reason
2. System creates pending purge request, logged in audit trail
3. User B (with `purge` scope) approves the request
4. Only after approval does deletion execute
5. Pending requests expire after 72 hours (configurable)
6. Auto-backup is created before any purge execution

### Rate Limiting

All endpoints are rate-limited per client to prevent abuse:

| Operation Type | Default Limit |
|---------------|---------------|
| Read (recall, expand, status) | 1,000 req/min |
| Write (retain, forget, import) | 100 req/min |
| Reflect | 10 req/min |
| Auth operations | 10 req/min |
| Purge | 5 req/hour |
| HTTP body size (global) | 50 MB max |

Rate limit exceeded returns HTTP 429 with `Retry-After` header. All rate limit hits are logged with client identifier and included in observability metrics.

---

## Transport Security

### Local Deployment (Default)

- HTTP API binds to `127.0.0.1` — not accessible from other machines on the network
- Unix domain sockets (macOS/Linux) for MCP — protected by filesystem permissions (owner-only access)
- No data traverses the network in default configuration

### Shared Deployment (Network-Accessible)

When `bind_address` is set to `0.0.0.0` for shared deployments:

- TLS is required: `--tls-cert` and `--tls-key` flags must be provided
- Minimum TLS version: 1.2 (1.3 preferred)
- Mutual TLS supported via `tls_client_ca_path` for zero-trust environments
- All traffic is encrypted in transit

```toml
[security]
bind_address = "127.0.0.1"     # default: local only
tls_cert_path = ""              # required if bind_address != 127.0.0.1
tls_key_path = ""
tls_client_ca_path = ""         # mutual TLS: require client certificates
```

---

## Secret Scanning & Redaction

A secret scanning pipeline runs on the `retain` path before any content is stored. This prevents Clear Memory from becoming a long-term credential store.

### Detection Patterns (Built-in)

| Pattern Category | Examples |
|-----------------|----------|
| AWS credentials | `AKIA...`, `aws_secret_access_key=` |
| GitHub tokens | `ghp_`, `gho_`, `ghs_`, `github_pat_` |
| Generic API keys | `api_key=`, `apikey:`, `x-api-key:` |
| Database connection strings | `postgres://`, `mysql://`, `mongodb://`, `redis://` |
| Private keys | `-----BEGIN RSA PRIVATE KEY-----`, `-----BEGIN OPENSSH PRIVATE KEY-----` |
| JWT tokens | `eyJ...` (base64 JSON with alg/typ headers) |
| Generic passwords | `password=`, `passwd:`, `secret=` (followed by non-whitespace) |
| Anthropic API keys | `sk-ant-` |
| OpenAI API keys | `sk-proj-`, `sk-` (40+ chars) |

Custom patterns can be added via config. Specific built-in patterns can be disabled.

### Scanning Modes

| Mode | Behavior | Use Case |
|------|----------|----------|
| `warn` (default) | Store memory as-is. Flag with `contains_secrets=true`. Auto-classify as `confidential`. Log warning. | Development environments where visibility is preferred over blocking |
| `redact` | Replace detected secrets with `[REDACTED:<pattern_type>]` before storage. Original content never stored. | Production environments with strict credential management |
| `block` | Reject the retain operation. Return error to caller. | High-security environments with zero tolerance for credential exposure |

### Retroactive Scanning

```bash
clearmemory security scan                          # scan all stored memories
clearmemory security scan --stream my-project      # scan specific stream
clearmemory security scan --remediate              # redact secrets in existing memories
```

Retroactive remediation re-encrypts the verbatim file with secrets replaced by `[REDACTED]` markers. The original content is overwritten and unrecoverable (this is intentional for credential management).

### Detection Limitations

The current secret scanning pipeline is **regex-based** and catches known pattern formats. It has inherent limitations:

| Limitation | Example | Why It's Missed |
|------------|---------|----------------|
| Encoded secrets | Base64-encoded API keys, URL-encoded tokens | Regex matches raw patterns, not decoded content |
| Context-dependent secrets | `password = config["db_pass"]` (no literal value) | The credential isn't in the text — only a reference to it |
| High-entropy strings without known prefixes | `a8f2b9c1d4e5...` (64-char hex string used as a key) | No known prefix like `AKIA` or `ghp_` to anchor the match |
| Secrets in non-text formats | Binary data, images with embedded metadata | Text-only scanning |
| Rotated/custom credential formats | Organization-specific token formats | Only built-in patterns are detected |

**The current scanning is a net — not a guarantee.** It catches the most common credential patterns but should not be relied upon as the sole control against credential exposure. Secret rotation, access scoping, and credential management policies remain essential.

### Secret Scanning Hardening Roadmap

**v1.1 — Entropy-based detection (planned)**

Add a Shannon entropy analysis pass for strings that appear in key-value contexts. When a string has entropy above a configurable threshold (default: 4.5 bits/char) and appears as a value in a key-value pattern (e.g., `token = "..."`, `api_key: ...`, `Authorization: Bearer ...`), flag it as a potential secret.

This catches high-entropy strings that don't match any known prefix pattern — such as custom-format API keys, generated passwords, and hex-encoded secrets.

```toml
[security.secret_scanning]
entropy_detection_enabled = false    # v1.1 planned
entropy_threshold = 4.5             # Shannon entropy bits per character
entropy_min_length = 20             # minimum string length to analyze
```

**v1.2 — Structured format scanning (planned)**

Parse JSON, YAML, TOML, and `.env` content within memories and scan values in keys matching secret-related names: `password`, `passwd`, `token`, `secret`, `key`, `credential`, `api_key`, `apikey`, `access_key`, `private_key`, `auth`. This catches secrets that are properly structured in config files but don't match any specific provider pattern.

**v2 — LLM-based secret detection (planned)**

Investigate integration with GitHub Advanced Security's secret scanning pattern database for broader coverage. Alternatively, use the curator model (Qwen3-0.6B) or a dedicated classifier to identify secrets through content understanding rather than pattern matching — recognizing that "the database password is hunter2" contains a credential even though `hunter2` has low entropy and no known prefix.

---

## Data Classification

Every memory carries a classification label that controls access and cloud eligibility.

| Classification | Access Control | Cloud API Eligible | Audit Behavior |
|----------------|---------------|-------------------|----------------|
| `public` | Anyone with stream access | Yes | Standard logging |
| `internal` (default) | Authenticated users only | Yes | Standard logging |
| `confidential` | Stream owner + authorized users only | No — local inference only | Enhanced logging |
| `pii` | Stream owner + authorized users only | No — local inference only | Enhanced logging + right-to-delete eligible |

### Classification Pipeline Tracing

The classification check applies to the entire content pipeline, not just raw memories:

```
Memory (confidential) → retrieval results
    → classification check: confidential content identified
        → if Tier 3 cloud: BLOCK from cloud API, fall back to local inference
    → curator receives content (local model, OK)
        → curator output inherits source classification: confidential
    → reflect receives curator output
        → if Tier 3 cloud AND source is confidential: BLOCK, use local model
    → final output to user: OK (never left the machine)
```

Every piece of derived content carries a `source_classifications` field tracking all source memory classifications. The highest classification in the chain determines cloud eligibility.

### Classification Roadmap

**Phase 1: v1 — Manual classification with auto-escalation (current)**

Classification is set manually on retain (`--classification confidential`) or defaults to `internal`. Auto-escalation occurs only when the secret scanner detects credentials — the memory is automatically classified as `confidential` regardless of the user-specified level.

**Phase 2: v1.x — PII pattern detection**

When `pii_detection_enabled = true` in config, the retain path runs PII pattern detection in addition to secret scanning. Detected PII auto-classifies the memory as `pii`.

Detected PII patterns:

| Pattern | Examples | Regex |
|---------|----------|-------|
| Email addresses | `user@company.com` | `[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}` |
| Phone numbers | `+1-555-123-4567`, `(555) 123-4567` | `(\+?1[-.]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}` |
| Social Security Numbers | `123-45-6789` | `\b\d{3}-\d{2}-\d{4}\b` |
| Credit card numbers | `4111-1111-1111-1111` | `\b\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}\b` |
| IP addresses (v4) | `192.168.1.1` | `\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b` |
| Names in key-value context | `name: John Smith`, `author: Jane Doe` | `(name|author|patient|employee)\s*[:=]\s*[A-Z][a-z]+\s+[A-Z][a-z]+` |
| Date of birth patterns | `DOB: 01/15/1990` | `(dob|date.of.birth|born)\s*[:=]\s*\d{1,2}[/.-]\d{1,2}[/.-]\d{2,4}` |

Like secret scanning, PII detection supports three modes: `warn` (flag + auto-classify as pii), `redact` (replace with `[PII:<type>]`), `block` (reject retain).

```toml
[compliance]
pii_detection_enabled = false    # enable for environments handling personal data
pii_detection_mode = "warn"      # "warn", "redact", "block"
```

**Phase 3: v2 — LLM-based content classification**

Use the curator model (Qwen3-0.6B) or a dedicated classification model to automatically classify content at ingestion time based on content analysis — not just pattern matching. This enables classification based on topic sensitivity (e.g., a discussion about a security vulnerability is `confidential` even without credentials present), organizational context, and semantic understanding of what constitutes sensitive information.

---

## Audit Logging

### Structure

Every operation that reads or modifies data creates an audit log entry:

```sql
audit_log (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,           -- ISO 8601
    user_id TEXT,                      -- from API token or request header
    operation TEXT NOT NULL,           -- retain, recall, expand, reflect, forget, import, purge, auth
    memory_id TEXT,                    -- affected memory (if applicable)
    stream_id TEXT,                    -- affected stream (if applicable)
    classification TEXT,               -- classification of affected memory
    compliance_event INTEGER DEFAULT 0,-- 1 for purge, legal hold, audit export
    anomaly_flag INTEGER DEFAULT 0,    -- 1 if insider detection flagged this
    chain_hash TEXT NOT NULL,          -- SHA-256(previous_chain_hash + this_entry)
    details TEXT                       -- JSON: query, results count, latency, etc.
)
```

### Tamper Evidence

**Chained hashes:** Each entry's `chain_hash` is computed as `SHA-256(previous_entry.chain_hash + current_entry_content)`. Modifying any entry in the middle breaks the chain for all subsequent entries.

**External checkpoint anchors:** Every 1,000 entries or every 6 hours (whichever comes first), the system writes the current chain hash to:
1. `~/.clearmemory/audit_checkpoints.log` (separate file, outside the database)
2. stdout/stderr (captured by enterprise log aggregators: Splunk, Datadog, syslog)
3. OpenTelemetry metrics pipeline (if configured)

If the entire audit log is replaced with a fabricated chain, the checkpoint mismatch is detectable from external records.

**Verification:**
```bash
clearmemory audit verify              # validate entire chain, report broken links
clearmemory audit verify --verbose    # show per-entry hashes
```

### Append-Only Guarantee

Audit log entries cannot be modified or deleted through any Clear Memory command, including admin operations. The only way to modify the audit log is direct filesystem access to the SQLite database — which is encrypted via SQLCipher, requiring the master passphrase.

### Export

```bash
clearmemory audit export --from 2026-01-01 --to 2026-04-12 --format csv
clearmemory audit export --format json
clearmemory audit export --stream my-project --format csv
clearmemory audit export --filter "compliance_event=1" --format json
```

---

## Compliance Capabilities

### Right to Delete (GDPR / CCPA)

Two distinct operations serve different compliance needs:

**`forget` (temporal invalidation):** Marks memories as superseded. Facts get `valid_until` timestamps. Memory is excluded from current queries but remains accessible for historical queries. This is the normal workflow operation.

**`purge` (permanent deletion):** Physically removes all traces of a memory. Deletes: SQLite record, LanceDB vectors, verbatim file (active + archive), associated facts, entity relationships, and tags. Writes a purge event to the audit log recording that deletion occurred (but not the deleted content). Requires `purge` scope token. Auto-backup created before execution.

### Legal Hold

Streams can be frozen to prevent modification or deletion during litigation:

```bash
clearmemory hold --stream q1-migration --reason "Litigation: Case #2026-1234"
clearmemory hold --release --stream q1-migration
clearmemory hold --list
```

**Held stream behavior:**
- Cannot be forgotten, purged, archived, or have memories modified
- New memories CAN be added (preservation doesn't prevent ongoing work)
- Hold is recorded in audit log with reason and timestamp
- Attempting to modify a held memory returns an error with the hold reason
- Release requires admin scope and is logged

### Compliance Reporting

```bash
clearmemory compliance report                    # full report to stdout
clearmemory compliance report --format csv       # for auditors
clearmemory compliance report --format json      # for tooling
```

**Report contents:**
- Total memory count by classification level (public, internal, confidential, pii)
- Memory age distribution (0-30d, 30-90d, 90-180d, 180d+)
- Per-stream breakdown: owner, visibility, memory count, classification distribution
- PII-flagged memory count and locations
- Secrets-flagged memory count
- Active legal holds with reasons and durations
- Recent purge operations
- Retention policy configuration and recent trigger events
- Token status (active, approaching expiry, expired)

---

## Insider Threat Detection

For shared deployments, Clear Memory monitors access patterns for anomalies.

### Access Pattern Tracking

The system maintains per-user baselines:
- Which streams they typically query
- How frequently they query
- What times of day they're active
- What classification levels they access

### Anomaly Detection

When a user's behavior deviates significantly (default: 3 standard deviations) from their baseline, the event is flagged:

- `anomaly_flag = 1` in the audit log entry
- Warning logged to tracing output
- Metric emitted via OpenTelemetry (if configured)

**Examples of flagged behavior:**
- User who normally queries Stream A suddenly queries Streams B, C, D, E
- User who averages 5 queries/day suddenly runs 200 queries in an hour
- User accessing confidential-classified memories for the first time
- Access outside the user's normal working hours pattern

### Confidential Access Justification

When `require_justification_for_confidential = true`, any recall or expand operation targeting a `confidential`-classified memory requires the caller to provide an access reason. The reason is recorded in the audit log alongside the access event. This doesn't block access — it creates accountability.

### Configuration

```toml
[security.insider_detection]
enabled = false                            # enable for shared deployments
anomaly_threshold_stddev = 3.0
require_justification_for_confidential = false
alert_on_anomaly = true
```

---

## Model Supply Chain Security

### Threat

ML models are executable code. A poisoned embedding model could produce subtly biased vectors that degrade retrieval quality without obvious errors. A poisoned curator model could exfiltrate data through its outputs.

### Mitigations

**Pinned model revisions:** The `models.manifest` references exact Hugging Face commit hashes, not just model names. Example: `BAAI/bge-m3@a1b2c3d4` — this prevents silent substitution.

**Self-published checksums:** SHA-256 checksums for all model files are published in the Clear Memory repository. Verification compares downloaded files against these checksums — not against checksums from Hugging Face. An attacker would need to compromise both Hugging Face AND the Clear Memory repository.

**ed25519 manifest signature:** The `models.manifest` file is signed. Clear Memory verifies the signature on every model load. Tampering with model files or the manifest is detected.

**Benchmark verification gate:** Before any model version is accepted into the manifest, it must pass the full LongMemEval benchmark suite in CI/CD. A poisoned model that degrades retrieval quality would fail this gate.

**Enterprise model mirror:** For maximum supply chain control:
1. Admin downloads models to an internal mirror: `clearmemory models download --all --output /path/`
2. Developer machines are configured to use the internal mirror only
3. `auto_download = false` prevents any network model downloads
4. The enterprise never trusts Hugging Face directly

**Verification command:**
```bash
clearmemory models verify                  # check all models against manifest
clearmemory models verify --verbose        # show per-file checksums and signature status
```

---

## Incident Response

See `CLAUDE.md` for the full incident response playbook covering five incident types:

1. **Device lost or stolen** — token revocation, encryption protects data, restore from backup
2. **Unauthorized stream access** — token revocation, legal hold for evidence preservation, audit export
3. **Poisoned model detected** — server stop, model verification, re-download from internal mirror, reindex
4. **Secret exposure in memories** — credential rotation, retroactive redaction, cloud API exposure assessment
5. **Audit log integrity breach** — chain verification, external checkpoint cross-reference, evidence preservation

Each playbook includes: detection criteria, immediate containment steps (with exact CLI commands), assessment procedures, and recovery steps.

---

## Security Configuration Reference

All security-related configuration in one place:

```toml
[encryption]
enabled = true
cipher = "aes-256-gcm"
sqlite_cipher = "aes-256-cbc"
kdf = "argon2id"
kdf_memory_mb = 64
kdf_iterations = 3
passphrase_env_var = "CLEARMEMORY_PASSPHRASE"

[auth]
require_token = true
default_token_ttl_days = 90

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
mode = "warn"
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

[compliance]
default_classification = "internal"
pii_detection_enabled = false
require_classification_on_retain = false
legal_hold_enabled = true
purge_requires_two_person = false
purge_request_ttl_hours = 72
```
