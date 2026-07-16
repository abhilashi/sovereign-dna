use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::db::queries::{self, SnpRow};
use crate::db::Database;
use crate::error::AppError;
use crate::parser::streaming;
use crate::parser::{ParsedSnp, SnpSink};

/// Number of SNPs buffered before a batch is flushed to SQLite. Bounds the
/// working-set memory during import to `BATCH_SIZE` rows regardless of file
/// size — the key property that lets whole-genome VCFs import without OOM.
const BATCH_SIZE: usize = 50_000;

/// A [`SnpSink`] that streams parsed SNPs into SQLite in bounded batches.
///
/// The parser hands SNPs here one at a time; we buffer up to [`BATCH_SIZE`] and
/// flush each batch inside a transaction, so neither the raw file nor the full
/// SNP set is ever held in memory at once.
struct DbBatchSink<'a> {
    conn: &'a rusqlite::Connection,
    genome_id: i64,
    buffer: Vec<SnpRow>,
    inserted: usize,
    channel: &'a Channel<ImportProgress>,
}

impl<'a> DbBatchSink<'a> {
    fn new(
        conn: &'a rusqlite::Connection,
        genome_id: i64,
        channel: &'a Channel<ImportProgress>,
    ) -> Self {
        Self {
            conn,
            genome_id,
            buffer: Vec::with_capacity(BATCH_SIZE),
            inserted: 0,
            channel,
        }
    }

    fn flush(&mut self) -> Result<(), AppError> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        queries::insert_snps_batch(self.conn, self.genome_id, &self.buffer)?;
        self.inserted += self.buffer.len();
        self.buffer.clear();
        let _ = self.channel.send(ImportProgress {
            phase: "storing".to_string(),
            // Total is unknown while streaming; report a cumulative count.
            progress: 0.6,
            message: format!("Stored {} SNPs...", self.inserted),
        });
        Ok(())
    }
}

impl<'a> SnpSink for DbBatchSink<'a> {
    fn push(&mut self, snp: ParsedSnp) -> Result<(), AppError> {
        self.buffer.push(SnpRow {
            id: None,
            genome_id: self.genome_id,
            rsid: snp.rsid,
            chromosome: snp.chromosome,
            position: snp.position,
            genotype: snp.genotype,
            ref_allele: snp.ref_allele,
            alt_allele: snp.alt_allele,
            sample: snp.sample,
        });
        if self.buffer.len() >= BATCH_SIZE {
            self.flush()?;
        }
        Ok(())
    }
}

/// Progress update sent to the frontend during import.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub phase: String,
    pub progress: f64,
    pub message: String,
}

/// Result of a genome import operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub genome_id: i64,
    pub snp_count: usize,
    pub format: String,
    pub build: Option<String>,
    pub quality_summary: QualitySummary,
}

/// Quality metrics for the imported genome data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualitySummary {
    pub total_lines: usize,
    pub skipped_lines: usize,
    pub valid_snps: usize,
    pub skip_rate: f64,
}

#[tauri::command]
pub async fn import_genome(
    file_path: String,
    db: State<'_, Database>,
    channel: Channel<ImportProgress>,
) -> Result<ImportResult, AppError> {
    // Phase 1: Open + detect format from a peeked header (no full-file read).
    let _ = channel.send(ImportProgress {
        phase: "reading".to_string(),
        progress: 0.0,
        message: "Opening file...".to_string(),
    });

    let path = std::path::PathBuf::from(&file_path);
    let file = std::fs::File::open(&path)
        .map_err(|e| AppError::Io(format!("Failed to open file {}: {}", file_path, e)))?;
    let (format, mut reader) = streaming::detect_and_wrap(file)?;

    if format == "unknown" {
        return Err(AppError::Parse(
            "Unable to detect file format. Supported formats: 23andMe, AncestryDNA, MyHeritage, FamilyTreeDNA, VCF."
                .to_string(),
        ));
    }

    let parser = crate::parser::parser_for_format(&format)
        .ok_or_else(|| AppError::Parse(format!("Unsupported file format: {}", format)))?;

    let _ = channel.send(ImportProgress {
        phase: "parsing".to_string(),
        progress: 0.3,
        message: format!("Detected format: {}. Streaming...", format),
    });

    // Filename for the genome record.
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let conn = db
        .0
        .lock()
        .map_err(|e| AppError::Database(format!("Failed to acquire database lock: {}", e)))?;

    // Insert the genome row up front with placeholder stats; SNPs stream in
    // referencing this id, then we finalize the counts/build below.
    let genome_id = queries::insert_genome(&conn, &filename, &format, 0, None)?;

    // Phase 2+3: stream-parse straight into the DB in bounded batches.
    let mut sink = DbBatchSink::new(&conn, genome_id, &channel);
    let summary = match parser.parse_streaming(reader.as_mut(), &mut sink) {
        Ok(s) => s,
        Err(e) => {
            // Roll back the partially-imported genome (snps cascade on delete).
            let _ = queries::delete_genome(&conn, genome_id);
            return Err(e);
        }
    };
    // Flush any residual SNPs left in the final partial batch.
    if let Err(e) = sink.flush() {
        let _ = queries::delete_genome(&conn, genome_id);
        return Err(e);
    }

    let snp_count = summary.snp_count;

    // Finalize the genome row with the real count + detected build.
    queries::update_genome_stats(
        &conn,
        genome_id,
        snp_count as i64,
        summary.build.as_deref(),
    )?;

    let _ = channel.send(ImportProgress {
        phase: "complete".to_string(),
        progress: 1.0,
        message: format!("Import complete: {} SNPs from {} format", snp_count, format),
    });

    let skip_rate = if summary.total_lines > 0 {
        (summary.skipped_lines as f64 / summary.total_lines as f64 * 100.0).round() / 100.0
    } else {
        0.0
    };

    Ok(ImportResult {
        genome_id,
        snp_count,
        format,
        build: summary.build,
        quality_summary: QualitySummary {
            total_lines: summary.total_lines,
            skipped_lines: summary.skipped_lines,
            valid_snps: snp_count,
            skip_rate,
        },
    })
}
