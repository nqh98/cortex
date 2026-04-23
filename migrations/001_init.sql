-- Cortex database schema

CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY,
    project_root TEXT NOT NULL,
    path TEXT NOT NULL,
    hash TEXT NOT NULL,
    language TEXT NOT NULL,
    last_indexed TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(project_root, path)
);

CREATE TABLE IF NOT EXISTS symbols (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    start_col INTEGER,
    end_col INTEGER,
    signature TEXT,
    documentation TEXT,
    embedding_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_files_path ON files(project_root, path);
CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind);
CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_id);
