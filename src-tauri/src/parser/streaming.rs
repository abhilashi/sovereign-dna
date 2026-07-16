//! Streaming ingestion core.
//!
//! This module is the foundation of Phase 1: it replaces the previous
//! `std::fs::read_to_string` (whole-file-into-RAM) approach with a buffered,
//! chunked reader and a small set of helpers that let the [`GenomeParser`]
//! implementations consume arbitrarily large genome files in constant memory.
//!
//! Key pieces:
//! - [`open_file_reader`] — a large-buffer [`BufReader`] over a file.
//! - [`maybe_decompress`] — transparently inflate gzip/BGZF (`.gz`/`.bgz`)
//!   streams on the fly, so compressed genomes never have to be decompressed to
//!   disk or fully into RAM first.
//! - [`detect_and_wrap`] — decompress if needed, then sniff the file format from
//!   the first few KiB **without consuming the stream**, then hand back a reader
//!   that still yields the full, un-consumed (decompressed) input.
//! - [`parse_path_streaming`] — the end-to-end convenience path used by the
//!   import command: open → decompress → detect → dispatch → stream into a sink.

use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Read};
use std::path::Path;

use flate2::read::MultiGzDecoder;

use crate::error::AppError;

use super::{detect_format, parser_for_format, ParseSummary, SnpSink};

/// Gzip member magic bytes (`1f 8b`). Present at the start of both ordinary
/// gzip streams and every BGZF block (BGZF is a gzip profile: a series of
/// concatenated gzip members, each carrying a `BC` extra subfield).
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// Number of leading bytes sniffed for format detection. `detect_format` only
/// inspects the first 4 KiB; we peek a little more to be safe.
const DETECT_PEEK_BYTES: usize = 8 * 1024;

/// Buffered-reader capacity (256 KiB). Large enough to keep the syscall count
/// low when streaming multi-gigabyte whole-genome files, small enough to be a
/// negligible, *constant* memory cost regardless of file size.
pub const READER_CAPACITY: usize = 256 * 1024;

/// Read up to `buf.len()` bytes, looping over short reads until the buffer is
/// full or EOF is reached. A single `Read::read` call may return fewer bytes
/// than requested (especially for pipes/compressed streams), so we must loop.
/// Returns the number of bytes actually read.
fn read_up_to<R: Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break, // EOF
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// Open a file as a large-buffer [`BufReader`]. This is the buffered, chunked
/// reader that all streaming parsers ultimately read from.
pub fn open_file_reader(path: &Path) -> Result<BufReader<File>, AppError> {
    let file = File::open(path)
        .map_err(|e| AppError::Io(format!("Failed to open {}: {}", path.display(), e)))?;
    Ok(BufReader::with_capacity(READER_CAPACITY, file))
}

/// Peek up to `n` leading bytes from `reader` **without consuming them**: the
/// returned reader still yields the complete original stream, because the peeked
/// bytes are chained back in front of the (still-open) reader.
///
/// This is the shared primitive behind both compression sniffing and format
/// detection — in each case we need to look at a fixed-size prefix and then hand
/// back a reader positioned as if we had never looked.
fn peek_prefix<R: Read + 'static>(
    mut reader: R,
    n: usize,
) -> Result<(Vec<u8>, Box<dyn Read>), AppError> {
    let mut prefix = vec![0u8; n];
    let got = read_up_to(&mut reader, &mut prefix).map_err(|e| AppError::Io(e.to_string()))?;
    prefix.truncate(got);
    let chained: Box<dyn Read> = Box::new(Cursor::new(prefix.clone()).chain(reader));
    Ok((prefix, chained))
}

/// If the stream begins with the gzip magic bytes (`1f 8b`), wrap it in a
/// **streaming, multi-member** gzip decoder; otherwise return it unchanged.
///
/// [`MultiGzDecoder`] transparently handles both ordinary single-member gzip and
/// **BGZF** (`.bgz`, the block-gzip variant used by `bgzip`/`tabix` and inside
/// BAM). BGZF is simply a sequence of concatenated gzip members, which is exactly
/// what the multi-member decoder consumes — a plain [`GzDecoder`] would stop
/// after the first block. Decompression is fully streaming: members are inflated
/// on demand as the parser reads, so a multi-gigabyte `.vcf.gz` is never
/// decompressed to disk or held whole in RAM.
fn maybe_decompress<R: Read + 'static>(reader: R) -> Result<Box<dyn Read>, AppError> {
    let (magic, chained) = peek_prefix(reader, GZIP_MAGIC.len())?;
    if magic.len() >= 2 && magic[0] == GZIP_MAGIC[0] && magic[1] == GZIP_MAGIC[1] {
        Ok(Box::new(MultiGzDecoder::new(chained)))
    } else {
        Ok(chained)
    }
}

