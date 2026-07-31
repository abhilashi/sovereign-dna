//! GRCh37 ⇄ GRCh38 coordinate liftover (Phase 1.6).
//!
//! Imported genomes come in different reference builds — 23andMe/AncestryDNA
//! arrays are almost all GRCh37 (build 37 / hg19), while modern WGS VCFs are
//! usually GRCh38. Our reference annotations (ClinVar, GWAS Catalog, …) are keyed
//! by `(chromosome, position)` on a *specific* build, so a variant imported on
//! the wrong build silently annotates against the wrong locus. This module lifts
//! variant coordinates from one build to another using UCSC **chain files**, the
//! same inputs the standard `liftOver` tool uses.
//!
//! Design:
//! - [`LiftOver::from_chain`] parses a UCSC `.chain` file into per-chromosome
//!   aligned blocks (0-based, half-open, strand-aware).
//! - [`LiftOver::lift`] maps a single 1-based variant coordinate to the target
//!   build, returning `None` when the position falls in a chain gap (a region
//!   with no counterpart in the target assembly — e.g. a deletion between builds).
//! - [`LiftoverSink`] wraps any [`SnpSink`] and transparently remaps every
//!   variant's `(chromosome, position)` (reverse-complementing the alleles when
//!   the chain flips strand), dropping and counting positions that don't lift.
//!
//! The multi-megabyte chain files themselves (`hg19ToHg38.over.chain`,
//! `hg38ToHg19.over.chain`) are downloaded/provided alongside the other
//! reference data, exactly like ClinVar — this module is the pure, testable
//! coordinate-mapping core.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use flate2::read::MultiGzDecoder;

use crate::error::AppError;

use super::{ParsedSnp, SnpSink};

/// One ungapped aligned block from a chain: a contiguous range of the *source*
/// (target, `t`) assembly that maps linearly to a range of the destination
/// (query, `q`) assembly.
#[derive(Debug, Clone)]
struct Block {
    /// 0-based, half-open source range `[t_start, t_end)`.
    t_start: u64,
    t_end: u64,
    /// Destination chromosome (normalized: no `chr` prefix, upper-cased).
    q_chrom: String,
    /// 0-based destination start, in the chain's (possibly reverse) strand
    /// coordinate system.
    q_start: u64,
    /// Destination strand (`+` or `-`).
    q_strand: char,
    /// Total size of the destination chromosome (needed to convert a
    /// reverse-strand coordinate back to the forward strand).
    q_size: u64,
}

/// A parsed chain-file liftover mapping between two reference builds.
pub struct LiftOver {
    /// Source chromosome (normalized) → aligned blocks, sorted by `t_start`.
    chains: HashMap<String, Vec<Block>>,
    pub from_build: String,
    pub to_build: String,
}

/// Normalize a chromosome name for keying: drop a leading `chr` and upper-case.
fn norm_chrom(chrom: &str) -> String {
    chrom.strip_prefix("chr").unwrap_or(chrom).to_uppercase()
}

/// Complement a single DNA base (IUPAC A/C/G/T/N), preserving case-insensitively
/// as upper-case. Non-bases are returned unchanged.
fn complement_base(b: char) -> char {
    match b.to_ascii_uppercase() {
        'A' => 'T',
        'T' => 'A',
        'C' => 'G',
        'G' => 'C',
        other => other,
    }
}

/// Reverse-complement a nucleotide string (for alleles carried across a strand
/// flip). For a single-base SNP allele this is just the complement.
fn reverse_complement(seq: &str) -> String {
    seq.chars().rev().map(complement_base).collect()
}

