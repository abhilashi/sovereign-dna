-- Phase 1.9: import provenance + quality metadata.
--
-- Records where each genome came from and how clean the import was, so the app
-- can show honest provenance (source service / file type, reference build,
-- skipped-line counts) and warn when position-based annotation might misalign.
--
-- All columns are additive and nullable, so existing genome rows (imported
-- before this migration) keep working with NULLs.
ALTER TABLE genomes ADD COLUMN source_label TEXT;
ALTER TABLE genomes ADD COLUMN total_lines INTEGER;
ALTER TABLE genomes ADD COLUMN skipped_lines INTEGER;
