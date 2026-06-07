use crate::analyzer::AnalysisResult;
use crate::memory::store::Fact;

/// Cap on candidates passed to the LLM — smaller input = sharper focus for tiny models.
const MAX_CANDIDATES_IN_PROMPT: usize = 10;

// ── Request type pre-detection ────────────────────────────────────────────────
// Cheap regex-style heuristic. Used to bias the LLM toward the right shape of answer.

pub fn detect_request_type(prompt: &str) -> &'static str {
    let p = prompt.to_lowercase();
    match () {
        _ if p.contains("fix")    || p.contains("bug")  || p.contains("error")
                                  || p.contains("broken")|| p.contains("crash")     => "bug_fix",
        _ if p.contains("refactor")|| p.contains("clean")|| p.contains("simplify")
                                  || p.contains("rewrite")                          => "refactor",
        _ if p.contains("add")    || p.contains("create")|| p.contains("implement")
                                  || p.contains("new ")                             => "new_feature",
        _ if p.contains("combine")|| p.contains("merge") || p.contains("join")      => "merge",
        _ if p.contains("test")                                                     => "test",
        _ if p.contains("doc")    || p.contains("comment")|| p.contains("readme")   => "documentation",
        _                                                                            => "general",
    }
}

// ── Call 1: Context selection ─────────────────────────────────────────────────
// Three few-shot examples lock in the pattern for a 0.5B model.

pub const CONTEXT_SYSTEM: &str = "\
You are a code context selector for a coding assistant. \
Your only job: given a user request and a list of files, pick the relevant files and write a one-sentence task plan.\n\n\
HARD RULES:\n\
- Only return file paths that appear EXACTLY in available_files.\n\
- NEVER invent file paths.\n\
- NEVER suggest a file with a different extension than what's in available_files.\n\
- Respond with ONLY valid JSON. No prose, no markdown.\n\n\
EXAMPLE 1 (bug fix in Python):\n\
Input:  request=\"fix divide-by-zero in math_utils\", available_files=[\"math_utils.py\",\"main.py\",\"README.md\"]\n\
Output: {\"relevant_files\":[\"math_utils.py\"],\"task_plan\":\"Fix divide-by-zero error in math_utils.py.\"}\n\n\
EXAMPLE 2 (merge in Python — do NOT cross extensions):\n\
Input:  request=\"combine these 2 python files\", available_files=[\"src/a.py\",\"src/b.py\",\"src/main.ts\"]\n\
Output: {\"relevant_files\":[\"src/a.py\",\"src/b.py\"],\"task_plan\":\"Combine src/a.py and src/b.py into a single Python module.\"}\n\n\
EXAMPLE 3 (new feature in TypeScript):\n\
Input:  request=\"add a /health endpoint\", available_files=[\"src/server.ts\",\"src/routes.ts\",\"package.json\"]\n\
Output: {\"relevant_files\":[\"src/routes.ts\",\"src/server.ts\"],\"task_plan\":\"Add a /health endpoint to the routes module.\"}";

pub fn build_context_message(prompt: &str, candidates: &AnalysisResult) -> String {
    // Closed-world list — capped to keep prompt small
    let file_list: Vec<String> = candidates
        .files
        .iter()
        .take(MAX_CANDIDATES_IN_PROMPT)
        .map(|f| format!("\"{}\"", f.path.display()))
        .collect();

    let lang_hint    = detect_language_hint(candidates);
    let request_kind = detect_request_type(prompt);

    let mut parts = vec![
        format!("request: \"{}\"", escape_quotes(prompt)),
        format!("request_type: {}", request_kind),
        format!("available_files: [{}]", file_list.join(", ")),
    ];

    if !lang_hint.is_empty() {
        parts.push(format!(
            "language_constraint: project uses {}. Do not suggest files with other extensions.",
            lang_hint
        ));
    }

    parts.push(
        "Respond with ONLY this JSON (relevant_files MUST be a subset of available_files):\n\
        {\"relevant_files\":[\"path/from/list\"],\"task_plan\":\"One sentence.\"}".into(),
    );

    parts.join("\n")
}

