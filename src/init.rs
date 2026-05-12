use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

pub fn run(repo_root: &Path) -> Result<()> {
    create_tokensaver_dir(repo_root)?;
    write_config(repo_root)?;
    write_memory(repo_root)?;
    patch_claude_settings(repo_root)?;

    println!("TokenSaver initialized in {}", repo_root.display());
    println!();
    println!("Add project facts:  tokensaver remember \"<fact>\"");
    println!("Preview context:    tokensaver context \"<your query>\"");
    println!("Open Claude Code and every prompt will be enriched automatically.");
    Ok(())
}

fn create_tokensaver_dir(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".tokensaver");
    if dir.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create {}", dir.display()))?;
    Ok(())
}

fn write_config(repo_root: &Path) -> Result<()> {
    let path = repo_root.join(".tokensaver/config.toml");
    if path.exists() {
        println!("  skipped: .tokensaver/config.toml (already exists)");
        return Ok(());
    }
    std::fs::write(&path, crate::config::default_toml())
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("  created: .tokensaver/config.toml");
    Ok(())
}

fn write_memory(repo_root: &Path) -> Result<()> {
    let path = repo_root.join(".tokensaver/memory.md");
    if path.exists() {
        println!("  skipped: .tokensaver/memory.md (already exists)");
        return Ok(());
    }
    std::fs::write(&path, "")
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("  created: .tokensaver/memory.md");

    // Also create changelog and tasks on first init
    let changelog = repo_root.join(".tokensaver/changelog.md");
    if !changelog.exists() {
        std::fs::write(&changelog, "")
            .with_context(|| format!("failed to write {}", changelog.display()))?;
        println!("  created: .tokensaver/changelog.md");
    }

    let tasks = repo_root.join(".tokensaver/tasks.jsonl");
    if !tasks.exists() {
        std::fs::write(&tasks, "")
            .with_context(|| format!("failed to write {}", tasks.display()))?;
        println!("  created: .tokensaver/tasks.jsonl");
    }

    Ok(())
}

fn patch_claude_settings(repo_root: &Path) -> Result<()> {
    let claude_dir = repo_root.join(".claude");
    if !claude_dir.exists() {
        std::fs::create_dir_all(&claude_dir)
            .with_context(|| format!("failed to create {}", claude_dir.display()))?;
    }

    let settings_path = claude_dir.join("settings.json");

    let mut settings: Value = if settings_path.exists() {
        let raw = std::fs::read_to_string(&settings_path)
            .with_context(|| format!("failed to read {}", settings_path.display()))?;
        serde_json::from_str(&raw).unwrap_or(Value::Object(serde_json::Map::new()))
    } else {
        Value::Object(serde_json::Map::new())
    };

    if already_configured(&settings) {
        println!("  skipped: .claude/settings.json (hook already present)");
        return Ok(());
    }

    inject_hook(&mut settings);

    let serialized = serde_json::to_string_pretty(&settings)
        .context("failed to serialize settings.json")?;
    std::fs::write(&settings_path, serialized)
        .with_context(|| format!("failed to write {}", settings_path.display()))?;
    println!("  updated: .claude/settings.json (hook added)");
    Ok(())
}

fn already_configured(settings: &Value) -> bool {
    settings["hooks"]["UserPromptSubmit"]
        .as_array()
        .map(|arr| {
            arr.iter().any(|entry| {
                entry["hooks"]
                    .as_array()
                    .map(|hooks| {
                        hooks.iter().any(|h| {
                            h["command"].as_str() == Some("tokensaver process")
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn inject_hook(settings: &mut Value) {
    let hook_entry = serde_json::json!({
        "matcher": ".*",
        "hooks": [
            {
                "type": "command",
                "command": "tokensaver process"
            }
        ]
    });

    let hooks = settings
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert(serde_json::json!({}))
        .as_object_mut()
        .unwrap()
        .entry("UserPromptSubmit")
        .or_insert(serde_json::json!([]));

    if let Some(arr) = hooks.as_array_mut() {
        arr.push(hook_entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn init_creates_expected_files() {
        let dir = tempdir().unwrap();
        run(dir.path()).unwrap();
        assert!(dir.path().join(".tokensaver/config.toml").exists());
        assert!(dir.path().join(".tokensaver/memory.md").exists());
        assert!(dir.path().join(".claude/settings.json").exists());
    }

    #[test]
    fn init_is_idempotent() {
        let dir = tempdir().unwrap();
        run(dir.path()).unwrap();
        run(dir.path()).unwrap(); // second run must not fail or duplicate the hook
        let raw = std::fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let settings: Value = serde_json::from_str(&raw).unwrap();
        let hooks = settings["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(hooks.len(), 1, "hook must not be duplicated");
    }

    #[test]
    fn init_preserves_existing_settings() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".claude")).unwrap();
        let existing = serde_json::json!({ "theme": "dark", "model": "claude-opus-4" });
        std::fs::write(
            dir.path().join(".claude/settings.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();
        run(dir.path()).unwrap();
        let raw = std::fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let settings: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(settings["theme"], "dark");
        assert!(settings["hooks"]["UserPromptSubmit"].is_array());
    }
}
