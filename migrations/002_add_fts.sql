-- FTS5 virtual table for full-text search on symbol content
CREATE VIRTUAL TABLE IF NOT EXISTS symbol_search USING fts5(
    name,
    signature,
    documentation,
    content='symbols',
    content_rowid='id'
);

-- Triggers to keep FTS index in sync with symbols table
CREATE TRIGGER IF NOT EXISTS symbols_fts_ai AFTER INSERT ON symbols BEGIN
    INSERT INTO symbol_search(rowid, name, signature, documentation)
    VALUES (new.id, new.name, new.signature, new.documentation);
END;

CREATE TRIGGER IF NOT EXISTS symbols_fts_ad AFTER DELETE ON symbols BEGIN
    INSERT INTO symbol_search(symbol_search, rowid, name, signature, documentation)
    VALUES ('delete', old.id, old.name, old.signature, old.documentation);
END;

CREATE TRIGGER IF NOT EXISTS symbols_fts_au AFTER UPDATE ON symbols BEGIN
    INSERT INTO symbol_search(symbol_search, rowid, name, signature, documentation)
    VALUES ('delete', old.id, old.name, old.signature, old.documentation);
    INSERT INTO symbol_search(rowid, name, signature, documentation)
    VALUES (new.id, new.name, new.signature, new.documentation);
END;
