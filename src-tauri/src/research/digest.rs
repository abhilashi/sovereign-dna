use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::queries;
use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DigestItem {
    pub article_id: String,
    pub title: String,
    pub authors: String,
    pub journal: String,
    pub published_date: String,
    pub matched_rsids: Vec<String>,
    pub relevance_score: f64,
    pub summary: String,
    pub pubmed_url: String,
    pub is_new: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchDigest {
    pub items: Vec<DigestItem>,
    pub total_new: usize,
    pub last_scan_date: Option<String>,
    pub next_scan_date: Option<String>,
}

/// Build a research digest for a given genome.
///
/// If `since` is provided, articles fetched after that date with no `seen_at`
/// are marked as `is_new`.
pub fn build_digest(
    conn: &Connection,
    genome_id: i64,
    since: Option<&str>,
) -> Result<ResearchDigest, AppError> {
    let last_scan_date = queries::get_last_scan_date(conn)?;

    // Calculate next scan date (24 hours after last scan)
    let next_scan_date = last_scan_date.as_ref().map(|d| {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(d) {
            let next = dt + chrono::Duration::hours(24);
            next.to_rfc3339()
        } else {
            // Fallback: just report tomorrow
            let now = chrono::Utc::now() + chrono::Duration::hours(24);
            now.to_rfc3339()
        }
    });

    // Get articles matched to this genome (up to 100)
    let article_rows = queries::get_research_articles(conn, genome_id, 100)?;

    let mut items = Vec::new();
    let mut total_new: usize = 0;

    for (article, matched_rsids_json, relevance_score) in &article_rows {
        let matched_rsids: Vec<String> =
            serde_json::from_str(matched_rsids_json).unwrap_or_default();

        // Determine if article is "new" -- unseen and fetched after `since`
        let is_new = if let Some(since_date) = since {
            article.fetched_date.as_str() > since_date
        } else {
            // Check if the article has been seen via the seen_at column
            let seen: bool = conn
                .query_row(
                    "SELECT seen_at IS NOT NULL FROM research_articles WHERE id = ?1",
                    [&article.id],
                    |row| row.get(0),
                )
                .unwrap_or(true); // Default to "seen" if query fails
            !seen
        };

        if is_new {
            total_new += 1;
        }

        // Parse authors and journal from source field (stored as "Author et al · Journal")
        let (authors, journal) = parse_source(&article.source);

        let pubmed_url = format!("https://pubmed.ncbi.nlm.nih.gov/{}/", article.id);

        let summary = article
            .abstract_text
            .clone()
            .unwrap_or_default();

        items.push(DigestItem {
            article_id: article.id.clone(),
            title: article.title.clone(),
            authors,
            journal,
            published_date: article
                .published_date
                .clone()
                .unwrap_or_default(),
            matched_rsids,
            relevance_score: *relevance_score,
            summary,
            pubmed_url,
            is_new,
        });
    }

    // Sort: new articles first, then by relevance_score descending
    items.sort_by(|a, b| {
        b.is_new
            .cmp(&a.is_new)
            .then_with(|| {
                b.relevance_score
                    .partial_cmp(&a.relevance_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    Ok(ResearchDigest {
        items,
        total_new,
        last_scan_date,
        next_scan_date,
    })
}

/// Parse the "authors · journal" source string into separate parts.
fn parse_source(source: &str) -> (String, String) {
    if let Some(idx) = source.find(" · ") {
        let authors = source[..idx].to_string();
        let journal = source[idx + " · ".len()..].to_string();
        (authors, journal)
    } else {
        // No separator -- treat entire string as journal
        (String::new(), source.to_string())
    }
}
