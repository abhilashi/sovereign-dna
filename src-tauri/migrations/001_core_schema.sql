-- Core schema for Genome Studio
-- Imported genomes
CREATE TABLE IF NOT EXISTS genomes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    filename TEXT NOT NULL,
    format TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    snp_count INTEGER NOT NULL,
    build TEXT
);

-- User's SNP data (~600K rows)
CREATE TABLE IF NOT EXISTS snps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    genome_id INTEGER NOT NULL REFERENCES genomes(id) ON DELETE CASCADE,
    rsid TEXT NOT NULL,
    chromosome TEXT NOT NULL,
    position INTEGER NOT NULL,
    genotype TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_snps_rsid ON snps(rsid);
CREATE INDEX IF NOT EXISTS idx_snps_chr_pos ON snps(chromosome, position);
CREATE INDEX IF NOT EXISTS idx_snps_genome_id ON snps(genome_id);
