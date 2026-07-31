-- Phase 1.3: multi-sample VCF support + (chr,pos,ref,alt) variant keying.
--
-- Genotyping-array formats (23andMe, AncestryDNA) are fully described by rsID +
-- genotype, so these columns are nullable and stay NULL for those imports.
--
-- For VCF, the true variant identity is the locus tuple (chromosome, position,
-- ref_allele, alt_allele) — an rsID is optional and absent for novel variants.
-- `sample` records which sample column a genotype came from so a multi-sample
-- VCF is no longer collapsed to just the first individual.
ALTER TABLE snps ADD COLUMN ref_allele TEXT;
ALTER TABLE snps ADD COLUMN alt_allele TEXT;
ALTER TABLE snps ADD COLUMN sample TEXT;

-- Index the full (chr,pos,ref,alt) variant key so co-located multiallelic /
-- indel variants can be distinguished and looked up efficiently.
CREATE INDEX IF NOT EXISTS idx_snps_variant_key
    ON snps(chromosome, position, ref_allele, alt_allele);

-- Index per-sample lookups within a genome (multi-sample VCF).
CREATE INDEX IF NOT EXISTS idx_snps_sample ON snps(genome_id, sample);
