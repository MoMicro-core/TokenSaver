use anyhow::{Context, Result};
use std::path::Path;

const MEMORY_FILE: &str = ".tokensaver/memory.md";

#[derive(Debug, Clone)]
pub struct Fact {
    pub id: String,
    pub category: String,
    pub text: String,
}

pub fn load(repo_root: &Path) -> Result<Vec<Fact>> {
    let path = repo_root.join(MEMORY_FILE);
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    Ok(parse(&content))
}

/// Appends a fact with category `general` — used by `tokensaver remember` CLI command.
pub fn append(repo_root: &Path, text: &str) -> Result<()> {
    append_with_category(repo_root, text, "general")
}

/// Appends a fact with an explicit category — used by the LLM memory writer.
pub fn append_with_category(repo_root: &Path, text: &str, category: &str) -> Result<()> {
    let dir = repo_root.join(".tokensaver");
    if !dir.exists() {
        anyhow::bail!("tokensaver not initialized — run `tokensaver init` first");
    }

    // Sanitise category to the allowed set
    let category = match category {
        "stack" | "conventions" | "constraints" | "decisions" | "bugs" => category,
        _ => "general",
    };

    let path = repo_root.join(MEMORY_FILE);
    let id   = new_id();
    let entry = format!("\n<!-- id: {id} category: {category} -->\n{text}\n");

    let mut content = if path.exists() {
        std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?
    } else {
        String::new()
    };
    content.push_str(&entry);
    std::fs::write(&path, &content)
        .with_context(|| format!("failed to write {}", path.display()))?;

    // Only print when called interactively (CLI), not from hook
    if std::env::var("TOKENSAVER_SILENT").is_err() {
        println!("remembered [{id}] ({category}): {text}");
    }
    Ok(())
}

/// Upserts a `key: value` fact. If an existing fact has the same key, its value
/// is replaced. Otherwise, a new fact is appended. This is the LLM's only memory
/// write path — `forget_ids` was removed because letting a 0.5B model delete
/// memory was too risky. Deduplication by key is mechanical and never loses data.
pub fn upsert_by_key(repo_root: &Path, key: &str, value: &str, category: &str) -> Result<()> {
    let new_text   = format!("{}: {}", key.trim(), value.trim());
    let key_prefix = format!("{}:", key.trim());

    let existing = load(repo_root)?;
    let match_id = existing.iter()
        .find(|f| f.text.starts_with(&key_prefix))
        .map(|f| f.id.clone());

    if let Some(id) = match_id {
        // Existing fact found — read existing text, decide whether to update
        if existing.iter().any(|f| f.id == id && f.text == new_text) {
            tracing::debug!("skipping unchanged memory fact: {new_text}");
            return Ok(());
        }
        // Remove old, then append new — atomic-ish, both go through file write
        let _ = remove_silent(repo_root, &id);
        tracing::debug!("updating memory fact: {key} → {value}");
    }

    append_with_category(repo_root, &new_text, category)
}

/// Like `remove` but never prints and returns Ok if the id doesn't exist.
fn remove_silent(repo_root: &Path, id: &str) -> Result<()> {
    let path = repo_root.join(MEMORY_FILE);
    if !path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let updated = remove_entry(&content, id);
    std::fs::write(&path, &updated)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn remove(repo_root: &Path, id: &str) -> Result<()> {
    let path = repo_root.join(MEMORY_FILE);
    if !path.exists() {
        anyhow::bail!("no memory file found — nothing to forget");
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let facts = parse(&content);
    if !facts.iter().any(|f| f.id == id) {
        anyhow::bail!("no memory entry with id '{id}'");
    }
    let updated = remove_entry(&content, id);
    std::fs::write(&path, &updated)
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("forgot [{id}]");
    Ok(())
}

fn parse(content: &str) -> Vec<Fact> {
    let mut facts = Vec::new();
    let mut current_id = None;
    let mut current_category = None;
    let mut current_lines: Vec<&str> = Vec::new();

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("<!-- id:") {
            // Flush previous entry
            if let (Some(id), Some(category)) = (current_id.take(), current_category.take()) {
                let text = current_lines.join("\n").trim().to_string();
                if !text.is_empty() {
                    facts.push(Fact { id, category, text });
                }
            }
            current_lines.clear();

            // Parse: <!-- id: abc123 category: constraints -->
            if let Some(rest) = rest.strip_suffix("-->") {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                // Expected: ["abc123", "category:", "constraints"]
                if parts.len() >= 3 {
                    current_id = Some(parts[0].to_string());
                    current_category = Some(parts[2].to_string());
                }
            }
        } else if current_id.is_some() {
            current_lines.push(line);
        }
    }

    // Flush final entry
    if let (Some(id), Some(category)) = (current_id, current_category) {
        let text = current_lines.join("\n").trim().to_string();
        if !text.is_empty() {
            facts.push(Fact { id, category, text });
        }
    }

    facts
}

fn remove_entry(content: &str, id: &str) -> String {
    let marker = format!("<!-- id: {id} ");
    let mut result = String::new();
    let mut skip = false;

    for line in content.lines() {
        if line.starts_with(&marker) {
            skip = true;
            continue;
        }
        if skip && line.starts_with("<!-- id:") {
            skip = false;
        }
        if !skip {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

fn new_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{:06x}", nanos & 0xFFFFFF)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parse_single_fact() {
        let content = "<!-- id: abc123 category: constraints -->\nDo not modify the DB.\n";
        let facts = parse(content);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].id, "abc123");
        assert_eq!(facts[0].category, "constraints");
        assert_eq!(facts[0].text, "Do not modify the DB.");
    }

    #[test]
    fn parse_multiple_facts() {
        let content = "<!-- id: aaa111 category: architecture -->\nUses FastAPI.\n\n<!-- id: bbb222 category: conventions -->\nSnake case everywhere.\n";
        let facts = parse(content);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].id, "aaa111");
        assert_eq!(facts[1].id, "bbb222");
    }

    #[test]
    fn load_returns_empty_when_no_file() {
        let dir = tempdir().unwrap();
        let facts = load(dir.path()).unwrap();
        assert!(facts.is_empty());
    }

    #[test]
    fn append_and_load_roundtrip() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".tokensaver")).unwrap();
        append(dir.path(), "Auth uses JWT").unwrap();
        let facts = load(dir.path()).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].text, "Auth uses JWT");
    }

    #[test]
    fn remove_deletes_correct_entry() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".tokensaver")).unwrap();
        let content = "<!-- id: abc123 category: general -->\nKeep this.\n\n<!-- id: def456 category: general -->\nRemove this.\n";
        std::fs::write(dir.path().join(".tokensaver/memory.md"), content).unwrap();
        remove(dir.path(), "def456").unwrap();
        let facts = load(dir.path()).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].id, "abc123");
    }
}
