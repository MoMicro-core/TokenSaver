use crate::analyzer::scanner::ScannedFile;
use std::path::PathBuf;

const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for",
    "of", "with", "by", "from", "is", "are", "was", "be", "been", "have",
    "has", "do", "does", "did", "will", "would", "can", "could", "should",
    "fix", "add", "get", "set", "use", "make", "run", "how", "why", "what",
    "when", "where", "after", "before", "this", "that", "then", "than",
    "into", "about", "up", "out", "not", "so", "if", "it", "my", "your",
];

#[derive(Debug)]
pub struct ScoredFile {
    pub path: PathBuf,
    pub rel_path: PathBuf,
    pub score: f32,
    pub matched_lines: Vec<(usize, String)>, // (1-based line number, trimmed line)
}

pub fn extract_keywords(query: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() >= 3 && !STOP_WORDS.contains(&w.as_str()))
        .filter(|w| seen.insert(w.clone())) // preserve order, deduplicate
        .collect()
}

pub fn score_files(files: &[ScannedFile], keywords: &[String], snippet_lines: usize) -> Vec<ScoredFile> {
    files.iter()
        .filter_map(|f| score_file(f, keywords, snippet_lines))
        .filter(|sf| sf.score > 0.0)
        .collect()
}

fn score_file(file: &ScannedFile, keywords: &[String], snippet_lines: usize) -> Option<ScoredFile> {
    let filename = file.rel_path.to_string_lossy().to_lowercase();
    let mut score = 0.0f32;

    // Score by filename — strong signal
    for kw in keywords {
        if filename.contains(kw.as_str()) {
            score += 10.0;
        }
    }

    let mut matched_lines: Vec<(usize, String)> = Vec::new();

    // Score by file content — only for readable files
    if crate::analyzer::scanner::is_readable_for_content(file) {
        if let Ok(content) = std::fs::read_to_string(&file.path) {
            for (i, line) in content.lines().enumerate() {
                let lower = line.to_lowercase();
                for kw in keywords {
                    if lower.contains(kw.as_str()) {
                        score += 1.0;
                        if matched_lines.len() < snippet_lines {
                            matched_lines.push((i + 1, line.trim().to_string()));
                        }
                        break; // one keyword match per line is enough for scoring
                    }
                }
            }
        }
    }

    if score == 0.0 {
        return None;
    }

    Some(ScoredFile {
        path: file.path.clone(),
        rel_path: file.rel_path.clone(),
        score,
        matched_lines,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_keywords_removes_stop_words() {
        let kw = extract_keywords("fix login redirect after session expiry");
        assert!(!kw.contains(&"fix".to_string()));
        assert!(!kw.contains(&"after".to_string()));
        assert!(kw.contains(&"login".to_string()));
        assert!(kw.contains(&"redirect".to_string()));
        assert!(kw.contains(&"session".to_string()));
        assert!(kw.contains(&"expiry".to_string()));
    }

    #[test]
    fn extracts_keywords_deduplicates() {
        let kw = extract_keywords("auth auth authentication");
        assert_eq!(kw.iter().filter(|k| k.as_str() == "auth").count(), 1);
    }

    #[test]
    fn short_words_filtered() {
        let kw = extract_keywords("fix it up now");
        assert!(kw.is_empty() || !kw.iter().any(|k| k.len() < 3));
    }
}
