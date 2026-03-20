CREATE TABLE IF NOT EXISTS workbench_sessions (
    id TEXT PRIMARY KEY,
    genome_id INTEGER NOT NULL,
    query TEXT NOT NULL,
    strategy TEXT NOT NULL,
    result_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (genome_id) REFERENCES genomes(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_workbench_genome ON workbench_sessions(genome_id);

CREATE TABLE IF NOT EXISTS workbench_chats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES workbench_sessions(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_workbench_chats_session ON workbench_chats(session_id);
