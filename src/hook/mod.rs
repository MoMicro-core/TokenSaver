mod input;
mod output;

pub use input::HookInput;

use anyhow::Result;

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

    Ok(crate::context::build(&candidates, &decision, &facts, &config))
}

/// Persists only what's substantive. Memory writes are best-effort —
/// any failure is logged but never blocks the hook.
fn persist_memory_updates(decision: &crate::llm::LlmDecision, repo_root: &std::path::Path) {

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
    // Use `tokensaver remember` or update tasks manually through CLI.
}

/// Filters out short, generic, or echoing changelog entries that would pollute the file.
fn is_substantive_changelog(text: &str) -> bool {
    let t = text.trim();
    if t.len() < 15                 { return false; }   // too short
    if t.len() > 200                { return false; }   // suspicious — likely a paragraph
    let lower = t.to_lowercase();
    if lower.starts_with("the user") || lower.starts_with("user ")    { return false; }
    if lower.starts_with("no ")      || lower.contains("nothing")     { return false; }
    if lower == "n/a" || lower == "none" || lower == "skip"           { return false; }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
