use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use super::fetcher::ResearchArticle;

/// A research article matched against the user's genome.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchedArticle {
    pub article: ResearchArticle,
    pub matched_rsids: Vec<String>,
    pub relevance_score: f64,
}

/// Match research articles against the user's actual SNP data.
///
/// Only uses rsIDs for matching - never exposes genotype data.
pub fn match_articles_to_genome(
    conn: &Connection,
    genome_id: i64,
    articles: &[ResearchArticle],
) -> Result<Vec<MatchedArticle>, AppError> {
    if articles.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all unique rsIDs from articles
    let all_article_rsids: std::collections::HashSet<&str> = articles
        .iter()
        .flat_map(|a| a.relevant_rsids.iter().map(|s| s.as_str()))
        .collect();

    if all_article_rsids.is_empty() {
        return Ok(Vec::new());
    }

    // Query which of these rsIDs exist in the user's genome
    let rsid_list: Vec<&str> = all_article_rsids.into_iter().collect();
    let placeholders: String = rsid_list.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT DISTINCT rsid FROM snps WHERE genome_id = ?1 AND rsid IN ({})",
        placeholders
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    params.push(Box::new(genome_id));
    for rsid in &rsid_list {
        params.push(Box::new(rsid.to_string()));
    }
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let user_rsids: std::collections::HashSet<String> = stmt
        .query_map(param_refs.as_slice(), |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();

    // Also check which rsIDs have annotations (higher relevance)
    let annotated_rsids: std::collections::HashSet<String> = if !user_rsids.is_empty() {
        let ann_placeholders: String = user_rsids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let ann_sql = format!(
            "SELECT rsid FROM annotations WHERE rsid IN ({})",
            ann_placeholders
        );
        let mut ann_stmt = conn.prepare(&ann_sql)?;
        let ann_params: Vec<Box<dyn rusqlite::types::ToSql>> = user_rsids
            .iter()
            .map(|s| Box::new(s.clone()) as Box<dyn rusqlite::types::ToSql>)
            .collect();
        let ann_refs: Vec<&dyn rusqlite::types::ToSql> =
            ann_params.iter().map(|p| p.as_ref()).collect();

        let results: std::collections::HashSet<String> = ann_stmt
            .query_map(ann_refs.as_slice(), |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        results
    } else {
        std::collections::HashSet::new()
    };

    // Match each article
    let mut matched = Vec::new();

    for article in articles {
        let matched_rsids: Vec<String> = article
            .relevant_rsids
            .iter()
            .filter(|rsid| user_rsids.contains(rsid.as_str()))
            .cloned()
            .collect();

        // Calculate relevance score
        let total_rsids = article.relevant_rsids.len().max(1) as f64;
        let match_ratio = matched_rsids.len() as f64 / total_rsids;

        // Boost score if matched rsIDs have annotations
        let annotation_boost: f64 = matched_rsids
            .iter()
            .filter(|rsid| annotated_rsids.contains(rsid.as_str()))
            .count() as f64
            * 0.1;

        let relevance_score = ((match_ratio + annotation_boost) * 100.0).round() / 100.0;

        matched.push(MatchedArticle {
            article: article.clone(),
            matched_rsids,
            relevance_score: relevance_score.min(1.0),
        });
    }

    // Sort by relevance score descending
    matched.sort_by(|a, b| {
        b.relevance_score
            .partial_cmp(&a.relevance_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(matched)
}
