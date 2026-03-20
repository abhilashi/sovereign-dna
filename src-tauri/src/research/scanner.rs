use std::collections::HashSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::db::queries::ResearchArticleRow;
use crate::error::AppError;

/// Result of a research scan operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub new_articles: usize,
    pub matched_articles: usize,
    pub scan_date: String,
}

/// Input data extracted before the async call so we don't hold locks.
pub struct ScanInput {
    pub rsids: Vec<String>,
}

/// An article fetched during scanning, with its matched rsIDs.
#[derive(Debug, Clone)]
pub struct ScannedArticle {
    pub article: ResearchArticleRow,
    pub search_rsids: Vec<String>,
}

/// NCBI ESearch response structures.
#[derive(Debug, Deserialize)]
struct ESearchResult {
    esearchresult: ESearchData,
}

#[derive(Debug, Deserialize)]
struct ESearchData {
    idlist: Vec<String>,
}

/// Scan PubMed for new research matching the user's notable rsIDs.
///
/// IMPORTANT: Only rsIDs (public identifiers) are sent in queries.
/// No genotype data ever leaves the device.
pub async fn scan_for_new_research(
    input: ScanInput,
    on_progress: impl Fn(&str, f64, &str),
) -> Result<Vec<ScannedArticle>, AppError> {
    if input.rsids.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

    let mut all_articles: Vec<ScannedArticle> = Vec::new();
    let mut seen_pmids: HashSet<String> = HashSet::new();

    // Process rsIDs in batches of 10 to stay under URL limits
    let batch_size = 10;
    let batches: Vec<&[String]> = input.rsids.chunks(batch_size).collect();
    let total_batches = batches.len();

    for (batch_idx, batch) in batches.iter().enumerate() {
        let progress = batch_idx as f64 / total_batches as f64;
        let batch_rsids_display = batch.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
        on_progress(
            "searching",
            progress,
            &format!("Searching PubMed for {}...", batch_rsids_display),
        );

        // Build combined OR query for this batch
        let query = batch
            .iter()
            .map(|rsid| rsid.as_str())
            .collect::<Vec<_>>()
            .join(" OR ");

        // ESearch: find PMIDs from last 30 days
        let esearch_url = format!(
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=pubmed&term={}&retmax=5&sort=date&datetype=pdat&reldate=30&retmode=json",
            urlencoded(&query)
        );

        let search_resp = match client.get(&esearch_url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                log::warn!("PubMed search request failed for batch {}: {}", batch_idx, e);
                // Rate limit compliance
                tokio::time::sleep(Duration::from_millis(350)).await;
                continue;
            }
        };

        if !search_resp.status().is_success() {
            log::warn!("PubMed search returned status: {}", search_resp.status());
            tokio::time::sleep(Duration::from_millis(350)).await;
            continue;
        }

        let search_text = search_resp
            .text()
            .await
            .map_err(|e| AppError::Network(format!("Failed to read search response: {}", e)))?;

        let search_result: ESearchResult = match serde_json::from_str(&search_text) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("Failed to parse PubMed search response: {}", e);
                tokio::time::sleep(Duration::from_millis(350)).await;
                continue;
            }
        };

        let pmids: Vec<String> = search_result
            .esearchresult
            .idlist
            .into_iter()
            .filter(|id| !seen_pmids.contains(id))
            .collect();

        if pmids.is_empty() {
            tokio::time::sleep(Duration::from_millis(350)).await;
            continue;
        }

        for id in &pmids {
            seen_pmids.insert(id.clone());
        }

        // Rate limit: max 3 requests/second
        tokio::time::sleep(Duration::from_millis(350)).await;

        // ESummary: get article details
        let ids = pmids.join(",");
        let esummary_url = format!(
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id={}&retmode=json",
            ids
        );

        let summary_resp = match client.get(&esummary_url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                log::warn!("PubMed summary fetch failed: {}", e);
                tokio::time::sleep(Duration::from_millis(350)).await;
                continue;
            }
        };

        if !summary_resp.status().is_success() {
            log::warn!("PubMed summary returned status: {}", summary_resp.status());
            tokio::time::sleep(Duration::from_millis(350)).await;
            continue;
        }

        let summary_text = summary_resp
            .text()
            .await
            .map_err(|e| AppError::Network(format!("Failed to read summary response: {}", e)))?;

        let summary_json: serde_json::Value = match serde_json::from_str(&summary_text) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("Failed to parse summary JSON: {}", e);
                tokio::time::sleep(Duration::from_millis(350)).await;
                continue;
            }
        };

        if let Some(result_obj) = summary_json.get("result") {
            for pmid in &pmids {
                if let Some(article_obj) = result_obj.get(pmid) {
                    let title = article_obj
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let journal = article_obj
                        .get("source")
                        .and_then(|v| v.as_str())
                        .unwrap_or("PubMed")
                        .to_string();

                    let pubdate = article_obj
                        .get("pubdate")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    // Extract first author
                    let authors = extract_authors(article_obj);

                    // Determine which rsIDs from this batch are relevant to this article
                    let relevant: Vec<String> = batch
                        .iter()
                        .filter(|rsid| title.to_lowercase().contains(&rsid.to_lowercase()))
                        .cloned()
                        .collect();

                    // If no specific rsID appears in the title, associate with the batch rsIDs
                    let search_rsids = if relevant.is_empty() {
                        batch.to_vec()
                    } else {
                        relevant
                    };

                    let rsids_json = serde_json::to_string(&search_rsids)
                        .unwrap_or_else(|_| "[]".to_string());

                    // Build source as "authors · journal"
                    let source_display = if authors.is_empty() {
                        journal.clone()
                    } else {
                        format!("{} · {}", authors, journal)
                    };

                    let now = chrono::Utc::now().to_rfc3339();

                    all_articles.push(ScannedArticle {
                        article: ResearchArticleRow {
                            id: pmid.clone(),
                            title,
                            abstract_text: None, // ESummary doesn't return abstracts
                            source: source_display,
                            published_date: Some(pubdate),
                            relevant_rsids: rsids_json,
                            fetched_date: now,
                        },
                        search_rsids,
                    });
                }
            }
        }

        // Rate limit between requests
        tokio::time::sleep(Duration::from_millis(350)).await;
    }

    on_progress(
        "complete",
        1.0,
        &format!("Found {} articles", all_articles.len()),
    );

    Ok(all_articles)
}

/// Extract first author + "et al" from ESummary article JSON.
fn extract_authors(article_obj: &serde_json::Value) -> String {
    if let Some(authors_arr) = article_obj.get("authors").and_then(|v| v.as_array()) {
        if let Some(first) = authors_arr.first() {
            let name = first
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            if authors_arr.len() > 1 {
                format!("{} et al", name)
            } else {
                name.to_string()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    }
}

/// Simple URL encoding for query parameters.
fn urlencoded(s: &str) -> String {
    s.replace(' ', "+")
        .replace('/', "%2F")
        .replace('[', "%5B")
        .replace(']', "%5D")
        .replace('(', "%28")
        .replace(')', "%29")
}
