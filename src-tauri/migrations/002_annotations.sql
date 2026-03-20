-- Annotation and research cache tables

-- Annotation cache (ClinVar, PharmGKB)
CREATE TABLE IF NOT EXISTS annotations (
    rsid TEXT PRIMARY KEY,
    gene TEXT,
    clinical_significance TEXT,
    condition TEXT,
    review_status TEXT,
    allele_frequency REAL,
    source TEXT,
    last_updated TEXT
);

-- Research article cache
CREATE TABLE IF NOT EXISTS research_articles (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    abstract_text TEXT,
    source TEXT NOT NULL,
    published_date TEXT,
    relevant_rsids TEXT,
    fetched_date TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_annotations_gene ON annotations(gene);
CREATE INDEX IF NOT EXISTS idx_annotations_significance ON annotations(clinical_significance);
CREATE INDEX IF NOT EXISTS idx_research_source ON research_articles(source);
