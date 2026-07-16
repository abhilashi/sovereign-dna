-- Phase 3.2: fleet events the agent scheduler reacts to.
--
-- Event-based triggers (OnReferenceUpdate / OnNewMatchedArticle) fire when an
-- event newer than an agent's last run is recorded here. Events carry only
-- non-sensitive metadata (a source name + a timestamp) — no genome data.

CREATE TABLE IF NOT EXISTS agent_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL,        -- reference_updated | new_matched_article
    source TEXT,               -- e.g. 'clinvar' for reference_updated
    at TEXT NOT NULL           -- RFC3339
);

CREATE INDEX IF NOT EXISTS idx_agent_events_at ON agent_events(at);
CREATE INDEX IF NOT EXISTS idx_agent_events_kind ON agent_events(kind);