impl LiftOver {
    /// Parse a UCSC chain file from a reader.
    ///
    /// Chain format (whitespace-separated):
    /// ```text
    /// chain score tName tSize tStrand tStart tEnd qName qSize qStrand qStart qEnd id
    /// size dt dq          <- alignment data lines (ungapped block + gaps to next)
    /// size dt dq
    /// size                <- final block (no trailing gaps)
    /// <blank line>
    /// ```
    /// The `t*` (target) fields are the *source* build (what we lift *from*); the
    /// `q*` (query) fields are the destination build. Target strand is always `+`
    /// in UCSC chains; query strand may be `-`.
    pub fn from_chain<R: Read>(
        reader: R,
        from_build: impl Into<String>,
        to_build: impl Into<String>,
    ) -> Result<Self, AppError> {
        let mut chains: HashMap<String, Vec<Block>> = HashMap::new();
        let buf = BufReader::new(reader);

        // Per-chain cursor state while walking the data lines.
        let mut cur_tname = String::new();
        let mut cur_qname = String::new();
        let mut cur_qstrand = '+';
        let mut cur_qsize = 0u64;
        let mut t = 0u64; // running target position
        let mut q = 0u64; // running query position (strand coordinates)
        let mut in_chain = false;

        for line in buf.lines() {
            let line = line.map_err(|e| AppError::Io(e.to_string()))?;
            let line = line.trim();
            if line.is_empty() {
                in_chain = false;
                continue;
            }
            if let Some(rest) = line.strip_prefix("chain") {
                let f: Vec<&str> = rest.split_whitespace().collect();
                // rest = score tName tSize tStrand tStart tEnd qName qSize qStrand qStart qEnd [id]
                if f.len() < 11 {
                    return Err(AppError::Parse(format!(
                        "Malformed chain header: {} fields",
                        f.len()
                    )));
                }
                cur_tname = norm_chrom(f[1]);
                cur_qname = norm_chrom(f[6]);
                cur_qsize = f[7].parse().map_err(|_| {
                    AppError::Parse("Invalid qSize in chain header".to_string())
                })?;
                cur_qstrand = f[8].chars().next().unwrap_or('+');
                t = f[4]
                    .parse()
                    .map_err(|_| AppError::Parse("Invalid tStart".to_string()))?;
                q = f[9]
                    .parse()
                    .map_err(|_| AppError::Parse("Invalid qStart".to_string()))?;
                in_chain = true;
                continue;
            }
            if !in_chain {
                continue; // stray line outside a chain block
            }

            let nums: Vec<&str> = line.split_whitespace().collect();
            let size: u64 = nums[0]
                .parse()
                .map_err(|_| AppError::Parse("Invalid block size in chain".to_string()))?;

            // Ungapped aligned block: [t, t+size) -> [q, q+size).
            // Saturating arithmetic keeps a malformed chain from panicking on
            // overflow (the coordinates simply become nonsensical, not a crash).
            if size > 0 {
                chains.entry(cur_tname.clone()).or_default().push(Block {
                    t_start: t,
                    t_end: t.saturating_add(size),
                    q_chrom: cur_qname.clone(),
                    q_start: q,
                    q_strand: cur_qstrand,
                    q_size: cur_qsize,
                });
            }

            if nums.len() >= 3 {
                let dt: u64 = nums[1].parse().unwrap_or(0);
                let dq: u64 = nums[2].parse().unwrap_or(0);
                t = t.saturating_add(size).saturating_add(dt);
                q = q.saturating_add(size).saturating_add(dq);
            } else {
                // Final block of the chain (only `size`).
                in_chain = false;
            }
        }

        for blocks in chains.values_mut() {
            blocks.sort_by_key(|b| b.t_start);
        }

        Ok(LiftOver {
            chains,
            from_build: from_build.into(),
            to_build: to_build.into(),
        })
    }

    /// Lift a **1-based** variant coordinate from the source build to the target
    /// build. Returns `(chromosome, position_1based, strand)` or `None` if the
    /// position has no counterpart in the target assembly (falls in a chain gap
    /// or an unmapped chromosome). `strand` is `'-'` when the block flips strand,
    /// signalling the caller to reverse-complement alleles.
    pub fn lift(&self, chrom: &str, position_1based: i64) -> Option<(String, i64, char)> {
        if position_1based < 1 {
            return None;
        }
        let pos0 = (position_1based - 1) as u64; // to 0-based
        let blocks = self.chains.get(&norm_chrom(chrom))?;

        // Binary search for the block whose [t_start, t_end) contains pos0.
        let idx = blocks.partition_point(|b| b.t_end <= pos0);
        let block = blocks.get(idx)?;
        if pos0 < block.t_start || pos0 >= block.t_end {
            return None; // in a gap between blocks
        }

        let offset = pos0 - block.t_start;
        // Use checked arithmetic so a malformed chain (inconsistent q_size /
        // q_start) can never panic on unsigned overflow — it just fails to lift.
        let q0 = match block.q_strand {
            '-' => {
                // Reverse-strand coordinate → forward-strand position.
                block
                    .q_size
                    .checked_sub(1)?
                    .checked_sub(block.q_start.checked_add(offset)?)?
            }
            _ => block.q_start.checked_add(offset)?,
        };
        // Back to 1-based.
        Some((block.q_chrom.clone(), q0 as i64 + 1, block.q_strand))
    }

