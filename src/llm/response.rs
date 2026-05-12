use serde::Deserialize;

/// The structured decision returned by the local LLM after analyzing the prompt and candidates.
#[derive(Debug, Deserialize, Default)]
pub struct LlmDecision {
    /// File paths (relative to repo root) the LLM considers relevant.
    #[serde(default)]
    pub relevant_files: Vec<String>,

    /// A clear, structured restatement of the task for Claude Code.
    #[serde(default)]
    pub task_plan: String,

    /// Why these files and this approach — injected as context for Claude.
    #[serde(default)]
    pub reasoning: String,

    /// New facts to persist into memory.md (empty if nothing new to remember).
    #[serde(default)]
    pub remember: Vec<String>,

    /// IDs of memory facts that are now outdated and should be removed.
    #[serde(default)]
    pub forget_ids: Vec<String>,

    /// One-line summary appended to changelog.md.
    #[serde(default)]
    pub changelog_entry: String,
}

impl LlmDecision {
    /// Returns true if the LLM produced a usable response with at least a task plan.
    pub fn is_usable(&self) -> bool {
        !self.task_plan.is_empty()
    }
}
