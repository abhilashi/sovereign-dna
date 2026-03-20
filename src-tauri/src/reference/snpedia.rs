use crate::db::queries::SnpediaEntry;
use crate::error::AppError;

/// Fetch SNPedia data for a list of rsIDs using the MediaWiki API.
///
/// Queries the rsID pages (e.g., `Rs1234`) in batches of 50 (the MediaWiki API limit).
/// Parses wiki markup to extract magnitude, repute, and summary fields.
/// Rate-limits requests to 1 second between batches to be respectful of the API.
pub async fn fetch_snpedia_for_rsids(
    rsids: &[String],
) -> Result<Vec<SnpediaEntry>, AppError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("GenomeStudio/0.1 (local DNA analysis tool)")
        .build()
        .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

    let mut all_entries = Vec::new();
    let batch_size = 50;

    for (batch_idx, batch) in rsids.chunks(batch_size).enumerate() {
        // Rate limit: wait 1 second between batches (skip first)
        if batch_idx > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        // Build titles: capitalize first letter of rs -> Rs
        let titles: Vec<String> = batch
            .iter()
            .map(|rsid| capitalize_rsid(rsid))
            .collect();
        let titles_param = titles.join("|");

        let url = format!(
            "https://bots.snpedia.com/api.php?action=query&titles={}&prop=revisions&rvprop=content&format=json",
            titles_param
        );

        let response = match client.get(&url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                log::warn!("SNPedia batch {} request failed: {}", batch_idx, e);
                continue;
            }
        };

        if !response.status().is_success() {
            log::warn!(
                "SNPedia batch {} returned status: {}",
                batch_idx,
                response.status()
            );
            continue;
        }

        let body: serde_json::Value = match response.json().await {
            Ok(v) => v,
            Err(e) => {
                log::warn!("SNPedia batch {} JSON parse failed: {}", batch_idx, e);
                continue;
            }
        };

        // Parse the MediaWiki response
        if let Some(pages) = body.get("query").and_then(|q| q.get("pages")) {
            if let Some(pages_obj) = pages.as_object() {
                for (_page_id, page) in pages_obj {
                    // Skip missing pages
                    if page.get("missing").is_some() {
                        continue;
                    }

                    let title = match page.get("title").and_then(|t| t.as_str()) {
                        Some(t) => t,
                        None => continue,
                    };

                    // Extract rsid from title (e.g., "Rs1234" -> "rs1234")
                    let rsid = title.to_lowercase();
                    if !rsid.starts_with("rs") {
                        continue;
                    }

                    // Get the wiki markup content
                    let content = page
                        .get("revisions")
                        .and_then(|r| r.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|rev| rev.get("*"))
                        .and_then(|c| c.as_str())
                        .unwrap_or("");

                    if content.is_empty() {
                        continue;
                    }

                    // Parse the wiki markup for relevant fields
                    let magnitude = extract_wiki_field(content, "magnitude")
                        .and_then(|v| v.parse::<f64>().ok());
                    let repute = extract_wiki_field(content, "repute");
                    let summary = extract_wiki_field(content, "summary");

                    // Only add entries that have at least some useful data
                    if magnitude.is_some() || repute.is_some() || summary.is_some() {
                        all_entries.push(SnpediaEntry {
                            rsid: rsid.clone(),
                            genotype: "overview".to_string(),
                            magnitude,
                            repute,
                            summary,
                        });
                    }
                }
            }
        }
    }

    log::info!(
        "Fetched {} SNPedia entries for {} rsIDs",
        all_entries.len(),
        rsids.len()
    );

    Ok(all_entries)
}

/// Capitalize rsID for SNPedia: "rs1234" -> "Rs1234"
fn capitalize_rsid(rsid: &str) -> String {
    let lower = rsid.to_lowercase();
    if lower.starts_with("rs") {
        format!("Rs{}", &lower[2..])
    } else {
        lower
    }
}

/// Extract a field value from wiki markup template syntax.
/// Looks for patterns like `|magnitude=3.5` or `| summary = Some text here`
fn extract_wiki_field(content: &str, field_name: &str) -> Option<String> {
    // Look for |field_name= pattern (case-insensitive)
    let field_lower = field_name.to_lowercase();

    for line in content.lines() {
        let trimmed = line.trim();
        let lower_line = trimmed.to_lowercase();

        // Match "|fieldname=" or "| fieldname ="
        if let Some(pos) = lower_line.find(&format!("|{}", field_lower)) {
            let after_field = &trimmed[pos + 1..]; // skip the |
            if let Some(eq_pos) = after_field.find('=') {
                let value = after_field[eq_pos + 1..].trim();
                // Trim trailing | or }} if present
                let value = value.trim_end_matches("}}").trim_end_matches('|').trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capitalize_rsid() {
        assert_eq!(capitalize_rsid("rs1234"), "Rs1234");
        assert_eq!(capitalize_rsid("RS5678"), "Rs5678");
        assert_eq!(capitalize_rsid("Rs9999"), "Rs9999");
    }

    #[test]
    fn test_extract_wiki_field() {
        let content = r#"
{{Rsnum
|rsid=1234
|Gene=BRCA1
|Chromosome=17
|magnitude=3.5
|repute=Bad
|summary=This variant is associated with increased risk.
}}
"#;
        assert_eq!(
            extract_wiki_field(content, "magnitude"),
            Some("3.5".to_string())
        );
        assert_eq!(
            extract_wiki_field(content, "repute"),
            Some("Bad".to_string())
        );
        assert_eq!(
            extract_wiki_field(content, "summary"),
            Some("This variant is associated with increased risk.".to_string())
        );
        assert_eq!(extract_wiki_field(content, "nonexistent"), None);
    }
}