    /// Load a chain file from disk, transparently decompressing a gzipped
    /// (`.chain.gz`) file — UCSC distributes the chains gzipped. Detection is by
    /// the gzip magic bytes, so a plain `.chain` works too.
    pub fn from_chain_file(
        path: &Path,
        from_build: impl Into<String>,
        to_build: impl Into<String>,
    ) -> Result<Self, AppError> {
        let file = File::open(path).map_err(|e| {
            AppError::Io(format!("Failed to open chain file {}: {}", path.display(), e))
        })?;
        let mut reader = BufReader::new(file);
        // Peek the magic to decide whether to inflate.
        let magic = reader
            .fill_buf()
            .map_err(|e| AppError::Io(e.to_string()))?;
        let is_gzip = magic.len() >= 2 && magic[0] == 0x1f && magic[1] == 0x8b;
        if is_gzip {
            Self::from_chain(MultiGzDecoder::new(reader), from_build, to_build)
        } else {
            Self::from_chain(reader, from_build, to_build)
        }
    }

    /// Number of source chromosomes with at least one aligned block.
    pub fn mapped_chromosomes(&self) -> usize {
        self.chains.len()
    }
}

/// A [`SnpSink`] adapter that lifts every variant's coordinates to a target
/// build before forwarding it to an inner sink. Positions that do not map are
/// dropped and counted in [`LiftoverSink::unmapped`]; forwarded variants are
/// counted in [`LiftoverSink::lifted`]. On a strand flip the `ref`/`alt` alleles
/// and genotype bases are reverse-complemented so they stay correct.
pub struct LiftoverSink<'a> {
    lift: &'a LiftOver,
    inner: &'a mut dyn SnpSink,
    pub lifted: usize,
    pub unmapped: usize,
}

impl<'a> LiftoverSink<'a> {
    pub fn new(lift: &'a LiftOver, inner: &'a mut dyn SnpSink) -> Self {
        Self {
            lift,
            inner,
            lifted: 0,
            unmapped: 0,
        }
    }
}

