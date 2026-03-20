use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::commands::workbench::{ChatMessage, ChatStreamEvent};
use crate::db::Database;
use crate::error::AppError;
use crate::research::intent::parse_question;

// ── Ollama Types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalLlmStatus {
    pub available: bool,
    pub provider: String,
    pub model: Option<String>,
    pub fallback: String,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Option<Vec<OllamaModel>>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct OllamaChatChunk {
    message: Option<OllamaChatMessage>,
    done: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaChatMessage {
    content: Option<String>,
}

// ── Preferred model order ────────────────────────────────────────

const PREFERRED_MODELS: &[&str] = &[
    "llama3.2",
    "llama3.1",
    "llama3",
    "mistral",
    "phi3",
    "gemma2",
];

/// Find the best available model from the Ollama tag list.
fn pick_preferred_model(models: &[OllamaModel]) -> Option<String> {
    for &preferred in PREFERRED_MODELS {
        for m in models {
            // Model names may include tags like "llama3.2:latest"
            let base = m.name.split(':').next().unwrap_or(&m.name);
            if base == preferred {
                return Some(m.name.clone());
            }
        }
    }
    // If none of the preferred models match, use the first available model
    models.first().map(|m| m.name.clone())
}

// ── check_local_llm ──────────────────────────────────────────────

#[tauri::command]
pub async fn check_local_llm() -> Result<LocalLlmStatus, AppError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

    let resp = match client.get("http://localhost:11434/api/tags").send().await {
        Ok(r) => r,
        Err(_) => {
            return Ok(LocalLlmStatus {
                available: false,
                provider: "none".to_string(),
                model: None,
                fallback: "structured_query".to_string(),
            });
        }
    };

    if !resp.status().is_success() {
        return Ok(LocalLlmStatus {
            available: false,
            provider: "none".to_string(),
            model: None,
            fallback: "structured_query".to_string(),
        });
    }

    let tags: OllamaTagsResponse = match resp.json().await {
        Ok(t) => t,
        Err(_) => {
            return Ok(LocalLlmStatus {
                available: false,
                provider: "none".to_string(),
                model: None,
                fallback: "structured_query".to_string(),
            });
        }
    };

    let models = tags.models.unwrap_or_default();
    if models.is_empty() {
        return Ok(LocalLlmStatus {
            available: false,
            provider: "ollama".to_string(),
            model: None,
            fallback: "structured_query".to_string(),
        });
    }

    let chosen = pick_preferred_model(&models);

    Ok(LocalLlmStatus {
        available: chosen.is_some(),
        provider: "ollama".to_string(),
        model: chosen,
        fallback: "structured_query".to_string(),
    })
}

// ── chat_local_llm ───────────────────────────────────────────────

#[tauri::command]
pub async fn chat_local_llm(
    messages: Vec<ChatMessage>,
    context: String,
    db: State<'_, Database>,
    genome_id: i64,
    channel: Channel<ChatStreamEvent>,
) -> Result<(), AppError> {
    // Check if Ollama is available
    let status = check_local_llm().await?;

    if status.available {
        // ── Ollama path ──────────────────────────────────────────
        let model = status.model.unwrap_or_else(|| "llama3.2".to_string());

        let system_prompt = format!(
            "You are a genetics research assistant. The user's genetic data is being analyzed \
             locally — none of it leaves this device. Here is the relevant context from their \
             genome:\n\n{}\n\nProvide accurate, helpful information. Always note this is \
             educational, not medical advice.",
            context
        );

        // Build messages array for Ollama API
        let mut api_messages = vec![serde_json::json!({
            "role": "system",
            "content": system_prompt,
        })];

        for msg in &messages {
            api_messages.push(serde_json::json!({
                "role": msg.role,
                "content": msg.content,
            }));
        }

        let body = serde_json::json!({
            "model": model,
            "messages": api_messages,
            "stream": true,
        });

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

        let resp = client
            .post("http://localhost:11434/api/chat")
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Ollama API request failed: {}", e)))?;

        if !resp.status().is_success() {
            let status_code = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(AppError::Network(format!(
                "Ollama API returned status {}: {}",
                status_code, body_text
            )));
        }

        // Stream newline-delimited JSON
        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        use futures_util::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|e| AppError::Network(format!("Stream error: {}", e)))?;
            let chunk_str = String::from_utf8_lossy(&chunk);
            buffer.push_str(&chunk_str);

            // Process complete lines
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if let Ok(parsed) = serde_json::from_str::<OllamaChatChunk>(&line) {
                    if parsed.done {
                        let _ = channel.send(ChatStreamEvent {
                            event_type: "complete".to_string(),
                            text: None,
                        });
                    } else if let Some(msg) = &parsed.message {
                        if let Some(content) = &msg.content {
                            if !content.is_empty() {
                                let _ = channel.send(ChatStreamEvent {
                                    event_type: "text_delta".to_string(),
                                    text: Some(content.clone()),
                                });
                            }
                        }
                    }
                }
            }
        }

        // Ensure completion event is sent
        let _ = channel.send(ChatStreamEvent {
            event_type: "complete".to_string(),
            text: None,
        });
    } else {
        // ── Fallback: structured query engine ────────────────────
        // Use the existing ask_genome logic to build an answer
        let question = messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        let answer = {
            let conn = db.0.lock().map_err(|e| {
                AppError::Database(format!("Failed to acquire database lock: {}", e))
            })?;

            let intent = parse_question(&question);

            match intent {
                crate::research::intent::QueryIntent::RsidLookup(rsid) => {
                    super::ask::build_rsid_answer(&conn, genome_id, &rsid)?
                }
                crate::research::intent::QueryIntent::GeneLookup(gene) => {
                    super::ask::build_gene_answer(&conn, genome_id, &gene)?
                }
                crate::research::intent::QueryIntent::ConditionRisk(condition) => {
                    super::ask::build_condition_risk_answer(&conn, genome_id, &condition)?
                }
                crate::research::intent::QueryIntent::DrugResponse(drug) => {
                    super::ask::build_drug_response_answer(&conn, genome_id, &drug)?
                }
                crate::research::intent::QueryIntent::TraitQuery(trait_name) => {
                    super::ask::build_trait_answer(&conn, genome_id, &trait_name)?
                }
                crate::research::intent::QueryIntent::CarrierQuery(condition) => {
                    super::ask::build_carrier_answer(&conn, genome_id, &condition)?
                }
                crate::research::intent::QueryIntent::ChromosomeQuery(chr) => {
                    super::ask::build_chromosome_answer(&conn, genome_id, &chr)?
                }
                crate::research::intent::QueryIntent::GeneralSummary => {
                    super::ask::build_general_summary(&conn, genome_id)?
                }
                crate::research::intent::QueryIntent::Unknown(q) => {
                    super::ask::build_unknown_answer(&q)
                }
            }
        };

        // Send the full answer as a single text_delta
        let _ = channel.send(ChatStreamEvent {
            event_type: "text_delta".to_string(),
            text: Some(answer.answer),
        });

        let _ = channel.send(ChatStreamEvent {
            event_type: "complete".to_string(),
            text: None,
        });
    }

    Ok(())
}
