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

pub fn append(repo_root: &Path, text: &str) -> Result<()> {
    let dir = repo_root.join(".tokensaver");
    if !dir.exists() {
        anyhow::bail!("tokensaver not initialized — run `tokensaver init` first");
    }
    let path = repo_root.join(MEMORY_FILE);
    let id = new_id();
    let entry = format!("\n<!-- id: {id} category: general -->\n{text}\n");
    let mut content = if path.exists() {
        std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?
    } else {
        String::new()
    };
    content.push_str(&entry);
    std::fs::write(&path, &content)
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("remembered [{id}]: {text}");
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
