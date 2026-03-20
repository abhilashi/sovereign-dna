use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::db::queries;
use crate::db::Database;
use crate::error::AppError;
use crate::research::digest::{self, ResearchDigest};
use crate::research::scanner::{self, ScanInput, ScanResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub phase: String,
    pub progress: f64,
    pub message: String,
}

/// Scan PubMed for new research articles matching the user's notable rsIDs.
///
/// Streams progress updates to the frontend via a Channel.
/// DB lock is acquired only for short reads/writes -- never held during HTTP requests.
#[tauri::command]
pub async fn scan_research(
    genome_id: i64,
    db: State<'_, Database>,
    channel: Channel<ScanProgress>,
) -> Result<ScanResult, AppError> {
    // Step 1: Get notable rsIDs (lock -> query -> unlock)
    let rsids: Vec<String> = {
        let conn = db.0.lock().map_err(|e| {
            AppError::Database(format!("Failed to acquire database lock: {}", e))
        })?;
        queries::get_notable_rsids(&conn, genome_id, 200)?
    };

    if rsids.is_empty() {
        return Ok(ScanResult {
            new_articles: 0,
            matched_articles: 0,
            scan_date: chrono::Utc::now().to_rfc3339(),
        });
    }

    let _ = channel.send(ScanProgress {
        phase: "preparing".to_string(),
        progress: 0.0,
        message: format!("Found {} notable variants to search", rsids.len()),
    });

    // Step 2: Run the async scanner (no DB lock held)
    let channel_ref = &channel;
    let scanned = scanner::scan_for_new_research(
        ScanInput { rsids: rsids.clone() },
        |phase, progress, message| {
            let _ = channel_ref.send(ScanProgress {
                phase: phase.to_string(),
                progress,
                message: message.to_string(),
            });
        },
    )
    .await?;

    let _ = channel.send(ScanProgress {
        phase: "storing".to_string(),
        progress: 0.9,
        message: "Matching against your genome...".to_string(),
    });

    // Step 3: Store results and compute matches (lock -> insert -> unlock)
    let mut new_count: usize = 0;
    let matched_count = scanned.len();

    {
        let conn = db.0.lock().map_err(|e| {
            AppError::Database(format!("Failed to acquire database lock: {}", e))
        })?;

        // Also get the user's rsID set for matching
        let user_rsids = queries::get_all_user_rsids(&conn, genome_id)?;

        // Get annotated rsIDs for relevance scoring boost
        let rsid_list: Vec<String> = user_rsids.iter().cloned().collect();
        let annotated_rsids: std::collections::HashSet<String> = if !rsid_list.is_empty() {
            // Check which rsIDs have annotations
            let sample: Vec<String> = rsid_list.iter().take(500).cloned().collect();
            let placeholders: String = sample.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT rsid FROM annotations WHERE rsid IN ({})",
                placeholders
            );
            let mut stmt = conn.prepare(&sql)?;
            let params: Vec<&dyn rusqlite::types::ToSql> = sample
                .iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();
            let rows: std::collections::HashSet<String> = stmt.query_map(params.as_slice(), |row| row.get::<_, String>(0))?
                .filter_map(|r| r.ok())
                .collect();
            rows
        } else {
            std::collections::HashSet::new()
        };

        for scanned_article in &scanned {
            // Insert the article
            let is_new = queries::insert_research_article(&conn, &scanned_article.article)?;
            if is_new {
                new_count += 1;
            }

            // Compute matched rsIDs (those in user's genome)
            let matched: Vec<String> = scanned_article
                .search_rsids
                .iter()
                .filter(|rsid| user_rsids.contains(rsid.as_str()))
                .cloned()
                .collect();

            // Compute relevance score
            let total = scanned_article.search_rsids.len().max(1) as f64;
            let match_ratio = matched.len() as f64 / total;
            let annotation_boost: f64 = matched
                .iter()
                .filter(|rsid| annotated_rsids.contains(rsid.as_str()))
                .count() as f64
                * 0.1;
            let relevance_score = ((match_ratio + annotation_boost) * 100.0).round() / 100.0;
            let relevance_score = relevance_score.min(1.0);

            let matched_json = serde_json::to_string(&matched)
                .unwrap_or_else(|_| "[]".to_string());

            queries::upsert_article_genome_match(
                &conn,
                &scanned_article.article.id,
                genome_id,
                &matched_json,
                relevance_score,
            )?;
        }
    }

    let scan_date = chrono::Utc::now().to_rfc3339();

    let _ = channel.send(ScanProgress {
        phase: "complete".to_string(),
        progress: 1.0,
        message: format!(
            "Found {} new articles, {} matched your genome",
            new_count, matched_count
        ),
    });

    Ok(ScanResult {
        new_articles: new_count,
        matched_articles: matched_count,
        scan_date,
    })
}

/// Get the research digest for a genome.
#[tauri::command]
pub fn get_research_digest(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<ResearchDigest, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;

    let result = digest::build_digest(&conn, genome_id, None)?;

    // Mark all returned articles as seen
    let article_ids: Vec<String> = result.items.iter().map(|i| i.article_id.clone()).collect();
    if !article_ids.is_empty() {
        queries::mark_articles_seen(&conn, &article_ids)?;
    }

    Ok(result)
}

/// Quick count of unseen articles for the nav badge.
#[tauri::command]
pub fn get_new_research_count(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<i64, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;
    queries::count_unseen_articles(&conn, genome_id)
}
