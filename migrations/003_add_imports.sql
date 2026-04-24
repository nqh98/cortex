CREATE TABLE IF NOT EXISTS imports (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    imported_symbol TEXT NOT NULL,
    imported_from_path TEXT,
    import_type TEXT NOT NULL,
    start_line INTEGER,
    raw_statement TEXT
);

CREATE INDEX IF NOT EXISTS idx_imports_file ON imports(file_id);
CREATE INDEX IF NOT EXISTS idx_imports_symbol ON imports(imported_symbol);
CREATE INDEX IF NOT EXISTS idx_imports_from_path ON imports(imported_from_path);