/// Transparently decompress (gzip/BGZF) if needed, then detect the genome-file
/// format from the first [`DETECT_PEEK_BYTES`] bytes of the **decompressed**
/// stream, and return the detected format string **together with a reader that
/// still yields the complete, un-consumed stream**.
///
/// This is the crux of streaming ingestion: we decide both *how to decompress*
/// and *which parser to use* after looking at only fixed-size prefixes — never
/// loading the whole (possibly compressed) file.
pub fn detect_and_wrap<R: Read + 'static>(
    reader: R,
) -> Result<(String, Box<dyn BufRead>), AppError> {
    // 1. Decompress on the fly if the outer stream is gzip/BGZF.
    let stream = maybe_decompress(reader)?;
    // 2. Sniff the (decompressed) header for the genome format, preserving it.
    let (header, chained) = peek_prefix(stream, DETECT_PEEK_BYTES)?;
    let format = detect_format(&String::from_utf8_lossy(&header));
    let buffered = BufReader::with_capacity(READER_CAPACITY, chained);
    Ok((format, Box::new(buffered)))
}

/// End-to-end streaming import: open `path`, detect its format, dispatch to the
/// matching [`GenomeParser`], and stream every SNP into `sink`.
///
/// Memory usage is bounded by the reader buffer plus whatever the `sink`
/// chooses to retain — the parser itself holds at most one line at a time.
pub fn parse_path_streaming(
    path: &Path,
    sink: &mut dyn SnpSink,
) -> Result<ParseSummary, AppError> {
    let file = File::open(path)
        .map_err(|e| AppError::Io(format!("Failed to open {}: {}", path.display(), e)))?;
    let (format, mut reader) = detect_and_wrap(file)?;

    if format == "unknown" {
        return Err(AppError::Parse(
            "Unable to detect file format. Supported formats: 23andMe, AncestryDNA, VCF."
                .to_string(),
        ));
    }

    let parser = parser_for_format(&format)
        .ok_or_else(|| AppError::Parse(format!("Unsupported file format: {}", format)))?;

    let mut summary = parser.parse_streaming(reader.as_mut(), sink)?;
    // Prefer the concrete detected identifier (e.g. "23andme_v5") over the
    // parser family name so downstream storage keeps the precise format.
    summary.format = format;
    Ok(summary)
}

