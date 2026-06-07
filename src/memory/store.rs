use anyhow::{Context, Result};
use std::io::Write as _;
use std::path::Path;

const MEMORY_FILE: &str = ".tokensaver/memory.md";

const VALID_CATEGORIES: &[&str] = &["stack", "conventions", "constraints", "decisions", "bugs"];

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

    let category = if VALID_CATEGORIES.contains(&category) { category } else { "general" };
    let id = super::new_id();
    let entry = format!("\n<!-- id: {id} category: {category} -->\n{text}\n");

    let path = repo_root.join(MEMORY_FILE);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    write!(file, "{entry}")
        .with_context(|| format!("failed to write {}", path.display()))?;

    if std::env::var("TOKENSAVER_SILENT").is_err() {
        println!("remembered [{id}] ({category}): {text}");
    }
    Ok(())
}

/// Upserts a `key: value` fact. If an existing fact shares the same key, its
/// value is replaced in-place. Otherwise a new fact is appended.
///
/// This is the LLM's only memory-write path. Deduplication is mechanical and
/// never deletes facts — it only updates existing ones, keeping memory safe even
/// when the model makes mistakes.
pub fn upsert_by_key(repo_root: &Path, key: &str, value: &str, category: &str) -> Result<()> {
    let new_text   = format!("{}: {}", key.trim(), value.trim());
    let key_prefix = format!("{}:", key.trim());

    let existing  = load(repo_root)?;
    let match_id  = existing.iter()
        .find(|f| f.text.starts_with(&key_prefix))
        .map(|f| f.id.clone());

    if let Some(id) = match_id {
        if existing.iter().any(|f| f.id == id && f.text == new_text) {
            tracing::debug!("skipping unchanged memory fact: {new_text}");
            return Ok(());
        }
        let _ = remove_silent(repo_root, &id);
        tracing::debug!("updating memory fact: {key} → {value}");
    }

    append_with_category(repo_root, &new_text, category)
}

/// Like `remove` but never prints and treats a missing id as a no-op.
fn remove_silent(repo_root: &Path, id: &str) -> Result<()> {
    let path = repo_root.join(MEMORY_FILE);
    if !path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    std::fs::write(&path, remove_entry(&content, id))
        .with_context(|| format!("failed to write {}", path.display()))
}

pub fn remove(repo_root: &Path, id: &str) -> Result<()> {
    let path = repo_root.join(MEMORY_FILE);
    if !path.exists() {
        anyhow::bail!("no memory file found — nothing to forget");
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    if !parse(&content).iter().any(|f| f.id == id) {
        anyhow::bail!("no memory entry with id '{id}'");
    }
    std::fs::write(&path, remove_entry(&content, id))
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("forgot [{id}]");
    Ok(())
}

fn parse(content: &str) -> Vec<Fact> {
    let mut facts = Vec::new();
    let mut current_id: Option<String> = None;
    let mut current_category: Option<String> = None;
    let mut current_lines: Vec<&str> = Vec::new();

    let flush = |id: Option<String>, category: Option<String>, lines: &[&str], facts: &mut Vec<Fact>| {
        if let (Some(id), Some(category)) = (id, category) {
            let text = lines.join("\n").trim().to_string();
            if !text.is_empty() {
                facts.push(Fact { id, category, text });
            }
        }
    };

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("<!-- id:") {
            flush(current_id.take(), current_category.take(), &current_lines, &mut facts);
            current_lines.clear();

            // Format: <!-- id: abc123 category: constraints -->
            if let Some(inner) = rest.strip_suffix("-->") {
                let parts: Vec<&str> = inner.split_whitespace().collect();
                if parts.len() >= 3 {
                    current_id       = Some(parts[0].to_string());
                    current_category = Some(parts[2].to_string());
                }
            }
        } else if current_id.is_some() {
            current_lines.push(line);
        }
    }

    flush(current_id, current_category, &current_lines, &mut facts);
    facts
}

/// Removes all lines belonging to the entry with `id`, leaving everything else intact.
fn remove_entry(content: &str, id: &str) -> String {
    let marker = format!("<!-- id: {id} ");
    let mut result = String::new();
    let mut in_removed_entry = false;

    for line in content.lines() {
        if line.starts_with(&marker) {
            in_removed_entry = true;
            continue;
        }
        if in_removed_entry && line.starts_with("<!-- id:") {
            in_removed_entry = false;
        }
        if !in_removed_entry {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
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
