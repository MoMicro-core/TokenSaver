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
    /// Enable local LLM via Ollama. Falls back to deterministic mode if false or Ollama unreachable.
    pub enabled: bool,
    /// Ollama model name. Must be pulled with `ollama pull <model>`.
    pub model: String,
    /// Ollama server endpoint.
    pub endpoint: String,
    /// Seconds to wait for the local LLM before falling back to deterministic mode.
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
            llm: LlmConfig::default(),
            prompt: PromptConfig::default(),
            analyzer: AnalyzerConfig::default(),
            memory: MemoryConfig::default(),
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            model: "qwen2.5-coder:0.5b".into(),
            endpoint: "http://localhost:11434".into(),
            timeout_secs: 30,
        }
    }
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            max_tokens: 8000,
            include_snippets: true,
            snippet_lines: 20,
        }
    }
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            max_files: 20,
            max_symbols: 50,
            languages: vec![
                "typescript".into(),
                "javascript".into(),
                "python".into(),
                "rust".into(),
                "go".into(),
            ],
            exclude: vec![
                "node_modules".into(),
                "dist".into(),
                "build".into(),
                ".git".into(),
                "target".into(),
            ],
        }
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            auto_inject: true,
            max_facts: 100,
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
enabled = true
model = "qwen2.5-coder:0.5b"   # any model available in your Ollama installation
endpoint = "http://localhost:11434"
timeout_secs = 30

[prompt]
max_tokens = 8000         # token budget for injected context
include_snippets = true   # include short code excerpts alongside file paths
snippet_lines = 20        # max lines per file snippet

[analyzer]
max_files = 20
max_symbols = 50
languages = ["typescript", "javascript", "python", "rust", "go"]
exclude = ["node_modules", "dist", "build", ".git", "target"]

[memory]
auto_inject = true
max_facts = 100
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
    }

    #[test]
    fn loads_custom_llm_model() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".tokensaver")).unwrap();
        std::fs::write(
            dir.path().join(".tokensaver/config.toml"),
            "[llm]\nmodel = \"llama3.2\"\n",
        )
        .unwrap();
        let config = load(dir.path()).unwrap();
        assert_eq!(config.llm.model, "llama3.2");
        assert_eq!(config.prompt.max_tokens, 8000); // default preserved
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
