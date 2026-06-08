mod input;
mod output;

pub use input::HookInput;

use anyhow::Result;
use std::path::Path;

pub fn run() -> Result<()> {
    // Silence the memory store's interactive println — stdout is reserved for hook JSON
    std::env::set_var("TOKENSAVER_SILENT", "1");

    let input = match input::read() {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!("failed to parse hook input: {e:#}");
            print!("{}", output::empty());
            return Ok(());
        }
    };

    tracing::debug!(prompt = %input.prompt, cwd = %input.cwd.display(), "hook fired");

    match process(&input) {
        Ok(ctx) => print!("{}", output::build(&ctx)),
        Err(e) => {
            tracing::warn!("hook processing failed, emitting empty context: {e:#}");
            print!("{}", output::empty());
        }
    }

    Ok(())
}

fn process(input: &HookInput) -> Result<String> {
    let config     = crate::config::load(&input.cwd)?;
    let facts      = crate::memory::store::load(&input.cwd)?;
    let candidates = crate::analyzer::analyze(&input.prompt, &input.cwd, &config)?;
    let decision   = crate::llm::decide(&input.prompt, &candidates, &facts, &config.llm);

    if let Some(ref d) = decision {
        persist_memory_updates(d, &input.cwd);
    }

    let ctx = crate::context::build(&candidates, &decision, &facts, &config);

    // If this project is missing essential setup files, tell Claude to create them first.
    Ok(prepend_setup_warning(&input.cwd, ctx))
}

/// If the project has been initialised (`.tokensaver/` exists) but is missing
/// `CLAUDE.md` or a PRD file, injects a priority instruction so Claude creates
/// them before working on the user's actual task.
///
/// Only fires when `.tokensaver/` is present — avoids bothering users who have
/// not explicitly opted into TokenSaver for this repo.
fn prepend_setup_warning(repo_root: &Path, ctx: String) -> String {
    // Only check initialised projects
    if !repo_root.join(".tokensaver").exists() {
        return ctx;
    }

    let missing = missing_project_files(repo_root);
    if missing.is_empty() {
        return ctx;
    }

    let file_list = missing
        .iter()
        .map(|f| format!("  - {}", f.name))
        .collect::<Vec<_>>()
        .join("\n");

    let instructions = missing
        .iter()
        .map(|f| format!("  - {}", f.instruction))
        .collect::<Vec<_>>()
        .join("\n");

    let warning = format!(
        "⚠️  PROJECT SETUP INCOMPLETE\n\n\
         The following essential files are missing from this repository:\n{file_list}\n\n\
         Please create these files BEFORE working on the original task:\n{instructions}\n\n\
         These files help Claude understand your project and give better answers on every \
         subsequent prompt."
    );

    if ctx.is_empty() {
        warning
    } else {
        format!("{warning}\n\n---\n\n{ctx}")
    }
}

struct MissingFile {
    name:        &'static str,
    instruction: &'static str,
}

/// Returns which essential project files are absent.
fn missing_project_files(repo_root: &Path) -> Vec<MissingFile> {
    let mut missing = Vec::new();

    if !repo_root.join("CLAUDE.md").exists() {
        missing.push(MissingFile {
            name:        "CLAUDE.md",
            instruction: "CLAUDE.md — create with build commands, architecture overview, and coding conventions",
        });
    }

    // Accept any common PRD location / filename
    let prd_candidates = [
        "PRD.md", "prd.md", "docs/PRD.md", "docs/prd.md",
        "PRODUCT.md", "REQUIREMENTS.md", "requirements.md",
    ];
    if !prd_candidates.iter().any(|p| repo_root.join(p).exists()) {
        missing.push(MissingFile {
            name:        "PRD.md",
            instruction: "PRD.md — create with project goals, features, and technical requirements",
        });
    }

    missing
}

/// Persists only what's substantive. Memory writes are best-effort —
/// any failure is logged but never blocks the hook.
fn persist_memory_updates(decision: &crate::llm::LlmDecision, repo_root: &Path) {

    // ── Facts: dedup by key via upsert (never duplicates, never deletes) ──────
    for fact in &decision.new_facts {
        if let Err(e) = crate::memory::store::upsert_by_key(
            repo_root, &fact.key, &fact.value, &fact.category,
        ) {
            tracing::warn!("failed to upsert fact '{}': {e:#}", fact.key);
        }
    }

    // ── Changelog: only write if the LLM produced something substantive ───────
    if is_substantive_changelog(&decision.changelog) {
        if let Err(e) = crate::memory::changelog::append(repo_root, &decision.changelog) {
            tracing::warn!("failed to append changelog: {e:#}");
        }
    } else {
        tracing::debug!("skipping non-substantive changelog: {:?}", decision.changelog);
    }

    // NOTE: tasks.jsonl is no longer written automatically on every prompt.
    // Use `tokensaver remember` or update tasks manually through the CLI.
}

/// Filters out short, generic, or echoing changelog entries that would pollute the file.
fn is_substantive_changelog(text: &str) -> bool {
    let t = text.trim();
    if t.len() < 15                 { return false; }
    if t.len() > 200                { return false; }
    let lower = t.to_lowercase();
    if lower.starts_with("the user") || lower.starts_with("user ")    { return false; }
    if lower.starts_with("no ")      || lower.contains("nothing")     { return false; }
    if lower == "n/a" || lower == "none" || lower == "skip"           { return false; }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn substantive_changelog_accepts_normal_entry() {
        assert!(is_substantive_changelog("Fixed JWT session expiry redirect in auth middleware."));
    }

    #[test]
    fn substantive_changelog_rejects_garbage() {
        assert!(!is_substantive_changelog(""));
        assert!(!is_substantive_changelog("ok"));
        assert!(!is_substantive_changelog("n/a"));
        assert!(!is_substantive_changelog("The user asked about fixing things"));
        assert!(!is_substantive_changelog("nothing to do"));
    }

    #[test]
    fn no_warning_when_not_initialized() {
        let dir = tempdir().unwrap();
        // No .tokensaver/ dir — warning must be suppressed
        let result = prepend_setup_warning(dir.path(), "some context".into());
        assert_eq!(result, "some context");
    }

    #[test]
    fn warns_when_both_files_missing() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".tokensaver")).unwrap();
        let result = prepend_setup_warning(dir.path(), String::new());
        assert!(result.contains("CLAUDE.md"));
        assert!(result.contains("PRD.md"));
        assert!(result.contains("⚠️"));
    }

    #[test]
    fn warns_only_for_missing_files() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".tokensaver")).unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "# Project").unwrap();
        let result = prepend_setup_warning(dir.path(), String::new());
        assert!(!result.contains("CLAUDE.md"));
        assert!(result.contains("PRD.md"));
    }

    #[test]
    fn no_warning_when_all_files_present() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".tokensaver")).unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "# Project").unwrap();
        std::fs::write(dir.path().join("PRD.md"), "# Requirements").unwrap();
        let result = prepend_setup_warning(dir.path(), "context here".into());
        assert_eq!(result, "context here");
    }

    #[test]
    fn warning_prepended_before_existing_context() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".tokensaver")).unwrap();
        let result = prepend_setup_warning(dir.path(), "context here".into());
        assert!(result.starts_with("⚠️"));
        assert!(result.contains("context here"));
    }
}
