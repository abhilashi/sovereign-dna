use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// A research article from PubMed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchArticle {
    pub id: String,
    pub title: String,
    pub abstract_text: String,
    pub source: String,
    pub published_date: String,
    pub relevant_rsids: Vec<String>,
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

/// NCBI EFetch XML parsing helpers.
#[derive(Debug, Deserialize)]
struct EFetchResult {
    #[serde(default)]
    result: std::collections::HashMap<String, PubMedArticle>,
}

#[derive(Debug, Deserialize)]
struct PubMedArticle {
    #[serde(default)]
    uid: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    pubdate: String,
}

/// Fetch PubMed articles related to the given rsIDs.
///
/// IMPORTANT: Only sends rsIDs (public identifiers), never user genotype data.
/// Uses NCBI E-utilities: esearch to find articles, esummary to get details.
pub async fn fetch_pubmed_articles(
    rsids: &[String],
    max_results: u32,
) -> Result<Vec<ResearchArticle>, AppError> {
    if rsids.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

    let mut all_articles = Vec::new();

    // Process rsIDs in batches to avoid overly long queries
    let batch_size = 10;
    let max_per_batch = (max_results / ((rsids.len() / batch_size).max(1) as u32)).max(5);

    for batch in rsids.chunks(batch_size) {
        let query = batch
            .iter()
            .map(|rsid| format!("{}[Title/Abstract]", rsid))
            .collect::<Vec<_>>()
            .join(" OR ");

        // Step 1: ESearch to find PMIDs
        let esearch_url = format!(
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=pubmed&term={}&retmax={}&retmode=json",
            urlencoded(&query),
            max_per_batch
        );

        let search_resp = client
            .get(&esearch_url)
            .send()
            .await
            .map_err(|e| AppError::Network(format!("PubMed search failed: {}", e)))?;

        if !search_resp.status().is_success() {
            log::warn!("PubMed search returned status: {}", search_resp.status());
            continue;
        }

        let search_text = search_resp
            .text()
            .await
            .map_err(|e| AppError::Network(format!("Failed to read search response: {}", e)))?;

        let search_result: ESearchResult = serde_json::from_str(&search_text)
            .map_err(|e| AppError::Parse(format!("Failed to parse search response: {}", e)))?;

        let pmids = &search_result.esearchresult.idlist;
        if pmids.is_empty() {
            continue;
        }

        // Step 2: ESummary to get article details
        let ids = pmids.join(",");
        let esummary_url = format!(
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id={}&retmode=json",
            ids
        );

        let summary_resp = client
            .get(&esummary_url)
            .send()
            .await
            .map_err(|e| AppError::Network(format!("PubMed summary fetch failed: {}", e)))?;

        if !summary_resp.status().is_success() {
            log::warn!("PubMed summary returned status: {}", summary_resp.status());
            continue;
        }

        let summary_text = summary_resp
            .text()
            .await
            .map_err(|e| AppError::Network(format!("Failed to read summary response: {}", e)))?;

        // Parse the summary JSON (ESummary format)
        let summary_json: serde_json::Value = serde_json::from_str(&summary_text)
            .map_err(|e| AppError::Parse(format!("Failed to parse summary response: {}", e)))?;

        if let Some(result_obj) = summary_json.get("result") {
            for pmid in pmids {
                if let Some(article_obj) = result_obj.get(pmid) {
                    let title = article_obj
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let source = article_obj
                        .get("source")
                        .and_then(|v| v.as_str())
                        .unwrap_or("PubMed")
                        .to_string();

                    let pubdate = article_obj
                        .get("pubdate")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    // Determine which rsIDs from our batch appear in the title
                    let relevant: Vec<String> = batch
                        .iter()
                        .filter(|rsid| {
                            title.contains(rsid.as_str())
                        })
                        .cloned()
                        .collect();

                    // If no specific match in title, associate with all batch rsIDs
                    let relevant_rsids = if relevant.is_empty() {
                        batch.to_vec()
                    } else {
                        relevant
                    };

                    all_articles.push(ResearchArticle {
                        id: pmid.clone(),
                        title,
                        abstract_text: String::new(), // ESummary doesn't return abstracts
                        source,
                        published_date: pubdate,
                        relevant_rsids,
                    });
                }
            }
        }
    }

    // Truncate to max_results
    all_articles.truncate(max_results as usize);

    Ok(all_articles)
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
