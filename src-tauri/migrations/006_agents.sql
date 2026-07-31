-- Phase 3.1: user-created agents.
--
-- An agent is a saved, reusable analysis worker composed from a skill set, a
-- data scope, an LLM choice and persistent memory (its findings log). Agent
-- DEFINITIONS carry no genome data (only public rsIDs / skill ids / topics), so
-- they are stored genome-agnostically and are safe to export/share (Phase 3.9).
-- A run records which genome it executed against.

CREATE TABLE IF NOT EXISTS agent_definitions (
    id TEXT PRIMARY KEY,               -- reverse-DNS agent id
    version TEXT NOT NULL,
    name TEXT NOT NULL,
    definition_json TEXT NOT NULL,     -- full AgentDefinition JSON (no genome data)
    template_id TEXT,                  -- origin template, if any
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- One row per agent execution (run history).
CREATE TABLE IF NOT EXISTS agent_runs (
    run_id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    genome_id INTEGER,                 -- which genome this run analysed
    started_at TEXT NOT NULL,
    finished_at TEXT NOT NULL,
    status TEXT NOT NULL,              -- completed | partial | failed
    summary TEXT,                      -- optional LLM digest note
    log_json TEXT NOT NULL DEFAULT '[]',
    FOREIGN KEY (agent_id) REFERENCES agent_definitions(id) ON DELETE CASCADE,
    FOREIGN KEY (genome_id) REFERENCES genomes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_runs_agent ON agent_runs(agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_runs_started ON agent_runs(started_at);

-- The agent's persistent memory: its append-only findings log.
CREATE TABLE IF NOT EXISTS agent_findings (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    kind TEXT NOT NULL,                -- skill_result | research_article | note
    title TEXT NOT NULL,
    detail TEXT NOT NULL,
    rsids_json TEXT NOT NULL DEFAULT '[]',  -- public rsIDs referenced (safe)
    created_at TEXT NOT NULL,
    seen INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (agent_id) REFERENCES agent_definitions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_findings_agent ON agent_findings(agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_findings_run ON agent_findings(run_id);
CREATE INDEX IF NOT EXISTS idx_agent_findings_seen ON agent_findings(seen);
