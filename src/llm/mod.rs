mod client;
mod prompt;
pub mod response;

pub use client::check_status;
pub use response::{LlmDecision, MemoryFact};

use crate::analyzer::AnalysisResult;
use crate::config::LlmConfig;
use crate::memory::store::Fact;
use response::{ContextDecision, MemoryDecision};
use std::collections::HashSet;

/// Max facts persisted per prompt. Tiny models tend to spam — keep memory tight.
const MAX_FACTS_PER_CALL: usize = 2;

/// Runs two sequential single-task LLM calls and combines their results.
///
/// Call 1 — context selection: which files + task plan.
/// Call 2 — memory extraction: atomic key-value facts + changelog line.
///
/// Returns None if LLM is disabled, unreachable, or Call 1 produces no task plan.
/// Call 2 is best-effort and never blocks the hook.
pub fn decide(
    user_prompt: &str,
    candidates: &AnalysisResult,
    facts: &[Fact],
    config: &LlmConfig,
) -> Option<LlmDecision> {
    if !config.enabled {
        tracing::debug!("LLM disabled in config");
        return None;
    }

    // ── Call 1: context selection ─────────────────────────────────────────────
    let ctx = call_context(user_prompt, candidates, config)?;

    if ctx.task_plan.trim().is_empty() {
        tracing::warn!("LLM returned empty task_plan, falling back to deterministic mode");
        return None;
    }

    let valid_files = validate_files(ctx.relevant_files, candidates);
    tracing::debug!(files = ?valid_files, plan = %ctx.task_plan, "context decision");

    // ── Call 2: memory extraction (best-effort) ───────────────────────────────
    let mem = call_memory(user_prompt, &ctx.task_plan, facts, config)
        .unwrap_or_default();

    let new_facts = sanitize_facts(mem.facts);
    tracing::debug!(
        facts = ?new_facts.iter().map(|f| f.to_memory_string()).collect::<Vec<_>>(),
        "memory decision (sanitized + capped)"
    );

    Some(LlmDecision {
        relevant_files: valid_files,
        task_plan:      ctx.task_plan,
        new_facts,
        changelog:      mem.changelog,
    })
}

/// Same as `decide` but returns raw outputs alongside the final decision —
/// for the `tokensaver debug` subcommand.
pub fn decide_with_trace(
    user_prompt: &str,
    candidates: &AnalysisResult,
    facts: &[Fact],
    config: &LlmConfig,
) -> DebugTrace {
    let request_type = prompt::detect_request_type(user_prompt);
    let ctx_msg      = prompt::build_context_message(user_prompt, candidates);

    let ctx            = call_context(user_prompt, candidates, config);
    let ctx_files_raw  = ctx.as_ref().map(|c| c.relevant_files.clone()).unwrap_or_default();
    let ctx_plan_raw   = ctx.as_ref().map(|c| c.task_plan.clone()).unwrap_or_default();
    let validated_files = validate_files(ctx_files_raw.clone(), candidates);

    let mem_msg = if ctx_plan_raw.is_empty() {
        String::new()
    } else {
        prompt::build_memory_message(user_prompt, &ctx_plan_raw, facts)
    };

    let mem = if ctx_plan_raw.is_empty() {
        None
    } else {
        call_memory(user_prompt, &ctx_plan_raw, facts, config)
    };

    let raw_facts: Vec<MemoryFact> = mem.as_ref().map(|m| m.facts.clone()).unwrap_or_default();
    let sanitized_facts = sanitize_facts(raw_facts.clone());

    DebugTrace {
        request_type: request_type.to_string(),
        candidates_in_prompt: candidates.files.iter().take(10)
            .map(|f| f.path.to_string_lossy().to_string()).collect(),
        ctx_message: ctx_msg,
        ctx_files_raw,
        ctx_plan_raw,
        validated_files,
        dropped_files: vec![],
        mem_message: mem_msg,
        raw_facts,
        sanitized_facts,
        changelog: mem.as_ref().map(|m| m.changelog.clone()).unwrap_or_default(),
    }
}

pub struct DebugTrace {
    pub request_type: String,
    pub candidates_in_prompt: Vec<String>,
    pub ctx_message: String,
    pub ctx_files_raw: Vec<String>,
    pub ctx_plan_raw: String,
    pub validated_files: Vec<String>,
    pub dropped_files: Vec<String>,
    pub mem_message: String,
    pub raw_facts: Vec<MemoryFact>,
    pub sanitized_facts: Vec<MemoryFact>,
    pub changelog: String,
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Sanitises, validates, and caps raw facts from the LLM to MAX_FACTS_PER_CALL.
fn sanitize_facts(raw: Vec<MemoryFact>) -> Vec<MemoryFact> {
    raw.into_iter()
        .map(|mut f| { f.sanitize(); f })
        .filter(|f| f.is_valid())
        .take(MAX_FACTS_PER_CALL)
        .collect()
}

fn call_context(
    prompt: &str,
    candidates: &AnalysisResult,
    config: &LlmConfig,
) -> Option<ContextDecision> {
    let msg = self::prompt::build_context_message(prompt, candidates);
    match client::call(config, self::prompt::CONTEXT_SYSTEM, &msg) {
        Ok(d)  => Some(d),
        Err(e) => { tracing::warn!("context call failed: {e:#}"); None }
    }
}

fn call_memory(
    prompt: &str,
    task_plan: &str,
    facts: &[Fact],
    config: &LlmConfig,
) -> Option<MemoryDecision> {
    let msg = self::prompt::build_memory_message(prompt, task_plan, facts);
    match client::call(config, self::prompt::MEMORY_SYSTEM, &msg) {
        Ok(d)  => Some(d),
        Err(e) => { tracing::warn!("memory call failed: {e:#}"); None }
    }
}

/// Keeps only file paths that appeared in the candidate list.
/// Falls back to the top-3 candidates if the LLM returned no valid paths.
fn validate_files(llm_files: Vec<String>, candidates: &AnalysisResult) -> Vec<String> {
    let valid_paths: HashSet<String> = candidates.files
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();

    let validated: Vec<String> = llm_files
        .into_iter()
        .filter(|f| {
            let keep = valid_paths.contains(f);
            if !keep { tracing::debug!("dropping hallucinated file: {f}"); }
            keep
        })
        .collect();

    if validated.is_empty() && !candidates.files.is_empty() {
        tracing::debug!("no valid files from LLM, using top-3 candidates");
        return candidates.files.iter().take(3)
            .map(|f| f.path.to_string_lossy().to_string()).collect();
    }
    validated
}
