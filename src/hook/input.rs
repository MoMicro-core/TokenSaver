use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct HookInput {
    pub prompt: String,
    pub cwd: PathBuf,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub hook_event_name: String,
    // Remaining Claude Code fields are ignored
}

pub fn read() -> Result<HookInput> {
    let mut raw = String::new();
    std::io::stdin().read_line(&mut raw)?;
    let input: HookInput = serde_json::from_str(raw.trim())?;
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_hook_input() {
        let json = r#"{
            "session_id": "abc123",
            "transcript_path": "/tmp/session.jsonl",
            "cwd": "/Users/dev/myproject",
            "permission_mode": "default",
            "hook_event_name": "UserPromptSubmit",
            "prompt": "fix login redirect"
        }"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.prompt, "fix login redirect");
        assert_eq!(input.session_id, "abc123");
    }

    #[test]
    fn parses_minimal_input() {
        let json = r#"{"prompt": "hello", "cwd": "/tmp"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.prompt, "hello");
    }
}
