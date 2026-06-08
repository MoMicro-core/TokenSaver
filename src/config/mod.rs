use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub llm: LlmConfig,
    pub prompt: PromptConfig,
    pub analyzer: AnalyzerConfig,
    pub memory: MemoryConfig,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct LlmConfig {
    /// Enable LLM enrichment. Falls back to deterministic mode if false or the provider is unreachable.
    pub enabled: bool,

    /// Which API protocol to use: `"ollama"` (default, local) or `"openai"` (any OpenAI-compatible API).
    pub provider: String,

    /// Model name as the provider knows it.
    /// Ollama examples: `qwen2.5-coder:0.5b`, `llama3.2`
    /// OpenAI examples: `gpt-4o-mini`, `gpt-4o`
    pub model: String,

    /// Base URL for the provider's API.
    /// Ollama default: `http://localhost:11434`
    /// OpenAI default: `https://api.openai.com`
    /// OpenRouter:     `https://openrouter.ai/api`
    pub endpoint: String,

    /// API key for cloud providers. If empty, reads TOKENSAVER_API_KEY then OPENAI_API_KEY from env.
    /// Leave empty for local Ollama (no key required).
    pub api_key: String,

    /// Seconds to wait for a response before falling back to deterministic mode.
    pub timeout_secs: u64,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct PromptConfig {
    pub max_tokens: usize,
    pub include_snippets: bool,
    pub snippet_lines: usize,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct AnalyzerConfig {
    pub max_files: usize,
    pub max_symbols: usize,
    pub languages: Vec<String>,
    pub exclude: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct MemoryConfig {
    pub auto_inject: bool,
    pub max_facts: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm:      LlmConfig::default(),
            prompt:   PromptConfig::default(),
            analyzer: AnalyzerConfig::default(),
            memory:   MemoryConfig::default(),
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            enabled:      true,
            provider:     "ollama".into(),
            model:        "qwen2.5-coder:0.5b".into(),
            endpoint:     "http://localhost:11434".into(),
            api_key:      String::new(),
            timeout_secs: 30,
        }
    }
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            max_tokens:       8000,
            include_snippets: true,
            snippet_lines:    20,
        }
    }
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            max_files:   20,
            max_symbols: 50,
            languages:   vec!["typescript".into(), "javascript".into(), "python".into(), "rust".into(), "go".into()],
            exclude:     vec!["node_modules".into(), "dist".into(), "build".into(), ".git".into(), "target".into()],
        }
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            auto_inject: true,
            max_facts:   100,
        }
    }
}

pub fn load(repo_root: &Path) -> Result<Config> {
    let config_path = repo_root.join(".tokensaver").join("config.toml");

    if !config_path.exists() {
        tracing::debug!("no config file found, using defaults");
        return Ok(Config::default());
    }

    let raw = std::fs::read_to_string(&config_path)
        .map_err(|e| anyhow::anyhow!("failed to read config: {e}"))?;

    let config: Config = toml::from_str(&raw).unwrap_or_else(|e| {
        tracing::warn!("config parse error, using defaults: {e}");
        Config::default()
    });

    Ok(config)
}

pub fn default_toml() -> &'static str {
    r#"[llm]
enabled  = true
provider = "ollama"               # "ollama" for local | "openai" for any OpenAI-compatible API
model    = "qwen2.5-coder:0.5b"  # model name as the provider knows it
endpoint = "http://localhost:11434"
# api_key = ""                   # cloud only — or set TOKENSAVER_API_KEY / OPENAI_API_KEY env var
timeout_secs = 30

# ── Cloud example (uncomment to use OpenAI) ────────────────────────────────────
# provider = "openai"
# model    = "gpt-4o-mini"
# endpoint = "https://api.openai.com"
# api_key  = ""    # set OPENAI_API_KEY in your environment instead

[prompt]
max_tokens       = 8000  # token budget for injected context
include_snippets = true  # include short code excerpts alongside file paths
snippet_lines    = 20    # max lines per file snippet

[analyzer]
max_files   = 20
max_symbols = 50
languages   = ["typescript", "javascript", "python", "rust", "go"]
exclude     = ["node_modules", "dist", "build", ".git", "target"]

[memory]
auto_inject = true
max_facts   = 100
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn loads_defaults_when_no_file() {
        let dir = tempdir().unwrap();
        let config = load(dir.path()).unwrap();
        assert_eq!(config.prompt.max_tokens, 8000);
        assert_eq!(config.analyzer.max_files, 20);
        assert!(config.memory.auto_inject);
        assert!(config.llm.enabled);
        assert_eq!(config.llm.model, "qwen2.5-coder:0.5b");
        assert_eq!(config.llm.provider, "ollama");
    }

    #[test]
    fn loads_custom_llm_model() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".tokensaver")).unwrap();
        std::fs::write(
            dir.path().join(".tokensaver/config.toml"),
            "[llm]\nmodel = \"llama3.2\"\n",
        ).unwrap();
        let config = load(dir.path()).unwrap();
        assert_eq!(config.llm.model, "llama3.2");
        assert_eq!(config.prompt.max_tokens, 8000); // default preserved
    }

    #[test]
    fn loads_openai_provider() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".tokensaver")).unwrap();
        std::fs::write(
            dir.path().join(".tokensaver/config.toml"),
            "[llm]\nprovider = \"openai\"\nmodel = \"gpt-4o-mini\"\nendpoint = \"https://api.openai.com\"\n",
        ).unwrap();
        let config = load(dir.path()).unwrap();
        assert_eq!(config.llm.provider, "openai");
        assert_eq!(config.llm.model, "gpt-4o-mini");
    }

    #[test]
    fn falls_back_on_malformed_toml() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".tokensaver")).unwrap();
        std::fs::write(dir.path().join(".tokensaver/config.toml"), "NOT VALID ===").unwrap();
        let config = load(dir.path()).unwrap();
        assert_eq!(config.llm.model, "qwen2.5-coder:0.5b");
    }
}
