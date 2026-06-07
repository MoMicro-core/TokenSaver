use anyhow::{Context, Result};
use std::path::Path;

const CHANGELOG_FILE: &str = ".tokensaver/changelog.md";

/// Appends one entry to changelog.md.
/// Format: `## YYYY-MM-DD HH:MM\n<summary>\n`
pub fn append(repo_root: &Path, summary: &str) -> Result<()> {
    if summary.trim().is_empty() {
        return Ok(());
    }

    let path = repo_root.join(CHANGELOG_FILE);
    let timestamp = current_timestamp();
    let entry = format!("## {timestamp}\n{}\n\n", summary.trim());

    let existing = if path.exists() {
        std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?
    } else {
        String::new()
    };

    // Prepend so newest entries appear first
    std::fs::write(&path, format!("{entry}{existing}"))
        .with_context(|| format!("failed to write {}", path.display()))
}

/// Returns the N most recent changelog entries as a formatted string.
pub fn recent(repo_root: &Path, limit: usize) -> Result<String> {
    let path = repo_root.join(CHANGELOG_FILE);
    if !path.exists() {
        return Ok(String::new());
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    // The file starts with "## ...", so the first chunk from split already carries
    // the prefix. Strip it so every chunk is in the same "TIMESTAMP\n..." shape.
    let entries: Vec<&str> = content
        .trim_start_matches("## ")
        .split("\n## ")
        .filter(|s| !s.trim().is_empty())
        .take(limit)
        .collect();

    if entries.is_empty() {
        return Ok(String::new());
    }

    Ok(entries
        .iter()
        .map(|e| format!("## {}", e.trim()))
        .collect::<Vec<_>>()
        .join("\n\n"))
}

pub(crate) fn current_timestamp() -> String {
    // std-only timestamp — avoids pulling in chrono
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let (y, mo, d, h, mi, s) = secs_to_datetime(secs);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

fn secs_to_datetime(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let s = (secs % 60) as u32;
    let min = ((secs / 60) % 60) as u32;
    let h = ((secs / 3600) % 24) as u32;
    let days = secs / 86400;

    // Days since 1970-01-01
    let mut year = 1970u32;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    let month_days: &[u32] = if is_leap(year) {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for &md in month_days {
        if remaining < md as u64 {
            break;
        }
        remaining -= md as u64;
        month += 1;
    }

    (year, month, remaining as u32 + 1, h, min, s)
}

fn is_leap(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn append_creates_file_and_prepends() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".tokensaver")).unwrap();

        append(dir.path(), "First entry").unwrap();
        append(dir.path(), "Second entry").unwrap();

        let content = std::fs::read_to_string(dir.path().join(".tokensaver/changelog.md")).unwrap();
        // Newest first
        assert!(content.find("Second entry").unwrap() < content.find("First entry").unwrap());
    }

    #[test]
    fn recent_returns_limited_entries() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".tokensaver")).unwrap();
        for i in 0..5 {
            append(dir.path(), &format!("Entry {i}")).unwrap();
        }
        let result = recent(dir.path(), 2).unwrap();
        assert_eq!(result.matches("##").count(), 2);
    }

    #[test]
    fn skips_empty_summary() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".tokensaver")).unwrap();
        append(dir.path(), "  ").unwrap();
        assert!(!dir.path().join(".tokensaver/changelog.md").exists());
    }
}
