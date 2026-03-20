use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::db::queries::{self, ChatMessageRow, WorkbenchSession};
use crate::db::Database;
use crate::error::AppError;
use crate::research::workbench::{
    self, PipelineInput, WorkbenchProgress, WorkbenchResult,
};

// ── Chat Types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamEvent {
    pub event_type: String,
    pub text: Option<String>,
}

// ── Research Query Command ──────────────────────────────────────

#[tauri::command]
pub async fn research_query(
    query: String,
    genome_id: i64,
    db: State<'_, Database>,
    channel: Channel<WorkbenchProgress>,
) -> Result<WorkbenchResult, AppError> {
    // Step 1: Lock DB -> get user_rsids, get relevant snp_rows -> unlock
    let (user_rsids, snp_rows) = {
        let conn = db.0.lock().map_err(|e| {
            AppError::Database(format!("Failed to acquire database lock: {}", e))
        })?;
        let user_rsids = queries::get_all_user_rsids(&conn, genome_id)?;
        let (snp_rows, _) = queries::get_snps_paginated(&conn, genome_id, 0, 1000, None, None)?;
        (user_rsids, snp_rows)
    };

    // Step 2: Build PipelineInput
    let input = PipelineInput {
        query: query.clone(),
        genome_id,
        user_rsids,
        snp_rows,
    };

    // Step 3: Call execute_pipeline (async, streams progress via channel)
    let channel_ref = &channel;
    let result = workbench::execute_pipeline(
        input,
        db.inner(),
        |prog| {
            let _ = channel_ref.send(prog);
        },
    )
    .await?;

    // Step 4: Lock DB -> save session to workbench_sessions -> unlock
    {
        let conn = db.0.lock().map_err(|e| {
            AppError::Database(format!("Failed to acquire database lock: {}", e))
        })?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let result_json = serde_json::to_string(&result)
            .unwrap_or_else(|_| "{}".to_string());

        queries::save_workbench_session(
            &conn,
            &session_id,
            genome_id,
            &query,
            &result.strategy,
            &result_json,
        )?;
    }

    // Step 5: Return result
    Ok(result)
}

// ── Claude Chat Command ─────────────────────────────────────────

#[tauri::command]
pub async fn chat_with_claude(
    api_key: String,
    messages: Vec<ChatMessage>,
    context: String,
    channel: Channel<ChatStreamEvent>,
) -> Result<(), AppError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

    let system_prompt = format!(
        "You are a genomics research assistant helping a user understand their DNA analysis results. \
         You have access to the following context about their genome and relevant research. \
         Use this information to provide accurate, evidence-based answers. \
         Always cite specific variants (rsIDs) and studies when relevant. \
         Be clear about the difference between statistical associations and definitive predictions. \
         Remind the user that genetic information should be discussed with a healthcare provider.\n\n\
         --- GENOME CONTEXT ---\n{}\n--- END CONTEXT ---",
        context
    );

    // Build API messages
    let api_messages: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content,
            })
        })
        .collect();

    let body = serde_json::json!({
        "model": "claude-sonnet-4-20250514",
        "max_tokens": 4096,
        "system": system_prompt,
        "messages": api_messages,
        "stream": true,
    });

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| AppError::Network(format!("Claude API request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        return Err(AppError::Network(format!(
            "Claude API returned status {}: {}",
            status, body_text
        )));
    }

    // Stream SSE response
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    use futures_util::StreamExt;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AppError::Network(format!("Stream error: {}", e)))?;
        let chunk_str = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_str);

        // Process complete SSE lines
        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim_end().to_string();
            buffer = buffer[newline_pos + 1..].to_string();

            if line.starts_with("data: ") {
                let json_str = &line[6..];

                if json_str == "[DONE]" {
                    continue;
                }

                if let Ok(event_json) = serde_json::from_str::<serde_json::Value>(json_str) {
                    let event_type = event_json
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    match event_type {
                        "content_block_delta" => {
                            if let Some(delta) = event_json.get("delta") {
                                if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                    let _ = channel.send(ChatStreamEvent {
                                        event_type: "delta".to_string(),
                                        text: Some(text.to_string()),
                                    });
                                }
                            }
                        }
                        "message_stop" => {
                            let _ = channel.send(ChatStreamEvent {
                                event_type: "complete".to_string(),
                                text: None,
                            });
                        }
                        "error" => {
                            let error_msg = event_json
                                .get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("Unknown streaming error");
                            let _ = channel.send(ChatStreamEvent {
                                event_type: "error".to_string(),
                                text: Some(error_msg.to_string()),
                            });
                            return Err(AppError::Network(format!("Claude streaming error: {}", error_msg)));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Send completion if not already sent
    let _ = channel.send(ChatStreamEvent {
        event_type: "complete".to_string(),
        text: None,
    });

    Ok(())
}

// ── Session Management Commands ─────────────────────────────────

#[tauri::command]
pub fn get_workbench_sessions(
    genome_id: i64,
    db: State<'_, Database>,
) -> Result<Vec<WorkbenchSession>, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;
    queries::get_workbench_sessions(&conn, genome_id, 50)
}

#[tauri::command]
pub fn save_workbench_chat(
    session_id: String,
    role: String,
    content: String,
    db: State<'_, Database>,
) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;
    queries::save_chat_message(&conn, &session_id, &role, &content)
}

#[tauri::command]
pub fn get_workbench_chat(
    session_id: String,
    db: State<'_, Database>,
) -> Result<Vec<ChatMessageRow>, AppError> {
    let conn = db.0.lock().map_err(|e| {
        AppError::Database(format!("Failed to acquire database lock: {}", e))
    })?;
    queries::get_chat_messages(&conn, &session_id)
}
