use super::{Symbol, SymbolKind};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

struct Patterns {
    typescript: Vec<(Regex, SymbolKind)>,
    python: Vec<(Regex, SymbolKind)>,
    rust: Vec<(Regex, SymbolKind)>,
    go: Vec<(Regex, SymbolKind)>,
}

fn patterns() -> &'static Patterns {
    static ONCE: OnceLock<Patterns> = OnceLock::new();
    ONCE.get_or_init(|| Patterns {
        typescript: vec![
            (Regex::new(r"(?:^|\s)(?:export\s+)?(?:async\s+)?function\s+(\w+)").unwrap(), SymbolKind::Function),
            (Regex::new(r"(?:^|\s)(?:export\s+)?class\s+(\w+)").unwrap(), SymbolKind::Class),
            (Regex::new(r"(?:^|\s)(?:export\s+)?interface\s+(\w+)").unwrap(), SymbolKind::Interface),
            (Regex::new(r"(?:^|\s)(?:export\s+)?type\s+(\w+)\s*=").unwrap(), SymbolKind::Type),
            // Arrow functions assigned to const: `const foo = () =>` or `const foo = async () =>`
            (Regex::new(r"(?:^|\s)(?:export\s+)?const\s+(\w+)\s*=\s*(?:async\s*)?\(").unwrap(), SymbolKind::Function),
        ],
        python: vec![
            (Regex::new(r"^(?:async\s+)?def\s+(\w+)").unwrap(), SymbolKind::Function),
            (Regex::new(r"^class\s+(\w+)").unwrap(), SymbolKind::Class),
        ],
        rust: vec![
            (Regex::new(r"(?:^|\s)(?:pub\s+)?(?:async\s+)?fn\s+(\w+)").unwrap(), SymbolKind::Function),
            (Regex::new(r"(?:^|\s)(?:pub\s+)?struct\s+(\w+)").unwrap(), SymbolKind::Struct),
            (Regex::new(r"(?:^|\s)(?:pub\s+)?trait\s+(\w+)").unwrap(), SymbolKind::Interface),
            (Regex::new(r"(?:^|\s)(?:pub\s+)?enum\s+(\w+)").unwrap(), SymbolKind::Type),
            (Regex::new(r"(?:^|\s)(?:pub\s+)?type\s+(\w+)\s*=").unwrap(), SymbolKind::Type),
        ],
        go: vec![
            (Regex::new(r"^func\s+(?:\(\w+\s+\*?\w+\)\s+)?(\w+)").unwrap(), SymbolKind::Function),
            (Regex::new(r"^type\s+(\w+)\s+struct").unwrap(), SymbolKind::Struct),
            (Regex::new(r"^type\s+(\w+)\s+interface").unwrap(), SymbolKind::Interface),
        ],
    })
}

pub fn extract_symbols(file_path: &Path, keywords: &[String]) -> Vec<Symbol> {
    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let pats = patterns();
    let lang_patterns: &[(Regex, SymbolKind)] = match ext.as_str() {
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => &pats.typescript,
        "py"  => &pats.python,
        "rs"  => &pats.rust,
        "go"  => &pats.go,
        _ => return vec![],
    };

    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let mut symbols = Vec::new();

    for (line_no, line) in content.lines().enumerate() {
        for (re, kind) in lang_patterns {
            if let Some(cap) = re.captures(line) {
                if let Some(name_match) = cap.get(1) {
                    let name = name_match.as_str().to_string();
                    // Skip private/internal names (start with _) unless they match a keyword
                    if name.starts_with('_') && !keywords.iter().any(|k| name.to_lowercase().contains(k.as_str())) {
                        continue;
                    }
                    symbols.push(Symbol {
                        name,
                        kind: kind.clone(),
                        file: file_path.to_path_buf(),
                        line: line_no + 1,
                    });
                    break; // one symbol per line
                }
            }
        }
    }

    symbols
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(content: &str, ext: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new().suffix(&format!(".{ext}")).tempfile().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn extracts_typescript_functions() {
        let f = write_temp("export function validateSession(token: string): boolean {\n  return true;\n}\n", "ts");
        let syms = extract_symbols(f.path(), &[]);
        assert!(syms.iter().any(|s| s.name == "validateSession"));
    }

    #[test]
    fn extracts_python_functions() {
        let f = write_temp("async def handle_login(request):\n    pass\n\nclass AuthManager:\n    pass\n", "py");
        let syms = extract_symbols(f.path(), &[]);
        assert!(syms.iter().any(|s| s.name == "handle_login"));
        assert!(syms.iter().any(|s| s.name == "AuthManager"));
    }

    #[test]
    fn extracts_rust_items() {
        let f = write_temp("pub fn validate() -> bool { true }\npub struct Session;\npub trait Auth {}\n", "rs");
        let syms = extract_symbols(f.path(), &[]);
        assert!(syms.iter().any(|s| s.name == "validate"));
        assert!(syms.iter().any(|s| s.name == "Session"));
        assert!(syms.iter().any(|s| s.name == "Auth"));
    }

    #[test]
    fn line_numbers_are_one_based() {
        let f = write_temp("\ndef foo():\n    pass\n", "py");
        let syms = extract_symbols(f.path(), &[]);
        let foo = syms.iter().find(|s| s.name == "foo").unwrap();
        assert_eq!(foo.line, 2);
    }
}
