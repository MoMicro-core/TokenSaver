use crate::analyzer::AnalysisResult;
use crate::memory::store::Fact;

/// System prompt sent to the local LLM.
/// Kept deliberately short — Qwen2.5-Coder-0.5B performs better with concise instructions.
pub const SYSTEM: &str = "\
You are a context router for a coding assistant. \
Given a user request, candidate files from the repository, and project memory, \
you decide which files are truly relevant and produce a structured task plan. \
Always respond with valid JSON only — no markdown, no explanation outside the JSON.";

/// Builds the user-turn message sent to the local LLM.
pub fn build_user_message(
    prompt: &str,
    candidates: &AnalysisResult,
    facts: &[Fact],
) -> String {
    let mut parts: Vec<String> = Vec::new();

    parts.push(format!("User request: \"{prompt}\""));

    // Candidate files with scores and snippets
    if !candidates.files.is_empty() {
        let file_lines: Vec<String> = candidates
            .files
            .iter()
            .map(|f| {
                let mut line = format!("- {} (score: {:.1})", f.path.display(), f.relevance_score);
                if let Some(snippet) = &f.snippet {
                    let first_two: Vec<&str> = snippet.lines().take(2).collect();
                    line.push_str(&format!("\n  {}", first_two.join(" | ")));
                }
                line
            })
            .collect();
        parts.push(format!("Candidate files:\n{}", file_lines.join("\n")));
    } else {
        parts.push("Candidate files: none found".into());
    }

    // Relevant symbols
    if !candidates.symbols.is_empty() {
        let sym_lines: Vec<String> = candidates
            .symbols
            .iter()
            .take(10) // cap to keep prompt small for 0.5B model
            .map(|s| format!("- {}() [{}:{}]", s.name, s.file.display(), s.line))
            .collect();
        parts.push(format!("Known symbols:\n{}", sym_lines.join("\n")));
    }

    // Project memory
    if !facts.is_empty() {
        let fact_lines: Vec<String> = facts
            .iter()
            .map(|f| format!("- [{}] {}", f.id, f.text))
            .collect();
        parts.push(format!("Project memory:\n{}", fact_lines.join("\n")));
    }

    // Output schema — explicit for small models
    parts.push(
        r#"Respond with only this JSON (no other text):
{
  "relevant_files": ["relative/path/to/file.ts"],
  "task_plan": "Clear description of what needs to be done.",
  "reasoning": "Why these files are relevant.",
  "remember": ["New important fact about the project, if any."],
  "forget_ids": ["id_of_outdated_memory_fact_if_any"],
  "changelog_entry": "One-line summary of this task."
}"#
        .into(),
    );

    parts.join("\n\n")
}
