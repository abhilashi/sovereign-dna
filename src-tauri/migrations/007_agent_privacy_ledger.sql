-- Phase 3.7: per-action privacy & consent ledger.
--
-- Legally load-bearing primitive: every agent action is authorised against
-- explicit, revocable consent and then recorded, so the user can see exactly
-- what left the device (which rsIDs, which endpoint). Only PUBLIC identifiers
-- (rsIDs / coordinates) are ever stored here — never genotypes.

CREATE TABLE IF NOT EXISTS agent_consents (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    scope_json TEXT NOT NULL,          -- ConsentScope JSON (actions, remote egress, rsid allowlist)
    granted_at TEXT NOT NULL,
    revoked_at TEXT,                   -- NULL while active; set on revoke (history preserved)
    note TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (agent_id) REFERENCES agent_definitions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_consents_agent ON agent_consents(agent_id);

-- Append-only audit log of every action an agent took.
CREATE TABLE IF NOT EXISTS agent_ledger (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    kind TEXT NOT NULL,                -- read_variants | query_pubmed | llm_local | llm_remote | ...
    rsids_json TEXT NOT NULL DEFAULT '[]',
    egress_json TEXT,                  -- NULL for on-device actions; Egress JSON otherwise (public ids only)
    outcome_json TEXT NOT NULL,        -- ActionOutcome JSON (allowed_local | allowed_by_consent | denied)
    description TEXT NOT NULL,
    FOREIGN KEY (agent_id) REFERENCES agent_definitions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_ledger_agent ON agent_ledger(agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_ledger_run ON agent_ledger(run_id);
CREATE INDEX IF NOT EXISTS idx_agent_ledger_ts ON agent_ledger(timestamp);
