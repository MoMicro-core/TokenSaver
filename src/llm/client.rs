use crate::config::LlmConfig;
use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

// ── Public API ────────────────────────────────────────────────────────────────

/// Calls the configured LLM provider and deserialises the response into T.
/// Dispatches to Ollama or OpenAI-compatible protocol based on `config.provider`.
pub fn call<T: DeserializeOwned + Default>(
    config: &LlmConfig,
    system: &str,
    user_message: &str,
) -> Result<T> {
    match config.provider.as_str() {
        "openai" => call_openai(config, system, user_message),
        _        => call_ollama(config, system, user_message),
    }
}

/// Checks whether the configured provider is reachable and the model is available.
pub fn check_status(config: &LlmConfig) -> String {
    match config.provider.as_str() {
        "openai" => check_status_openai(config),
        _        => check_status_ollama(config),
    }
}

// ── Ollama (/api/chat) ────────────────────────────────────────────────────────

fn call_ollama<T: DeserializeOwned + Default>(
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
        "format": "json",   // grammar-constrained JSON at the sampler level
        "options": {
            "temperature": 0.1,   // near-deterministic structured extraction
            "top_p":       0.9,
            "num_predict": 512,
        }
    });

    let raw: serde_json::Value = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(config.timeout_secs))
        .send_json(body)
        .context("ollama unreachable")?
        .into_json()
        .context("failed to parse Ollama response")?;

    let content = extract_ollama_content(&raw);
    tracing::debug!(content = %content, "raw Ollama output");
    parse_llm_response(&content)
}

fn check_status_ollama(config: &LlmConfig) -> String {
    let url = format!("{}/api/tags", config.endpoint.trim_end_matches('/'));

    match ureq::get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .call()
    {
        Err(e) => format!("OFFLINE — cannot reach Ollama at {}\n  {e}", config.endpoint),
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
                    config.endpoint, config.model,
                    if models.is_empty() { "none".into() } else { models.join(", ") },
                    config.model
                )
            }
        }
    }
}

fn extract_ollama_content(raw: &serde_json::Value) -> String {
    raw["message"]["content"]
        .as_str()
        .unwrap_or("{}")
        .trim()
        .to_string()
}

// ── OpenAI-compatible (/v1/chat/completions) ──────────────────────────────────

fn call_openai<T: DeserializeOwned + Default>(
    config: &LlmConfig,
    system: &str,
    user_message: &str,
) -> Result<T> {
    let url     = format!("{}/v1/chat/completions", config.endpoint.trim_end_matches('/'));
    let api_key = resolve_api_key(config);

    let body = serde_json::json!({
        "model": config.model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user",   "content": user_message }
        ],
        "temperature":    0.1,
        "max_tokens":     512,
        "response_format": { "type": "json_object" }
    });

    let raw: serde_json::Value = ureq::post(&url)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(config.timeout_secs))
        .send_json(body)
        .context("OpenAI-compatible endpoint unreachable")?
        .into_json()
        .context("failed to parse OpenAI response")?;

    let content = extract_openai_content(&raw);
    tracing::debug!(content = %content, "raw OpenAI output");
    parse_llm_response(&content)
}

fn check_status_openai(config: &LlmConfig) -> String {
    let api_key = resolve_api_key(config);
    if api_key.is_empty() {
        return format!(
            "MISCONFIGURED — no API key for '{}'\n  \
             Set TOKENSAVER_API_KEY or OPENAI_API_KEY in your environment, \
             or add api_key to .tokensaver/config.toml",
            config.endpoint
        );
    }

    let url = format!("{}/v1/models", config.endpoint.trim_end_matches('/'));
    match ureq::get(&url)
        .set("Authorization", &format!("Bearer {api_key}"))
        .timeout(std::time::Duration::from_secs(5))
        .call()
    {
        Err(e) => format!("OFFLINE — cannot reach {} — {e}", config.endpoint),
        Ok(resp) => {
            let body: serde_json::Value = match resp.into_json() {
                Ok(v)  => v,
                Err(_) => return format!("ONLINE — {} (could not parse model list)", config.endpoint),
            };
            let models: Vec<String> = body["data"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|m| m["id"].as_str().map(String::from))
                .collect();
            let model_listed = models.iter().any(|m| m == &config.model);
            if model_listed {
                format!("ONLINE — {} | model '{}' ready", config.endpoint, config.model)
            } else {
                format!(
                    "ONLINE — {} | model '{}' not in list (may still work)\n  Listed: {}",
                    config.endpoint, config.model,
                    if models.is_empty() { "none returned".into() } else { models.join(", ") }
                )
            }
        }
    }
}

fn extract_openai_content(raw: &serde_json::Value) -> String {
    raw["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}")
        .trim()
        .to_string()
}

/// Resolves the API key: config file → TOKENSAVER_API_KEY → OPENAI_API_KEY.
fn resolve_api_key(config: &LlmConfig) -> String {
    if !config.api_key.is_empty() {
        return config.api_key.clone();
    }
    std::env::var("TOKENSAVER_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .unwrap_or_default()
}

// ── Shared JSON parsing ───────────────────────────────────────────────────────

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

/// Finds the outermost `{…}` substring, or `None` if the content has no braces.
fn extract_json_block(content: &str) -> Option<&str> {
    let start = content.find('{')?;
    let end   = content.rfind('}')?;
    if end > start { Some(&content[start..=end]) } else { None }
}
