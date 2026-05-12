use serde::Serialize;

#[derive(Serialize)]
pub struct HookOutput {
    #[serde(rename = "hookSpecificOutput")]
    pub hook_specific_output: HookSpecificOutput,
}

#[derive(Serialize)]
pub struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String,
    #[serde(rename = "additionalContext")]
    pub additional_context: String,
}

pub fn build(additional_context: &str) -> String {
    let output = HookOutput {
        hook_specific_output: HookSpecificOutput {
            hook_event_name: "UserPromptSubmit".to_string(),
            additional_context: additional_context.to_string(),
        },
    };
    serde_json::to_string(&output).unwrap_or_else(|_| empty())
}

pub fn empty() -> String {
    r#"{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":""}}"#
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_produces_valid_json() {
        let json = build("some context");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed["hookSpecificOutput"]["hookEventName"],
            "UserPromptSubmit"
        );
        assert_eq!(
            parsed["hookSpecificOutput"]["additionalContext"],
            "some context"
        );
    }

    #[test]
    fn empty_is_valid_json() {
        let json = empty();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["hookSpecificOutput"]["additionalContext"], "");
    }
}
