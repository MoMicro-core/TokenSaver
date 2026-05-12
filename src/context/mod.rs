use crate::analyzer::AnalysisResult;
use crate::config::Config;
use crate::llm::LlmDecision;
use crate::memory::store::Fact;

/// Builds the `additionalContext` string injected by Claude Code alongside the user's prompt.
///
/// When the local LLM is available its structured decision is used — task plan, reasoning,
/// and LLM-selected files take priority. When the LLM is unavailable the deterministic
/// analysis results are used as a fallback so the hook always produces useful context.
pub fn build(
    candidates: &AnalysisResult,
    decision: &Option<LlmDecision>,
    facts: &[Fact],
    config: &Config,
) -> String {
    let mut sections: Vec<String> = Vec::new();

    match decision {
        Some(d) => build_from_llm_decision(d, candidates, facts, &mut sections),
        None    => build_from_deterministic(candidates, facts, config, &mut sections),
    }

    if sections.is_empty() {
        return String::new();
    }

    let assembled = sections.join("\n\n");
    apply_token_budget(assembled, config.prompt.max_tokens)
}

// ── LLM-driven path ──────────────────────────────────────────────────────────

fn build_from_llm_decision(
    decision: &LlmDecision,
    candidates: &AnalysisResult,
    facts: &[Fact],
    sections: &mut Vec<String>,
) {
    sections.push(format!("Task:\n{}", decision.task_plan));

    // Use LLM-selected files; if it returned none, fall back to top candidates
    let file_list = if !decision.relevant_files.is_empty() {
        decision.relevant_files
            .iter()
            .map(|f| format!("- {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        candidates.files
            .iter()
            .map(|f| format!("- {}", f.path.display()))
            .collect::<Vec<_>>()
            .join("\n")
    };

    if !file_list.is_empty() {
        sections.push(format!("Relevant Files:\n{file_list}"));
    }

    // Symbols from deterministic analysis (LLM doesn't extract these directly)
    if !candidates.symbols.is_empty() {
        let sym_lines = candidates.symbols
            .iter()
            .map(|s| format!("- {}() [{}:{}]", s.name, s.file.display(), s.line))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("Relevant Symbols:\n{sym_lines}"));
    }

    if !facts.is_empty() {
        let constraints = crate::memory::inject::format(facts);
        sections.push(format!("Constraints (from project memory):\n{constraints}"));
    }

    sections.push(
        "Instructions:\n\
        Work only with the listed files first.\n\
        Explain before editing any file not listed above.\n\
        Avoid unrelated refactors."
            .into(),
    );
}

// ── Deterministic fallback path ───────────────────────────────────────────────

fn build_from_deterministic(
    candidates: &AnalysisResult,
    facts: &[Fact],
    config: &Config,
    sections: &mut Vec<String>,
) {
    if !candidates.files.is_empty() {
        let file_lines = candidates.files
            .iter()
            .map(|f| {
                if config.prompt.include_snippets {
                    if let Some(snippet) = &f.snippet {
                        return format!("- {}\n{}", f.path.display(), indent(snippet, "  "));
                    }
                }
                format!("- {}", f.path.display())
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("Relevant Files:\n{file_lines}"));
    }

    if !candidates.symbols.is_empty() {
        let sym_lines = candidates.symbols
            .iter()
            .map(|s| format!("- {}() [{}:{}]", s.name, s.file.display(), s.line))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("Relevant Symbols:\n{sym_lines}"));
    }

    if config.memory.auto_inject && !facts.is_empty() {
        sections.push(format!(
            "Project Memory:\n{}",
            crate::memory::inject::format(facts)
        ));
    }

    if !sections.is_empty() {
        sections.push(
            "Instructions:\n\
            Inspect only the listed files first.\n\
            Avoid unrelated refactors.\n\
            If additional files are needed, explain why before editing."
                .into(),
        );
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn indent(text: &str, prefix: &str) -> String {
    text.lines()
        .map(|l| format!("{prefix}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn apply_token_budget(text: String, max_tokens: usize) -> String {
    if crate::tokens::fits_budget(&text, max_tokens) {
        text
    } else {
        crate::tokens::truncate_to_budget(&text, max_tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::{AnalysisResult, RelevantFile, Symbol, SymbolKind};
    use crate::llm::response::LlmDecision;
    use crate::memory::store::Fact;
    use std::path::PathBuf;

    fn make_fact(text: &str) -> Fact {
        Fact { id: "abc".into(), category: "constraints".into(), text: text.into() }
    }

    fn make_file(path: &str) -> RelevantFile {
        RelevantFile {
            path: PathBuf::from(path),
            abs_path: PathBuf::from(path),
            relevance_score: 10.0,
            snippet: None,
        }
    }

    fn make_symbol(name: &str) -> Symbol {
        Symbol {
            name: name.into(),
            kind: SymbolKind::Function,
            file: PathBuf::from("src/auth.ts"),
            line: 12,
        }
    }

    #[test]
    fn llm_path_includes_task_plan_and_reasoning() {
        let candidates = AnalysisResult {
            files: vec![make_file("src/auth.ts")],
            symbols: vec![make_symbol("validateSession")],
        };
        let decision = Some(LlmDecision {
            task_plan: "Fix the session expiry redirect.".into(),
            relevant_files: vec!["src/auth.ts".into()],
            reasoning: "Session module handles JWT expiry.".into(),
            ..Default::default()
        });
        let ctx = build(&candidates, &decision, &[], &Config::default());
        assert!(ctx.contains("Task:"));
        assert!(ctx.contains("Fix the session expiry redirect."));
        assert!(ctx.contains("Reasoning:"));
        assert!(ctx.contains("validateSession"));
    }

    #[test]
    fn deterministic_fallback_used_when_no_decision() {
        let candidates = AnalysisResult {
            files: vec![make_file("src/auth.ts")],
            symbols: vec![],
        };
        let ctx = build(&candidates, &None, &[make_fact("Auth uses JWT")], &Config::default());
        assert!(ctx.contains("Relevant Files:"));
        assert!(ctx.contains("Project Memory:"));
        assert!(ctx.contains("Auth uses JWT"));
    }

    #[test]
    fn empty_when_no_data_at_all() {
        let ctx = build(&AnalysisResult::default(), &None, &[], &Config::default());
        assert!(ctx.is_empty());
    }
}
