-- FTS5 full-text search index for keyword matching
-- Replaces naive substring matching with proper BM25-scored word-boundary search
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    memory_id UNINDEXED,
    summary,
    tokenize = 'porter unicode61'
);

-- Populate from existing data
INSERT INTO memories_fts (memory_id, summary)
SELECT id, summary FROM memories WHERE summary IS NOT NULL;
