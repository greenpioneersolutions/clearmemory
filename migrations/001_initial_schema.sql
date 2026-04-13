-- Clear Memory v1 initial schema
-- All tables for the core engine

-- Schema version tracking
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL,
    description TEXT
);

CREATE TABLE IF NOT EXISTS migration_log (
    id TEXT PRIMARY KEY,
    from_version INTEGER,
    to_version INTEGER,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    status TEXT NOT NULL,       -- 'success', 'failed', 'rolled_back'
    error_message TEXT
);

-- Memories: the core record linking to verbatim content
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    summary TEXT,
    source_format TEXT NOT NULL,
    classification TEXT NOT NULL DEFAULT 'internal',
    created_at TEXT NOT NULL,
    last_accessed_at TEXT,
    access_count INTEGER DEFAULT 0,
    archived INTEGER DEFAULT 0,
    owner_id TEXT,
    stream_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_memories_content_hash ON memories(content_hash);
CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at);
CREATE INDEX IF NOT EXISTS idx_memories_stream_id ON memories(stream_id);
CREATE INDEX IF NOT EXISTS idx_memories_archived ON memories(archived);
CREATE INDEX IF NOT EXISTS idx_memories_classification ON memories(classification);

-- Tags: many-to-many between memories and tags
CREATE TABLE IF NOT EXISTS memory_tags (
    memory_id TEXT NOT NULL,
    tag_type TEXT NOT NULL,
    tag_value TEXT NOT NULL,
    FOREIGN KEY (memory_id) REFERENCES memories(id),
    PRIMARY KEY (memory_id, tag_type, tag_value)
);

CREATE INDEX IF NOT EXISTS idx_memory_tags_type_value ON memory_tags(tag_type, tag_value);

-- Facts: extracted temporal assertions
CREATE TABLE IF NOT EXISTS facts (
    id TEXT PRIMARY KEY,
    memory_id TEXT NOT NULL,
    subject TEXT NOT NULL,
    predicate TEXT NOT NULL,
    object TEXT NOT NULL,
    valid_from TEXT,
    valid_until TEXT,
    ingested_at TEXT NOT NULL,
    invalidated_at TEXT,
    confidence REAL DEFAULT 1.0,
    FOREIGN KEY (memory_id) REFERENCES memories(id)
);

CREATE INDEX IF NOT EXISTS idx_facts_memory_id ON facts(memory_id);
CREATE INDEX IF NOT EXISTS idx_facts_subject ON facts(subject);
CREATE INDEX IF NOT EXISTS idx_facts_valid_until ON facts(valid_until);

-- Entities: resolved entity nodes
CREATE TABLE IF NOT EXISTS entities (
    id TEXT PRIMARY KEY,
    canonical_name TEXT NOT NULL,
    entity_type TEXT,
    first_seen TEXT NOT NULL,
    last_seen TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_entities_canonical_name ON entities(canonical_name);
CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type);

-- Entity aliases
CREATE TABLE IF NOT EXISTS entity_aliases (
    alias TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    FOREIGN KEY (entity_id) REFERENCES entities(id),
    PRIMARY KEY (alias, entity_id)
);

CREATE INDEX IF NOT EXISTS idx_entity_aliases_alias ON entity_aliases(alias);

-- Entity relationships: edges in the entity graph
CREATE TABLE IF NOT EXISTS entity_relationships (
    source_entity_id TEXT NOT NULL,
    target_entity_id TEXT NOT NULL,
    relationship TEXT NOT NULL,
    memory_id TEXT,
    valid_from TEXT,
    valid_until TEXT,
    FOREIGN KEY (source_entity_id) REFERENCES entities(id),
    FOREIGN KEY (target_entity_id) REFERENCES entities(id),
    PRIMARY KEY (source_entity_id, target_entity_id, relationship)
);

CREATE INDEX IF NOT EXISTS idx_entity_rel_source ON entity_relationships(source_entity_id);
CREATE INDEX IF NOT EXISTS idx_entity_rel_target ON entity_relationships(target_entity_id);

-- Streams: scoped views across tags
CREATE TABLE IF NOT EXISTS streams (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    owner_id TEXT NOT NULL,
    visibility TEXT DEFAULT 'private',
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_streams_name ON streams(name);

-- Stream tag filters
CREATE TABLE IF NOT EXISTS stream_tags (
    stream_id TEXT NOT NULL,
    tag_type TEXT NOT NULL,
    tag_value TEXT NOT NULL,
    FOREIGN KEY (stream_id) REFERENCES streams(id),
    PRIMARY KEY (stream_id, tag_type, tag_value)
);

-- Stream write access
CREATE TABLE IF NOT EXISTS stream_writers (
    stream_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    FOREIGN KEY (stream_id) REFERENCES streams(id),
    PRIMARY KEY (stream_id, user_id)
);

-- Audit log with chained hashes
CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    user_id TEXT,
    operation TEXT NOT NULL,
    memory_id TEXT,
    stream_id TEXT,
    details TEXT,
    classification TEXT,
    compliance_event INTEGER DEFAULT 0,
    anomaly_flag INTEGER DEFAULT 0,
    hash TEXT NOT NULL,
    previous_hash TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_log_timestamp ON audit_log(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_log_operation ON audit_log(operation);
CREATE INDEX IF NOT EXISTS idx_audit_log_memory_id ON audit_log(memory_id);

-- Retention events
CREATE TABLE IF NOT EXISTS retention_events (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    trigger_type TEXT NOT NULL,
    memories_archived INTEGER,
    details TEXT
);

-- Performance baselines
CREATE TABLE IF NOT EXISTS performance_baselines (
    id TEXT PRIMARY KEY,
    measured_at TEXT NOT NULL,
    p95_recall_ms REAL NOT NULL,
    corpus_size_bytes INTEGER NOT NULL,
    memory_count INTEGER NOT NULL
);

-- Legal holds
CREATE TABLE IF NOT EXISTS legal_holds (
    id TEXT PRIMARY KEY,
    stream_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    held_by TEXT NOT NULL,
    held_at TEXT NOT NULL,
    released_at TEXT,
    released_by TEXT,
    FOREIGN KEY (stream_id) REFERENCES streams(id)
);

CREATE INDEX IF NOT EXISTS idx_legal_holds_stream ON legal_holds(stream_id);
CREATE INDEX IF NOT EXISTS idx_legal_holds_active ON legal_holds(released_at);

-- Insert initial schema version
INSERT OR IGNORE INTO schema_version (version, applied_at, description)
VALUES (1, datetime('now'), 'Initial schema');
