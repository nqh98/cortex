-- Drop old triggers and FTS table if they exist (for upgrade path)
DROP TRIGGER IF EXISTS symbols_fts_ai;
DROP TRIGGER IF EXISTS symbols_fts_ad;
DROP TRIGGER IF EXISTS symbols_fts_au;
DROP TABLE IF EXISTS symbol_search;

-- FTS5 virtual table for full-text search on tokenized symbol content
-- name_tokens: camelCase/snake_case split identifier (e.g. "RfqBuyerService" → "rfq buyer service")
-- file_path_tokens: split file path segments for discovery (e.g. "rfq-event-publisher.service.ts" → "rfq event publisher service ts")
CREATE VIRTUAL TABLE IF NOT EXISTS symbol_search USING fts5(
    name_tokens,
    signature,
    documentation,
    file_path_tokens,
    content='symbols',
    content_rowid='id'
);

-- Triggers to keep FTS index in sync with symbols table
CREATE TRIGGER IF NOT EXISTS symbols_fts_ai AFTER INSERT ON symbols BEGIN
    INSERT INTO symbol_search(rowid, name_tokens, signature, documentation, file_path_tokens)
    VALUES (new.id, new.name_tokens, new.signature, new.documentation,
        (SELECT REPLACE(REPLACE(REPLACE(f.path, '/', ' '), '.', ' '), '-', ' ') FROM files f WHERE f.id = new.file_id));
END;

CREATE TRIGGER IF NOT EXISTS symbols_fts_ad AFTER DELETE ON symbols BEGIN
    INSERT INTO symbol_search(symbol_search, rowid, name_tokens, signature, documentation, file_path_tokens)
    VALUES ('delete', old.id, old.name_tokens, old.signature, old.documentation, '');
END;

CREATE TRIGGER IF NOT EXISTS symbols_fts_au AFTER UPDATE ON symbols BEGIN
    INSERT INTO symbol_search(symbol_search, rowid, name_tokens, signature, documentation, file_path_tokens)
    VALUES ('delete', old.id, old.name_tokens, old.signature, old.documentation, '');
    INSERT INTO symbol_search(rowid, name_tokens, signature, documentation, file_path_tokens)
    VALUES (new.id, new.name_tokens, new.signature, new.documentation,
        (SELECT REPLACE(REPLACE(REPLACE(f.path, '/', ' '), '.', ' '), '-', ' ') FROM files f WHERE f.id = new.file_id));
END;
