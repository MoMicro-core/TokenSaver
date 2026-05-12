// Token estimation using a character-based heuristic.
// cl100k_base (Claude/GPT-4) averages ~3.5 chars per token for mixed code+English.
// This is accurate enough for budget enforcement — within ~15% of the real count.
// Upgrade to tiktoken-rs for exact counts if needed.

const CHARS_PER_TOKEN: f32 = 3.5;

pub fn count(text: &str) -> usize {
    ((text.len() as f32) / CHARS_PER_TOKEN).ceil() as usize
}

pub fn fits_budget(text: &str, budget: usize) -> bool {
    count(text) <= budget
}

/// Truncates `text` to fit within `budget` tokens, breaking at a newline boundary.
/// Appends a notice so Claude knows context was trimmed.
pub fn truncate_to_budget(text: &str, budget: usize) -> String {
    if fits_budget(text, budget) {
        return text.to_string();
    }

    // Reserve tokens for the truncation notice
    let notice = "\n\n[Context truncated to fit token budget]";
    let notice_tokens = count(notice);
    let target_tokens = budget.saturating_sub(notice_tokens);
    let char_limit = (target_tokens as f32 * CHARS_PER_TOKEN) as usize;

    let truncated = if char_limit >= text.len() {
        text
    } else {
        &text[..char_limit]
    };

    // Break at last newline for cleaner output
    let clean_cut = truncated.rfind('\n').unwrap_or(truncated.len());
    format!("{}{}", &truncated[..clean_cut], notice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_is_proportional_to_length() {
        let short = "hello";
        let long = "hello ".repeat(100);
        assert!(count(&long) > count(short));
    }

    #[test]
    fn fits_budget_true_for_short_text() {
        assert!(fits_budget("short text", 1000));
    }

    #[test]
    fn fits_budget_false_for_long_text() {
        let long = "word ".repeat(10_000);
        assert!(!fits_budget(&long, 100));
    }

    #[test]
    fn truncate_respects_budget() {
        let long = "token ".repeat(5_000);
        let truncated = truncate_to_budget(&long, 100);
        assert!(count(&truncated) <= 100 + 20); // small margin for notice
    }

    #[test]
    fn truncate_is_noop_within_budget() {
        let text = "short enough text";
        let result = truncate_to_budget(text, 1000);
        assert_eq!(result, text);
    }

    #[test]
    fn truncate_appends_notice() {
        let long = "x ".repeat(10_000);
        let result = truncate_to_budget(&long, 50);
        assert!(result.contains("[Context truncated"));
    }
}
