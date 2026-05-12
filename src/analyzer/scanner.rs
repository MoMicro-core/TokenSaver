use crate::config::Config;
use std::path::{Path, PathBuf};

const MAX_FILE_BYTES: u64 = 1_000_000; // skip files over 1MB for content scanning

pub struct ScannedFile {
    pub path: PathBuf,         // absolute path
    pub rel_path: PathBuf,     // relative to repo root
    pub size_bytes: u64,
}

pub fn walk(repo_root: &Path, config: &Config) -> Vec<ScannedFile> {
    let extensions = language_extensions(&config.analyzer.languages);
    let exclude: Vec<&str> = config.analyzer.exclude.iter().map(|s| s.as_str()).collect();

    ignore::WalkBuilder::new(repo_root)
        .standard_filters(true) // respects .gitignore, .ignore, hidden files
        .build()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter_map(|entry| {
            let abs = entry.into_path();
            let rel = abs.strip_prefix(repo_root).ok()?.to_path_buf();

            // Skip excluded directory segments
            if rel.components().any(|c| {
                exclude.contains(&c.as_os_str().to_string_lossy().as_ref())
            }) {
                return None;
            }

            // Only keep supported extensions
            let ext = abs.extension()?.to_string_lossy().to_lowercase();
            if !extensions.contains(ext.as_str()) {
                return None;
            }

            let size_bytes = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
            Some(ScannedFile { path: abs, rel_path: rel, size_bytes })
        })
        .collect()
}

fn language_extensions(languages: &[String]) -> std::collections::HashSet<String> {
    let mut exts = std::collections::HashSet::new();
    for lang in languages {
        match lang.as_str() {
            "typescript" => { exts.insert("ts"); exts.insert("tsx"); }
            "javascript" => { exts.insert("js"); exts.insert("jsx"); exts.insert("mjs"); exts.insert("cjs"); }
            "python"     => { exts.insert("py"); }
            "rust"       => { exts.insert("rs"); }
            "go"         => { exts.insert("go"); }
            _ => {}
        }
    }
    exts.into_iter().map(String::from).collect()
}

pub fn is_readable_for_content(file: &ScannedFile) -> bool {
    file.size_bytes > 0 && file.size_bytes <= MAX_FILE_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::fs;
    use tempfile::tempdir;

    fn make_config() -> Config { Config::default() }

    #[test]
    fn finds_supported_files() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("main.ts"), "const x = 1;").unwrap();
        fs::write(dir.path().join("README.md"), "# hello").unwrap();
        fs::write(dir.path().join("app.py"), "def foo(): pass").unwrap();

        let files = walk(dir.path(), &make_config());
        let names: Vec<_> = files.iter()
            .map(|f| f.rel_path.to_string_lossy().to_string())
            .collect();
        assert!(names.iter().any(|n| n == "main.ts"), "should find main.ts");
        assert!(names.iter().any(|n| n == "app.py"), "should find app.py");
        assert!(!names.iter().any(|n| n == "README.md"), "should skip README.md");
    }

    #[test]
    fn skips_excluded_dirs() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("node_modules/lib.ts"), "export {}").unwrap();
        fs::write(dir.path().join("index.ts"), "import './x'").unwrap();

        let files = walk(dir.path(), &make_config());
        let names: Vec<_> = files.iter()
            .map(|f| f.rel_path.to_string_lossy().to_string())
            .collect();
        assert!(!names.iter().any(|n| n.contains("node_modules")));
        assert!(names.iter().any(|n| n == "index.ts"));
    }
}
