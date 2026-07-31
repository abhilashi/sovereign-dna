//! Malformed-input hardening / fuzz corpus (Phase 1.8).
//!
//! Genome files are untrusted input — a user can hand us a truncated download,
//! a wrong-format file, binary garbage, or a deliberately adversarial record.
//! None of that may crash the app. These tests exercise every parser and the
//! format detector against:
//!
//! 1. a hand-curated **corpus** of nasty inputs (empty, header-only, truncated
//!    mid-record, absurd column counts, huge fields, unicode, injected control
//!    chars, malformed VCF/gVCF genotypes, invalid UTF-8, …);
//! 2. **truncations** of the known-good fixtures at every byte boundary; and
//! 3. **randomized** inputs from a genomics-flavored alphabet driven by a
//!    deterministic, dependency-free PRNG (so failures reproduce exactly).
//!
//! The contract asserted is simply: *no parser panics and every parse
//! terminates* — each parser must return `Ok`/`Err`, never abort. A regression
//! that reintroduces an index-underflow or `unwrap` panic will fail here.
//!
//! This is a pure `cargo test` (stable, no libfuzzer), so it runs in the same CI
//! as everything else; the corpus doubles as regression seeds.

#![cfg(test)]

use std::io::Cursor;

use super::{
    ancestry::AncestryParser, csvarray::CsvArrayParser, detect_format, gvcf::GvcfParser,
    twentythree::TwentyThreeParser, vcf::VcfParser, CountingSink, GenomeParser,
};

/// Run a byte slice through the format detector and **every** parser with a
/// counting sink. Panicking or hanging fails the test; Ok/Err are both fine.
fn exercise_all(bytes: &[u8]) {
    // Detection must tolerate arbitrary (incl. non-UTF-8) bytes.
    let _ = detect_format(&String::from_utf8_lossy(bytes));

    let parsers: [&dyn GenomeParser; 5] = [
        &TwentyThreeParser,
        &AncestryParser,
        &VcfParser,
        &GvcfParser,
        &CsvArrayParser,
    ];
    for p in parsers {
        let mut sink = CountingSink::default();
        let mut reader = Cursor::new(bytes);
        // Result intentionally discarded: we only require that this returns.
        let _ = p.parse_streaming(&mut reader, &mut sink);
    }
}

/// A hand-curated corpus of adversarial / malformed inputs.
fn corpus() -> Vec<Vec<u8>> {
    let mut c: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"\n\n\n".to_vec(),
        b"#".to_vec(),
        b"##fileformat=VCFv4.2".to_vec(), // header, no records, no newline
        b"##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS1\n"
            .to_vec(),
        // VCF record with too few columns.
        b"##fileformat=VCFv4.2\n1\t100\trs1\tA\n".to_vec(),
        // Non-numeric position.
        b"rs1\tX\tnotaposition\tAA\n".to_vec(),
        // VCF genotype that indexes past the ALT list.
        b"##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS1\n1\t1\t.\tA\tG\t.\t.\t.\tGT\t9/9\n".to_vec(),
        // gVCF malformed genotypes: "00" (index 0), empty, single-allele.
        b"##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS1\n1\t1\t.\tA\tG,<NON_REF>\t.\t.\t.\tGT\t00/00\n".to_vec(),
        b"##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS1\n1\t1\t.\tA\t<NON_REF>\t.\t.\t.\tGT\t/\n".to_vec(),
        b"1\t1\t.\tA\tG,T\t.\t.\t.\tGT\t2/1\n".to_vec(),
        // Empty REF/ALT.
        b"##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS1\n1\t1\t.\t\t\t.\t.\t.\tGT\t0/1\n".to_vec(),
        // Quoted-CSV with missing columns / stray quotes.
        b"RSID,CHROMOSOME,POSITION,RESULT\n\"rs1\",\"1\"\n\"\",\"\",\"\",\"\"\n".to_vec(),
        b"\"rs1\",\"1\",\"x\",\"AA\"\n".to_vec(),
        // Ancestry with a ragged number of allele columns.
        b"#AncestryDNA\nrs1\t1\t100\tA\nrs2\t1\t200\tA\tC\tEXTRA\n".to_vec(),
        // Very long single line (no newline) — must not blow up.
        {
            let mut v = b"rs1\t1\t100\t".to_vec();
            v.extend(std::iter::repeat(b'A').take(200_000));
            v
        },
        // Many columns.
        {
            let mut v = Vec::new();
            for _ in 0..5000 {
                v.extend_from_slice(b"x\t");
            }
            v.push(b'\n');
            v
        },
        // Control characters and unicode.
        "rs1\t1\t100\t\u{0}\u{7}\u{1b}[31mAA\n".as_bytes().to_vec(),
        "☣\t🧬\t42\tAG\n# reference build 38\n".as_bytes().to_vec(),
        // Invalid UTF-8 bytes (parsers read via read_line → must Err, not panic).
        vec![0xff, 0xfe, 0x00, 0x9a, b'\n', 0xc0, 0x80],
        // Windows line endings + BOM.
        b"\xEF\xBB\xBF# rsid\r\nrs1\t1\t100\tAA\r\n".to_vec(),
        // A gVCF ref block only.
        b"##fileformat=VCFv4.2\n##ALT=<ID=NON_REF>\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS1\n1\t1\t.\tA\t<NON_REF>\t.\t.\tEND=100\tGT\t0/0\n".to_vec(),
    ];
    // Truncate each known-good fixture at every byte boundary.
    for fixture in GOOD_FIXTURES {
        let bytes = fixture.as_bytes();
        for cut in 0..bytes.len() {
            c.push(bytes[..cut].to_vec());
        }
    }
    c
}

