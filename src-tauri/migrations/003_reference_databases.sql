CREATE TABLE IF NOT EXISTS reference_status (
    source TEXT PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'not_started',
    downloaded_at TEXT,
    parsed_at TEXT,
    record_count INTEGER DEFAULT 0,
    file_size_bytes INTEGER DEFAULT 0,
    error_message TEXT,
    version TEXT
);

CREATE TABLE IF NOT EXISTS gwas_associations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    rsid TEXT NOT NULL,
    trait_name TEXT NOT NULL,
    p_value REAL,
    odds_ratio REAL,
    risk_allele TEXT,
    study_accession TEXT,
    pubmed_id TEXT,
    sample_size INTEGER,
    source TEXT DEFAULT 'GWAS Catalog'
);
CREATE INDEX IF NOT EXISTS idx_gwas_rsid ON gwas_associations(rsid);
CREATE INDEX IF NOT EXISTS idx_gwas_trait ON gwas_associations(trait_name);

CREATE TABLE IF NOT EXISTS snpedia_entries (
    rsid TEXT NOT NULL,
    genotype TEXT NOT NULL,
    magnitude REAL,
    repute TEXT,
    summary TEXT,
    PRIMARY KEY (rsid, genotype)
);
CREATE INDEX IF NOT EXISTS idx_snpedia_rsid ON snpedia_entries(rsid);
