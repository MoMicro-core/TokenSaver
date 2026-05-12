mod input;
mod output;

pub use input::HookInput;

use anyhow::Result;

pub fn run() -> Result<()> {
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
        Ok(additional_context) => print!("{}", output::build(&additional_context)),
        Err(e) => {
            tracing::warn!("hook processing failed, emitting empty context: {e:#}");
            print!("{}", output::empty());
        }
    }

    Ok(())
}

fn process(input: &HookInput) -> Result<String> {
    let config = crate::config::load(&input.cwd)?;
    let facts   = crate::memory::store::load(&input.cwd)?;

    // Step 1 — fast deterministic scan: candidate files + symbols
    let candidates = crate::analyzer::analyze(&input.prompt, &input.cwd, &config)?;

    // Step 2 — local LLM decides what's truly relevant and structures the task
    let decision = crate::llm::decide(&input.prompt, &candidates, &facts, &config.llm);

    // Step 3 — persist LLM memory decisions (best-effort, never blocks the hook)
    if let Some(ref d) = decision {
        persist_memory_updates(d, &input.cwd);
    }

    // Step 4 — build additionalContext from LLM decision (or fall back to deterministic)
    let additional_context = crate::context::build(&candidates, &decision, &facts, &config);

    Ok(additional_context)
}

/// Applies memory updates from the LLM decision.
/// All operations are best-effort — a failure here never blocks the hook.
fn persist_memory_updates(decision: &crate::llm::LlmDecision, repo_root: &std::path::Path) {
    for fact in &decision.remember {
        if let Err(e) = crate::memory::store::append(repo_root, fact) {
            tracing::warn!("failed to remember fact: {e:#}");
        }
    }

    for id in &decision.forget_ids {
        if let Err(e) = crate::memory::store::remove(repo_root, id) {
            tracing::warn!("failed to forget memory id '{id}': {e:#}");
        }
    }

    if !decision.changelog_entry.is_empty() {
        if let Err(e) = crate::memory::changelog::append(repo_root, &decision.changelog_entry) {
            tracing::warn!("failed to append changelog: {e:#}");
        }
    }

    if !decision.task_plan.is_empty() {
        if let Err(e) = crate::memory::tasks::add(repo_root, &decision.task_plan, "") {
            tracing::warn!("failed to add task: {e:#}");
        }
    }
}