const GOOD_FIXTURES: &[&str] = &[
    "# rsid\tchromosome\tposition\tgenotype\nrs1\t1\t100\tAA\nrs2\tX\t200\tAG\n",
    "#AncestryDNA\nrsid\tchromosome\tposition\tallele1\tallele2\nrs1\t1\t100\tA\tC\n",
    "##fileformat=VCFv4.2\n##reference=GRCh38\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS1\tS2\n1\t100\trs1\tA\tG,T\t.\tPASS\t.\tGT\t0/1\t2/2\n",
    "##fileformat=VCFv4.2\n##ALT=<ID=NON_REF>\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS1\n1\t1\trs1\tC\tT,<NON_REF>\t.\tPASS\t.\tGT\t0/1\n",
    "RSID,CHROMOSOME,POSITION,RESULT\n\"rs1\",\"1\",\"100\",\"AA\"\n",
];

/// Minimal deterministic xorshift64* PRNG — no external deps, reproducible.
struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

#[test]
fn corpus_never_panics() {
    for input in corpus() {
        exercise_all(&input);
    }
}

#[test]
fn randomized_inputs_never_panic() {
    // A genomics-flavored alphabet that can accidentally form (broken) records:
    // separators, digits, bases, VCF genotype punctuation, quotes, symbolics.
    const ALPHABET: &[u8] = b"ACGTN0123\t ,|/.:<>*\"#\nrschrXYMT-\r=";
    let mut rng = Rng(0x5DEE_CE66_D2A6_1957); // fixed seed → reproducible
    for _ in 0..4000 {
        let len = rng.below(280);
        let mut buf = Vec::with_capacity(len);
        for _ in 0..len {
            buf.push(ALPHABET[rng.below(ALPHABET.len())]);
        }
        exercise_all(&buf);
    }
}

#[test]
fn structured_random_vcf_genotypes_never_panic() {
    // Target the VCF/gVCF genotype-decode paths specifically: valid record
    // skeleton, adversarial GT + ALT fields (indices out of range, "00", "./.",
    // symbolic-only, multiallelic).
    let alts = ["A", "G,T", "T,<NON_REF>", "<NON_REF>", "<*>", "A,G,C", "", "."];
    let gts = ["0/1", "9/9", "00/00", "./.", "1|2", "2/2", "", "/", "3", "0/0"];
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    for _ in 0..2000 {
        let alt = alts[rng.below(alts.len())];
        let gt = gts[rng.below(gts.len())];
        let rec = format!(
            "##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS1\n1\t{}\t.\tA\t{}\t.\tPASS\t.\tGT\t{}\n",
            rng.below(1_000_000) + 1,
            alt,
            gt
        );
        exercise_all(rec.as_bytes());

        // Same record decoded specifically by the gVCF parser (its own path).
        let mut sink = CountingSink::default();
        let mut reader = Cursor::new(rec.as_bytes());
        let _ = GvcfParser.parse_streaming(&mut reader, &mut sink);
    }
}
