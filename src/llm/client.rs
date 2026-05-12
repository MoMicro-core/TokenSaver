use crate::config::LlmConfig;
use crate::llm::response::LlmDecision;
use anyhow::{Context, Result};

/// Calls Ollama's /api/chat endpoint synchronously.
/// Returns an error if Ollama is unreachable or the response cannot be parsed —
/// callers must treat this as a signal to fall back to deterministic mode.
pub fn call(config: &LlmConfig, system: &str, user_message: &str) -> Result<LlmDecision> {
    let url = format!("{}/api/chat", config.endpoint.trim_end_matches('/'));

    let body = serde_json::json!({
        "model": config.model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user",   "content": user_message }
        ],
        "stream": false,
        "format": "json"   // instructs Ollama to enforce JSON output
    });

    let response = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(config.timeout_secs))
        .send_json(body)
        .context("ollama unreachable — falling back to deterministic mode")?;

    let raw: serde_json::Value = response
        .into_json()
        .context("failed to parse Ollama response body")?;

    let content = raw["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    tracing::debug!(content = %content, "raw LLM response");

    let decision: LlmDecision = serde_json::from_str(&content)
        .context("LLM returned invalid JSON — falling back to deterministic mode")?;

    Ok(decision)
}

/// Checks whether the configured Ollama server is reachable and the model is available.
/// Returns a human-readable status string suitable for `tokensaver llm-status`.
pub fn check_status(config: &LlmConfig) -> String {
    let url = format!("{}/api/tags", config.endpoint.trim_end_matches('/'));

    match ureq::get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .call()
    {
        Err(e) => format!(
            "OFFLINE — cannot reach Ollama at {}\n  {e}",
            config.endpoint
        ),
        Ok(response) => {
            let body: serde_json::Value = match response.into_json() {
                Ok(v) => v,
                Err(_) => return format!("ONLINE — {}, but could not parse model list", config.endpoint),
            };

            let models: Vec<String> = body["models"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|m| m["name"].as_str().map(String::from))
                .collect();

            let model_available = models.iter().any(|m| {
                m == &config.model || m.starts_with(&format!("{}:", config.model))
            });

            if model_available {
                format!(
                    "ONLINE — {} | model '{}' is ready",
                    config.endpoint, config.model
                )
            } else {
                format!(
                    "ONLINE — {} | model '{}' not found\n  Available: {}\n  Run: ollama pull {}",
                    config.endpoint,
                    config.model,
                    if models.is_empty() { "none".into() } else { models.join(", ") },
                    config.model
                )
            }
        }
    }
}