/// Inspect a comment/meta line for a reference-build declaration.
///
/// Shared by all text parsers so build detection is consistent across formats.
/// Recognizes GRCh36/37/38 and their `hg`/`build NN` aliases.
pub(crate) fn detect_build_line(line: &str) -> Option<String> {
    if line.contains("build 38") || line.contains("GRCh38") || line.contains("hg38") {
        Some("GRCh38".to_string())
    } else if line.contains("build 37") || line.contains("GRCh37") || line.contains("hg19") {
        Some("GRCh37".to_string())
    } else if line.contains("build 36") || line.contains("GRCh36") || line.contains("hg18") {
        Some("GRCh36".to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{CountingSink, GenomeParser, VecSink};

    /// A `Read` that hands out at most `chunk` bytes per call, simulating a
    /// slow/segmented stream. Proves our parsers correctly reassemble lines
    /// across read boundaries instead of assuming one read == one line/file.
    struct ChunkReader {
        data: Vec<u8>,
        pos: usize,
        chunk: usize,
    }
    impl ChunkReader {
        fn new(data: &[u8], chunk: usize) -> Self {
            Self { data: data.to_vec(), pos: 0, chunk }
        }
    }
    impl Read for ChunkReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.pos >= self.data.len() {
                return Ok(0);
            }
            let end = (self.pos + self.chunk).min(self.data.len());
            let n = (end - self.pos).min(buf.len());
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    const SAMPLE_23ANDME: &str = "# This data has been generated by 23andMe\n\
# build 37\n\
# rsid\tchromosome\tposition\tgenotype\n\
rs4477212\t1\t82154\tAA\n\
rs3094315\t1\t752566\tAG\n\
rs3131972\t1\t752721\t--\n\
rs12124819\t1\t776546\tGG\n";

    #[test]
    fn detect_and_wrap_preserves_full_stream() {
        // The header is peeked for detection, but every byte (including the
        // header) must still be readable afterwards.
        let cursor = Cursor::new(SAMPLE_23ANDME.as_bytes().to_vec());
        let (format, mut reader) = detect_and_wrap(cursor).unwrap();
        assert_eq!(format, "23andme_v5");
        let mut all = String::new();
        reader.read_to_string(&mut all).unwrap();
        assert_eq!(all, SAMPLE_23ANDME);
    }

    #[test]
    fn streaming_survives_tiny_read_chunks() {
        // Feed the data one byte at a time; the buffered reader + read_line must
        // still reassemble whole records. This is the real "streaming" proof.
        let reader = ChunkReader::new(SAMPLE_23ANDME.as_bytes(), 1);
        let (format, mut buffered) = detect_and_wrap(reader).unwrap();
        assert_eq!(format, "23andme_v5");

        let mut sink = VecSink::default();
        let summary = crate::parser::twentythree::TwentyThreeParser
            .parse_streaming(buffered.as_mut(), &mut sink)
            .unwrap();

        // 4 data rows, 1 is a no-call ("--") → 3 valid SNPs.
        assert_eq!(sink.snps.len(), 3);
        assert_eq!(summary.snp_count, 3);
        assert_eq!(summary.build.as_deref(), Some("GRCh37"));
        assert_eq!(sink.snps[0].rsid, "rs4477212");
        assert_eq!(sink.snps[1].genotype, "AG");
    }

    #[test]
    fn counting_sink_does_not_retain() {
        let reader = ChunkReader::new(SAMPLE_23ANDME.as_bytes(), 3);
        let (_fmt, mut buffered) = detect_and_wrap(reader).unwrap();
        let mut sink = CountingSink::default();
        crate::parser::twentythree::TwentyThreeParser
            .parse_streaming(buffered.as_mut(), &mut sink)
            .unwrap();
        assert_eq!(sink.count, 3);
    }

    // ── Phase 1.2: transparent gzip/BGZF streaming decode ─────────────

    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    /// gzip-compress `data` into a single gzip member.
    fn gzip(data: &[u8]) -> Vec<u8> {
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(data).unwrap();
        enc.finish().unwrap()
    }

    #[test]
    fn detect_and_wrap_transparently_decodes_gzip() {
        // A `.vcf.gz`-style stream: detection and parsing must operate on the
        // decompressed bytes, transparently.
        let compressed = gzip(SAMPLE_23ANDME.as_bytes());
        // Sanity: the compressed bytes start with the gzip magic and are NOT the
        // plaintext (so we're really exercising the decompressor).
        assert_eq!(&compressed[..2], &[0x1f, 0x8b]);

        let (format, mut reader) = detect_and_wrap(Cursor::new(compressed)).unwrap();
        assert_eq!(format, "23andme_v5");
        let mut all = String::new();
        reader.read_to_string(&mut all).unwrap();
        assert_eq!(all, SAMPLE_23ANDME);
    }

    #[test]
    fn gzip_stream_survives_tiny_read_chunks() {
        // Feed the *compressed* bytes one at a time: proves decompression is
        // streaming and reassembles inflate output across read boundaries — a
        // whole-file decompress-first approach would be impossible here.
        let compressed = gzip(SAMPLE_23ANDME.as_bytes());
        let reader = ChunkReader::new(&compressed, 1);
        let (format, mut buffered) = detect_and_wrap(reader).unwrap();
        assert_eq!(format, "23andme_v5");

        let mut sink = VecSink::default();
        let summary = crate::parser::twentythree::TwentyThreeParser
            .parse_streaming(buffered.as_mut(), &mut sink)
            .unwrap();
        assert_eq!(summary.snp_count, 3);
        assert_eq!(summary.build.as_deref(), Some("GRCh37"));
        assert_eq!(sink.snps[0].rsid, "rs4477212");
    }

    #[test]
    fn multi_member_gzip_decodes_all_blocks_like_bgzf() {
        // BGZF is a series of concatenated gzip members. A plain single-member
        // decoder would stop after the first block and silently truncate the
        // genome; MultiGzDecoder must read every member. Simulate BGZF by
        // concatenating independent gzip members split mid-file.
        let text = SAMPLE_23ANDME.as_bytes();
        let split = text.len() / 2;
        let mut bgzf_like = gzip(&text[..split]);
        bgzf_like.extend_from_slice(&gzip(&text[split..]));

        let (format, mut reader) = detect_and_wrap(Cursor::new(bgzf_like)).unwrap();
        assert_eq!(format, "23andme_v5");
        let mut all = String::new();
        reader.read_to_string(&mut all).unwrap();
        // The full original content must be recovered across *both* members.
        assert_eq!(all, SAMPLE_23ANDME);
    }

    #[test]
    fn uncompressed_stream_still_detected_after_1_2() {
        // Regression: plain (non-gzip) input must be untouched by the new
        // decompression sniffing.
        let (format, mut reader) =
            detect_and_wrap(Cursor::new(SAMPLE_23ANDME.as_bytes().to_vec())).unwrap();
        assert_eq!(format, "23andme_v5");
        let mut all = String::new();
        reader.read_to_string(&mut all).unwrap();
        assert_eq!(all, SAMPLE_23ANDME);
    }
}