// ── Call 2: Memory extraction ─────────────────────────────────────────────────

pub const MEMORY_SYSTEM: &str = "\
You are a memory extractor for a coding assistant. \
Your only job: extract a few atomic key-value facts worth remembering long-term.\n\n\
HARD RULES:\n\
- Write facts as short key: value pairs — NEVER full sentences.\n\
- Keep value under 10 words.\n\
- Only extract facts that are STABLE long-term project properties.\n\
- Skip transient details, obvious things, or anything specific to one task.\n\
- Return AT MOST 2 facts. Return [] if nothing is worth remembering.\n\
- Respond with ONLY valid JSON.\n\n\
BAD facts (never write these — too verbose or too transient):\n\
- {\"key\":\"task\",     \"value\":\"User wants to fix login redirect bug\"}\n\
- {\"key\":\"context\",  \"value\":\"The application uses JWT tokens for authentication and session management across the API layer\"}\n\
- {\"key\":\"todo\",     \"value\":\"Need to look at session.ts file\"}\n\n\
GOOD facts (concise + stable):\n\
- {\"key\":\"language\", \"value\":\"Python 3.11\",                       \"category\":\"stack\"}\n\
- {\"key\":\"auth\",     \"value\":\"JWT\",                               \"category\":\"conventions\"}\n\
- {\"key\":\"db\",       \"value\":\"PostgreSQL, no direct schema changes\",\"category\":\"constraints\"}";

pub fn build_memory_message(
    prompt: &str,
    task_plan: &str,
    facts: &[Fact],
) -> String {
    let mut parts = vec![
        format!("user_request: \"{}\"",  escape_quotes(prompt)),
        format!("task_context: \"{}\"",  escape_quotes(task_plan)),
    ];

    // Show existing memory keys (just the keys, not full text) to prevent duplicates
    if !facts.is_empty() {
        let keys: Vec<String> = facts
            .iter()
            .take(20)
            .filter_map(|f| f.text.split(':').next().map(|k| k.trim().to_string()))
            .filter(|k| !k.is_empty())
            .collect();
        if !keys.is_empty() {
            parts.push(format!("existing_keys: [{}]", keys.join(", ")));
            parts.push("note: do not duplicate keys already in existing_keys.".into());
        }
    }

    parts.push(
        r#"Respond with ONLY this JSON:
{
  "facts": [{"key":"short_key","value":"short value","category":"stack|conventions|constraints|decisions|bugs"}],
  "changelog": "One-line summary of what was done, or empty string if trivial."
}"#
        .into(),
    );

    parts.join("\n\n")
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn detect_language_hint(candidates: &AnalysisResult) -> String {
    use std::collections::HashMap;

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for f in &candidates.files {
        let ext = f.path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let lang = match ext {
            "py"                          => "Python (.py)",
            "ts" | "tsx"                  => "TypeScript (.ts/.tsx)",
            "js" | "jsx" | "mjs" | "cjs" => "JavaScript (.js/.jsx)",
            "rs"                          => "Rust (.rs)",
            "go"                          => "Go (.go)",
            _                             => continue,
        };
        *counts.entry(lang).or_insert(0) += 1;
    }

    let mut langs: Vec<(&str, usize)> = counts.into_iter().collect();
    langs.sort_by(|a, b| b.1.cmp(&a.1));
    langs.iter().map(|(l, _)| *l).collect::<Vec<_>>().join(" and ")
}

fn escape_quotes(s: &str) -> String {
    s.replace('"', "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_request_type_basic() {
        assert_eq!(detect_request_type("fix the bug in auth.py"),    "bug_fix");
        assert_eq!(detect_request_type("refactor the user model"),   "refactor");
        assert_eq!(detect_request_type("add a new endpoint"),        "new_feature");
        assert_eq!(detect_request_type("combine these 2 py files"),  "merge");
        assert_eq!(detect_request_type("write tests for auth"),      "test");
        assert_eq!(detect_request_type("update the readme"),         "documentation");
        assert_eq!(detect_request_type("what does this do?"),        "general");
    }
}
