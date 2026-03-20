use tauri::State;

use crate::db::Database;
use crate::error::AppError;
use crate::research::fetcher;
use crate::research::matcher::{self, MatchedArticle};

#[tauri::command]
pub async fn fetch_research(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<Vec<MatchedArticle>, AppError> {
    // Step 1: Get annotated SNPs (rsIDs with known significance)
    let rsids: Vec<String> = {
        let conn = db.0.lock().map_err(|e| {
            AppError::Database(format!("Failed to acquire database lock: {}", e))
        })?;

        // Get rsIDs that have annotations (these are most likely to have research articles)
        let mut stmt = conn.prepare(
            "SELECT DISTINCT s.rsid FROM snps s
             INNER JOIN annotations a ON s.rsid = a.rsid
             WHERE s.genome_id = ?1
             AND a.clinical_significance IS NOT NULL
             LIMIT 50",
        )?;

        let results: Vec<String> = stmt.query_map([genome_id], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        results
    };

    if rsids.is_empty() {
        return Ok(Vec::new());
    }

    // Step 2: Fetch articles from PubMed (only sends rsIDs, never genotype data)
    let articles = fetcher::fetch_pubmed_articles(&rsids, 50).await?;

    // Step 3: Cache articles in database
    {
        let conn = db.0.lock().map_err(|e| {
            AppError::Database(format!("Failed to acquire database lock: {}", e))
        })?;

        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO research_articles (id, title, abstract_text, source, published_date, relevant_rsids, fetched_date)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))",
            )?;

            for article in &articles {
                let rsids_json = serde_json::to_string(&article.relevant_rsids)
                    .unwrap_or_else(|_| "[]".to_string());
                stmt.execute(rusqlite::params![
                    article.id,
                    article.title,
                    article.abstract_text,
                    article.source,
                    article.published_date,
                    rsids_json,
                ])?;
            }
        }
        tx.commit()?;
    }

    // Step 4: Match articles to genome locally
    let matched = {
        let conn = db.0.lock().map_err(|e| {
            AppError::Database(format!("Failed to acquire database lock: {}", e))
        })?;
        matcher::match_articles_to_genome(&conn, genome_id, &articles)?
    };

    Ok(matched)
}

#[tauri::command]
pub fn get_cached_research(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<Vec<MatchedArticle>, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;

    // Load cached articles from database
    let mut stmt = conn.prepare(
        "SELECT id, title, abstract_text, source, published_date, relevant_rsids
         FROM research_articles
         ORDER BY fetched_date DESC",
    )?;

    let articles: Vec<fetcher::ResearchArticle> = stmt
        .query_map([], |row| {
            let rsids_json: String = row.get(5)?;
            let relevant_rsids: Vec<String> =
                serde_json::from_str(&rsids_json).unwrap_or_default();
            Ok(fetcher::ResearchArticle {
                id: row.get(0)?,
                title: row.get(1)?,
                abstract_text: row.get(2)?,
                source: row.get(3)?,
                published_date: row.get(4)?,
                relevant_rsids,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    if articles.is_empty() {
        return Ok(Vec::new());
    }

    matcher::match_articles_to_genome(&conn, genome_id, &articles)
}
