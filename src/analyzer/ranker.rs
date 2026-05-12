use super::{RelevantFile, Symbol};
use crate::analyzer::search::ScoredFile;

pub fn rank_files(
    scored: Vec<ScoredFile>,
    max_files: usize,
    include_snippets: bool,
    snippet_lines: usize,
) -> Vec<RelevantFile> {
    let mut sorted = scored;
    sorted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    sorted.truncate(max_files);

    sorted
        .into_iter()
        .map(|sf| {
            let rel = sf.rel_path.clone();
            let snippet = if include_snippets && !sf.matched_lines.is_empty() {
                Some(build_snippet(&sf.matched_lines, snippet_lines))
            } else {
                None
            };
            RelevantFile {
                path: rel,
                abs_path: sf.path,
                relevance_score: sf.score,
                snippet,
            }
        })
        .collect()
}

pub fn rank_symbols(
    mut symbols: Vec<Symbol>,
    keywords: &[String],
    max_symbols: usize,
) -> Vec<Symbol> {
    // Sort: keyword matches in name first, then by file order
    symbols.sort_by(|a, b| {
        let a_match = symbol_keyword_score(&a.name, keywords);
        let b_match = symbol_keyword_score(&b.name, keywords);
        b_match.partial_cmp(&a_match).unwrap_or(std::cmp::Ordering::Equal)
    });
    symbols.truncate(max_symbols);
    symbols
}

fn symbol_keyword_score(name: &str, keywords: &[String]) -> f32 {
    let lower = name.to_lowercase();
    keywords.iter().filter(|k| lower.contains(k.as_str())).count() as f32
}

fn build_snippet(lines: &[(usize, String)], max: usize) -> String {
    lines
        .iter()
        .take(max)
        .map(|(n, l)| format!("{n:>4} | {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::search::ScoredFile;
    use std::path::PathBuf; // needed for make_scored

    fn make_scored(rel: &str, score: f32) -> ScoredFile {
        ScoredFile {
            path: PathBuf::from(rel),
            rel_path: PathBuf::from(rel),
            score,
            matched_lines: vec![],
        }
    }

    #[test]
    fn ranks_by_score_descending() {
        let files = vec![
            make_scored("low.ts", 1.0),
            make_scored("high.ts", 10.0),
            make_scored("mid.ts", 5.0),
        ];
        let ranked = rank_files(files, 10, false, 0);
        assert_eq!(ranked[0].path.to_string_lossy(), "high.ts");
        assert_eq!(ranked[1].path.to_string_lossy(), "mid.ts");
    }

    #[test]
    fn respects_max_files() {
        let files = (0..10).map(|i| make_scored(&format!("f{i}.ts"), i as f32)).collect();
        let ranked = rank_files(files, 3, false, 0);
        assert_eq!(ranked.len(), 3);
    }
}