impl SnpSink for LiftoverSink<'_> {
    fn push(&mut self, mut snp: ParsedSnp) -> Result<(), AppError> {
        match self.lift.lift(&snp.chromosome, snp.position) {
            Some((chrom, pos, strand)) => {
                snp.chromosome = chrom;
                snp.position = pos;
                if strand == '-' {
                    // Alleles are on the opposite strand in the new build.
                    if let Some(r) = snp.ref_allele.take() {
                        snp.ref_allele = Some(reverse_complement(&r));
                    }
                    if let Some(a) = snp.alt_allele.take() {
                        // Multi-allelic ALT lists are comma-separated.
                        snp.alt_allele =
                            Some(a.split(',').map(reverse_complement).collect::<Vec<_>>().join(","));
                    }
                    // Genotype letters are single-base allele calls at this locus.
                    snp.genotype = snp.genotype.chars().map(complement_base).collect();
                }
                self.inner.push(snp)?;
                self.lifted += 1;
            }
            None => {
                self.unmapped += 1;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::VecSink;

    // A tiny synthetic chain: chr1 source [1000,1100) maps to chr1 dest starting
    // at 5000 (forward), then after a 10bp target gap / 5bp query gap another
    // aligned block [1110,1150) continues. Forward strand.
    const CHAIN_FWD: &str = "chain 1000 chr1 249000000 + 1000 1150 chr1 248000000 + 5000 5145 1\n\
100 10 5\n\
40\n\
\n";

    #[test]
    fn parses_and_lifts_forward_chain() {
        let lo = LiftOver::from_chain(CHAIN_FWD.as_bytes(), "GRCh37", "GRCh38").unwrap();
        assert_eq!(lo.from_build, "GRCh37");
        assert_eq!(lo.mapped_chromosomes(), 1);

        // First block: target [1000,1100) -> query [5000,5100). Target 1-based
        // 1001 (0-based 1000) -> query 0-based 5000 -> 1-based 5001.
        assert_eq!(lo.lift("1", 1001), Some(("1".to_string(), 5001, '+')));
        // Offset 50 within the block.
        assert_eq!(lo.lift("chr1", 1051), Some(("1".to_string(), 5051, '+')));

        // A position in the gap (target 1100..1110) does not map.
        assert_eq!(lo.lift("1", 1105), None);

        // Second block starts at target 0-based 1110 (1-based 1111) -> query
        // 0-based 5105 (1-based 5106).
        assert_eq!(lo.lift("1", 1111), Some(("1".to_string(), 5106, '+')));

        // Off the end of every block -> None. Unknown chromosome -> None.
        assert_eq!(lo.lift("1", 999999), None);
        assert_eq!(lo.lift("7", 1001), None);
    }

    // Reverse-strand chain: target chr2 [0,10) maps to query chr2 on the '-'
    // strand. q_size=100, q_start=20. Forward query pos = 100-1-(20+offset).
    const CHAIN_REV: &str = "chain 500 chr2 200 + 0 10 chr2 100 - 20 30 1\n\
10\n\
\n";

    #[test]
    fn reverse_strand_lift_and_revcomp() {
        let lo = LiftOver::from_chain(CHAIN_REV.as_bytes(), "GRCh37", "GRCh38").unwrap();
        // Target 0-based 0 (1-based 1): forward query = 100-1-(20+0)=79 -> 1-based 80.
        assert_eq!(lo.lift("2", 1), Some(("2".to_string(), 80, '-')));
        // Target 0-based 5 (1-based 6): forward query = 100-1-(20+5)=74 -> 1-based 75.
        assert_eq!(lo.lift("2", 6), Some(("2".to_string(), 75, '-')));
    }

    #[test]
    fn liftover_sink_remaps_and_counts() {
        let lo = LiftOver::from_chain(CHAIN_FWD.as_bytes(), "GRCh37", "GRCh38").unwrap();
        let mut collected = VecSink::default();
        let mut sink = LiftoverSink::new(&lo, &mut collected);

        // Two mappable variants + one in the gap (dropped).
        sink.push(ParsedSnp {
            rsid: "rs1".into(),
            chromosome: "1".into(),
            position: 1001,
            genotype: "AG".into(),
            ref_allele: None,
            alt_allele: None,
            sample: None,
        })
        .unwrap();
        sink.push(ParsedSnp {
            rsid: "rs2".into(),
            chromosome: "1".into(),
            position: 1105, // gap
            genotype: "CC".into(),
            ref_allele: None,
            alt_allele: None,
            sample: None,
        })
        .unwrap();

        assert_eq!(sink.lifted, 1);
        assert_eq!(sink.unmapped, 1);
        assert_eq!(collected.snps.len(), 1);
        assert_eq!(collected.snps[0].position, 5001);
        assert_eq!(collected.snps[0].chromosome, "1");
    }

    #[test]
    fn from_chain_file_reads_plain_and_gzip() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let dir = std::env::temp_dir();
        let pid = std::process::id();

        // Plain .chain file.
        let plain = dir.join(format!("sdna_lift_{pid}.chain"));
        std::fs::write(&plain, CHAIN_FWD).unwrap();
        let lo = LiftOver::from_chain_file(&plain, "GRCh37", "GRCh38").unwrap();
        assert_eq!(lo.lift("1", 1001), Some(("1".to_string(), 5001, '+')));
        let _ = std::fs::remove_file(&plain);

        // Gzipped .chain.gz file.
        let gz = dir.join(format!("sdna_lift_{pid}.chain.gz"));
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(CHAIN_FWD.as_bytes()).unwrap();
        std::fs::write(&gz, enc.finish().unwrap()).unwrap();
        let lo2 = LiftOver::from_chain_file(&gz, "GRCh37", "GRCh38").unwrap();
        assert_eq!(lo2.lift("1", 1051), Some(("1".to_string(), 5051, '+')));
        let _ = std::fs::remove_file(&gz);
    }

    #[test]
    fn liftover_sink_reverse_complements_alleles_on_strand_flip() {
        let lo = LiftOver::from_chain(CHAIN_REV.as_bytes(), "GRCh37", "GRCh38").unwrap();
        let mut collected = VecSink::default();
        let mut sink = LiftoverSink::new(&lo, &mut collected);

        sink.push(ParsedSnp {
            rsid: "rsX".into(),
            chromosome: "2".into(),
            position: 1,
            genotype: "AG".into(),
            ref_allele: Some("A".into()),
            alt_allele: Some("G".into()),
            sample: Some("S1".into()),
        })
        .unwrap();

        assert_eq!(sink.lifted, 1);
        let s = &collected.snps[0];
        assert_eq!(s.position, 80);
        // Strand flip -> alleles complemented: A->T, G->C.
        assert_eq!(s.ref_allele.as_deref(), Some("T"));
        assert_eq!(s.alt_allele.as_deref(), Some("C"));
        assert_eq!(s.genotype, "TC");
    }
}
