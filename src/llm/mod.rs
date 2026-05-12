mod client;
mod prompt;
pub mod response;

pub use client::check_status;
pub use response::LlmDecision;

use crate::analyzer::AnalysisResult;
use crate::config::LlmConfig;
use crate::memory::store::Fact;

/// Asks the local LLM to decide which files are relevant and produce a task plan.
/// Falls back silently (returns None) if LLM is disabled, unreachable, or returns bad JSON.
pub fn decide(
    user_prompt: &str,
    candidates: &AnalysisResult,
    facts: &[Fact],
    config: &LlmConfig,
) -> Option<LlmDecision> {
    if !config.enabled {
        tracing::debug!("LLM disabled in config, skipping");
        return None;
    }

    let user_message = prompt::build_user_message(user_prompt, candidates, facts);

    match client::call(config, prompt::SYSTEM, &user_message) {
        Ok(decision) if decision.is_usable() => {
            tracing::debug!(files = ?decision.relevant_files, "LLM decision received");
            Some(decision)
        }
        Ok(_) => {
            tracing::warn!("LLM returned an empty task_plan, falling back to deterministic mode");
            None
        }
        Err(e) => {
            tracing::warn!("{e:#}");
            None
        }
    }
}
