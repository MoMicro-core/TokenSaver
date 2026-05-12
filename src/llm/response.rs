use serde::Deserialize;

/// Call 1 result: which files are relevant + what the task is.
#[derive(Debug, Deserialize, Default)]
pub struct ContextDecision {
    /// File paths chosen by the LLM. Validated post-parse: any path not in the
    /// candidate list is silently dropped — the model cannot hallucinate new files.
    #[serde(default)]
    pub relevant_files: Vec<String>,

    /// One-sentence task description. Used as the "Task:" header Claude receives.
    #[serde(default)]
    pub task_plan: String,
}

/// A single atomic fact extracted from the session.
/// Stored as `key: value` — never as a full sentence.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct MemoryFact {
    /// Short snake_case identifier. Examples: `language`, `db_driver`, `auth_method`.
    #[serde(default)]
    pub key: String,

    /// Short concrete value. Max ~80 chars. Examples: `Python 3.11`, `PostgreSQL`, `JWT`.
    #[serde(default)]
    pub value: String,

    /// One of: `stack` | `conventions` | `constraints` | `decisions` | `bugs` | `general`.
    #[serde(default)]
    pub category: String,
}

/// Allowed categories. Anything else falls back to `general`.
const VALID_CATEGORIES: &[&str] = &[
    "stack", "conventions", "constraints", "decisions", "bugs", "general",
];

/// Max length of a fact value (chars). Anything longer is truncated at a word boundary.
const MAX_VALUE_CHARS: usize = 80;

impl MemoryFact {
    /// Normalises a fact in-place: snake_case key, valid category, truncated value.
    /// Required before persisting — small models output unpredictable shapes.
    pub fn sanitize(&mut self) {
        self.key = to_snake_case(&self.key);
        self.value = truncate_value(self.value.trim());
        if !VALID_CATEGORIES.contains(&self.category.as_str()) {
            self.category = "general".to_string();
        }
    }

    /// True if both key and value are present after sanitisation.
    pub fn is_valid(&self) -> bool {
        !self.key.is_empty() && !self.value.is_empty()
    }

    /// String form stored in memory.md.
    pub fn to_memory_string(&self) -> String {
        format!("{}: {}", self.key, self.value)
    }
}

/// Converts arbitrary text to snake_case: lowercase, alphanumerics joined by underscores.
fn to_snake_case(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Truncates to MAX_VALUE_CHARS at the last word boundary that fits.
fn truncate_value(s: &str) -> String {
    if s.len() <= MAX_VALUE_CHARS {
        return s.to_string();
    }
    let cut = &s[..MAX_VALUE_CHARS];
    match cut.rfind(' ') {
        Some(pos) => cut[..pos].trim_end().to_string(),
        None => cut.to_string(),
    }
}

/// Call 2 result: what to remember + changelog line.
/// Note: forget_ids was removed — letting a 0.5B model delete memory was too risky.
/// Deduplication is now handled mechanically by key in `memory::store::upsert_by_key`.
#[derive(Debug, Deserialize, Default)]
pub struct MemoryDecision {
    #[serde(default)]
    pub facts: Vec<MemoryFact>,

    #[serde(default)]
    pub changelog: String,
}

/// Combined result returned to the hook after both LLM calls.
#[derive(Debug, Default)]
pub struct LlmDecision {
    pub relevant_files: Vec<String>,
    pub task_plan: String,
    pub new_facts: Vec<MemoryFact>,
    pub changelog: String,
}

impl LlmDecision {
    pub fn is_usable(&self) -> bool {
        !self.task_plan.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_snake_case_basic() {
        assert_eq!(to_snake_case("Auth Method"), "auth_method");
        assert_eq!(to_snake_case("db-driver"),  "db_driver");
        assert_eq!(to_snake_case("CamelCase"),  "camelcase");
        assert_eq!(to_snake_case("  spaced  "), "spaced");
    }

    #[test]
    fn truncate_at_word_boundary() {
        let long = "a".repeat(120);
        let cut = truncate_value(&long);
        assert!(cut.len() <= MAX_VALUE_CHARS);
    }

    #[test]
    fn sanitize_normalises_invalid_category() {
        let mut f = MemoryFact {
            key: "Auth Method".into(),
            value: "JWT".into(),
            category: "nonsense".into(),
        };
        f.sanitize();
        assert_eq!(f.key, "auth_method");
        assert_eq!(f.category, "general");
        assert!(f.is_valid());
    }

    #[test]
    fn invalid_fact_detected() {
        let mut f = MemoryFact {
            key: "".into(),
            value: "something".into(),
            category: "stack".into(),
        };
        f.sanitize();
        assert!(!f.is_valid());
    }
}
