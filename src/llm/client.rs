use crate::config::LlmConfig;
use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

/// Calls Ollama /api/chat and deserialises the response into T.
/// Generic over T so it works for both ContextDecision and MemoryDecision.
pub fn call<T: DeserializeOwned + Default>(
    config: &LlmConfig,
    system: &str,
    user_message: &str,
) -> Result<T> {
    let url = format!("{}/api/chat", config.endpoint.trim_end_matches('/'));

    let body = serde_json::json!({
        "model": config.model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user",   "content": user_message }
        ],
        "stream": false,
        "format": "json",   // Ollama grammar-constrains JSON output at the sampler level
        "options": {
            "temperature": 0.1,   // near-deterministic — critical for structured extraction
            "top_p":       0.9,
            "num_predict": 512,   // cap response length, prevents runaway generation
        }
    });

    let response = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(config.timeout_secs))
        .send_json(body)
        .context("ollama unreachable")?;

    let raw: serde_json::Value = response
        .into_json()
        .context("failed to parse Ollama response")?;

    let content = raw["message"]["content"]
        .as_str()
        .unwrap_or("{}")
        .trim()
        .to_string();

    tracing::debug!(content = %content, "raw LLM output");

    parse_llm_response(&content)
}

/// Tries to deserialise `content` as T. Falls back to extracting the first `{…}` block
/// from the string in case the model wrapped its JSON answer in prose.
fn parse_llm_response<T: DeserializeOwned>(content: &str) -> Result<T> {
    if let Ok(parsed) = serde_json::from_str::<T>(content) {
        return Ok(parsed);
    }
    if let Some(json_block) = extract_json_block(content) {
        if let Ok(parsed) = serde_json::from_str::<T>(json_block) {
            tracing::debug!("JSON extracted from prose response");
            return Ok(parsed);
        }
    }
    anyhow::bail!("LLM response could not be parsed as JSON: {content}")
}

/// Finds the outermost `{…}` substring, or `None` if the content contains no braces.
fn extract_json_block(content: &str) -> Option<&str> {
    let start = content.find('{')?;
    let end   = content.rfind('}')?;
    if end > start { Some(&content[start..=end]) } else { None }
}

/// Checks whether Ollama is reachable and the configured model is available.
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
        Ok(resp) => {
            let body: serde_json::Value = match resp.into_json() {
                Ok(v)  => v,
                Err(_) => return format!("ONLINE — {} (could not parse model list)", config.endpoint),
            };

            let models: Vec<String> = body["models"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|m| m["name"].as_str().map(String::from))
                .collect();

            let model_ready = models.iter().any(|m| {
                m == &config.model || m.starts_with(&format!("{}:", config.model))
            });

            if model_ready {
                format!("ONLINE — {} | model '{}' ready", config.endpoint, config.model)
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
