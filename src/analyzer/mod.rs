pub mod parser;
pub mod ranker;
pub mod scanner;
pub mod search;

use crate::config::Config;
use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Function,
    Class,
    Type,
    Interface,
    Struct,
}

#[derive(Debug)]
pub struct RelevantFile {
    pub path: PathBuf,      // relative to repo root — used for display
    pub abs_path: PathBuf,  // absolute — used for reading
    pub relevance_score: f32,
    pub snippet: Option<String>,
}

#[derive(Debug)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file: PathBuf, // relative to repo root
    pub line: usize,
}

#[derive(Debug, Default)]
pub struct AnalysisResult {
    pub files: Vec<RelevantFile>,
    pub symbols: Vec<Symbol>,
}

pub fn analyze(query: &str, repo_root: &Path, config: &Config) -> Result<AnalysisResult> {
    let keywords = search::extract_keywords(query);
    if keywords.is_empty() {
        return Ok(AnalysisResult::default());
    }

    tracing::debug!(?keywords, "extracted keywords");

    // 1. Walk the repo
    let scanned = scanner::walk(repo_root, config);
    tracing::debug!(count = scanned.len(), "files scanned");

    // 2. Score files by keyword match in filename and content
    let scored = search::score_files(&scanned, &keywords, config.prompt.snippet_lines);
    tracing::debug!(count = scored.len(), "files scored > 0");

    // 3. Rank and cap
    let files = ranker::rank_files(
        scored,
        config.analyzer.max_files,
        config.prompt.include_snippets,
        config.prompt.snippet_lines,
    );

    // 4. Extract symbols from top-ranked files
    let mut all_symbols: Vec<Symbol> = files
        .iter()
        .flat_map(|f| {
            let mut syms = parser::extract_symbols(&f.abs_path, &keywords);
            // Make symbol paths relative
            for s in &mut syms {
                if let Ok(rel) = s.file.strip_prefix(repo_root) {
                    s.file = rel.to_path_buf();
                }
            }
            syms
        })
        .collect();

    // 5. Rank symbols — keyword matches in name first
    all_symbols = ranker::rank_symbols(all_symbols, &keywords, config.analyzer.max_symbols);

    Ok(AnalysisResult { files, symbols: all_symbols })
}
