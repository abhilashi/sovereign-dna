-- Research digest enhancements

-- Add seen_at column to track when user viewed articles
ALTER TABLE research_articles ADD COLUMN seen_at TEXT;

-- Track which genome matched which articles (and which rsIDs)
CREATE TABLE IF NOT EXISTS article_genome_matches (
    article_id TEXT NOT NULL,
    genome_id INTEGER NOT NULL,
    matched_rsids TEXT NOT NULL,  -- JSON array
    relevance_score REAL NOT NULL DEFAULT 0.0,
    PRIMARY KEY (article_id, genome_id),
    FOREIGN KEY (article_id) REFERENCES research_articles(id),
    FOREIGN KEY (genome_id) REFERENCES genomes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_article_genome_matches_genome ON article_genome_matches(genome_id);
CREATE INDEX IF NOT EXISTS idx_research_articles_fetched ON research_articles(fetched_date);
CREATE INDEX IF NOT EXISTS idx_research_articles_seen ON research_articles(seen_at);
